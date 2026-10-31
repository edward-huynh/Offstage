//! Connects audio sources to speech-to-text and reports transcripts as [`AppEvent`]s.
//!
//! ```text
//! AudioSource -> frames -> Segmenter (VAD) -> utterances -> SpeechToText -> AppEvent::Transcript
//! ```
//!
//! Every source gets its own lane of two threads: one cuts the audio into utterances, the
//! other transcribes them one at a time. Splitting them keeps audio flowing into the
//! segmenter while a slow transcription runs. The speaker label comes from the source
//! (mic = `Me`, loopback = `Remote`), so no speaker separation is needed.
//!
//! The stages block (capture callbacks, `transcribe`), so they run on plain threads
//! connected by bounded channels. Nothing here depends on Tauri: events leave through an
//! [`EventSink`] closure.
//!
//! Audio lives in memory only and is dropped as soon as it has been transcribed.

use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Instant,
};

use offstage_audio::{
    segment::{Segmenter, SegmenterConfig, Utterance},
    vad::{EnergyVad, Vad},
    AudioError, AudioFrame, AudioSource,
};
use offstage_core::{AppEvent, Speaker};
use offstage_stt::SpeechToText;
use serde::Serialize;

/// Receives every event. Called from pipeline threads, so it must be quick and thread safe.
pub type EventSink = Arc<dyn Fn(AppEvent) + Send + Sync>;

/// Builds a fresh voice detector for each lane.
pub type VadFactory = Arc<dyn Fn() -> Box<dyn Vad> + Send + Sync>;

/// Audio buffers waiting for the segmenter. It is far faster than real time, so this only
/// absorbs scheduling hiccups; a full queue drops the newest buffer.
const FRAME_QUEUE: usize = 512;

#[derive(Debug, Clone)]
pub struct PipelineConfig {
    pub segmenter: SegmenterConfig,
    /// Utterances waiting for transcription. When transcription falls this far behind,
    /// further utterances are dropped and an error event says so.
    pub max_backlog: usize,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            segmenter: SegmenterConfig::default(),
            max_backlog: 16,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error("{0:?} is already being listened to")]
    AlreadyRunning(Speaker),
    #[error(transparent)]
    Audio(#[from] AudioError),
    #[error("could not start a pipeline thread: {0}")]
    Thread(#[from] std::io::Error),
}

/// What a running lane has done so far.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LaneStatus {
    pub speaker: Speaker,
    /// Utterances that went through transcription, whether or not they held words.
    pub utterances: u64,
    /// Utterances thrown away because transcription was too far behind.
    pub dropped: u64,
    /// From the moment an utterance was complete to the moment its transcript was ready.
    /// This leaves out the closing silence the segmenter waits for (400 ms by default).
    pub last_latency_ms: Option<u64>,
}

/// Marks "no latency measured yet" inside an atomic.
const NO_LATENCY: u64 = u64::MAX;

struct Stats {
    utterances: AtomicU64,
    dropped: AtomicU64,
    last_latency_ms: AtomicU64,
}

impl Stats {
    fn new() -> Self {
        Self {
            utterances: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
            last_latency_ms: AtomicU64::new(NO_LATENCY),
        }
    }
}

struct Lane {
    speaker: Speaker,
    source: Box<dyn AudioSource>,
    threads: Vec<JoinHandle<()>>,
    stats: Arc<Stats>,
}

/// An utterance and the moment it was completed, to measure how long transcription took.
struct Pending {
    utterance: Utterance,
    completed_at: Instant,
}

pub struct Pipeline {
    stt: Arc<dyn SpeechToText>,
    sink: EventSink,
    vad_factory: VadFactory,
    config: PipelineConfig,
    lanes: Vec<Lane>,
}

impl Pipeline {
    /// Uses the energy based voice detector. See [`Pipeline::with_vad`] to change it.
    pub fn new(stt: Arc<dyn SpeechToText>, sink: EventSink, config: PipelineConfig) -> Self {
        Self::with_vad(
            stt,
            sink,
            config,
            Arc::new(|| Box::new(EnergyVad::default())),
        )
    }

    pub fn with_vad(
        stt: Arc<dyn SpeechToText>,
        sink: EventSink,
        config: PipelineConfig,
        vad_factory: VadFactory,
    ) -> Self {
        Self {
            stt,
            sink,
            vad_factory,
            config,
            lanes: Vec::new(),
        }
    }

    /// Starts listening to `source`. Only one source per speaker can run at a time.
    pub fn start(&mut self, mut source: Box<dyn AudioSource>) -> Result<(), PipelineError> {
        let speaker = source.speaker();
        if self.lanes.iter().any(|l| l.speaker == speaker) {
            return Err(PipelineError::AlreadyRunning(speaker));
        }

        let stats = Arc::new(Stats::new());
        let (frame_tx, frame_rx) = mpsc::sync_channel::<AudioFrame>(FRAME_QUEUE);
        let (utterance_tx, utterance_rx) =
            mpsc::sync_channel::<Pending>(self.config.max_backlog.max(1));

        let segmenter = Segmenter::new((self.vad_factory)(), &self.config.segmenter);
        let segment_thread = {
            let sink = Arc::clone(&self.sink);
            let stats = Arc::clone(&stats);
            thread::Builder::new()
                .name("offstage-segmenter".into())
                .spawn(move || segment_loop(frame_rx, segmenter, utterance_tx, &stats, &sink))?
        };
        let stt_thread = {
            let stt = Arc::clone(&self.stt);
            let sink = Arc::clone(&self.sink);
            let stats = Arc::clone(&stats);
            thread::Builder::new()
                .name("offstage-stt".into())
                .spawn(move || stt_loop(utterance_rx, stt.as_ref(), speaker, &stats, &sink))?
        };
        let threads = vec![segment_thread, stt_thread];

        // The callback only queues the frame: it runs on the source's own thread.
        let started = source.start(Box::new(move |frame| {
            let _ = frame_tx.try_send(frame);
        }));
        if let Err(e) = started {
            // The callback was dropped, which closes the channels and ends both threads.
            for thread in threads {
                let _ = thread.join();
            }
            return Err(e.into());
        }

        self.lanes.push(Lane {
            speaker,
            source,
            threads,
            stats,
        });
        Ok(())
    }

    /// Stops listening to `speaker`. Speech still in progress is cut off and transcribed, and
    /// utterances already queued are finished first, so this can take a moment.
    /// Does nothing if that speaker is not running.
    pub fn stop(&mut self, speaker: Speaker) {
        if let Some(index) = self.lanes.iter().position(|l| l.speaker == speaker) {
            let mut lane = self.lanes.remove(index);
            // Stopping the source drops its callback, which lets the threads drain and end.
            lane.source.stop();
            for thread in lane.threads {
                let _ = thread.join();
            }
        }
    }

    pub fn stop_all(&mut self) {
        let running: Vec<Speaker> = self.lanes.iter().map(|l| l.speaker).collect();
        for speaker in running {
            self.stop(speaker);
        }
    }

    pub fn status(&self) -> Vec<LaneStatus> {
        self.lanes
            .iter()
            .map(|lane| {
                let latency = lane.stats.last_latency_ms.load(Ordering::Relaxed);
                LaneStatus {
                    speaker: lane.speaker,
                    utterances: lane.stats.utterances.load(Ordering::Relaxed),
                    dropped: lane.stats.dropped.load(Ordering::Relaxed),
                    last_latency_ms: (latency != NO_LATENCY).then_some(latency),
                }
            })
            .collect()
    }
}

impl Drop for Pipeline {
    fn drop(&mut self) {
        self.stop_all();
    }
}

/// Cuts incoming audio into utterances. Ends when the source stops, after handing on
/// whatever speech was still in progress.
fn segment_loop(
    frames: Receiver<AudioFrame>,
    mut segmenter: Segmenter<Box<dyn Vad>>,
    utterances: SyncSender<Pending>,
    stats: &Stats,
    sink: &EventSink,
) {
    let hand_on = |utterance: Utterance| {
        let pending = Pending {
            utterance,
            completed_at: Instant::now(),
        };
        if let Err(TrySendError::Full(_)) = utterances.try_send(pending) {
            stats.dropped.fetch_add(1, Ordering::Relaxed);
            sink(AppEvent::Error {
                message: "transcription is falling behind, an utterance was skipped".into(),
            });
        }
    };

    while let Ok(frame) = frames.recv() {
        segmenter.push(&frame.samples).into_iter().for_each(hand_on);
    }
    if let Some(last) = segmenter.flush() {
        hand_on(last);
    }
}

/// Transcribes utterances in order and reports each result.
fn stt_loop(
    utterances: Receiver<Pending>,
    stt: &dyn SpeechToText,
    speaker: Speaker,
    stats: &Stats,
    sink: &EventSink,
) {
    while let Ok(pending) = utterances.recv() {
        let result = stt.transcribe(&pending.utterance.samples);
        let latency = pending.completed_at.elapsed();
        stats.utterances.fetch_add(1, Ordering::Relaxed);
        stats.last_latency_ms.store(
            u64::try_from(latency.as_millis()).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
        match result {
            Ok(transcript) => {
                let text = transcript.text.trim();
                if !text.is_empty() {
                    sink(AppEvent::Transcript {
                        speaker,
                        text: text.to_owned(),
                        is_final: true,
                        language: transcript.language.clone(),
                    });
                }
            }
            Err(e) => sink(AppEvent::Error {
                message: format!("transcription failed: {e}"),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        f32::consts::TAU,
        sync::{
            atomic::{AtomicBool, AtomicUsize},
            Mutex,
        },
        time::Duration,
    };

    use offstage_stt::{SttError, Transcript};

    use super::*;

    const RATE: usize = 16_000;

    type Callback = Box<dyn FnMut(AudioFrame) + Send>;

    /// Lets a test push frames into a source after the pipeline has started it.
    #[derive(Clone, Default)]
    struct Feed(Arc<Mutex<Option<Callback>>>);

    impl Feed {
        fn push(&self, speaker: Speaker, samples: &[f32]) {
            // 20 ms frames, like a real source.
            for chunk in samples.chunks(RATE / 50) {
                if let Some(callback) = self.0.lock().unwrap().as_mut() {
                    callback(AudioFrame {
                        speaker,
                        samples: chunk.to_vec(),
                    });
                }
            }
        }
    }

    struct FakeSource {
        speaker: Speaker,
        feed: Feed,
        fail: bool,
    }

    impl FakeSource {
        fn new(speaker: Speaker) -> (Box<Self>, Feed) {
            let feed = Feed::default();
            let source = Box::new(Self {
                speaker,
                feed: feed.clone(),
                fail: false,
            });
            (source, feed)
        }
    }

    impl AudioSource for FakeSource {
        fn speaker(&self) -> Speaker {
            self.speaker
        }
        fn start(&mut self, sink: Callback) -> Result<(), AudioError> {
            if self.fail {
                return Err(AudioError::Permission("denied".into()));
            }
            *self.feed.0.lock().unwrap() = Some(sink);
            Ok(())
        }
        fn stop(&mut self) {
            *self.feed.0.lock().unwrap() = None;
        }
    }

    /// Answers with the number of samples it was given, so a test can tell utterances apart.
    #[derive(Default)]
    struct FakeStt {
        /// Language the fake reports for every utterance.
        language: Option<&'static str>,
        delay: Duration,
        fail_first: AtomicBool,
        empty: AtomicBool,
        calls: AtomicUsize,
    }

    impl SpeechToText for FakeStt {
        fn transcribe(&self, samples: &[f32]) -> Result<Transcript, SttError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            thread::sleep(self.delay);
            if self.fail_first.swap(false, Ordering::SeqCst) {
                return Err(SttError::Failed("boom".into()));
            }
            let text = if self.empty.load(Ordering::SeqCst) {
                "   ".to_owned()
            } else {
                format!("  {} samples ", samples.len())
            };
            Ok(Transcript {
                text,
                language: self.language.map(str::to_owned),
            })
        }
    }

    fn collecting_sink() -> (EventSink, Arc<Mutex<Vec<AppEvent>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let writer = Arc::clone(&events);
        let sink: EventSink = Arc::new(move |e| writer.lock().unwrap().push(e));
        (sink, events)
    }

    fn tone(seconds: f32) -> Vec<f32> {
        (0..(RATE as f32 * seconds) as usize)
            .map(|i| 0.3 * (TAU * 300.0 * i as f32 / RATE as f32).sin())
            .collect()
    }

    fn silence(seconds: f32) -> Vec<f32> {
        vec![0.0; (RATE as f32 * seconds) as usize]
    }

    /// Speech with enough silence on both sides to be one complete utterance.
    fn burst(speech_seconds: f32) -> Vec<f32> {
        [silence(0.3), tone(speech_seconds), silence(0.6)].concat()
    }

    fn transcripts(events: &Arc<Mutex<Vec<AppEvent>>>) -> Vec<(Speaker, String)> {
        events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                AppEvent::Transcript {
                    speaker,
                    text,
                    is_final: true,
                    ..
                } => Some((*speaker, text.clone())),
                _ => None,
            })
            .collect()
    }

    fn errors(events: &Arc<Mutex<Vec<AppEvent>>>) -> Vec<String> {
        events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                AppEvent::Error { message } => Some(message.clone()),
                _ => None,
            })
            .collect()
    }

    fn wait_until(what: &str, mut condition: impl FnMut() -> bool) {
        for _ in 0..500 {
            if condition() {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("timed out waiting for {what}");
    }

    fn pipeline_with(stt: Arc<FakeStt>) -> (Pipeline, Arc<Mutex<Vec<AppEvent>>>) {
        let (sink, events) = collecting_sink();
        (Pipeline::new(stt, sink, PipelineConfig::default()), events)
    }

    #[test]
    fn a_burst_of_speech_becomes_one_labelled_transcript() {
        let (mut pipeline, events) = pipeline_with(Arc::new(FakeStt::default()));
        let (source, feed) = FakeSource::new(Speaker::Me);
        pipeline.start(source).unwrap();

        feed.push(Speaker::Me, &burst(1.0));
        wait_until("a transcript", || !transcripts(&events).is_empty());

        let got = transcripts(&events);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0, Speaker::Me);
        // The fake reports the sample count: about a second of speech plus its pre-roll and tail.
        let samples: usize = got[0].1.split(' ').next().unwrap().parse().unwrap();
        assert!(samples > RATE && samples < 2 * RATE, "{samples} samples");
        assert!(errors(&events).is_empty());
    }

    #[test]
    fn the_transcript_event_carries_the_language_the_engine_reported() {
        let stt = Arc::new(FakeStt {
            language: Some("vi"),
            ..FakeStt::default()
        });
        let (mut pipeline, events) = pipeline_with(stt);
        let (source, feed) = FakeSource::new(Speaker::Remote);
        pipeline.start(source).unwrap();
        feed.push(Speaker::Remote, &burst(1.0));
        wait_until("a transcript", || !transcripts(&events).is_empty());

        let languages: Vec<Option<String>> = events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                AppEvent::Transcript { language, .. } => Some(language.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(languages, [Some("vi".to_owned())]);
    }

    #[test]
    fn transcripts_are_trimmed() {
        let (mut pipeline, events) = pipeline_with(Arc::new(FakeStt::default()));
        let (source, feed) = FakeSource::new(Speaker::Me);
        pipeline.start(source).unwrap();
        feed.push(Speaker::Me, &burst(1.0));
        wait_until("a transcript", || !transcripts(&events).is_empty());
        let text = &transcripts(&events)[0].1;
        assert_eq!(text, text.trim());
    }

    #[test]
    fn silence_makes_no_transcript_and_never_calls_the_engine() {
        let stt = Arc::new(FakeStt::default());
        let (mut pipeline, events) = pipeline_with(Arc::clone(&stt));
        let (source, feed) = FakeSource::new(Speaker::Remote);
        pipeline.start(source).unwrap();

        feed.push(Speaker::Remote, &silence(3.0));
        pipeline.stop(Speaker::Remote);

        assert!(transcripts(&events).is_empty());
        assert_eq!(stt.calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn each_source_keeps_its_own_speaker_label() {
        let (mut pipeline, events) = pipeline_with(Arc::new(FakeStt::default()));
        let (me, me_feed) = FakeSource::new(Speaker::Me);
        let (remote, remote_feed) = FakeSource::new(Speaker::Remote);
        pipeline.start(me).unwrap();
        pipeline.start(remote).unwrap();

        // Different lengths, so each transcript can be matched to its source.
        me_feed.push(Speaker::Me, &burst(0.6));
        remote_feed.push(Speaker::Remote, &burst(1.6));
        wait_until("both transcripts", || transcripts(&events).len() == 2);

        let count = |speaker: Speaker| -> usize {
            let text = transcripts(&events)
                .into_iter()
                .find(|(s, _)| *s == speaker)
                .unwrap()
                .1;
            text.split(' ').next().unwrap().parse().unwrap()
        };
        assert!(count(Speaker::Remote) > count(Speaker::Me));
        assert_eq!(pipeline.status().len(), 2);
    }

    #[test]
    fn stopping_transcribes_the_speech_still_in_progress() {
        let (mut pipeline, events) = pipeline_with(Arc::new(FakeStt::default()));
        let (source, feed) = FakeSource::new(Speaker::Me);
        pipeline.start(source).unwrap();

        // No closing silence: the utterance is still open when the user stops listening.
        feed.push(Speaker::Me, &[silence(0.2), tone(1.0)].concat());
        assert!(transcripts(&events).is_empty());
        pipeline.stop(Speaker::Me);

        assert_eq!(transcripts(&events).len(), 1);
    }

    #[test]
    fn a_failed_transcription_is_reported_and_the_next_one_still_works() {
        let stt = Arc::new(FakeStt {
            fail_first: AtomicBool::new(true),
            ..FakeStt::default()
        });
        let (mut pipeline, events) = pipeline_with(stt);
        let (source, feed) = FakeSource::new(Speaker::Me);
        pipeline.start(source).unwrap();

        feed.push(Speaker::Me, &burst(1.0));
        feed.push(Speaker::Me, &burst(1.2));
        wait_until("an error and a transcript", || {
            !errors(&events).is_empty() && !transcripts(&events).is_empty()
        });

        assert!(errors(&events)[0].contains("transcription failed"));
        assert_eq!(transcripts(&events).len(), 1);
    }

    #[test]
    fn a_transcript_with_no_words_is_not_reported() {
        let stt = Arc::new(FakeStt::default());
        stt.empty.store(true, Ordering::SeqCst);
        let (mut pipeline, events) = pipeline_with(Arc::clone(&stt));
        let (source, feed) = FakeSource::new(Speaker::Me);
        pipeline.start(source).unwrap();

        feed.push(Speaker::Me, &burst(1.0));
        wait_until("the engine to run", || {
            stt.calls.load(Ordering::SeqCst) == 1
        });
        pipeline.stop(Speaker::Me);

        assert!(transcripts(&events).is_empty());
        // It still counts as processed.
        assert_eq!(stt.calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_speaker_cannot_be_started_twice() {
        let (mut pipeline, _events) = pipeline_with(Arc::new(FakeStt::default()));
        let (first, _) = FakeSource::new(Speaker::Me);
        let (second, _) = FakeSource::new(Speaker::Me);
        pipeline.start(first).unwrap();
        assert!(matches!(
            pipeline.start(second),
            Err(PipelineError::AlreadyRunning(Speaker::Me))
        ));
    }

    #[test]
    fn a_source_that_fails_to_start_leaves_nothing_running() {
        let (mut pipeline, _events) = pipeline_with(Arc::new(FakeStt::default()));
        let (mut source, _) = FakeSource::new(Speaker::Remote);
        source.fail = true;

        let err = pipeline.start(source).unwrap_err();
        assert!(matches!(
            err,
            PipelineError::Audio(AudioError::Permission(_))
        ));
        assert!(pipeline.status().is_empty());

        // The speaker is free again, so a later attempt can succeed.
        let (retry, _) = FakeSource::new(Speaker::Remote);
        pipeline.start(retry).unwrap();
    }

    #[test]
    fn stopping_a_speaker_that_is_not_running_is_harmless() {
        let (mut pipeline, _events) = pipeline_with(Arc::new(FakeStt::default()));
        pipeline.stop(Speaker::Me);
        pipeline.stop_all();
    }

    #[test]
    fn a_stopped_speaker_can_be_started_again() {
        let (mut pipeline, events) = pipeline_with(Arc::new(FakeStt::default()));
        let (first, _) = FakeSource::new(Speaker::Me);
        pipeline.start(first).unwrap();
        pipeline.stop(Speaker::Me);

        let (second, feed) = FakeSource::new(Speaker::Me);
        pipeline.start(second).unwrap();
        feed.push(Speaker::Me, &burst(1.0));
        wait_until("a transcript", || !transcripts(&events).is_empty());
    }

    #[test]
    fn status_reports_progress_and_latency() {
        let stt = Arc::new(FakeStt {
            delay: Duration::from_millis(40),
            ..FakeStt::default()
        });
        let (mut pipeline, events) = pipeline_with(stt);
        let (source, feed) = FakeSource::new(Speaker::Me);
        pipeline.start(source).unwrap();

        assert_eq!(pipeline.status()[0].utterances, 0);
        assert_eq!(pipeline.status()[0].last_latency_ms, None);

        feed.push(Speaker::Me, &burst(1.0));
        wait_until("a transcript", || !transcripts(&events).is_empty());

        let status = &pipeline.status()[0];
        assert_eq!(status.speaker, Speaker::Me);
        assert_eq!(status.utterances, 1);
        assert_eq!(status.dropped, 0);
        // At least the engine's own time, and not wildly more.
        let latency = status.last_latency_ms.unwrap();
        assert!((40..2000).contains(&latency), "latency {latency} ms");
    }

    #[test]
    fn falling_behind_drops_utterances_loudly_instead_of_blocking() {
        let stt = Arc::new(FakeStt {
            delay: Duration::from_millis(300),
            ..FakeStt::default()
        });
        let (sink, events) = collecting_sink();
        let config = PipelineConfig {
            max_backlog: 1,
            ..PipelineConfig::default()
        };
        let mut pipeline = Pipeline::new(stt, sink, config);
        let (source, feed) = FakeSource::new(Speaker::Me);
        pipeline.start(source).unwrap();

        // Five utterances arrive far faster than the slow engine can take them.
        for _ in 0..5 {
            feed.push(Speaker::Me, &burst(0.6));
        }
        pipeline.stop(Speaker::Me);

        let dropped = errors(&events)
            .iter()
            .filter(|m| m.contains("falling behind"))
            .count();
        assert!(dropped >= 1, "nothing was dropped");
        // Every utterance was either transcribed or reported as skipped.
        assert_eq!(transcripts(&events).len() + dropped, 5);
    }
}
