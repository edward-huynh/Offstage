//! Audio permissions. macOS asks the user before an app may record, and a refusal is silent:
//! the audio just never arrives. So the app checks first, asks when it can, and sends the user
//! to the right System Settings page when it cannot.
//!
//! The microphone permission can be read and requested. The system audio permission cannot be
//! checked ahead of time (macOS has no public call for it), so it shows as `unknown` until a
//! start attempt fails. Windows needs no permission for loopback capture.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    Granted,
    Denied,
    /// The user has not been asked yet. Starting capture, or `request_microphone_access`, asks.
    NotDetermined,
    Unknown,
}

#[derive(Debug, Serialize)]
pub struct Permissions {
    pub microphone: Access,
    pub system_audio: Access,
}

/// A privacy page the user can be sent to. Fixed set, so no caller-supplied text reaches the OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyPane {
    Microphone,
    SystemAudio,
}

/// Maps the raw `AVAuthorizationStatus` number: 0 not determined, 1 restricted, 2 denied,
/// 3 authorized. "Restricted" means a policy forbids it, which the user cannot change from the
/// prompt, so it is reported as denied.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn access_from_status(status: isize) -> Access {
    match status {
        0 => Access::NotDetermined,
        1 | 2 => Access::Denied,
        3 => Access::Granted,
        _ => Access::Unknown,
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::sync::mpsc;

    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_av_foundation::{AVCaptureDevice, AVMediaTypeAudio};

    use super::{access_from_status, Access, PrivacyPane};

    pub fn microphone() -> Access {
        // SAFETY: `AVMediaTypeAudio` is a constant string AVFoundation provides, and the call
        // only reads the current permission.
        let Some(audio) = (unsafe { AVMediaTypeAudio }) else {
            return Access::Unknown;
        };
        let status = unsafe { AVCaptureDevice::authorizationStatusForMediaType(audio) };
        access_from_status(status.0)
    }

    /// Asks for the microphone and waits for the answer. A decision made earlier is returned
    /// at once, without a prompt.
    pub fn request_microphone() -> Access {
        let Some(audio) = (unsafe { AVMediaTypeAudio }) else {
            return Access::Unknown;
        };
        let (tx, rx) = mpsc::channel();
        let handler = RcBlock::new(move |granted: Bool| {
            let _ = tx.send(granted.as_bool());
        });
        // SAFETY: the block lives until the call returns, and AVFoundation copies it if it
        // needs it longer. It is called once, on an arbitrary queue, which `Sender` allows.
        unsafe { AVCaptureDevice::requestAccessForMediaType_completionHandler(audio, &handler) };
        match rx.recv() {
            Ok(true) => Access::Granted,
            Ok(false) => Access::Denied,
            Err(_) => Access::Unknown,
        }
    }

    pub fn system_audio() -> Access {
        Access::Unknown
    }

    pub fn settings_url(pane: PrivacyPane) -> &'static str {
        match pane {
            PrivacyPane::Microphone => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
            }
            // System audio recording lives on the "Screen & System Audio Recording" page.
            PrivacyPane::SystemAudio => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
            }
        }
    }

    pub fn open(url: &str) -> std::io::Result<()> {
        std::process::Command::new("open")
            .arg(url)
            .status()
            .map(|_| ())
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::{Access, PrivacyPane};

    pub fn microphone() -> Access {
        Access::Unknown
    }

    pub fn request_microphone() -> Access {
        Access::Unknown
    }

    pub fn system_audio() -> Access {
        Access::Unknown
    }

    pub fn settings_url(pane: PrivacyPane) -> &'static str {
        match pane {
            PrivacyPane::Microphone => "ms-settings:privacy-microphone",
            // Windows loopback capture needs no permission, so there is no page for it.
            PrivacyPane::SystemAudio => "",
        }
    }

    #[cfg(windows)]
    pub fn open(url: &str) -> std::io::Result<()> {
        // `start` hands the ms-settings link to the shell. The empty title is required when
        // the next argument could be quoted.
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .status()
            .map(|_| ())
    }

    #[cfg(not(windows))]
    pub fn open(_url: &str) -> std::io::Result<()> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "opening system settings is not supported on this system",
        ))
    }
}

pub fn microphone_access() -> Access {
    platform::microphone()
}

/// Asks for the microphone if the user has not decided yet. Blocks until they answer.
pub fn request_microphone() -> Access {
    platform::request_microphone()
}

#[tauri::command]
pub fn audio_permissions() -> Permissions {
    Permissions {
        microphone: platform::microphone(),
        system_audio: platform::system_audio(),
    }
}

/// Asks for the microphone and resolves with the user's answer. Off the main thread, because
/// the prompt waits for a person.
#[tauri::command]
pub async fn request_microphone_access() -> Result<Access, String> {
    tauri::async_runtime::spawn_blocking(platform::request_microphone)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_privacy_settings(pane: PrivacyPane) -> Result<(), String> {
    let url = platform::settings_url(pane);
    if url.is_empty() {
        return Err("this system needs no permission for that source".to_owned());
    }
    platform::open(url).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorization_status_numbers_map_to_access() {
        assert_eq!(access_from_status(0), Access::NotDetermined);
        assert_eq!(access_from_status(1), Access::Denied);
        assert_eq!(access_from_status(2), Access::Denied);
        assert_eq!(access_from_status(3), Access::Granted);
        assert_eq!(access_from_status(99), Access::Unknown);
        assert_eq!(access_from_status(-1), Access::Unknown);
    }

    #[test]
    fn access_serializes_in_snake_case() {
        assert_eq!(
            serde_json::to_string(&Access::NotDetermined).unwrap(),
            r#""not_determined""#
        );
    }

    #[test]
    fn privacy_panes_deserialize_from_snake_case() {
        let pane: PrivacyPane = serde_json::from_str(r#""system_audio""#).unwrap();
        assert_eq!(pane, PrivacyPane::SystemAudio);
        assert!(serde_json::from_str::<PrivacyPane>(r#""../etc""#).is_err());
    }

    /// Only reads the state, so it never opens a prompt. Never call `request_microphone` from a
    /// test binary: without a usage description in its Info.plist, macOS ends the process.
    #[test]
    #[ignore = "reads the real microphone permission of the process running the tests"]
    fn the_real_microphone_permission_can_be_read() {
        let access = microphone_access();
        println!("microphone permission for this process: {access:?}");
        #[cfg(target_os = "macos")]
        assert_ne!(
            access,
            Access::Unknown,
            "AVFoundation returned an unknown status"
        );
    }

    #[test]
    fn settings_urls_are_fixed_links_to_the_right_page() {
        let mic = platform::settings_url(PrivacyPane::Microphone);
        assert!(
            mic.contains("Microphone") || mic.contains("microphone"),
            "{mic}"
        );
        // Only the fixed links above can ever reach the OS.
        assert!(!mic.contains(' '));
    }
}
