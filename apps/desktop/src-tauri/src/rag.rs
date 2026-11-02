//! Connects the document index (`offstage-rag`) to the app: documents are read into the index
//! in the background after they are added, the Knowledge screen asks how each one went, and the
//! assistant searches it for every question (see `assistant.rs`).
//!
//! Reading a PDF can take seconds, so none of it runs on the window's event loop and none of it
//! holds the document store's lock while it works.

use offstage_rag::{ingest::index_file, ChunkStore, IndexStatus};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::knowledge::Knowledge;

/// The index, shared with the assistant.
pub struct Rag(pub ChunkStore);

/// Sent after a document was read into the index, or failed to be. The Knowledge screen answers
/// by asking for the statuses again.
pub const INDEXED_EVENT: &str = "knowledge_indexed";

/// Reads the documents with these ids into the index, one after the other, and returns at once.
pub fn index_in_background(app: &AppHandle, ids: Vec<String>) {
    if ids.is_empty() {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        for id in ids {
            index_one(&app, &id);
            // A failed emit means the window is gone, so there is nobody to tell.
            let _ = app.emit(INDEXED_EVENT, ());
        }
        // Words are searchable now; meaning follows when the embedding model is there.
        crate::embedding::embed_pending_blocking(&app);
    });
}

/// Reads the documents the index has never seen: those added before indexing existed, or while
/// the app was closed halfway. Called once at start.
pub fn index_missing(app: &AppHandle) {
    let Some(rag) = app.try_state::<Rag>() else {
        return;
    };
    match rag.0.unindexed() {
        Ok(ids) => index_in_background(app, ids),
        Err(e) => eprintln!("documents not indexed: {e}"),
    }
}

fn index_one(app: &AppHandle, id: &str) {
    let (Some(knowledge), Some(rag)) = (app.try_state::<Knowledge>(), app.try_state::<Rag>())
    else {
        return;
    };
    // Look the document up under the lock, then read it without.
    let found = knowledge
        .0
        .lock()
        .map_err(|_| "document storage is unavailable".to_owned())
        .and_then(|store| {
            let doc = store.get(id).map_err(|e| e.to_string())?;
            let path = store.path_of(id).map_err(|e| e.to_string())?;
            Ok((doc, path))
        });
    match found {
        Ok((doc, path)) => {
            if let Err(e) = index_file(&rag.0, &doc.id, &doc.name, doc.kind, &path) {
                // The reason is stored for the Knowledge screen; this is for a developer.
                eprintln!("{} not indexed: {e}", doc.name);
            }
        }
        // Deleted before its turn came.
        Err(e) => eprintln!("document {id} not indexed: {e}"),
    }
}

/// How each document went: how many chunks it gave, or why it could not be read. A document
/// missing from the list has not been looked at yet.
#[tauri::command]
pub async fn knowledge_index_status(rag: State<'_, Rag>) -> Result<Vec<IndexStatus>, String> {
    let store = rag.0.clone();
    crate::off_main(move || store.statuses().map_err(|e| e.to_string())).await
}
