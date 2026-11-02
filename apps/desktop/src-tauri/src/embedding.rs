//! The embedding model (multilingual-e5-small, about 135 MB): downloading it, keeping it loaded
//! while it is needed, and making vectors for the document chunks. With it the assistant finds
//! paragraphs by meaning, in any language; without it search still works by words.
//!
//! It is loaded while listening (a question may come any moment) and when chunks need vectors,
//! and released afterwards, to keep an idle app small (about 300 MB while loaded).

use std::sync::{Arc, Mutex};

use offstage_core::AppEvent;
use offstage_embed::onnx::{E5Embedder, ModelFiles};
use offstage_models::{multilingual_e5_small, CancellationToken, Downloader, ModelSpec, Progress};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{listening::emit, rag::INDEXED_EVENT};

/// Tags the vectors in the database and the `model_progress` events of the download.
pub const MODEL_ID: &str = "multilingual-e5-small";

#[derive(Default)]
pub struct Embedding {
    slot: Mutex<Slot>,
    /// Makes vectors for one batch of chunks at a time.
    work: Mutex<()>,
    download: Mutex<Option<CancellationToken>>,
}

#[derive(Default)]
struct Slot {
    embedder: Option<Arc<E5Embedder>>,
    /// Listening is on, so the model stays loaded.
    pinned: bool,
    /// Chunks are being embedded right now, so the model is not released under them.
    working: u32,
}

fn models_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("models"))
        .map_err(|e| e.to_string())
}

fn spec_named(name: &str) -> ModelSpec {
    let id = format!("{MODEL_ID}/{name}");
    multilingual_e5_small()
        .into_iter()
        .find(|spec| spec.id == id)
        // The preset list is fixed in code and tested to hold these names.
        .unwrap_or_else(|| unreachable!("preset {id} exists"))
}

fn files(app: &AppHandle) -> Result<ModelFiles, String> {
    let dir = models_dir(app)?;
    let path = |name: &str| spec_named(name).path_in(&dir);
    Ok(ModelFiles {
        model: path("model"),
        tokenizer: path("tokenizer"),
        config: path("config"),
        special_tokens_map: path("special-tokens"),
        tokenizer_config: path("tokenizer-config"),
    })
}

/// All files are on disk with the right size. The checksum is verified when they are downloaded.
fn installed(app: &AppHandle) -> bool {
    let Ok(dir) = models_dir(app) else {
        return false;
    };
    multilingual_e5_small().iter().all(|spec| {
        std::fs::metadata(spec.path_in(&dir)).is_ok_and(|m| m.is_file() && m.len() == spec.size)
    })
}

fn with_slot<T>(app: &AppHandle, f: impl FnOnce(&mut Slot) -> T) -> Option<T> {
    let embedding = app.try_state::<Embedding>()?;
    let mut slot = embedding.slot.lock().ok()?;
    Some(f(&mut slot))
}

/// The loaded model, if it is loaded right now. Never loads it and never waits for a load, so a
/// question is not held up: without the model the answer is searched by words alone.
pub fn current(app: &AppHandle) -> Option<Arc<E5Embedder>> {
    let embedding = app.try_state::<Embedding>()?;
    let slot = embedding.slot.try_lock().ok()?;
    slot.embedder.clone()
}

/// Loads the model if it is installed and not loaded yet. Blocks while it loads.
fn ensure_loaded(app: &AppHandle) -> Option<Arc<E5Embedder>> {
    let embedding = app.try_state::<Embedding>()?;
    let mut slot = embedding.slot.lock().ok()?;
    if let Some(embedder) = &slot.embedder {
        return Some(Arc::clone(embedder));
    }
    if !installed(app) {
        return None;
    }
    match files(app).and_then(|f| E5Embedder::load(&f).map_err(|e| e.to_string())) {
        Ok(embedder) => {
            let embedder = Arc::new(embedder);
            slot.embedder = Some(Arc::clone(&embedder));
            Some(embedder)
        }
        Err(e) => {
            eprintln!("embedding model not loaded: {e}");
            None
        }
    }
}

fn release_if_idle(app: &AppHandle) {
    with_slot(app, |slot| {
        if !slot.pinned && slot.working == 0 {
            slot.embedder = None;
        }
    });
}

/// Listening started: load the model in the background if the assistant is on, so the first
/// question does not wait for it.
pub fn pin(app: &AppHandle) {
    with_slot(app, |slot| slot.pinned = true);
    if !crate::assistant::is_enabled(app) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        ensure_loaded(&app);
        // Listening may have stopped while it loaded.
        release_if_idle(&app);
    });
}

/// Listening ended: release the model unless chunks are being embedded.
pub fn unpin(app: &AppHandle) {
    with_slot(app, |slot| slot.pinned = false);
    release_if_idle(app);
}

/// Makes vectors for the chunks that have none, then tells the Knowledge screen. Does nothing,
/// without loading the model, when there is nothing to do or the model is not installed. Blocks.
pub fn embed_pending_blocking(app: &AppHandle) {
    let (Some(rag), Some(embedding)) = (
        app.try_state::<crate::rag::Rag>(),
        app.try_state::<Embedding>(),
    ) else {
        return;
    };
    if !installed(app) || rag.0.pending_vector_count(MODEL_ID).unwrap_or(0) == 0 {
        return;
    }
    // One run at a time: a second one would only wait here and then find nothing to do.
    let Ok(_turn) = embedding.work.lock() else {
        return;
    };
    with_slot(app, |slot| slot.working += 1);
    if let Some(embedder) = ensure_loaded(app) {
        let result = rag.0.embed_pending(embedder.as_ref(), MODEL_ID, |_| {
            // A failed emit means the window is gone, so there is nobody to tell.
            let _ = app.emit(INDEXED_EVENT, ());
        });
        if let Err(e) = result {
            eprintln!("chunks not embedded: {e}");
        }
    }
    with_slot(app, |slot| slot.working -= 1);
    release_if_idle(app);
    let _ = app.emit(INDEXED_EVENT, ());
}

/// Same, on the blocking pool.
pub fn embed_pending_in_background(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || embed_pending_blocking(&app));
}

#[derive(Debug, Serialize)]
pub struct EmbeddingInfo {
    pub installed: bool,
    /// Bytes to download in all.
    pub size: u64,
    pub license: String,
    pub license_url: Option<String>,
    /// Chunks still waiting for a vector. Zero once everything is searchable by meaning.
    pub pending_chunks: usize,
}

#[tauri::command]
pub async fn embedding_model_info(
    app: AppHandle,
    rag: State<'_, crate::rag::Rag>,
) -> Result<EmbeddingInfo, String> {
    let store = rag.0.clone();
    crate::off_main(move || {
        let specs = multilingual_e5_small();
        Ok(EmbeddingInfo {
            installed: installed(&app),
            size: specs.iter().map(|s| s.size).sum(),
            license: specs[0].license.clone(),
            license_url: specs[0].license_url.clone(),
            pending_chunks: store
                .pending_vector_count(MODEL_ID)
                .map_err(|e| e.to_string())?,
        })
    })
    .await
}

/// Downloads the model files one after the other (or finishes a partial download) and then makes
/// vectors for the documents already stored. Progress arrives as `model_progress` events for
/// [`MODEL_ID`], counted over all files.
#[tauri::command]
pub async fn download_embedding_model(
    app: AppHandle,
    state: State<'_, Embedding>,
) -> Result<(), String> {
    let specs = multilingual_e5_small();
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

    let total: u64 = specs.iter().map(|s| s.size).sum();
    let result = async {
        let downloader = Downloader::new().map_err(|e| e.to_string())?;
        let mut done = 0;
        for spec in &specs {
            let progress = {
                let app = app.clone();
                move |p: Progress| {
                    emit(
                        &app,
                        &AppEvent::ModelProgress {
                            id: MODEL_ID.to_owned(),
                            downloaded: done + p.downloaded,
                            total,
                        },
                    );
                }
            };
            downloader
                .ensure(spec, &dir, &progress, &cancel)
                .await
                .map_err(|e| e.to_string())?;
            done += spec.size;
        }
        Ok::<(), String>(())
    }
    .await;

    if let Ok(mut slot) = state.download.lock() {
        *slot = None;
    }
    if result.is_ok() {
        embed_pending_in_background(&app);
    }
    result
}

/// Stops the running download. Finished files and the partial one are kept, so the next download
/// resumes.
#[tauri::command]
pub fn cancel_embedding_download(state: State<Embedding>) {
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
    fn every_file_name_the_code_asks_for_is_in_the_presets() {
        for name in [
            "model",
            "tokenizer",
            "config",
            "special-tokens",
            "tokenizer-config",
        ] {
            assert_eq!(spec_named(name).id, format!("{MODEL_ID}/{name}"));
        }
    }
}
