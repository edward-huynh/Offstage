//! Speech-to-text. Phase 2: whisper-rs (whisper.cpp), optional PhoWhisper for Vietnamese.
//!
//! The trait and the configuration are always available. The whisper engine lives behind
//! the `whisper` cargo feature (see `Cargo.toml`), so the workspace builds and tests
//! without cmake or a model.

mod config;
mod text;
#[cfg(feature = "whisper")]
mod whisper;

pub use config::{Language, WhisperConfig};
pub use text::join_segments;
#[cfg(feature = "whisper")]
pub use whisper::WhisperStt;

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Transcript {
    pub text: String,
    pub language: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum SttError {
    #[error("model not loaded: {0}")]
    ModelNotLoaded(String),
    #[error("model file not found: {}", .0.display())]
    ModelNotFound(PathBuf),
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("no audio samples")]
    EmptyAudio,
    #[error("transcription failed: {0}")]
    Failed(String),
}

pub trait SpeechToText: Send + Sync {
    /// `samples`: 16 kHz mono PCM.
    fn transcribe(&self, samples: &[f32]) -> Result<Transcript, SttError>;
}
