//! Global shortcuts: the defaults, the ones the user changed, and (re)registering them with the
//! system. They work when Offstage is not in front, so a bad choice would take a key away from
//! every other app: [`check`] refuses a shortcut that would.

use std::{str::FromStr, sync::Mutex};

use offstage_core::{HotkeyAction, HotkeyOverrides};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

use crate::{handle_hotkey, storage};

/// Where the user's changes are kept. Only the shortcuts that differ from the default.
const OVERRIDES_KEY: &str = "hotkey_overrides";

/// Shortcuts that could not be registered, e.g. because another app already owns them.
/// The setup runs before the webview exists, so the UI pulls this instead of waiting for an event.
#[derive(Default)]
pub struct HotkeyFailures(pub Mutex<Vec<String>>);

/// The user's changes. An unreadable value is no changes: the defaults are always safe.
fn overrides(app: &AppHandle) -> HotkeyOverrides {
    storage::get::<HotkeyOverrides>(app, OVERRIDES_KEY)
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// Whether `accelerator` may be the shortcut of `action`: it parses, it has a modifier other than
/// Shift (a global shortcut of a bare letter would swallow that key in every app), and no other
/// action already uses it. Returns the parsed shortcut. The messages are shown to the user.
fn check(
    action: HotkeyAction,
    accelerator: &str,
    overrides: &HotkeyOverrides,
) -> Result<Shortcut, String> {
    let shortcut =
        Shortcut::from_str(accelerator).map_err(|_| "Không hiểu tổ hợp phím này".to_owned())?;
    if !shortcut
        .mods
        .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER)
    {
        return Err("Tổ hợp phím cần có Ctrl, Alt hoặc ⌘ (Super) đi kèm".to_owned());
    }
    let taken = HotkeyAction::ALL
        .into_iter()
        .filter(|&other| other != action)
        .filter_map(|other| Shortcut::from_str(&other.accelerator(overrides)).ok())
        .any(|other| other.id() == shortcut.id());
    if taken {
        return Err("Tổ hợp phím này đã dùng cho một hành động khác".to_owned());
    }
    Ok(shortcut)
}

/// Registers every shortcut in use, after dropping the ones registered before, and returns a
/// message for each that failed. A conflict must not stop the app: the others still work.
pub fn apply(app: &AppHandle) -> Vec<String> {
    let shortcuts = app.global_shortcut();
    // Nothing registered yet is not an error.
    let _ = shortcuts.unregister_all();
    let overrides = overrides(app);
    let mut failures = Vec::new();
    for action in HotkeyAction::ALL {
        let accelerator = action.accelerator(&overrides);
        let result = Shortcut::from_str(&accelerator)
            .map_err(|e| e.to_string())
            .and_then(|shortcut| {
                shortcuts
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
    if let Some(state) = app.try_state::<HotkeyFailures>() {
        if let Ok(mut current) = state.0.lock() {
            current.clone_from(&failures);
        }
    }
    failures
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct HotkeyBinding {
    action: HotkeyAction,
    accelerator: String,
    /// The shortcut is the default one, so there is nothing to reset.
    is_default: bool,
}

fn bindings_of(overrides: &HotkeyOverrides) -> Vec<HotkeyBinding> {
    HotkeyAction::ALL
        .iter()
        .map(|&action| {
            let accelerator = action.accelerator(overrides);
            HotkeyBinding {
                action,
                is_default: accelerator == action.default_accelerator(),
                accelerator,
            }
        })
        .collect()
}

/// Every shortcut in use, for the Settings screen and the overlay.
#[tauri::command]
pub fn hotkey_bindings(app: AppHandle) -> Vec<HotkeyBinding> {
    bindings_of(&overrides(&app))
}

/// Shortcuts that could not be registered.
#[tauri::command]
pub fn hotkey_failures(failures: State<HotkeyFailures>) -> Vec<String> {
    failures.0.lock().map(|f| f.clone()).unwrap_or_default()
}

/// Saves `overrides` and registers the shortcuts they give.
fn store(app: &AppHandle, overrides: &HotkeyOverrides) -> Result<Vec<String>, String> {
    storage::set(app, OVERRIDES_KEY, overrides)?;
    Ok(apply(app))
}

/// Gives `action` another shortcut. If the system refuses it (another app owns it), the old one is
/// put back and the error says so, so a change never leaves an action without a shortcut.
#[tauri::command]
pub fn set_hotkey(
    app: AppHandle,
    action: HotkeyAction,
    accelerator: String,
) -> Result<Vec<HotkeyBinding>, String> {
    let before = overrides(&app);
    check(action, &accelerator, &before)?;
    let mut after = before.clone();
    if accelerator == action.default_accelerator() {
        after.remove(&action);
    } else {
        after.insert(action, accelerator.clone());
    }
    let failures = store(&app, &after)?;
    if failures
        .iter()
        .any(|f| f.starts_with(&format!("{accelerator}:")))
    {
        store(&app, &before)?;
        return Err(
            "Không đăng ký được tổ hợp phím này: có thể một ứng dụng khác đang dùng nó".to_owned(),
        );
    }
    Ok(bindings_of(&after))
}

/// Puts `action` back on its default shortcut.
#[tauri::command]
pub fn reset_hotkey(app: AppHandle, action: HotkeyAction) -> Result<Vec<HotkeyBinding>, String> {
    let mut current = overrides(&app);
    current.remove(&action);
    store(&app, &current)?;
    Ok(bindings_of(&current))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(action: HotkeyAction, accelerator: &str) -> HotkeyOverrides {
        let mut o = HotkeyOverrides::new();
        o.insert(action, accelerator.to_owned());
        o
    }

    #[test]
    fn every_default_is_a_valid_shortcut_and_passes_the_same_rules_as_a_chosen_one() {
        let none = HotkeyOverrides::new();
        for action in HotkeyAction::ALL {
            let accelerator = action.default_accelerator();
            assert!(
                check(action, accelerator, &none).is_ok(),
                "{accelerator} is not accepted for {action:?}"
            );
        }
    }

    #[test]
    fn a_normal_combination_is_accepted() {
        let none = HotkeyOverrides::new();
        assert!(check(HotkeyAction::PlayPause, "Control+Shift+P", &none).is_ok());
        assert!(check(HotkeyAction::SpeedUp, "Alt+F5", &none).is_ok());
        assert!(check(HotkeyAction::PlayPause, "CommandOrControl+Alt+Space", &none).is_ok());
    }

    #[test]
    fn something_that_is_not_a_shortcut_is_refused() {
        let none = HotkeyOverrides::new();
        for bad in ["", "Control+", "Control+Shift", "Hyper+P", "nonsense"] {
            assert!(
                check(HotkeyAction::PlayPause, bad, &none).is_err(),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn a_key_alone_or_with_only_shift_is_refused_because_it_would_take_the_key_from_every_app() {
        let none = HotkeyOverrides::new();
        for bad in ["P", "F5", "Shift+P", "Space"] {
            let err = check(HotkeyAction::PlayPause, bad, &none).unwrap_err();
            assert!(err.contains("Ctrl, Alt"), "{bad:?}: {err}");
        }
    }

    #[test]
    fn a_combination_another_action_uses_is_refused_but_its_own_is_not() {
        let none = HotkeyOverrides::new();
        // The default of the speed-up action.
        let err = check(HotkeyAction::PlayPause, "CommandOrControl+Alt+Up", &none).unwrap_err();
        assert!(err.contains("đã dùng"));
        assert!(check(HotkeyAction::SpeedUp, "CommandOrControl+Alt+Up", &none).is_ok());
        // The same combination written in another order or case is the same combination.
        assert!(check(HotkeyAction::PlayPause, "alt+commandorcontrol+up", &none).is_err());
    }

    #[test]
    fn a_changed_shortcut_frees_its_old_combination_and_takes_the_new_one() {
        let changed = with(HotkeyAction::SpeedUp, "Control+Alt+F9");
        // The old default of speed-up is free now.
        assert!(check(HotkeyAction::PlayPause, "CommandOrControl+Alt+Up", &changed).is_ok());
        // The new one is taken.
        assert!(check(HotkeyAction::PlayPause, "Control+Alt+F9", &changed).is_err());
    }

    #[test]
    fn bindings_show_the_changed_ones_and_say_which_are_default() {
        let bindings = bindings_of(&with(HotkeyAction::PlayPause, "Control+Shift+P"));
        assert_eq!(bindings.len(), HotkeyAction::ALL.len());
        let play = bindings
            .iter()
            .find(|b| b.action == HotkeyAction::PlayPause)
            .unwrap();
        assert_eq!(
            (play.accelerator.as_str(), play.is_default),
            ("Control+Shift+P", false)
        );
        assert!(bindings
            .iter()
            .filter(|b| b.action != HotkeyAction::PlayPause)
            .all(|b| b.is_default));
    }
}
