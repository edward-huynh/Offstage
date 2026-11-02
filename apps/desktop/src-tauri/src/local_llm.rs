//! The language model on this machine: where its files are, downloading it, and starting the
//! sidecar that runs it (`offstage-llm-host`, a separate process because `llama.cpp` cannot share
//! one with `whisper.cpp`). The router in `offstage-llm` asks [`Source`] for it when an answer
//! needs it.
//!
//! The sidecar holds a few gigabytes while it runs, so it is started when an answer needs it (or
//! when listening starts in local-only mode) and stopped when listening ends.

use std::{path::PathBuf, sync::Arc, time::Duration};

use offstage_core::AppEvent;
use offstage_llm::{
    config::{AssistantConfig, AUTO_LOCAL_MODEL},
    local::LocalLlm,
    router::{ChatBackend, LocalSource},
    LlmError,
};
use offstage_models::{
    local_llms, recommended_llm, CancellationToken, Downloader, ModelSpec, Progress,
};
use serde::Serialize;
use tauri::{async_runtime::Mutex, AppHandle, Manager, State};

use crate::listening::emit;

/// File name of the sidecar next to the app's own executable.
const HOST_NAME: &str = if cfg!(windows) {
    "offstage-llm-host.exe"
} else {
    "offstage-llm-host"
};

/// How long loading the model may take. A cold read of 2.5 GB from a slow disk is the worst case.
const LOAD_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Default)]
pub struct LocalModel {
    /// The running sidecar. The lock is held while one starts, so two answers do not start two.
    loaded: Mutex<Option<Arc<LocalLlm>>>,
    download: std::sync::Mutex<Option<CancellationToken>>,
}

/// The memory of this computer in bytes, or 0 if it cannot be read. Read once: it does not change.
fn total_ram() -> u64 {
    static RAM: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *RAM.get_or_init(|| {
        let mut system = sysinfo::System::new();
        system.refresh_memory();
        system.total_memory()
    })
}

/// The id of the model a setting stands for: [`AUTO_LOCAL_MODEL`] becomes the one that suits the
/// memory of the computer, anything else is already an id.
fn resolve_with(id: &str, ram_bytes: u64) -> String {
    if id == AUTO_LOCAL_MODEL {
        recommended_llm(ram_bytes).id
    } else {
        id.to_owned()
    }
}

fn resolve(id: &str) -> String {
    resolve_with(id, total_ram())
}

fn spec_of(id: &str) -> Option<ModelSpec> {
    let id = resolve(id);
    local_llms().into_iter().find(|spec| spec.id == id)
}

fn label_of(id: &str) -> &'static str {
    match id {
        "qwen3-4b-q4_k_m" => "Qwen3 4B",
        "qwen3-1.7b-q4_k_m" => "Qwen3 1.7B",
        _ => "Model trên máy",
    }
}

fn models_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("models"))
        .map_err(|e| e.to_string())
}

/// Where the sidecar should be: next to the app's executable. In a bundle the build puts it
/// there, and in development `cargo build` does.
fn host_path() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe.parent().ok_or("the app has no folder")?;
    Ok(dir.join(HOST_NAME))
}

/// The model file is on disk with the right size. The checksum is verified when it is downloaded.
pub fn installed(app: &AppHandle, id: &str) -> bool {
    let (Some(spec), Ok(dir)) = (spec_of(id), models_dir(app)) else {
        return false;
    };
    std::fs::metadata(spec.path_in(&dir)).is_ok_and(|m| m.is_file() && m.len() == spec.size)
}

/// The local model for this choice, or `None` when it is not downloaded: the router then has no
/// local model to use or fall back to.
pub fn source(app: &AppHandle, config: &AssistantConfig) -> Option<Arc<dyn LocalSource>> {
    installed(app, &config.local_model).then(|| {
        Arc::new(Source {
            app: app.clone(),
            model_id: config.local_model.clone(),
        }) as Arc<dyn LocalSource>
    })
}

pub struct Source {
    app: AppHandle,
    model_id: String,
}

fn start(app: &AppHandle, id: &str) -> Result<LocalLlm, LlmError> {
    let spec =
        spec_of(id).ok_or_else(|| LlmError::Request(format!("unknown local model {id:?}")))?;
    let dir = models_dir(app).map_err(LlmError::Request)?;
    let host = host_path().map_err(LlmError::Request)?;
    if !host.is_file() {
        return Err(LlmError::Request(format!(
            "the program that runs the local model is missing ({}). Build it with `cargo build -p offstage-llm-host`",
            host.display()
        )));
    }
    LocalLlm::start(
        &host,
        &spec.path_in(&dir),
        // Qwen3 writes a long "thinking" section first unless told there is none.
        spec.id.starts_with("qwen3"),
        LOAD_TIMEOUT,
    )
}

#[async_trait::async_trait]
impl LocalSource for Source {
    async fn get(&self) -> Result<Arc<dyn ChatBackend>, LlmError> {
        let state = self.app.state::<LocalModel>();
        let mut loaded = state.loaded.lock().await;
        let wanted = spec_of(&self.model_id).map(|spec| spec.file_name);
        if let Some(llm) = loaded
            .as_ref()
            .filter(|llm| llm.is_alive() && Some(llm.model()) == wanted.as_deref())
        {
            return Ok(Arc::clone(llm) as Arc<dyn ChatBackend>);
        }
        // A stopped one, or another model than the one wanted now: let it go before loading.
        let old = loaded.take();
        let (app, id) = (self.app.clone(), self.model_id.clone());
        let started = tauri::async_runtime::spawn_blocking(move || {
            // Dropping the old one stops its process, which waits a moment.
            drop(old);
            start(&app, &id)
        })
        .await
        .map_err(|e| LlmError::Request(e.to_string()))??;
        let llm = Arc::new(started);
        *loaded = Some(Arc::clone(&llm));
        Ok(llm as Arc<dyn ChatBackend>)
    }
}

/// Stops the sidecar and gives its memory back.
pub fn release(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let old = app.state::<LocalModel>().loaded.lock().await.take();
        // Stopping the process waits for it, so not on the async runtime.
        let _ = tauri::async_runtime::spawn_blocking(move || drop(old)).await;
    });
}

/// Listening started. With the assistant on and local-only, load the model now so the first
/// question does not wait for it. Otherwise it loads when an answer first needs it.
pub fn pin(app: &AppHandle) {
    let Ok(Some(config)) =
        crate::storage::get::<AssistantConfig>(app, crate::assistant::CONFIG_KEY)
    else {
        return;
    };
    if !(config.enabled && config.local_only) {
        return;
    }
    if let Some(source) = source(app, &config) {
        tauri::async_runtime::spawn(async move {
            if let Err(e) = source.get().await {
                eprintln!("local model not loaded: {e}");
            }
        });
    }
}

/// Listening ended.
pub fn unpin(app: &AppHandle) {
    release(app);
}

#[derive(Debug, Serialize)]
pub struct LocalModelInfo {
    pub id: String,
    pub label: String,
    pub size: u64,
    pub license: String,
    pub installed: bool,
}

#[derive(Debug, Serialize)]
pub struct LocalLlmInfo {
    pub models: Vec<LocalModelInfo>,
    /// The memory of this computer in bytes, 0 if it could not be read.
    pub ram_bytes: u64,
    /// The id of the model that "automatic" stands for on this computer.
    pub recommended: String,
    /// The program that runs the model is next to the app. False in a development build that did
    /// not build it.
    pub engine_found: bool,
}

#[tauri::command]
pub fn local_llm_info(app: AppHandle) -> Result<LocalLlmInfo, String> {
    Ok(LocalLlmInfo {
        models: local_llms()
            .into_iter()
            .map(|spec| LocalModelInfo {
                installed: installed(&app, &spec.id),
                label: label_of(&spec.id).to_owned(),
                size: spec.size,
                license: spec.license,
                id: spec.id,
            })
            .collect(),
        ram_bytes: total_ram(),
        recommended: resolve(AUTO_LOCAL_MODEL),
        engine_found: host_path().is_ok_and(|path| path.is_file()),
    })
}

/// Downloads a model, or finishes a partial download. Progress arrives as `model_progress`
/// events carrying the model's id.
#[tauri::command]
pub async fn download_local_llm(
    app: AppHandle,
    state: State<'_, LocalModel>,
    id: String,
) -> Result<(), String> {
    // The screen can ask for "automatic": the file to fetch is the model that stands for.
    let spec = spec_of(&id).ok_or_else(|| format!("unknown local model {id:?}"))?;
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

/// Stops the running download. The partial file is kept, so the next one resumes.
#[tauri::command]
pub fn cancel_local_llm_download(state: State<LocalModel>) {
    if let Ok(slot) = state.download.lock() {
        if let Some(cancel) = slot.as_ref() {
            cancel.cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_local_model_has_a_label() {
        for spec in local_llms() {
            assert_ne!(label_of(&spec.id), "Model trên máy", "{}", spec.id);
        }
    }

    #[test]
    fn automatic_is_a_real_model_and_a_named_model_is_left_alone() {
        // Whatever the memory of the machine running the test, "auto" is one of the presets.
        assert!(spec_of(AUTO_LOCAL_MODEL).is_some());
        assert!(spec_of("nope").is_none());
        assert_eq!(
            resolve_with("qwen3-1.7b-q4_k_m", 64 << 30),
            "qwen3-1.7b-q4_k_m"
        );
        assert_eq!(resolve_with("qwen3-4b-q4_k_m", 0), "qwen3-4b-q4_k_m");
    }

    #[test]
    fn automatic_follows_the_memory() {
        assert_eq!(resolve_with(AUTO_LOCAL_MODEL, 8 << 30), "qwen3-1.7b-q4_k_m");
        assert_eq!(resolve_with(AUTO_LOCAL_MODEL, 16 << 30), "qwen3-4b-q4_k_m");
        assert_eq!(resolve_with(AUTO_LOCAL_MODEL, 0), "qwen3-1.7b-q4_k_m");
    }

    #[test]
    fn the_memory_of_this_machine_can_be_read() {
        assert!(total_ram() > 512 << 20, "{}", total_ram());
    }

    #[test]
    fn the_sidecar_is_looked_for_next_to_the_executable() {
        let path = host_path().unwrap();
        assert!(path.ends_with(HOST_NAME));
        assert_eq!(path.parent(), std::env::current_exe().unwrap().parent());
    }
}
