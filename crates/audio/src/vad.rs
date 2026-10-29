//! Voice activity detection: how likely is it that a short window of audio holds speech?

/// Decides, window by window, whether someone is speaking. Windows are 16 kHz mono f32.
pub trait Vad: Send {
    /// Number of samples [`Vad::probability`] expects each time.
    fn window_size(&self) -> usize;

    /// Probability from 0.0 to 1.0 that `window` (exactly `window_size` samples) holds speech.
    fn probability(&mut self, window: &[f32]) -> f32;
}

/// A light detector that calls a window speech when it is louder than a fixed level.
///
/// It needs no model file and costs next to nothing, but it cannot tell speech from
/// other loud sounds and depends on the input gain. It is the fallback until a model
/// based detector (Silero) replaces it as the default.
pub struct EnergyVad {
    window: usize,
    threshold: f32,
}

impl EnergyVad {
    /// 512 samples is 32 ms at 16 kHz, the window size Silero uses too.
    pub const WINDOW: usize = 512;
    /// Level in dBFS above which a window counts as speech.
    pub const DEFAULT_THRESHOLD_DB: f32 = -42.0;

    pub fn new(threshold_db: f32) -> Self {
        Self {
            window: Self::WINDOW,
            threshold: 10f32.powf(threshold_db / 20.0),
        }
    }
}

impl Default for EnergyVad {
    fn default() -> Self {
        Self::new(Self::DEFAULT_THRESHOLD_DB)
    }
}

impl Vad for EnergyVad {
    fn window_size(&self) -> usize {
        self.window
    }

    fn probability(&mut self, window: &[f32]) -> f32 {
        if window.is_empty() {
            return 0.0;
        }
        let rms = (window.iter().map(|s| s * s).sum::<f32>() / window.len() as f32).sqrt();
        if rms > self.threshold {
            1.0
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::TAU;

    use super::*;

    fn tone(amplitude: f32) -> Vec<f32> {
        (0..EnergyVad::WINDOW)
            .map(|i| amplitude * (TAU * 300.0 * i as f32 / 16_000.0).sin())
            .collect()
    }

    #[test]
    fn silence_is_not_speech() {
        let mut vad = EnergyVad::default();
        assert_eq!(vad.probability(&vec![0.0; EnergyVad::WINDOW]), 0.0);
    }

    #[test]
    fn a_loud_tone_is_speech() {
        let mut vad = EnergyVad::default();
        assert_eq!(vad.probability(&tone(0.3)), 1.0);
    }

    #[test]
    fn faint_background_noise_is_not_speech() {
        // About -60 dBFS, well under the default threshold.
        let mut vad = EnergyVad::default();
        assert_eq!(vad.probability(&tone(0.001)), 0.0);
    }

    #[test]
    fn the_threshold_is_adjustable() {
        let quiet = tone(0.01); // about -43 dBFS rms
        assert_eq!(EnergyVad::new(-60.0).probability(&quiet), 1.0);
        assert_eq!(EnergyVad::new(-20.0).probability(&quiet), 0.0);
    }

    #[test]
    fn an_empty_window_is_not_speech() {
        assert_eq!(EnergyVad::default().probability(&[]), 0.0);
    }
}
