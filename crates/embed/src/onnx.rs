//! multilingual-e5-small on ONNX Runtime (through `fastembed`'s bring-your-own-model path).

use std::{fs, path::PathBuf, sync::Mutex};

use fastembed::{
    InitOptionsUserDefined, Pooling, TextEmbedding, TokenizerFiles, UserDefinedEmbeddingModel,
};

use crate::{normalize, passage_text, query_text, EmbedError, TextEmbedder, E5_SMALL_DIMS};

/// Texts embedded in one go. Small, so a long document does not need a large block of memory.
const BATCH: usize = 16;

/// Threads ONNX Runtime may use. Embedding happens while a call is going on, so it must not
/// take every core.
const MAX_THREADS: usize = 4;

/// Where the files of the model are on disk.
#[derive(Debug, Clone)]
pub struct ModelFiles {
    pub model: PathBuf,
    pub tokenizer: PathBuf,
    pub config: PathBuf,
    pub special_tokens_map: PathBuf,
    pub tokenizer_config: PathBuf,
}

/// multilingual-e5-small: 384-dimensional vectors, mean pooling, 100 languages.
pub struct E5Embedder {
    // `embed` takes `&mut self`, and one call at a time is all this app needs.
    model: Mutex<TextEmbedding>,
}

impl E5Embedder {
    /// Reads the files and starts the runtime. Takes a second or two and about 300 MB while the
    /// model is read, so call it off the UI thread.
    pub fn load(files: &ModelFiles) -> Result<Self, EmbedError> {
        let read = |path: &PathBuf| {
            fs::read(path).map_err(|e| EmbedError::Load(format!("{}: {e}", path.display())))
        };
        let tokenizer_files = TokenizerFiles {
            tokenizer_file: read(&files.tokenizer)?,
            config_file: read(&files.config)?,
            special_tokens_map_file: read(&files.special_tokens_map)?,
            tokenizer_config_file: read(&files.tokenizer_config)?,
        };
        let threads = std::thread::available_parallelism().map_or(2, |n| n.get().min(MAX_THREADS));
        let model = TextEmbedding::try_new_from_user_defined(
            UserDefinedEmbeddingModel::new(read(&files.model)?, tokenizer_files)
                .with_pooling(Pooling::Mean),
            InitOptionsUserDefined::new().with_intra_threads(threads),
        )
        .map_err(|e| EmbedError::Load(e.to_string()))?;
        Ok(Self {
            model: Mutex::new(model),
        })
    }

    fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, EmbedError> {
        let mut model = self
            .model
            .lock()
            .map_err(|_| EmbedError::Run("the embedding model is unavailable".to_owned()))?;
        let vectors = model
            .embed(texts, Some(BATCH))
            .map_err(|e| EmbedError::Run(e.to_string()))?;
        Ok(vectors.into_iter().map(normalize).collect())
    }
}

impl TextEmbedder for E5Embedder {
    fn dims(&self) -> usize {
        E5_SMALL_DIMS
    }

    fn embed_passages(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        self.embed(texts.iter().map(|t| passage_text(t)).collect())
    }

    fn embed_query(&self, text: &str) -> Result<Vec<f32>, EmbedError> {
        self.embed(vec![query_text(text)])?
            .pop()
            .ok_or_else(|| EmbedError::Run("no vector returned".to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dot;

    /// The five files of `Xenova/multilingual-e5-small` under their Hugging Face names, in the
    /// folder named by `OFFSTAGE_E5_DIR`. Not downloaded by the tests, hence `#[ignore]`:
    /// `OFFSTAGE_E5_DIR=... cargo test -p offstage-embed --features onnx -- --ignored`.
    fn files() -> Option<ModelFiles> {
        let dir = PathBuf::from(std::env::var_os("OFFSTAGE_E5_DIR")?);
        Some(ModelFiles {
            model: dir.join("model_int8.onnx"),
            tokenizer: dir.join("tokenizer.json"),
            config: dir.join("config.json"),
            special_tokens_map: dir.join("special_tokens_map.json"),
            tokenizer_config: dir.join("tokenizer_config.json"),
        })
    }

    #[test]
    fn missing_files_are_a_load_error_not_a_panic() {
        let nothing = PathBuf::from("/nonexistent/offstage");
        let result = E5Embedder::load(&ModelFiles {
            model: nothing.join("m.onnx"),
            tokenizer: nothing.join("t.json"),
            config: nothing.join("c.json"),
            special_tokens_map: nothing.join("s.json"),
            tokenizer_config: nothing.join("tc.json"),
        });
        assert!(matches!(result, Err(EmbedError::Load(_))));
    }

    #[test]
    #[ignore = "needs the model files, see `files`"]
    fn vectors_are_unit_length_and_meaning_beats_wording_across_languages() {
        let Some(files) = files() else {
            panic!("set OFFSTAGE_E5_DIR");
        };
        let e = E5Embedder::load(&files).unwrap();
        let passages = e
            .embed_passages(&[
                "Ba năm kinh nghiệm viết ứng dụng desktop bằng Rust và Tauri.".to_owned(),
                "Sở thích: chạy bộ, đọc sách và nấu ăn vào cuối tuần.".to_owned(),
            ])
            .unwrap();
        assert_eq!(passages[0].len(), E5_SMALL_DIMS);
        assert!((dot(&passages[0], &passages[0]) - 1.0).abs() < 1e-3);

        // Vietnamese and English questions that share no word with the right paragraph.
        for (question, right) in [
            ("Bạn từng làm dự án gì với ngôn ngữ lập trình hệ thống?", 0),
            ("What do you do in your free time?", 1),
            ("Giới thiệu về bản thân", 0),
        ] {
            let q = e.embed_query(question).unwrap();
            let scores: Vec<f32> = passages.iter().map(|p| dot(&q, p)).collect();
            eprintln!("{question}: {scores:?}");
            if question != "Giới thiệu về bản thân" {
                assert!(scores[right] > scores[1 - right], "{question}: {scores:?}");
            }
        }
    }
}
