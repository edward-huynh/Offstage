//! App settings: speech language, interview sources and screen-share hiding.
//! Stored in the local database, under their own key.

use offstage_core::{settings::AppSettings, HotkeyAction};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::{play::MAIN, storage};

const SETTINGS_KEY: &str = "app_settings";

/// Reads the saved settings. A missing or unreadable value yields the defaults.
pub fn load(app: &AppHandle) -> Result<AppSettings, String> {
    Ok(storage::get::<AppSettings>(app, SETTINGS_KEY)?
        .unwrap_or_default()
        .clamped())
}

/// Hides the main window from screen shares, or shows it to them again.
pub fn apply_capture_protection(app: &AppHandle, hidden: bool) -> Result<(), String> {
    match app.get_webview_window(MAIN) {
        Some(window) => window
            .set_content_protected(hidden)
            .map_err(|e| e.to_string()),
        None => Ok(()),
    }
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> Result<AppSettings, String> {
    load(&app)
}

/// Saves the settings and applies the ones that act at once. Resolves to what was stored,
/// which can differ from the request when a value had to be corrected.
#[tauri::command]
pub fn update_settings(app: AppHandle, settings: AppSettings) -> Result<AppSettings, String> {
    let settings = settings.clamped();
    storage::set(&app, SETTINGS_KEY, &settings)?;
    apply_capture_protection(&app, settings.hide_main_from_capture)?;
    Ok(settings)
}

#[derive(Serialize)]
pub struct HotkeyBinding {
    action: HotkeyAction,
    accelerator: &'static str,
}

/// Every global shortcut with its default key combination, for the Settings screen.
#[tauri::command]
pub fn hotkey_bindings() -> Vec<HotkeyBinding> {
    HotkeyAction::ALL
        .iter()
        .map(|&action| HotkeyBinding {
            action,
            accelerator: action.default_accelerator(),
        })
        .collect()
}
