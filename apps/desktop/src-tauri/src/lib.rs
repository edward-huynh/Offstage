use std::{
    fs,
    str::FromStr,
    sync::{
        mpsc::{self, RecvTimeoutError},
        Mutex,
    },
    time::Duration,
};

use offstage_core::scripts::{self, ScriptError, ScriptMeta, ScriptStore};
use offstage_core::{CaptureProtection, HotkeyAction, OverlayConfig, WindowState};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, RunEvent, State, WebviewWindow,
    WindowEvent, Wry,
};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_plugin_store::StoreExt;

mod listening;
mod permissions;

const OVERLAY: &str = "overlay";
const STORE_FILE: &str = "settings.json";
const CONFIG_KEY: &str = "overlay_config";
const HOTKEY_EVENT: &str = "hotkey";
const CLICK_THROUGH_EVENT: &str = "click_through";
const WINDOW_KEY: &str = "window_state";
const SCRIPTS_SEEDED_KEY: &str = "scripts_seeded";
/// Scripts are read aloud, so anything bigger than this is not a script.
const MAX_IMPORT_BYTES: u64 = 1_000_000;
/// How long the window must stay still before its geometry is written.
const SAVE_DELAY: Duration = Duration::from_millis(500);

/// Shortcuts that could not be registered, e.g. because another app already owns them.
/// The setup runs before the webview exists, so the UI pulls this instead of waiting for an event.
struct HotkeyFailures(Vec<String>);

/// The tray checkbox, kept so a hotkey toggle can update its tick.
struct ClickThroughMenu(CheckMenuItem<Wry>);

/// Pings the thread that saves the window geometry after it stops changing.
struct WindowSaver(mpsc::Sender<()>);

/// The script storage. The lock keeps two commands from interleaving their
/// read-modify-write of the index.
struct Scripts(Mutex<ScriptStore>);

fn with_scripts<T>(
    scripts: &Scripts,
    op: impl FnOnce(&ScriptStore) -> Result<T, ScriptError>,
) -> Result<T, String> {
    let store = scripts
        .0
        .lock()
        .map_err(|_| "script storage is unavailable".to_owned())?;
    op(&store).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_scripts(scripts: State<Scripts>) -> Result<Vec<ScriptMeta>, String> {
    with_scripts(&scripts, |s| s.list())
}

#[tauri::command]
fn create_script(
    scripts: State<Scripts>,
    title: String,
    body: String,
) -> Result<ScriptMeta, String> {
    with_scripts(&scripts, |s| s.create(&title, &body))
}

#[tauri::command]
fn read_script(scripts: State<Scripts>, id: String) -> Result<String, String> {
    with_scripts(&scripts, |s| s.read(&id))
}

#[tauri::command]
fn save_script(scripts: State<Scripts>, id: String, body: String) -> Result<ScriptMeta, String> {
    with_scripts(&scripts, |s| s.save(&id, &body))
}

#[tauri::command]
fn rename_script(scripts: State<Scripts>, id: String, title: String) -> Result<ScriptMeta, String> {
    with_scripts(&scripts, |s| s.rename(&id, &title))
}

#[tauri::command]
fn delete_script(scripts: State<Scripts>, id: String) -> Result<(), String> {
    with_scripts(&scripts, |s| s.delete(&id))
}

/// Lets the user pick a `.txt` or `.md` file and stores it as a new script.
/// `Ok(None)` means the dialog was cancelled. The file is read here, not in the webview.
/// Async so the blocking dialog does not run on the main thread, which would deadlock.
#[tauri::command]
async fn import_script(
    app: AppHandle,
    scripts: State<'_, Scripts>,
) -> Result<Option<ScriptMeta>, String> {
    let Some(picked) = app
        .dialog()
        .file()
        .add_filter("Kịch bản", &["txt", "md", "markdown"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;

    // The filter is only a hint on some systems, so check the extension again.
    let is_text = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "txt" | "md" | "markdown"));
    if !is_text {
        return Err("Chỉ nhập được tệp .txt hoặc .md".to_owned());
    }
    let size = fs::metadata(&path).map_err(|e| e.to_string())?.len();
    if size > MAX_IMPORT_BYTES {
        return Err("Tệp quá lớn để làm kịch bản (tối đa 1 MB)".to_owned());
    }
    let content =
        fs::read_to_string(&path).map_err(|_| "Không đọc được tệp, cần mã hoá UTF-8".to_owned())?;
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    let title = scripts::title_for_import(stem, &content);
    with_scripts(&scripts, |s| s.create(&title, &content)).map(Some)
}

/// On the very first run, adds one sample script so the teleprompter is not empty.
/// Remembered in the settings store, so deleting every script later does not bring it back.
fn seed_sample_script(app: &AppHandle, scripts: &Scripts) {
    let Ok(settings) = app.store(STORE_FILE) else {
        return;
    };
    if settings
        .get(SCRIPTS_SEEDED_KEY)
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return;
    }
    let result = with_scripts(scripts, |s| {
        if s.list()?.is_empty() {
            s.create(scripts::SAMPLE_TITLE, scripts::SAMPLE_BODY)?;
        }
        Ok(())
    });
    match result {
        Ok(()) => settings.set(SCRIPTS_SEEDED_KEY, true),
        Err(e) => eprintln!("sample script not created: {e}"),
    }
}

/// Reads the saved config. A missing or unreadable value yields the defaults.
fn load_config(app: &AppHandle) -> Result<OverlayConfig, String> {
    let store = app.store(STORE_FILE).map_err(|e| e.to_string())?;
    Ok(store
        .get(CONFIG_KEY)
        .and_then(|v| serde_json::from_value::<OverlayConfig>(v).ok())
        .unwrap_or_default()
        .clamped())
}

#[tauri::command]
fn get_config(app: AppHandle) -> Result<OverlayConfig, String> {
    load_config(&app)
}

fn save_config(app: &AppHandle, config: &OverlayConfig) -> Result<(), String> {
    let store = app.store(STORE_FILE).map_err(|e| e.to_string())?;
    let value = serde_json::to_value(config).map_err(|e| e.to_string())?;
    store.set(CONFIG_KEY, value);
    Ok(())
}

#[tauri::command]
fn update_config(app: AppHandle, config: OverlayConfig) -> Result<OverlayConfig, String> {
    let mut config = config.clamped();
    // Rust owns this flag: the hotkey and tray change it, so a delayed save from the UI
    // must not overwrite it with a stale value.
    config.click_through = load_config(&app)?.click_through;
    save_config(&app, &config)?;
    Ok(config)
}

fn overlay(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(OVERLAY)
}

fn current_window_state(window: &WebviewWindow) -> Option<WindowState> {
    // A minimized window reports a meaningless position (e.g. -32000 on Windows).
    if window.is_minimized().unwrap_or(false) {
        return None;
    }
    let position = window.outer_position().ok()?;
    let size = window.inner_size().ok()?;
    Some(WindowState {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    })
}

fn save_window_state(app: &AppHandle) {
    let Some(state) = overlay(app).and_then(|w| current_window_state(&w)) else {
        return;
    };
    let saved = app
        .store(STORE_FILE)
        .map_err(|e| e.to_string())
        .and_then(|store| {
            let value = serde_json::to_value(state).map_err(|e| e.to_string())?;
            store.set(WINDOW_KEY, value);
            Ok(())
        });
    if let Err(e) = saved {
        eprintln!("window position not saved: {e}");
    }
}

/// Puts the overlay back where it was, adjusted to the screens that exist now.
/// With nothing saved the window keeps the position from `tauri.conf.json`.
fn restore_window_state(app: &AppHandle) {
    let Some(window) = overlay(app) else {
        return;
    };
    let Some(saved) = app
        .store(STORE_FILE)
        .ok()
        .and_then(|store| store.get(WINDOW_KEY))
        .and_then(|v| serde_json::from_value::<WindowState>(v).ok())
    else {
        return;
    };
    // The work area excludes the menu bar, dock and taskbar.
    let monitors: Vec<WindowState> = window
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let area = m.work_area();
            WindowState {
                x: area.position.x,
                y: area.position.y,
                width: area.size.width,
                height: area.size.height,
            }
        })
        .collect();
    let fitted = saved.fit_to_monitors(&monitors);
    let applied = window
        .set_size(PhysicalSize::new(fitted.width, fitted.height))
        .and_then(|_| window.set_position(PhysicalPosition::new(fitted.x, fitted.y)));
    if let Err(e) = applied {
        eprintln!("window position not restored: {e}");
    }
}

/// Starts the thread that saves the geometry once the window has been still for `SAVE_DELAY`,
/// so dragging does not write on every pixel.
fn spawn_window_saver(app: AppHandle) -> mpsc::Sender<()> {
    let (tx, rx) = mpsc::channel::<()>();
    std::thread::spawn(move || {
        while rx.recv().is_ok() {
            loop {
                match rx.recv_timeout(SAVE_DELAY) {
                    Ok(()) => continue,
                    Err(RecvTimeoutError::Timeout) => break,
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
            save_window_state(&app);
        }
    });
    tx
}

fn toggle_overlay(app: &AppHandle) {
    if let Some(w) = overlay(app) {
        if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
        } else {
            let _ = w.show();
            let _ = w.set_focus();
        }
    }
}

/// Lets the mouse pass through the overlay to the app underneath, saves the choice,
/// and tells the tray and the UI so both show the same state.
fn apply_click_through(app: &AppHandle, enabled: bool) -> Result<(), String> {
    if let Some(w) = overlay(app) {
        w.set_ignore_cursor_events(enabled)
            .map_err(|e| e.to_string())?;
    }
    let mut config = load_config(app)?;
    config.click_through = enabled;
    save_config(app, &config)?;
    if let Some(menu) = app.try_state::<ClickThroughMenu>() {
        let _ = menu.0.set_checked(enabled);
    }
    let _ = app.emit(CLICK_THROUGH_EVENT, enabled);
    Ok(())
}

fn toggle_click_through(app: &AppHandle) {
    let current = load_config(app).map(|c| c.click_through).unwrap_or(false);
    if let Err(e) = apply_click_through(app, !current) {
        eprintln!("click-through not changed: {e}");
    }
}

#[tauri::command]
fn set_click_through(app: AppHandle, enabled: bool) -> Result<(), String> {
    apply_click_through(&app, enabled)
}

fn handle_hotkey(app: &AppHandle, action: HotkeyAction) {
    match action {
        HotkeyAction::ToggleVisibility => toggle_overlay(app),
        HotkeyAction::ToggleClickThrough => toggle_click_through(app),
        // Playback state lives in the UI, so it decides what to do with these.
        HotkeyAction::PlayPause | HotkeyAction::SpeedUp | HotkeyAction::SpeedDown => {
            let _ = app.emit(HOTKEY_EVENT, action);
        }
    }
}

/// Registers every default shortcut and returns a message for each one that failed.
/// A conflict must not stop the app: the remaining shortcuts still work.
fn register_hotkeys(app: &AppHandle) -> Vec<String> {
    let mut failures = Vec::new();
    for action in HotkeyAction::ALL {
        let accelerator = action.default_accelerator();
        let result = Shortcut::from_str(accelerator)
            .map_err(|e| e.to_string())
            .and_then(|shortcut| {
                app.global_shortcut()
                    .on_shortcut(shortcut, move |app, _, event| {
                        if event.state() == ShortcutState::Pressed {
                            handle_hotkey(app, action);
                        }
                    })
                    .map_err(|e| e.to_string())
            });
        if let Err(e) = result {
            failures.push(format!("{accelerator}: {e}"));
        }
    }
    failures
}

#[cfg(windows)]
fn capture_protection() -> CaptureProtection {
    offstage_core::protection_for_windows_build(windows_version::OsVersion::current().build)
}

#[cfg(target_os = "macos")]
fn capture_protection() -> CaptureProtection {
    // Whether the window is hidden depends on how the sharing app captures the screen.
    CaptureProtection::BestEffort
}

#[cfg(not(any(windows, target_os = "macos")))]
fn capture_protection() -> CaptureProtection {
    CaptureProtection::Unknown
}

#[tauri::command]
fn capture_protection_status() -> CaptureProtection {
    capture_protection()
}

#[tauri::command]
fn hotkey_failures(failures: State<HotkeyFailures>) -> Vec<String> {
    failures.0.clone()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .on_window_event(|window, event| {
            if window.label() == OVERLAY
                && matches!(event, WindowEvent::Moved(_) | WindowEvent::Resized(_))
            {
                if let Some(saver) = window.try_state::<WindowSaver>() {
                    let _ = saver.0.send(());
                }
            }
        })
        .setup(|app| {
            restore_window_state(app.handle());
            app.manage(WindowSaver(spawn_window_saver(app.handle().clone())));

            let scripts_dir = app.path().app_data_dir()?.join("scripts");
            app.manage(Scripts(Mutex::new(ScriptStore::new(scripts_dir))));
            app.manage(listening::Listening::default());
            seed_sample_script(app.handle(), &app.state::<Scripts>());

            // Belt and braces: config already sets contentProtected, enforce it at runtime too.
            if let Some(w) = overlay(app.handle()) {
                w.set_content_protected(true)?;
            }

            let failures = register_hotkeys(app.handle());
            for failure in &failures {
                eprintln!("global shortcut not registered: {failure}");
            }
            app.manage(HotkeyFailures(failures));

            // Restore the saved click-through state so it survives a restart.
            let click_through = load_config(app.handle())?.click_through;
            if click_through {
                if let Some(w) = overlay(app.handle()) {
                    w.set_ignore_cursor_events(true)?;
                }
            }

            let toggle = MenuItem::with_id(app, "toggle", "Ẩn / Hiện", true, None::<&str>)?;
            let through = CheckMenuItem::with_id(
                app,
                "click_through",
                "Chuột xuyên qua",
                true,
                click_through,
                None::<&str>,
            )?;
            let quit = MenuItem::with_id(app, "quit", "Thoát", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&toggle, &through, &quit])?;
            app.manage(ClickThroughMenu(through));

            let mut tray = TrayIconBuilder::new()
                .tooltip("Offstage")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "toggle" => toggle_overlay(app),
                    "click_through" => toggle_click_through(app),
                    "quit" => {
                        // `exit` skips the exit request, so save here too.
                        save_window_state(app);
                        app.exit(0);
                    }
                    _ => {}
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            update_config,
            set_click_through,
            hotkey_failures,
            capture_protection_status,
            list_scripts,
            create_script,
            read_script,
            save_script,
            rename_script,
            delete_script,
            import_script,
            listening::model_info,
            listening::download_model,
            listening::cancel_download,
            listening::start_listening,
            listening::stop_listening,
            listening::pipeline_status,
            permissions::audio_permissions,
            permissions::request_microphone_access,
            permissions::open_privacy_settings
        ])
        .build(tauri::generate_context!())
        .expect("error while building Offstage")
        .run(|app, event| {
            if let RunEvent::ExitRequested { .. } = event {
                save_window_state(app);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_accelerators_parse() {
        for action in HotkeyAction::ALL {
            let accelerator = action.default_accelerator();
            assert!(
                Shortcut::from_str(accelerator).is_ok(),
                "{accelerator} is not a valid accelerator"
            );
        }
    }
}
