//! Speech-to-text. Phase 2: whisper-rs (whisper.cpp), optional PhoWhisper for Vietnamese.

#[derive(Debug, Clone)]
pub struct Transcript {
    pub text: String,
    pub language: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum SttError {
    #[error("model not loaded: {0}")]
    ModelNotLoaded(String),
    #[error("transcription failed: {0}")]
    Failed(String),
}

pub trait SpeechToText: Send + Sync {
    /// `samples`: 16 kHz mono PCM.
    fn transcribe(&self, samples: &[f32]) -> Result<Transcript, SttError>;
}
