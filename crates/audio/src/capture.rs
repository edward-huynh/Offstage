//! Microphone and system-audio capture on top of `cpal`.
//!
//! `cpal` records "what the speakers play" when an input stream is built on an output
//! device: it uses a Core Audio tap on macOS (14.6+) and WASAPI loopback on Windows. So
//! one source type covers both the microphone (`Me`) and the other side of the call (`Remote`).
//!
//! The stream lives on its own thread. Starting a stream can block while the OS shows a
//! permission prompt, and a blocked caller would freeze the app, so [`AudioSource::start`]
//! waits with a timeout and reports a missing permission instead.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    ErrorKind, FromSample, Sample, SampleFormat, SizedSample,
};
use offstage_core::Speaker;

use crate::{resample::FrameNormalizer, AudioError, AudioFrame, AudioSource};

/// How long the stream may take to open and start. A pending permission prompt exceeds it.
const START_TIMEOUT: Duration = Duration::from_secs(5);
/// How long to wait for the first audio once the stream runs. Devices deliver buffers
/// continuously, even silent ones, so none at all means the OS is withholding the audio.
const FIRST_DATA_TIMEOUT: Duration = Duration::from_secs(3);
/// Raw buffers waiting for the worker. If it falls this far behind, new buffers are dropped.
const QUEUE_DEPTH: usize = 64;
/// How often the worker checks whether it was asked to stop.
const WORKER_POLL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Microphone,
    SystemLoopback,
}

pub struct CpalSource {
    speaker: Speaker,
    kind: Kind,
    running: Option<Running>,
}

struct Running {
    /// Dropping this tells the stream thread to close the stream.
    stop_stream: mpsc::Sender<()>,
    stop_worker: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl CpalSource {
    /// The default microphone, labelled [`Speaker::Me`].
    pub fn microphone() -> Self {
        Self {
            speaker: Speaker::Me,
            kind: Kind::Microphone,
            running: None,
        }
    }

    /// Everything the default output device plays, labelled [`Speaker::Remote`].
    pub fn system_loopback() -> Self {
        Self {
            speaker: Speaker::Remote,
            kind: Kind::SystemLoopback,
            running: None,
        }
    }
}

impl Drop for CpalSource {
    fn drop(&mut self) {
        self.stop();
    }
}

impl AudioSource for CpalSource {
    fn speaker(&self) -> Speaker {
        self.speaker
    }

    /// `sink` runs on a worker thread, not the audio thread, but it should still return
    /// quickly: while it runs, new audio queues up (and is dropped once the queue is full).
    fn start(&mut self, mut sink: Box<dyn FnMut(AudioFrame) + Send>) -> Result<(), AudioError> {
        self.stop();

        let (data_tx, data_rx) = mpsc::sync_channel::<Vec<f32>>(QUEUE_DEPTH);
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(u32, u16), AudioError>>();
        let (stop_tx, stop_rx) = mpsc::channel::<()>();

        let kind = self.kind;
        thread::Builder::new()
            .name("offstage-audio-stream".into())
            .spawn(move || run_stream(kind, data_tx, ready_tx, stop_rx))
            .map_err(|e| AudioError::Device(e.to_string()))?;

        // From here on, returning early drops `stop_tx`, which closes the stream.
        let (sample_rate, channels) = match ready_rx.recv_timeout(START_TIMEOUT) {
            Ok(opened) => opened?,
            Err(_) => return Err(permission_pending(kind)),
        };
        let first = data_rx
            .recv_timeout(FIRST_DATA_TIMEOUT)
            .map_err(|_| permission_pending(kind))?;
        let mut normalizer = FrameNormalizer::new(sample_rate, channels)?;

        let stop_worker = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop_worker);
        let speaker = self.speaker;
        let worker = thread::Builder::new()
            .name("offstage-audio-worker".into())
            .spawn(move || {
                forward(first, &mut normalizer, speaker, &mut sink);
                worker_loop(&data_rx, &worker_stop, &mut normalizer, speaker, &mut sink);
            })
            .map_err(|e| AudioError::Device(e.to_string()))?;

        self.running = Some(Running {
            stop_stream: stop_tx,
            stop_worker,
            worker: Some(worker),
        });
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(mut running) = self.running.take() {
            running.stop_worker.store(true, Ordering::Relaxed);
            drop(running.stop_stream);
            if let Some(worker) = running.worker.take() {
                let _ = worker.join();
            }
        }
    }
}

fn permission_pending(kind: Kind) -> AudioError {
    let what = match kind {
        Kind::Microphone => "microphone",
        Kind::SystemLoopback => "system audio",
    };
    AudioError::Permission(format!(
        "no audio from the {what} yet; the system may be waiting for the user to allow access"
    ))
}

fn map_error(e: cpal::Error) -> AudioError {
    match e.kind() {
        ErrorKind::PermissionDenied => AudioError::Permission(e.to_string()),
        _ => AudioError::Device(e.to_string()),
    }
}

/// Resamples one raw buffer and hands the result to the sink.
fn forward(
    chunk: Vec<f32>,
    normalizer: &mut FrameNormalizer,
    speaker: Speaker,
    sink: &mut dyn FnMut(AudioFrame),
) {
    match normalizer.push(&chunk) {
        Ok(samples) if !samples.is_empty() => sink(AudioFrame { speaker, samples }),
        Ok(_) => {}
        Err(e) => eprintln!("audio buffer skipped: {e}"),
    }
}

/// Processes buffers until asked to stop or the stream side goes away.
fn worker_loop(
    data_rx: &Receiver<Vec<f32>>,
    stop: &AtomicBool,
    normalizer: &mut FrameNormalizer,
    speaker: Speaker,
    sink: &mut dyn FnMut(AudioFrame),
) {
    loop {
        match data_rx.recv_timeout(WORKER_POLL) {
            Ok(chunk) => forward(chunk, normalizer, speaker, sink),
            Err(RecvTimeoutError::Timeout) => {
                if stop.load(Ordering::Relaxed) {
                    return;
                }
            }
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// Body of the stream thread: open, start, report, then keep the stream alive until told to stop.
fn run_stream(
    kind: Kind,
    data_tx: SyncSender<Vec<f32>>,
    ready_tx: mpsc::Sender<Result<(u32, u16), AudioError>>,
    stop_rx: Receiver<()>,
) {
    let (stream, sample_rate, channels) = match open_stream(kind, data_tx) {
        Ok(opened) => opened,
        Err(e) => {
            let _ = ready_tx.send(Err(e));
            return;
        }
    };
    // This call can block while the OS asks for permission.
    if let Err(e) = stream.play() {
        let _ = ready_tx.send(Err(map_error(e)));
        return;
    }
    let _ = ready_tx.send(Ok((sample_rate, channels)));
    // Returns when the source is stopped or dropped. Then the stream is closed.
    let _ = stop_rx.recv();
    drop(stream);
}

fn open_stream(
    kind: Kind,
    data_tx: SyncSender<Vec<f32>>,
) -> Result<(cpal::Stream, u32, u16), AudioError> {
    let host = cpal::default_host();
    let (device, what) = match kind {
        Kind::Microphone => (host.default_input_device(), "microphone"),
        Kind::SystemLoopback => (host.default_output_device(), "output device"),
    };
    let device = device.ok_or_else(|| AudioError::Device(format!("no default {what} found")))?;

    // cpal turns an input stream on an input-capable device into a plain recording. For
    // loopback that would silently capture a headset microphone instead of the call audio.
    if kind == Kind::SystemLoopback && device.supports_input() {
        return Err(AudioError::Device(
            "the default output device also records (a headset or audio interface), so its \
             output cannot be captured; switch the output to speakers or another device"
                .into(),
        ));
    }

    let supported = if device.supports_input() {
        device.default_input_config()
    } else {
        device.default_output_config()
    }
    .map_err(map_error)?;
    let sample_rate = supported.sample_rate();
    let channels = supported.channels();
    let format = supported.sample_format();
    let config = supported.config();

    let stream = match format {
        SampleFormat::F32 => build::<f32>(&device, config, data_tx),
        SampleFormat::I16 => build::<i16>(&device, config, data_tx),
        SampleFormat::I32 => build::<i32>(&device, config, data_tx),
        SampleFormat::U16 => build::<u16>(&device, config, data_tx),
        other => {
            return Err(AudioError::Device(format!(
                "unsupported sample format {other}"
            )))
        }
    }
    .map_err(map_error)?;
    Ok((stream, sample_rate, channels))
}

/// Builds the input stream. The callback only converts to f32 and queues the buffer, so the
/// audio thread never waits on resampling or on the sink.
fn build<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    data_tx: SyncSender<Vec<f32>>,
) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            let samples: Vec<f32> = data.iter().map(|&s| f32::from_sample(s)).collect();
            // A full queue means the worker is behind: dropping audio beats blocking the device.
            let _ = data_tx.try_send(samples);
        },
        |e: cpal::Error| eprintln!("audio stream error: {e}"),
        None,
    )
}

#[cfg(test)]
mod tests {
    use std::{
        f32::consts::TAU,
        sync::{Arc, Mutex},
    };

    use super::*;
    use crate::SAMPLE_RATE;

    fn assert_send<T: Send>() {}

    #[test]
    fn sources_can_move_between_threads() {
        assert_send::<CpalSource>();
    }

    #[test]
    fn sources_report_which_side_of_the_call_they_hear() {
        assert_eq!(CpalSource::microphone().speaker(), Speaker::Me);
        assert_eq!(CpalSource::system_loopback().speaker(), Speaker::Remote);
    }

    #[test]
    fn stopping_a_source_that_never_started_is_harmless() {
        let mut source = CpalSource::microphone();
        source.stop();
        source.stop();
    }

    /// Stereo 48 kHz sine, as one long interleaved buffer.
    fn stereo_sine(seconds: f32) -> Vec<f32> {
        let n = (48_000.0 * seconds) as usize;
        (0..n)
            .flat_map(|i| {
                let s = 0.5 * (TAU * 440.0 * i as f32 / 48_000.0).sin();
                [s, s]
            })
            .collect()
    }

    type Collected = Arc<Mutex<Vec<AudioFrame>>>;
    type Sink = Box<dyn FnMut(AudioFrame) + Send>;

    fn collecting_sink() -> (Collected, Sink) {
        let frames = Arc::new(Mutex::new(Vec::new()));
        let writer = Arc::clone(&frames);
        (frames, Box::new(move |f| writer.lock().unwrap().push(f)))
    }

    #[test]
    fn worker_delivers_labelled_16k_mono_frames_until_the_stream_ends() {
        let (tx, rx) = mpsc::sync_channel::<Vec<f32>>(QUEUE_DEPTH);
        let (frames, mut sink) = collecting_sink();
        let stop = Arc::new(AtomicBool::new(false));
        let mut normalizer = FrameNormalizer::new(48_000, 2).unwrap();

        let handle = thread::spawn({
            let stop = Arc::clone(&stop);
            move || worker_loop(&rx, &stop, &mut normalizer, Speaker::Remote, &mut sink)
        });
        let signal = stereo_sine(1.0);
        for chunk in signal.chunks(1024) {
            tx.send(chunk.to_vec()).unwrap();
        }
        drop(tx);
        handle.join().unwrap();

        let frames = frames.lock().unwrap();
        assert!(!frames.is_empty());
        assert!(frames.iter().all(|f| f.speaker == Speaker::Remote));
        let total: usize = frames.iter().map(|f| f.samples.len()).sum();
        // One second of input, minus what the resampler still holds.
        assert!(
            total > 14_000 && total <= SAMPLE_RATE as usize,
            "total {total}"
        );
    }

    #[test]
    fn worker_stops_when_asked_even_if_the_stream_stays_open() {
        let (tx, rx) = mpsc::sync_channel::<Vec<f32>>(QUEUE_DEPTH);
        let (_frames, mut sink) = collecting_sink();
        let stop = Arc::new(AtomicBool::new(false));
        let mut normalizer = FrameNormalizer::new(16_000, 1).unwrap();

        let handle = thread::spawn({
            let stop = Arc::clone(&stop);
            move || worker_loop(&rx, &stop, &mut normalizer, Speaker::Me, &mut sink)
        });
        stop.store(true, Ordering::Relaxed);
        // `tx` is still alive, so only the stop flag can end the loop.
        handle.join().unwrap();
        drop(tx);
    }

    /// Needs a microphone and, on macOS, the microphone permission, which may open a prompt.
    /// Run with `cargo test -p offstage-audio -- --ignored microphone`.
    #[test]
    #[ignore = "needs a microphone and microphone permission"]
    fn microphone_delivers_frames() {
        deliver_frames(CpalSource::microphone());
    }

    /// Needs audio playing on the default output and, on macOS 14.6+, the system audio
    /// permission, which may open a prompt. Run with `-- --ignored loopback`.
    #[test]
    #[ignore = "needs system audio permission and sound playing"]
    fn loopback_delivers_frames() {
        deliver_frames(CpalSource::system_loopback());
    }

    fn deliver_frames(mut source: CpalSource) {
        let (frames, sink) = collecting_sink();
        source.start(sink).expect("source should start");
        thread::sleep(Duration::from_secs(2));
        source.stop();
        let total: usize = frames.lock().unwrap().iter().map(|f| f.samples.len()).sum();
        // About two seconds of 16 kHz audio, with room for start-up.
        assert!(total > 16_000, "only {total} samples in two seconds");
    }
}
