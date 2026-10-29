//! Audio capture (mic + system loopback) and voice activity detection.
//! Phase 2: WASAPI loopback on Windows, ScreenCaptureKit / Core Audio taps on macOS.

pub mod capture;
pub mod resample;
pub mod segment;
pub mod vad;

use offstage_core::Speaker;

/// 16 kHz mono PCM, the format Whisper expects.
pub const SAMPLE_RATE: u32 = 16_000;

#[derive(Debug, Clone)]
pub struct AudioFrame {
    pub speaker: Speaker,
    pub samples: Vec<f32>,
}

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("audio device unavailable: {0}")]
    Device(String),
    #[error("permission denied: {0}")]
    Permission(String),
    #[error("resampling failed: {0}")]
    Resample(String),
}

pub trait AudioSource: Send {
    fn speaker(&self) -> Speaker;
    fn start(&mut self, sink: Box<dyn FnMut(AudioFrame) + Send>) -> Result<(), AudioError>;
    fn stop(&mut self);
}
