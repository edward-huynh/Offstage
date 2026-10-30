//! App-wide settings that are not about the teleprompter look (that is `OverlayConfig`):
//! speech language, which sources an interview listens to, and screen-share hiding.

use serde::{Deserialize, Serialize};

/// Language handed to the speech engine.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpeechLanguage {
    /// Let the model detect the language on every utterance.
    #[default]
    Auto,
    Vietnamese,
    English,
}

impl SpeechLanguage {
    /// Code understood by `offstage_stt::Language`: `auto`, `vi` or `en`.
    pub fn code(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Vietnamese => "vi",
            Self::English => "en",
        }
    }
}

/// Persisted app settings. Missing fields fall back to defaults so files written by
/// older versions keep loading.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct AppSettings {
    pub speech_language: SpeechLanguage,
    /// Listen to the system audio (the other side of the call) when an interview starts.
    pub interview_system_audio: bool,
    /// Listen to the microphone when an interview starts.
    pub interview_microphone: bool,
    /// Hide the main window from screen shares. It shows CV and JD text.
    pub hide_main_from_capture: bool,
}

impl AppSettings {
    /// Returns a copy that is safe to use. An interview that listens to nothing would show
    /// empty captions, so the system audio is turned back on in that case.
    pub fn clamped(&self) -> Self {
        let mut settings = *self;
        if !settings.interview_system_audio && !settings.interview_microphone {
            settings.interview_system_audio = true;
        }
        settings
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            speech_language: SpeechLanguage::Auto,
            interview_system_audio: true,
            interview_microphone: false,
            hide_main_from_capture: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_listen_to_the_system_audio_only_and_hide_the_window() {
        let d = AppSettings::default();
        assert_eq!(d.speech_language, SpeechLanguage::Auto);
        assert!(d.interview_system_audio);
        assert!(!d.interview_microphone);
        assert!(d.hide_main_from_capture);
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let s: AppSettings = serde_json::from_str(r#"{"speech_language":"vietnamese"}"#).unwrap();
        assert_eq!(s.speech_language, SpeechLanguage::Vietnamese);
        assert_eq!(
            s.interview_system_audio,
            AppSettings::default().interview_system_audio
        );
        assert!(s.hide_main_from_capture);
    }

    #[test]
    fn an_empty_object_is_the_defaults() {
        let s: AppSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(s, AppSettings::default());
    }

    #[test]
    fn serde_round_trip() {
        let s = AppSettings {
            speech_language: SpeechLanguage::English,
            interview_system_audio: false,
            interview_microphone: true,
            hide_main_from_capture: false,
        };
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<AppSettings>(&json).unwrap(), s);
    }

    #[test]
    fn language_uses_snake_case_names_on_the_wire() {
        assert_eq!(
            serde_json::to_string(&SpeechLanguage::Vietnamese).unwrap(),
            "\"vietnamese\""
        );
        assert!(serde_json::from_str::<SpeechLanguage>("\"french\"").is_err());
    }

    #[test]
    fn language_codes_match_the_speech_crate() {
        assert_eq!(SpeechLanguage::Auto.code(), "auto");
        assert_eq!(SpeechLanguage::Vietnamese.code(), "vi");
        assert_eq!(SpeechLanguage::English.code(), "en");
    }

    #[test]
    fn clamped_never_leaves_an_interview_with_no_source() {
        let none = AppSettings {
            interview_system_audio: false,
            interview_microphone: false,
            ..AppSettings::default()
        };
        let fixed = none.clamped();
        assert!(fixed.interview_system_audio);
        assert!(!fixed.interview_microphone);
    }

    #[test]
    fn clamped_keeps_a_valid_choice() {
        let mic_only = AppSettings {
            interview_system_audio: false,
            interview_microphone: true,
            ..AppSettings::default()
        };
        assert_eq!(mic_only.clamped(), mic_only);
    }
}
