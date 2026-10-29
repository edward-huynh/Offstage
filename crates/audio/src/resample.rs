//! Turns whatever a device delivers (any channel count, any sample rate) into the
//! 16 kHz mono f32 that voice activity detection and Whisper expect.

use rubato::{audioadapter_buffers::direct::InterleavedSlice, Fft, FixedSync, Resampler};

use crate::{AudioError, SAMPLE_RATE};

/// Input frames per resampler call, about 20 ms at 48 kHz. The resampler may round it up.
const CHUNK_FRAMES: usize = 1024;

fn resample_error(e: impl std::fmt::Display) -> AudioError {
    AudioError::Resample(e.to_string())
}

/// Averages interleaved channels into one. Returns an empty vector for zero channels.
pub fn downmix_to_mono(interleaved: &[f32], channels: usize) -> Vec<f32> {
    match channels {
        0 => Vec::new(),
        1 => interleaved.to_vec(),
        n => interleaved
            .chunks_exact(n)
            .map(|frame| frame.iter().sum::<f32>() / n as f32)
            .collect(),
    }
}

/// Streaming mono resampler to [`SAMPLE_RATE`]. Feed it chunks of any size and it returns
/// whatever output is ready, so the pieces can be concatenated into the full signal.
pub struct MonoResampler {
    /// `None` when the input is already at the target rate.
    fft: Option<Fft<f32>>,
    pending: Vec<f32>,
    /// Output the resampler produces before the first real sample (its filter delay).
    delay_left: usize,
}

impl MonoResampler {
    pub fn new(input_rate: u32) -> Result<Self, AudioError> {
        if input_rate == 0 {
            return Err(AudioError::Resample("sample rate must be above 0".into()));
        }
        if input_rate == SAMPLE_RATE {
            return Ok(Self {
                fft: None,
                pending: Vec::new(),
                delay_left: 0,
            });
        }
        let fft = Fft::<f32>::new(
            input_rate as usize,
            SAMPLE_RATE as usize,
            CHUNK_FRAMES,
            1,
            FixedSync::Input,
        )
        .map_err(resample_error)?;
        let delay_left = fft.output_delay();
        Ok(Self {
            fft: Some(fft),
            pending: Vec::new(),
            delay_left,
        })
    }

    pub fn push(&mut self, mono: &[f32]) -> Result<Vec<f32>, AudioError> {
        let Some(fft) = self.fft.as_mut() else {
            return Ok(mono.to_vec());
        };
        self.pending.extend_from_slice(mono);

        let mut output = Vec::new();
        let mut scratch = vec![0.0f32; fft.output_frames_max()];
        loop {
            let needed = fft.input_frames_next();
            if self.pending.len() < needed {
                break;
            }
            let input = InterleavedSlice::new(&self.pending[..needed], 1, needed)
                .map_err(resample_error)?;
            let capacity = scratch.len();
            let mut out =
                InterleavedSlice::new_mut(&mut scratch[..], 1, capacity).map_err(resample_error)?;
            let (used, produced) = fft
                .process_into_buffer(&input, &mut out, None)
                .map_err(resample_error)?;

            let skip = self.delay_left.min(produced);
            self.delay_left -= skip;
            output.extend_from_slice(&scratch[skip..produced]);
            self.pending.drain(..used);
        }
        Ok(output)
    }
}

/// Device audio in, 16 kHz mono out: downmixes, then resamples.
pub struct FrameNormalizer {
    channels: usize,
    resampler: MonoResampler,
}

impl FrameNormalizer {
    pub fn new(sample_rate: u32, channels: u16) -> Result<Self, AudioError> {
        if channels == 0 {
            return Err(AudioError::Resample("channel count must be above 0".into()));
        }
        Ok(Self {
            channels: usize::from(channels),
            resampler: MonoResampler::new(sample_rate)?,
        })
    }

    /// `interleaved` holds whole frames of `channels` samples. A trailing partial frame is dropped.
    pub fn push(&mut self, interleaved: &[f32]) -> Result<Vec<f32>, AudioError> {
        let mono = downmix_to_mono(interleaved, self.channels);
        self.resampler.push(&mono)
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::TAU;

    use super::*;

    fn sine(rate: u32, freq: f32, seconds: f32, amplitude: f32) -> Vec<f32> {
        let n = (rate as f32 * seconds) as usize;
        (0..n)
            .map(|i| amplitude * (TAU * freq * i as f32 / rate as f32).sin())
            .collect()
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    /// Rising zero crossings per second: the frequency of a clean tone.
    fn frequency(samples: &[f32], rate: u32) -> f32 {
        let crossings = samples
            .windows(2)
            .filter(|w| w[0] <= 0.0 && w[1] > 0.0)
            .count();
        crossings as f32 * rate as f32 / samples.len() as f32
    }

    fn resample_all(rate: u32, input: &[f32]) -> Vec<f32> {
        MonoResampler::new(rate).unwrap().push(input).unwrap()
    }

    #[test]
    fn downmix_averages_channels() {
        let stereo = [1.0, 3.0, -1.0, 1.0, 0.0, 0.0];
        assert_eq!(downmix_to_mono(&stereo, 2), vec![2.0, 0.0, 0.0]);
        assert_eq!(downmix_to_mono(&[0.5, -0.5], 1), vec![0.5, -0.5]);
        assert!(downmix_to_mono(&[1.0, 2.0], 0).is_empty());
    }

    #[test]
    fn downmix_drops_a_trailing_partial_frame() {
        assert_eq!(downmix_to_mono(&[1.0, 1.0, 5.0], 2), vec![1.0]);
    }

    #[test]
    fn target_rate_passes_through_unchanged() {
        let input = sine(SAMPLE_RATE, 300.0, 0.1, 0.5);
        assert_eq!(resample_all(SAMPLE_RATE, &input), input);
    }

    #[test]
    fn zero_rate_or_channels_are_rejected() {
        assert!(MonoResampler::new(0).is_err());
        assert!(FrameNormalizer::new(48_000, 0).is_err());
        assert!(FrameNormalizer::new(0, 2).is_err());
    }

    fn check_tone_survives(rate: u32) {
        let input = sine(rate, 440.0, 2.0, 0.5);
        let output = resample_all(rate, &input);

        // About two seconds at 16 kHz, minus what is still buffered inside the resampler.
        let expected = 2 * SAMPLE_RATE as usize;
        assert!(
            output.len() <= expected && output.len() + 2 * CHUNK_FRAMES > expected,
            "{rate} Hz: got {} samples, expected about {expected}",
            output.len()
        );
        // Judge the steady state, not the start-up.
        let steady = &output[2000..];
        let hz = frequency(steady, SAMPLE_RATE);
        assert!(
            (hz - 440.0).abs() < 8.0,
            "{rate} Hz: tone came out at {hz} Hz"
        );
        let level = rms(steady);
        assert!(
            (level - 0.5 / 2f32.sqrt()).abs() < 0.02,
            "{rate} Hz: rms {level}"
        );
    }

    #[test]
    fn a_tone_survives_downsampling_from_48k() {
        check_tone_survives(48_000);
    }

    #[test]
    fn a_tone_survives_downsampling_from_44_1k() {
        check_tone_survives(44_100);
    }

    #[test]
    fn a_tone_above_the_new_nyquist_is_filtered_out() {
        // 10 kHz cannot exist at 16 kHz (limit 8 kHz). It must vanish, not fold down as an alias.
        let input = sine(48_000, 10_000.0, 1.0, 0.5);
        let output = resample_all(48_000, &input);
        let level = rms(&output[2000..]);
        assert!(level < 0.02, "aliased energy rms {level}");
    }

    #[test]
    fn chunking_does_not_change_the_result() {
        let input = sine(48_000, 330.0, 1.0, 0.4);
        let whole = resample_all(48_000, &input);

        let mut resampler = MonoResampler::new(48_000).unwrap();
        let mut pieces = Vec::new();
        for chunk in input.chunks(333) {
            pieces.extend(resampler.push(chunk).unwrap());
        }
        assert_eq!(pieces.len(), whole.len());
        let worst = pieces
            .iter()
            .zip(&whole)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        assert!(worst < 1e-5, "largest difference {worst}");
    }

    #[test]
    fn normalizer_turns_stereo_48k_into_mono_16k() {
        let left = sine(48_000, 440.0, 1.0, 0.5);
        // Opposite phase on the right would cancel; same signal keeps the level.
        let interleaved: Vec<f32> = left.iter().flat_map(|&s| [s, s]).collect();
        let mut normalizer = FrameNormalizer::new(48_000, 2).unwrap();
        let output = normalizer.push(&interleaved).unwrap();
        assert!(output.len() > 14_000 && output.len() <= 16_000);
        assert!((rms(&output[2000..]) - 0.5 / 2f32.sqrt()).abs() < 0.02);
    }

    #[test]
    fn normalizer_cancels_out_of_phase_channels() {
        let left = sine(48_000, 440.0, 1.0, 0.5);
        let interleaved: Vec<f32> = left.iter().flat_map(|&s| [s, -s]).collect();
        let mut normalizer = FrameNormalizer::new(48_000, 2).unwrap();
        let output = normalizer.push(&interleaved).unwrap();
        assert!(rms(&output[2000..]) < 1e-4);
    }
}
