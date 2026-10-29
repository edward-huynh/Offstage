use std::sync::Mutex;

use whisper_rs::{
    get_lang_str, FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters,
    WhisperState,
};

use crate::{join_segments, SpeechToText, SttError, Transcript, WhisperConfig};

/// whisper.cpp engine. The model is loaded once and kept in memory; the decoding state is
/// reused across calls, so utterances are transcribed one at a time (the pipeline calls
/// this from `spawn_blocking`).
pub struct WhisperStt {
    config: WhisperConfig,
    state: Mutex<WhisperState>,
}

impl WhisperStt {
    /// Loads the model from `config.model_path`. Slow (hundreds of ms to seconds), so call
    /// it once at startup, not per utterance.
    pub fn load(config: WhisperConfig) -> Result<Self, SttError> {
        config.validate()?;

        let mut params = WhisperContextParameters::default();
        params.use_gpu(config.use_gpu);
        let ctx = WhisperContext::new_with_params(&config.model_path, params)
            .map_err(|e| SttError::ModelNotLoaded(e.to_string()))?;
        let state = ctx
            .create_state()
            .map_err(|e| SttError::ModelNotLoaded(e.to_string()))?;

        Ok(Self {
            config,
            // The state holds its own reference to the model, so `ctx` can go out of scope.
            state: Mutex::new(state),
        })
    }

    pub fn config(&self) -> &WhisperConfig {
        &self.config
    }
}

impl SpeechToText for WhisperStt {
    fn transcribe(&self, samples: &[f32]) -> Result<Transcript, SttError> {
        if samples.is_empty() {
            return Err(SttError::EmptyAudio);
        }

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some(self.config.language.code().unwrap_or("auto")));
        params.set_n_threads(self.config.threads() as i32);
        params.set_translate(false);
        // Each utterance is independent; carrying text over invites hallucinated repeats.
        params.set_no_context(true);
        params.set_single_segment(false);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        let mut state = self
            .state
            .lock()
            .map_err(|_| SttError::Failed("decoder state poisoned".into()))?;
        state
            .full(params, samples)
            .map_err(|e| SttError::Failed(e.to_string()))?;

        let mut pieces = Vec::new();
        for segment in state.as_iter() {
            let text = segment
                .to_str_lossy()
                .map_err(|e| SttError::Failed(e.to_string()))?;
            pieces.push(text.into_owned());
        }
        let text = join_segments(pieces.iter().map(String::as_str));

        let language = self
            .config
            .language
            .code()
            .map(str::to_owned)
            .or_else(|| get_lang_str(state.full_lang_id_from_state()).map(str::to_owned));

        Ok(Transcript { text, language })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real-model smoke test. Run with:
    ///   OFFSTAGE_STT_MODEL=/path/ggml-small-q5_1.bin \
    ///   OFFSTAGE_STT_SAMPLE=/path/utterance.wav \
    ///   cargo test -p offstage-stt --features whisper -- --ignored --nocapture
    /// The sample must be 16 kHz mono 16-bit PCM WAV. Optional: OFFSTAGE_STT_LANG
    /// (auto|vi|en) and OFFSTAGE_STT_EXPECT (substring the text must contain).
    #[test]
    #[ignore = "needs a real model and audio sample, see doc comment"]
    fn transcribes_real_sample() {
        let model = std::env::var("OFFSTAGE_STT_MODEL").expect("OFFSTAGE_STT_MODEL");
        let sample = std::env::var("OFFSTAGE_STT_SAMPLE").expect("OFFSTAGE_STT_SAMPLE");
        let mut config = WhisperConfig::new(model);
        if let Ok(lang) = std::env::var("OFFSTAGE_STT_LANG") {
            config.language = lang.parse().expect("OFFSTAGE_STT_LANG");
        }

        let samples = read_wav_16k_mono(&sample);
        let load_start = std::time::Instant::now();
        let stt = WhisperStt::load(config).expect("load model");
        eprintln!("model load: {:?}", load_start.elapsed());

        for run in 0..2 {
            let start = std::time::Instant::now();
            let out = stt.transcribe(&samples).expect("transcribe");
            eprintln!(
                "run {run}: {:?} for {:.1}s of audio -> {:?} ({:?})",
                start.elapsed(),
                samples.len() as f32 / 16_000.0,
                out.text,
                out.language
            );
            assert!(!out.text.is_empty());
            if let Ok(expect) = std::env::var("OFFSTAGE_STT_EXPECT") {
                assert!(
                    out.text.to_lowercase().contains(&expect.to_lowercase()),
                    "expected {expect:?} in {:?}",
                    out.text
                );
            }
        }
    }

    #[test]
    #[ignore = "needs a real model, see transcribes_real_sample"]
    fn empty_samples_are_rejected() {
        let model = std::env::var("OFFSTAGE_STT_MODEL").expect("OFFSTAGE_STT_MODEL");
        let stt = WhisperStt::load(WhisperConfig::new(model)).expect("load model");
        assert!(matches!(stt.transcribe(&[]), Err(SttError::EmptyAudio)));
    }

    #[test]
    fn load_fails_cleanly_for_missing_model() {
        let err = WhisperStt::load(WhisperConfig::new("/no/such/model.bin"))
            .err()
            .expect("load should fail");
        assert!(matches!(err, SttError::ModelNotFound(_)));
    }

    /// Minimal canonical-WAV reader (44-byte header) to avoid a dev-dependency.
    fn read_wav_16k_mono(path: &str) -> Vec<f32> {
        let bytes = std::fs::read(path).expect("read sample");
        assert!(
            bytes.len() > 44 && &bytes[0..4] == b"RIFF",
            "not a WAV file"
        );
        let channels = u16::from_le_bytes([bytes[22], bytes[23]]);
        let rate = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]);
        let bits = u16::from_le_bytes([bytes[34], bytes[35]]);
        assert_eq!(
            (channels, rate, bits),
            (1, 16_000, 16),
            "need 16 kHz mono s16"
        );
        bytes[44..]
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
            .collect()
    }
}
