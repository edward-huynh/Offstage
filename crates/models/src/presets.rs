use crate::ModelSpec;

/// whisper.cpp `small` q5_1 (multilingual), the default STT model.
///
/// Verified against the Hugging Face API for `ggerganov/whisper.cpp` (LFS oid = SHA-256, size)
/// and the `x-linked-etag` header of the resolve URL. The URL is pinned to a commit so the
/// hash cannot drift if the repo is updated; the model card declares `license: mit`.
pub fn whisper_small_q5() -> ModelSpec {
    ModelSpec {
        id: "whisper-small-q5_1".into(),
        file_name: "ggml-small-q5_1.bin".into(),
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/\
              5359861c739e955e79d9a303bcbc70fb988958b1/ggml-small-q5_1.bin"
            .into(),
        sha256: "ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb".into(),
        size: 190_085_487,
        license: "MIT".into(),
        license_url: Some("https://huggingface.co/ggerganov/whisper.cpp".into()),
    }
}

/// Commit of `Xenova/multilingual-e5-small` that the embedding files below are pinned to. The
/// repo is the ONNX export of `intfloat/multilingual-e5-small` (model card: MIT). The hashes are
/// the LFS oids reported by the Hugging Face API for the large files, and the SHA-256 of the
/// downloaded content for the small ones.
const E5_COMMIT: &str = "761b726dd34fb83930e26aab4e9ac3899aa1fa78";

fn e5_file(id: &str, file: &str, remote: &str, sha256: &str, size: u64) -> ModelSpec {
    ModelSpec {
        id: format!("multilingual-e5-small/{id}"),
        file_name: format!("multilingual-e5-small.{file}"),
        url: format!(
            "https://huggingface.co/Xenova/multilingual-e5-small/resolve/{E5_COMMIT}/{remote}"
        ),
        sha256: sha256.into(),
        size,
        license: "MIT".into(),
        license_url: Some("https://huggingface.co/intfloat/multilingual-e5-small".into()),
    }
}

/// The files of multilingual-e5-small (384 dimensions, 100 languages, int8 weights), which the
/// document search uses to match a question to a paragraph by meaning. Order: the model, then the
/// tokenizer files. About 135 MB in all, almost all of it the first file.
pub fn multilingual_e5_small() -> Vec<ModelSpec> {
    vec![
        e5_file(
            "model",
            "model.onnx",
            "onnx/model_int8.onnx",
            "4d24e2bc01a447951524466ef533e52944bf48509e6552810bcee1a2711cb02c",
            118_054_593,
        ),
        e5_file(
            "tokenizer",
            "tokenizer.json",
            "tokenizer.json",
            "0b44a9d7b51c3c62626640cda0e2c2f70fdacdc25bbbd68038369d14ebdf4c39",
            17_082_730,
        ),
        e5_file(
            "config",
            "config.json",
            "config.json",
            "cb99455288675345e1a4f411438d5d0adbba5fbd3a67ea4fb03c015433b996c1",
            658,
        ),
        e5_file(
            "special-tokens",
            "special_tokens_map.json",
            "special_tokens_map.json",
            "d05497f1da52c5e09554c0cd874037a083e1dc1b9cfd48034d1c717f1afc07a7",
            167,
        ),
        e5_file(
            "tokenizer-config",
            "tokenizer_config.json",
            "tokenizer_config.json",
            "a1d6bc8734a6f635dc158508bef000f8e2e5a759c7d92f984b2c86e5ff53425b",
            443,
        ),
    ]
}

/// Qwen3 4B, 4-bit (Q4_K_M), the local language model that measured best for the grounded
/// answer (`docs/LLM-BENCH.md`). The URL is pinned to a commit, and the hash is the LFS oid the
/// Hugging Face API reports (also checked against a copy downloaded earlier). Apache-2.0.
pub fn qwen3_4b() -> ModelSpec {
    ModelSpec {
        id: "qwen3-4b-q4_k_m".into(),
        file_name: "Qwen3-4B-Q4_K_M.gguf".into(),
        url: "https://huggingface.co/Qwen/Qwen3-4B-GGUF/resolve/\
              bc640142c66e1fdd12af0bd68f40445458f3869b/Qwen3-4B-Q4_K_M.gguf"
            .into(),
        sha256: "7485fe6f11af29433bc51cab58009521f205840f5b4ae3a32fa7f92e8534fdf5".into(),
        size: 2_497_280_256,
        license: "Apache-2.0".into(),
        license_url: Some("https://huggingface.co/Qwen/Qwen3-4B-GGUF".into()),
    }
}

/// Qwen3 1.7B, 4-bit: less than half the size and about 2.5 times faster to the first token than
/// the 4B, for computers with little memory, at the cost of more invented detail
/// (`docs/LLM-BENCH.md`). Apache-2.0.
pub fn qwen3_1_7b() -> ModelSpec {
    ModelSpec {
        id: "qwen3-1.7b-q4_k_m".into(),
        file_name: "Qwen3-1.7B-Q4_K_M.gguf".into(),
        url: "https://huggingface.co/unsloth/Qwen3-1.7B-GGUF/resolve/\
              d7f544eead698dbd1f15126ef60b45a1e1933222/Qwen3-1.7B-Q4_K_M.gguf"
            .into(),
        sha256: "b139949c5bd74937ad8ed8c8cf3d9ffb1e99c866c823204dc42c0d91fa181897".into(),
        size: 1_107_409_472,
        license: "Apache-2.0".into(),
        license_url: Some("https://huggingface.co/unsloth/Qwen3-1.7B-GGUF".into()),
    }
}

/// The language models that can run on this machine, best first.
pub fn local_llms() -> Vec<ModelSpec> {
    vec![qwen3_4b(), qwen3_1_7b()]
}

pub fn all() -> Vec<ModelSpec> {
    let mut all = vec![whisper_small_q5()];
    all.extend(multilingual_e5_small());
    all.extend(local_llms());
    all
}

pub fn find(id: &str) -> Option<ModelSpec> {
    all().into_iter().find(|m| m.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_well_formed() {
        for m in all() {
            assert_eq!(m.sha256.len(), 64, "{}", m.id);
            assert!(m.sha256.chars().all(|c| c.is_ascii_hexdigit()), "{}", m.id);
            assert_eq!(m.sha256, m.sha256.to_lowercase(), "{}", m.id);
            assert!(m.url.starts_with("https://"), "{}", m.id);
            assert!(!m.file_name.contains(['/', '\\']), "{}", m.id);
            assert!(m.size > 0 && !m.license.is_empty(), "{}", m.id);
        }
    }

    #[test]
    fn the_embedding_bundle_has_the_model_and_four_small_files_pinned_to_a_commit() {
        let files = multilingual_e5_small();
        assert_eq!(files.len(), 5);
        assert!(files[0].size > 100_000_000 && files[1..].iter().all(|f| f.size < 20_000_000));
        assert!(files.iter().all(|f| f.url.contains(E5_COMMIT)));
        let mut names: Vec<_> = files.iter().map(|f| &f.file_name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 5);
    }

    #[test]
    fn preset_ids_are_unique_and_findable() {
        let ids: Vec<_> = all().into_iter().map(|m| m.id).collect();
        let mut dedup = ids.clone();
        dedup.sort();
        dedup.dedup();
        assert_eq!(ids.len(), dedup.len());
        assert!(find("whisper-small-q5_1").is_some());
        assert!(find("multilingual-e5-small/model").is_some());
        assert!(find("qwen3-4b-q4_k_m").is_some());
        assert!(find("nope").is_none());
    }
}
