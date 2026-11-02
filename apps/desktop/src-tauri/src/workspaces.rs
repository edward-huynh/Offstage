//! Commands for workspaces: a set of documents, a note for the model, and the choices that go
//! with one situation. The one in use decides which documents the Knowledge screen lists, which
//! ones the assistant searches, and how it answers (see `assistant::effective_config`).

use offstage_core::workspaces::{Workspace, WorkspaceStore, WorkspaceUpdate};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::knowledge::Knowledge;

/// Sent after a workspace was created, changed, deleted or put in use. The screens that show
/// workspaces or documents answer by asking again.
pub const CHANGED_EVENT: &str = "workspace_changed";

/// The workspace in use.
pub fn active(app: &AppHandle) -> Result<Workspace, String> {
    app.try_state::<WorkspaceStore>()
        .ok_or_else(|| "workspace storage is unavailable".to_owned())?
        .active()
        .map_err(|e| e.to_string())
}

fn changed(app: &AppHandle) {
    // A failed emit means the window is gone, so there is nobody to tell.
    let _ = app.emit(CHANGED_EVENT, ());
}

#[tauri::command]
pub async fn list_workspaces(store: State<'_, WorkspaceStore>) -> Result<Vec<Workspace>, String> {
    let store = store.inner().clone();
    crate::off_main(move || store.list().map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn active_workspace(store: State<'_, WorkspaceStore>) -> Result<Workspace, String> {
    let store = store.inner().clone();
    crate::off_main(move || store.active().map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn set_active_workspace(
    app: AppHandle,
    store: State<'_, WorkspaceStore>,
    id: String,
) -> Result<Workspace, String> {
    let store = store.inner().clone();
    let workspace =
        crate::off_main(move || store.set_active(&id).map_err(|e| e.to_string())).await?;
    // The new workspace may want another model, or none.
    crate::local_llm::refresh(&app);
    changed(&app);
    Ok(workspace)
}

#[tauri::command]
pub async fn create_workspace(
    app: AppHandle,
    store: State<'_, WorkspaceStore>,
    name: String,
) -> Result<Workspace, String> {
    let store = store.inner().clone();
    let workspace = crate::off_main(move || store.create(&name).map_err(|e| e.to_string())).await?;
    changed(&app);
    Ok(workspace)
}

#[tauri::command]
pub async fn update_workspace(
    app: AppHandle,
    store: State<'_, WorkspaceStore>,
    id: String,
    update: WorkspaceUpdate,
) -> Result<Workspace, String> {
    let store = store.inner().clone();
    let workspace =
        crate::off_main(move || store.update(&id, update).map_err(|e| e.to_string())).await?;
    // Its choices (local only, provider) may have changed what the running model should be.
    crate::local_llm::refresh(&app);
    changed(&app);
    Ok(workspace)
}

/// Deletes a workspace and its documents (their files, text and vectors with them).
#[tauri::command]
pub async fn delete_workspace(
    app: AppHandle,
    store: State<'_, WorkspaceStore>,
    knowledge: State<'_, Knowledge>,
    id: String,
) -> Result<(), String> {
    let store = store.inner().clone();
    let documents = knowledge.0.clone();
    crate::off_main(move || {
        // Refuse before touching the documents: the last workspace cannot go, and then its
        // documents must stay.
        let all = store.list().map_err(|e| e.to_string())?;
        if !all.iter().any(|w| w.id == id) {
            return Err(format!("workspace not found: {id}"));
        }
        if all.len() <= 1 {
            return Err("Phải còn ít nhất một workspace".to_owned());
        }
        let documents = documents
            .lock()
            .map_err(|_| "document storage is unavailable".to_owned())?;
        for doc in documents.list_in(&id).map_err(|e| e.to_string())? {
            documents.delete(&doc.id).map_err(|e| e.to_string())?;
        }
        store.delete(&id).map_err(|e| e.to_string())
    })
    .await?;
    // If the one in use went, another took its place.
    crate::local_llm::refresh(&app);
    changed(&app);
    Ok(())
}
