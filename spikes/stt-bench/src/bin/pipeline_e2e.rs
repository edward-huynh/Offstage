//! Throwaway end-to-end check: a WAV played in real time through the real pipeline and whisper.
//! Usage: pipeline_e2e <model> <file.wav> <me|remote>
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use offstage_audio::{AudioError, AudioFrame, AudioSource};
use offstage_core::{AppEvent, Speaker};
use offstage_pipeline::{EventSink, Pipeline, PipelineConfig};
use offstage_stt::{SpeechToText, WhisperConfig, WhisperStt};

fn read_wav(path: &str) -> Vec<f32> {
    let bytes = std::fs::read(path).expect("read wav");
    let mut pos = 12;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
        if id == b"data" {
            let end = (pos + 8 + size).min(bytes.len());
            return bytes[pos + 8..end]
                .chunks_exact(2)
                .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
                .collect();
        }
        pos += 8 + size + (size % 2);
    }
    panic!("no data chunk");
}

struct PlaybackSource {
    speaker: Speaker,
    samples: Vec<f32>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl AudioSource for PlaybackSource {
    fn speaker(&self) -> Speaker {
        self.speaker
    }
    fn start(&mut self, mut sink: Box<dyn FnMut(AudioFrame) + Send>) -> Result<(), AudioError> {
        let samples = self.samples.clone();
        let stop = Arc::clone(&self.stop);
        let speaker = self.speaker;
        self.thread = Some(thread::spawn(move || {
            let begun = Instant::now();
            // Speech, then 1.5 s of silence so the segmenter can close the utterance.
            let mut all = samples;
            all.extend(std::iter::repeat_n(0.0, 24_000));
            for (i, chunk) in all.chunks(320).enumerate() {
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                sink(AudioFrame { speaker, samples: chunk.to_vec() });
                // Real time pace: 320 samples = 20 ms.
                let due = Duration::from_millis(20 * (i as u64 + 1));
                if let Some(wait) = due.checked_sub(begun.elapsed()) {
                    thread::sleep(wait);
                }
            }
        }));
        Ok(())
    }
    fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (model, wav) = (&args[1], &args[2]);
    let speaker = if args.get(3).map(String::as_str) == Some("me") { Speaker::Me } else { Speaker::Remote };

    let t = Instant::now();
    let engine: Arc<dyn SpeechToText> = Arc::new(WhisperStt::load(WhisperConfig::new(model)).expect("load model"));
    println!("model loaded in {:.2?}", t.elapsed());

    let samples = read_wav(wav);
    println!("audio: {:.1} s", samples.len() as f32 / 16_000.0);

    let events: Arc<Mutex<Vec<(Duration, AppEvent)>>> = Arc::default();
    let started = Instant::now();
    let sink: EventSink = {
        let events = Arc::clone(&events);
        Arc::new(move |e| events.lock().unwrap().push((started.elapsed(), e)))
    };
    let mut pipeline = Pipeline::new(engine, sink, PipelineConfig::default());
    pipeline
        .start(Box::new(PlaybackSource { speaker, samples, stop: Arc::default(), thread: None }))
        .expect("start");

    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline && events.lock().unwrap().is_empty() {
        thread::sleep(Duration::from_millis(50));
    }
    let status = pipeline.status();
    pipeline.stop(speaker);
    for (at, e) in events.lock().unwrap().iter() {
        println!("[{:>6.2?} after start] {e:?}", at);
    }
    println!("status: {status:?}");
}
