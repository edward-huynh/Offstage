//! Playing: the main window picks what to play, the overlay shows it.
//!
//! Starting a session hides the main window and brings up the overlay; stopping does the
//! reverse. Rust holds the session so both windows agree on it. The interface only asks,
//! and listens for `play_started` and `play_stopped`.

use std::sync::Mutex;

use offstage_core::play::PlaySession;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{listening, OVERLAY};

pub const MAIN: &str = "main";
const PLAY_STARTED: &str = "play_started";
const PLAY_STOPPED: &str = "play_stopped";

/// The session in progress, if any.
#[derive(Default)]
pub struct Play(Mutex<Option<PlaySession>>);

fn set_session(app: &AppHandle, session: Option<PlaySession>) -> Result<(), String> {
    let play = app.state::<Play>();
    let mut slot = play
        .0
        .lock()
        .map_err(|_| "play state is unavailable".to_owned())?;
    *slot = session;
    Ok(())
}

pub fn is_playing(app: &AppHandle) -> bool {
    app.state::<Play>()
        .0
        .lock()
        .map(|slot| slot.is_some())
        .unwrap_or(false)
}

/// Brings the main window to the front, restoring it if it was hidden or minimized.
pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Opens the overlay for `session` and gets the main window out of the way, so it cannot
/// show up in a screen share.
#[tauri::command]
pub fn start_play(app: AppHandle, session: PlaySession) -> Result<(), String> {
    let session = session.validated().map_err(|e| e.to_string())?;
    let overlay = app
        .get_webview_window(OVERLAY)
        .ok_or_else(|| "the overlay window is missing".to_owned())?;

    set_session(&app, Some(session.clone()))?;
    // The overlay comes up first, so a failure leaves the main window where it was.
    if let Err(e) = overlay.show() {
        set_session(&app, None)?;
        return Err(e.to_string());
    }
    let _ = overlay.set_focus();
    if let Some(main) = app.get_webview_window(MAIN) {
        let _ = main.hide();
    }
    let _ = app.emit(PLAY_STARTED, &session);
    Ok(())
}

/// Stops listening, hides the overlay and returns to the main window. Fine to call when
/// nothing is playing. Async because stopping a source waits for queued transcription.
#[tauri::command]
pub async fn stop_play(app: AppHandle) -> Result<(), String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || listening::stop_all(&handle))
        .await
        .map_err(|e| e.to_string())?;

    set_session(&app, None)?;
    if let Some(overlay) = app.get_webview_window(OVERLAY) {
        let _ = overlay.hide();
    }
    show_main(&app);
    let _ = app.emit(PLAY_STOPPED, ());
    Ok(())
}

#[tauri::command]
pub fn get_play_session(play: State<Play>) -> Option<PlaySession> {
    play.0.lock().ok().and_then(|slot| slot.clone())
}
