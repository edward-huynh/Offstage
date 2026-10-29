//! Cuts a continuous 16 kHz stream into utterances, ready to send to speech-to-text.
//!
//! An utterance starts when speech is detected (with a little audio from just before it,
//! so the first syllable is not clipped) and ends after a stretch of silence. A very long
//! utterance is cut early so the transcript never lags far behind.

use std::collections::VecDeque;

use crate::{vad::Vad, SAMPLE_RATE};

#[derive(Debug, Clone, PartialEq)]
pub struct SegmenterConfig {
    /// A window with at least this speech probability counts as speech.
    pub speech_threshold: f32,
    /// Silence this long ends an utterance. 400 ms keeps natural pauses inside a sentence.
    pub end_silence_ms: u32,
    /// Shorter bursts of speech are dropped as clicks and coughs.
    pub min_speech_ms: u32,
    /// An utterance is cut at this length even if the speaker keeps going.
    pub max_utterance_ms: u32,
    /// Audio kept from before speech was detected.
    pub pre_roll_ms: u32,
}

impl Default for SegmenterConfig {
    fn default() -> Self {
        Self {
            speech_threshold: 0.5,
            end_silence_ms: 400,
            min_speech_ms: 250,
            max_utterance_ms: 15_000,
            pre_roll_ms: 200,
        }
    }
}

/// One stretch of speech, 16 kHz mono, including the pre-roll and the closing silence.
#[derive(Debug, Clone, PartialEq)]
pub struct Utterance {
    pub samples: Vec<f32>,
}

#[derive(Debug)]
enum State {
    Idle,
    Speaking {
        /// Windows of the current utterance that were speech.
        speech_windows: usize,
        /// Consecutive non-speech windows at the end.
        silence_windows: usize,
    },
}

pub struct Segmenter<V: Vad> {
    vad: V,
    window: usize,
    speech_threshold: f32,
    end_silence_windows: usize,
    min_speech_windows: usize,
    max_utterance_windows: usize,
    pre_roll_windows: usize,

    /// Samples not yet enough for a whole window.
    pending: Vec<f32>,
    /// The most recent windows while idle.
    pre_roll: VecDeque<Vec<f32>>,
    current: Vec<f32>,
    state: State,
}

impl<V: Vad> Segmenter<V> {
    pub fn new(vad: V, config: &SegmenterConfig) -> Self {
        let window = vad.window_size().max(1);
        // Rounds up so a setting is never shorter than asked for.
        let windows_for = |ms: u32| {
            let samples = u64::from(ms) * u64::from(SAMPLE_RATE) / 1000;
            (samples as usize).div_ceil(window)
        };
        Self {
            vad,
            window,
            speech_threshold: config.speech_threshold,
            end_silence_windows: windows_for(config.end_silence_ms).max(1),
            min_speech_windows: windows_for(config.min_speech_ms),
            max_utterance_windows: windows_for(config.max_utterance_ms).max(1),
            pre_roll_windows: windows_for(config.pre_roll_ms),
            pending: Vec::new(),
            pre_roll: VecDeque::new(),
            current: Vec::new(),
            state: State::Idle,
        }
    }

    /// Feeds audio of any length and returns the utterances that finished.
    pub fn push(&mut self, samples: &[f32]) -> Vec<Utterance> {
        self.pending.extend_from_slice(samples);
        let mut finished = Vec::new();
        while self.pending.len() >= self.window {
            let window: Vec<f32> = self.pending.drain(..self.window).collect();
            if let Some(utterance) = self.process_window(window) {
                finished.push(utterance);
            }
        }
        finished
    }

    /// Call when the stream ends: returns the utterance still in progress, if it was long enough.
    pub fn flush(&mut self) -> Option<Utterance> {
        self.pending.clear();
        self.pre_roll.clear();
        let State::Speaking { speech_windows, .. } = self.state else {
            return None;
        };
        self.state = State::Idle;
        self.finish(speech_windows)
    }

    fn process_window(&mut self, window: Vec<f32>) -> Option<Utterance> {
        let is_speech = self.vad.probability(&window) >= self.speech_threshold;
        match self.state {
            State::Idle => {
                if is_speech {
                    // Start with what came just before, so the first syllable is whole.
                    self.current.clear();
                    for earlier in self.pre_roll.drain(..) {
                        self.current.extend(earlier);
                    }
                    self.current.extend(window);
                    self.state = State::Speaking {
                        speech_windows: 1,
                        silence_windows: 0,
                    };
                } else {
                    self.pre_roll.push_back(window);
                    while self.pre_roll.len() > self.pre_roll_windows {
                        self.pre_roll.pop_front();
                    }
                }
                None
            }
            State::Speaking {
                mut speech_windows,
                mut silence_windows,
            } => {
                self.current.extend(window);
                if is_speech {
                    speech_windows += 1;
                    silence_windows = 0;
                } else {
                    silence_windows += 1;
                }

                if silence_windows >= self.end_silence_windows {
                    self.state = State::Idle;
                    return self.finish(speech_windows);
                }
                if self.current.len() / self.window >= self.max_utterance_windows {
                    // Too long: send what there is and keep listening to the same speaker.
                    self.state = State::Speaking {
                        speech_windows: 0,
                        silence_windows,
                    };
                    return self.finish(speech_windows);
                }
                self.state = State::Speaking {
                    speech_windows,
                    silence_windows,
                };
                None
            }
        }
    }

    /// Takes the buffered audio as an utterance, or drops it if it was too short to be speech.
    fn finish(&mut self, speech_windows: usize) -> Option<Utterance> {
        let samples = std::mem::take(&mut self.current);
        (speech_windows >= self.min_speech_windows).then_some(Utterance { samples })
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::TAU;

    use super::*;
    use crate::vad::EnergyVad;

    const WINDOW: usize = 160; // 10 ms, so one window is easy to reason about

    /// Answers from a script, one entry per window, then reports silence.
    struct ScriptedVad {
        script: VecDeque<bool>,
    }

    impl Vad for ScriptedVad {
        fn window_size(&self) -> usize {
            WINDOW
        }
        fn probability(&mut self, _window: &[f32]) -> f32 {
            if self.script.pop_front().unwrap_or(false) {
                1.0
            } else {
                0.0
            }
        }
    }

    /// `"..SSSS.."`: `S` is a speech window, `.` is silence.
    fn script(pattern: &str) -> Vec<bool> {
        pattern.chars().map(|c| c == 'S').collect()
    }

    fn config() -> SegmenterConfig {
        SegmenterConfig {
            speech_threshold: 0.5,
            end_silence_ms: 400, // 40 windows
            min_speech_ms: 100,  // 10 windows
            max_utterance_ms: 1000,
            pre_roll_ms: 30, // 3 windows
        }
    }

    fn run(pattern: &str) -> Vec<Utterance> {
        let vad = ScriptedVad {
            script: script(pattern).into(),
        };
        let mut segmenter = Segmenter::new(vad, &config());
        let audio = vec![0.0f32; pattern.len() * WINDOW];
        segmenter.push(&audio)
    }

    fn windows(u: &Utterance) -> usize {
        u.samples.len() / WINDOW
    }

    fn repeat(c: char, n: usize) -> String {
        std::iter::repeat_n(c, n).collect()
    }

    #[test]
    fn silence_produces_nothing() {
        assert!(run(&repeat('.', 500)).is_empty());
    }

    #[test]
    fn one_utterance_includes_pre_roll_speech_and_closing_silence() {
        let pattern = format!("{}{}{}", repeat('.', 5), repeat('S', 10), repeat('.', 40));
        let out = run(&pattern);
        assert_eq!(out.len(), 1);
        // 3 windows of pre-roll (not all 5 silent ones) + 10 speech + 40 closing silence.
        assert_eq!(windows(&out[0]), 3 + 10 + 40);
    }

    #[test]
    fn pre_roll_cannot_exceed_the_audio_that_exists() {
        let pattern = format!("{}{}{}", repeat('.', 1), repeat('S', 10), repeat('.', 40));
        let out = run(&pattern);
        assert_eq!(windows(&out[0]), 1 + 10 + 40);
    }

    #[test]
    fn a_short_burst_is_dropped_as_noise() {
        let pattern = format!("{}{}{}", repeat('.', 5), repeat('S', 3), repeat('.', 60));
        assert!(run(&pattern).is_empty());
    }

    #[test]
    fn a_pause_shorter_than_the_end_silence_does_not_split_the_sentence() {
        let pattern = format!(
            "{}{}{}{}",
            repeat('S', 10),
            repeat('.', 20),
            repeat('S', 10),
            repeat('.', 40)
        );
        let out = run(&pattern);
        assert_eq!(out.len(), 1);
        assert_eq!(windows(&out[0]), 10 + 20 + 10 + 40);
    }

    #[test]
    fn a_long_pause_splits_into_two_utterances() {
        let pattern = format!(
            "{}{}{}{}",
            repeat('S', 10),
            repeat('.', 45),
            repeat('S', 12),
            repeat('.', 40)
        );
        let out = run(&pattern);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn endless_speech_is_cut_at_the_maximum_length() {
        // 1000 ms limit is 100 windows. 250 windows of speech, then silence to close.
        let pattern = format!("{}{}", repeat('S', 250), repeat('.', 40));
        let out = run(&pattern);
        assert!(out.len() >= 3, "got {} utterances", out.len());
        assert_eq!(windows(&out[0]), 100);
        assert_eq!(windows(&out[1]), 100);
    }

    #[test]
    fn flush_returns_the_utterance_in_progress() {
        let vad = ScriptedVad {
            script: script(&repeat('S', 20)).into(),
        };
        let mut segmenter = Segmenter::new(vad, &config());
        assert!(segmenter.push(&vec![0.0; 20 * WINDOW]).is_empty());
        let last = segmenter.flush().expect("speech was in progress");
        assert_eq!(windows(&last), 20);
        assert!(segmenter.flush().is_none());
    }

    #[test]
    fn flush_drops_an_utterance_that_was_too_short() {
        let vad = ScriptedVad {
            script: script(&repeat('S', 3)).into(),
        };
        let mut segmenter = Segmenter::new(vad, &config());
        segmenter.push(&vec![0.0; 3 * WINDOW]);
        assert!(segmenter.flush().is_none());
    }

    #[test]
    fn flush_after_silence_returns_nothing() {
        let vad = ScriptedVad {
            script: script(&repeat('.', 10)).into(),
        };
        let mut segmenter = Segmenter::new(vad, &config());
        segmenter.push(&vec![0.0; 10 * WINDOW]);
        assert!(segmenter.flush().is_none());
    }

    #[test]
    fn a_partial_window_waits_for_the_rest() {
        let vad = ScriptedVad {
            script: script(&repeat('S', 20)).into(),
        };
        let mut segmenter = Segmenter::new(vad, &config());
        // Half a window at a time: nothing is lost between calls.
        for _ in 0..40 {
            segmenter.push(&vec![0.0; WINDOW / 2]);
        }
        assert_eq!(windows(&segmenter.flush().unwrap()), 20);
    }

    #[test]
    fn settings_round_up_to_whole_windows() {
        // 15 ms on a 10 ms window is 2 windows, never 1.
        let cfg = SegmenterConfig {
            end_silence_ms: 15,
            ..config()
        };
        let vad = ScriptedVad {
            script: VecDeque::new(),
        };
        let segmenter = Segmenter::new(vad, &cfg);
        assert_eq!(segmenter.end_silence_windows, 2);
    }

    /// Tone bursts separated by silence, through the real energy detector and real window size.
    fn burst_signal() -> Vec<f32> {
        let tone = |seconds: f32| -> Vec<f32> {
            (0..(16_000.0 * seconds) as usize)
                .map(|i| 0.3 * (TAU * 300.0 * i as f32 / 16_000.0).sin())
                .collect()
        };
        let silence = |seconds: f32| vec![0.0f32; (16_000.0 * seconds) as usize];
        [
            silence(0.5),
            tone(1.0),
            silence(1.0),
            tone(0.8),
            silence(1.0),
        ]
        .concat()
    }

    #[test]
    fn real_detector_finds_each_burst() {
        let mut segmenter = Segmenter::new(EnergyVad::default(), &SegmenterConfig::default());
        let out = segmenter.push(&burst_signal());
        assert_eq!(out.len(), 2, "two bursts of tone");
        // Each utterance holds the tone plus the 400 ms closing silence and some pre-roll.
        let seconds = |u: &Utterance| u.samples.len() as f32 / 16_000.0;
        assert!(
            (1.4..1.8).contains(&seconds(&out[0])),
            "{}",
            seconds(&out[0])
        );
        assert!(
            (1.2..1.6).contains(&seconds(&out[1])),
            "{}",
            seconds(&out[1])
        );
    }

    #[test]
    fn chunk_size_does_not_change_the_utterances() {
        let signal = burst_signal();
        let whole = Segmenter::new(EnergyVad::default(), &SegmenterConfig::default()).push(&signal);

        let mut segmenter = Segmenter::new(EnergyVad::default(), &SegmenterConfig::default());
        let mut pieces = Vec::new();
        for chunk in signal.chunks(777) {
            pieces.extend(segmenter.push(chunk));
        }
        assert_eq!(pieces, whole);
    }
}
