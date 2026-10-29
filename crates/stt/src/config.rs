use std::path::PathBuf;
use std::str::FromStr;

use crate::SttError;

/// Spoken language handed to the decoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    /// Let the model detect the language on every utterance.
    #[default]
    Auto,
    Vietnamese,
    English,
}

impl Language {
    /// Code understood by whisper.cpp, `None` meaning auto-detect.
    pub fn code(self) -> Option<&'static str> {
        match self {
            Language::Auto => None,
            Language::Vietnamese => Some("vi"),
            Language::English => Some("en"),
        }
    }
}

impl FromStr for Language {
    type Err = SttError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" | "" => Ok(Language::Auto),
            "vi" => Ok(Language::Vietnamese),
            "en" => Ok(Language::English),
            other => Err(SttError::InvalidConfig(format!(
                "unsupported language `{other}` (expected auto, vi or en)"
            ))),
        }
    }
}

#[derive(Debug, Clone)]
pub struct WhisperConfig {
    /// ggml model file (e.g. `ggml-small-q5_1.bin`). Downloading it is the caller's job.
    pub model_path: PathBuf,
    pub language: Language,
    /// Decoder threads; `None` lets the engine pick from the available cores.
    pub n_threads: Option<usize>,
    /// Use the GPU backend when the crate was built with one (`metal`, `cuda`, `vulkan`).
    pub use_gpu: bool,
}

impl WhisperConfig {
    pub fn new(model_path: impl Into<PathBuf>) -> Self {
        Self {
            model_path: model_path.into(),
            language: Language::default(),
            n_threads: None,
            use_gpu: true,
        }
    }

    pub fn validate(&self) -> Result<(), SttError> {
        if !self.model_path.is_file() {
            return Err(SttError::ModelNotFound(self.model_path.clone()));
        }
        if self.n_threads == Some(0) {
            return Err(SttError::InvalidConfig(
                "n_threads must be at least 1".into(),
            ));
        }
        Ok(())
    }

    /// Thread count to hand to the decoder: the configured value, else the available
    /// cores capped at 8 (whisper.cpp scales poorly beyond that).
    pub fn threads(&self) -> usize {
        self.n_threads.unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
                .min(8)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_codes() {
        assert_eq!(Language::Auto.code(), None);
        assert_eq!(Language::Vietnamese.code(), Some("vi"));
        assert_eq!(Language::English.code(), Some("en"));
    }

    #[test]
    fn language_parses_known_values() {
        assert_eq!("auto".parse::<Language>().unwrap(), Language::Auto);
        assert_eq!(" VI ".parse::<Language>().unwrap(), Language::Vietnamese);
        assert_eq!("en".parse::<Language>().unwrap(), Language::English);
    }

    #[test]
    fn language_rejects_unknown_values() {
        let err = "fr".parse::<Language>().unwrap_err();
        assert!(matches!(err, SttError::InvalidConfig(_)));
    }

    #[test]
    fn validate_reports_missing_model() {
        let cfg = WhisperConfig::new("/definitely/not/here/ggml-small-q5_1.bin");
        match cfg.validate() {
            Err(SttError::ModelNotFound(p)) => assert!(p.ends_with("ggml-small-q5_1.bin")),
            other => panic!("expected ModelNotFound, got {other:?}"),
        }
    }

    #[test]
    fn validate_rejects_directory_as_model() {
        let cfg = WhisperConfig::new(std::env::temp_dir());
        assert!(matches!(cfg.validate(), Err(SttError::ModelNotFound(_))));
    }

    #[test]
    fn validate_rejects_zero_threads() {
        let mut cfg = WhisperConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"));
        cfg.n_threads = Some(0);
        assert!(matches!(cfg.validate(), Err(SttError::InvalidConfig(_))));
    }

    #[test]
    fn validate_accepts_existing_file() {
        let mut cfg = WhisperConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"));
        cfg.n_threads = Some(2);
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn threads_defaults_within_bounds() {
        let cfg = WhisperConfig::new("m.bin");
        assert!((1..=8).contains(&cfg.threads()));
        let mut cfg = cfg;
        cfg.n_threads = Some(3);
        assert_eq!(cfg.threads(), 3);
    }

    #[test]
    fn error_messages_are_readable() {
        assert_eq!(SttError::EmptyAudio.to_string(), "no audio samples");
        let msg = SttError::ModelNotFound("a/b.bin".into()).to_string();
        assert!(msg.contains("a/b.bin"));
    }
}
