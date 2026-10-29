//! Commands for listening: start and stop each audio source, download the speech model,
//! and report progress. The work happens in `offstage-pipeline`; this only connects it to Tauri.
//!
//! The speech engine (about 190 MB of model) is loaded when listening starts and released
//! when the last source stops, so an idle app stays small.

use std::sync::{Arc, Mutex, TryLockError};

use offstage_audio::{capture::CpalSource, AudioSource};
use offstage_core::{AppEvent, Speaker};
use offstage_models::{CancellationToken, Downloader, ModelSpec, Progress};
use offstage_pipeline::{EventSink, LaneStatus, Pipeline, PipelineConfig};
use offstage_stt::{SpeechToText, WhisperConfig, WhisperStt};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

/// Error text meaning the model file is not on disk yet. The UI answers by offering a download.
pub const MODEL_MISSING: &str = "model_missing";

#[derive(Default)]
pub struct Listening {
    inner: Mutex<Inner>,
    download: Mutex<Option<CancellationToken>>,
}

#[derive(Default)]
struct Inner {
    engine: Option<Arc<dyn SpeechToText>>,
    pipeline: Option<Pipeline>,
}

#[derive(Serialize)]
pub struct ModelInfo {
    pub spec: ModelSpec,
    pub installed: bool,
}

#[derive(Serialize)]
pub struct ListeningStatus {
    pub lanes: Vec<LaneStatus>,
    /// True while a start or stop is in progress (loading the model can take several seconds).
    pub busy: bool,
}

fn emit(app: &AppHandle, event: &AppEvent) {
    // A failed emit means the window is gone, so there is nobody to tell.
    let _ = app.emit(event.name(), event);
}

fn model_spec() -> ModelSpec {
    offstage_models::whisper_small_q5()
}

fn models_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("models"))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn model_info(app: AppHandle) -> Result<ModelInfo, String> {
    let spec = model_spec();
    let path = spec.path_in(&models_dir(&app)?);
    // The size check is cheap. The full checksum is verified when the model is downloaded.
    let installed = std::fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() == spec.size);
    Ok(ModelInfo { spec, installed })
}

/// Downloads the speech model, or finishes a partial download. Progress arrives as
/// `model_progress` events. Resolves when the file is complete and its checksum matches.
#[tauri::command]
pub async fn download_model(app: AppHandle, state: State<'_, Listening>) -> Result<(), String> {
    let spec = model_spec();
    let dir = models_dir(&app)?;
    let cancel = CancellationToken::new();
    {
        let mut slot = state
            .download
            .lock()
            .map_err(|_| "download state is unavailable".to_owned())?;
        if slot.is_some() {
            return Err("a download is already running".to_owned());
        }
        *slot = Some(cancel.clone());
    }

    let progress = {
        let app = app.clone();
        let id = spec.id.clone();
        move |p: Progress| {
            emit(
                &app,
                &AppEvent::ModelProgress {
                    id: id.clone(),
                    downloaded: p.downloaded,
                    total: p.total,
                },
            );
        }
    };
    let result = match Downloader::new() {
        Ok(downloader) => downloader.ensure(&spec, &dir, &progress, &cancel).await,
        Err(e) => Err(e),
    };

    if let Ok(mut slot) = state.download.lock() {
        *slot = None;
    }
    result.map(|_| ()).map_err(|e| e.to_string())
}

/// Stops the running download. The partial file is kept, so the next download resumes.
#[tauri::command]
pub fn cancel_download(state: State<Listening>) {
    if let Ok(slot) = state.download.lock() {
        if let Some(cancel) = slot.as_ref() {
            cancel.cancel();
        }
    }
}

fn load_engine(app: &AppHandle) -> Result<Arc<dyn SpeechToText>, String> {
    let path = model_spec().path_in(&models_dir(app)?);
    if !path.is_file() {
        return Err(MODEL_MISSING.to_owned());
    }
    let engine = WhisperStt::load(WhisperConfig::new(path)).map_err(|e| e.to_string())?;
    Ok(Arc::new(engine))
}

fn start_blocking(app: &AppHandle, speaker: Speaker) -> Result<(), String> {
    let listening = app.state::<Listening>();
    let mut inner = listening
        .inner
        .lock()
        .map_err(|_| "listening state is unavailable".to_owned())?;

    let engine = match &inner.engine {
        Some(engine) => Arc::clone(engine),
        None => {
            let engine = load_engine(app)?;
            inner.engine = Some(Arc::clone(&engine));
            engine
        }
    };
    let pipeline = inner.pipeline.get_or_insert_with(|| {
        let handle = app.clone();
        let sink: EventSink = Arc::new(move |event| emit(&handle, &event));
        Pipeline::new(engine, sink, PipelineConfig::default())
    });

    let source: Box<dyn AudioSource> = match speaker {
        Speaker::Me => Box::new(CpalSource::microphone()),
        Speaker::Remote => Box::new(CpalSource::system_loopback()),
    };
    let started = pipeline.start(source).map_err(|e| e.to_string());

    // Do not keep the model in memory for a source that never started.
    if started.is_err() && pipeline.status().is_empty() {
        inner.pipeline = None;
        inner.engine = None;
    }
    started
}

fn stop_blocking(app: &AppHandle, speaker: Speaker) -> Result<(), String> {
    let listening = app.state::<Listening>();
    let mut inner = listening
        .inner
        .lock()
        .map_err(|_| "listening state is unavailable".to_owned())?;
    if let Some(pipeline) = inner.pipeline.as_mut() {
        pipeline.stop(speaker);
        if pipeline.status().is_empty() {
            // Last source stopped: release the model.
            inner.pipeline = None;
            inner.engine = None;
        }
    }
    Ok(())
}

/// Starts listening to the microphone (`me`) or the system audio (`remote`). Fails with
/// [`MODEL_MISSING`] when the model has not been downloaded. Transcripts arrive as `transcript`
/// events. Both loading the model and opening the device can take seconds, so this runs off
/// the main thread.
#[tauri::command]
pub async fn start_listening(app: AppHandle, speaker: Speaker) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || start_blocking(&app, speaker))
        .await
        .map_err(|e| e.to_string())?
}

/// Stops one source. Speech still in progress is transcribed first.
#[tauri::command]
pub async fn stop_listening(app: AppHandle, speaker: Speaker) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || stop_blocking(&app, speaker))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn pipeline_status(state: State<Listening>) -> ListeningStatus {
    // `try_lock`, because a start in progress holds the lock for as long as the model takes to load.
    match state.inner.try_lock() {
        Ok(inner) => ListeningStatus {
            lanes: inner
                .pipeline
                .as_ref()
                .map(Pipeline::status)
                .unwrap_or_default(),
            busy: false,
        },
        Err(TryLockError::WouldBlock) => ListeningStatus {
            lanes: Vec::new(),
            busy: true,
        },
        Err(TryLockError::Poisoned(_)) => ListeningStatus {
            lanes: Vec::new(),
            busy: false,
        },
    }
}
