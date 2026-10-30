//! Shared types: configuration, workspaces and app-wide events.

pub mod db;
pub mod knowledge;
pub mod play;
pub mod scripts;
pub mod settings;

use serde::{Deserialize, Serialize};

/// Persisted overlay settings. Missing fields fall back to defaults so files
/// written by older versions keep loading.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct OverlayConfig {
    pub font_size: u16,
    pub opacity: f32,
    /// Teleprompter speed in pixels per second.
    pub scroll_speed: f32,
    pub mirror: bool,
    /// Vertical position of the reading line as a fraction of the window height.
    pub reading_line: f32,
    /// When true the overlay ignores the mouse so clicks reach the app underneath.
    pub click_through: bool,
    /// Id of the script shown in the teleprompter, `None` until one is picked.
    pub active_script: Option<String>,
}

impl OverlayConfig {
    pub const FONT_SIZE: (u16, u16) = (16, 96);
    pub const OPACITY: (f32, f32) = (0.2, 1.0);
    pub const SCROLL_SPEED: (f32, f32) = (5.0, 300.0);
    pub const READING_LINE: (f32, f32) = (0.1, 0.9);

    /// Returns a copy with every value forced into its allowed range.
    /// Non-finite floats (NaN, infinity) fall back to the default.
    pub fn clamped(&self) -> Self {
        let d = Self::default();
        let fit = |v: f32, (lo, hi): (f32, f32), fallback: f32| {
            if v.is_finite() {
                v.clamp(lo, hi)
            } else {
                fallback
            }
        };
        Self {
            font_size: self.font_size.clamp(Self::FONT_SIZE.0, Self::FONT_SIZE.1),
            opacity: fit(self.opacity, Self::OPACITY, d.opacity),
            scroll_speed: fit(self.scroll_speed, Self::SCROLL_SPEED, d.scroll_speed),
            mirror: self.mirror,
            reading_line: fit(self.reading_line, Self::READING_LINE, d.reading_line),
            click_through: self.click_through,
            active_script: self.active_script.clone(),
        }
    }
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            font_size: 32,
            opacity: 0.85,
            scroll_speed: 40.0,
            mirror: false,
            reading_line: 0.38,
            click_through: false,
            active_script: None,
        }
    }
}

/// Window geometry in physical pixels, restored on the next launch.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct WindowState {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl WindowState {
    /// Keep in sync with `minWidth` / `minHeight` in `tauri.conf.json`.
    pub const MIN_WIDTH: u32 = 360;
    pub const MIN_HEIGHT: u32 = 140;

    fn overlap_area(&self, other: &WindowState) -> i64 {
        let left = i64::from(self.x.max(other.x));
        let top = i64::from(self.y.max(other.y));
        let right = (i64::from(self.x) + i64::from(self.width))
            .min(i64::from(other.x) + i64::from(other.width));
        let bottom = (i64::from(self.y) + i64::from(self.height))
            .min(i64::from(other.y) + i64::from(other.height));
        (right - left).max(0) * (bottom - top).max(0)
    }

    /// Makes a saved geometry safe to restore. `monitors` are the screen areas in
    /// physical pixels, described with the same fields as a window.
    ///
    /// The size is kept between the minimum and the monitor size. The position is left
    /// alone when at least half of the window is on a screen, since it is still easy to
    /// grab. Otherwise (a monitor was unplugged, a resolution changed, or the window had
    /// to shrink to fit) the window is pulled fully onto the monitor it overlaps most, or
    /// the first one if none.
    pub fn fit_to_monitors(self, monitors: &[WindowState]) -> WindowState {
        let Some(first) = monitors.first() else {
            return self;
        };
        let mut best = first;
        let mut best_overlap = self.overlap_area(first);
        for monitor in &monitors[1..] {
            let overlap = self.overlap_area(monitor);
            if overlap > best_overlap {
                best = monitor;
                best_overlap = overlap;
            }
        }

        let mut fitted = WindowState {
            width: self
                .width
                .clamp(Self::MIN_WIDTH.min(best.width), best.width),
            height: self
                .height
                .clamp(Self::MIN_HEIGHT.min(best.height), best.height),
            ..self
        };
        // A window that had to shrink was bigger than the screen, so it cannot keep its old
        // position and still fit.
        let shrunk = fitted.width < self.width || fitted.height < self.height;
        let area = i64::from(fitted.width) * i64::from(fitted.height);
        if !shrunk && fitted.overlap_area(best) * 2 >= area {
            return fitted;
        }
        fitted.x = clamp_axis(fitted.x, fitted.width, best.x, best.width);
        fitted.y = clamp_axis(fitted.y, fitted.height, best.y, best.height);
        fitted
    }
}

/// Moves `pos` so a segment of length `len` lies inside `[origin, origin + span]`.
/// Requires `len <= span`.
fn clamp_axis(pos: i32, len: u32, origin: i32, span: u32) -> i32 {
    let lo = i64::from(origin);
    let hi = lo + i64::from(span) - i64::from(len);
    // `hi >= lo` because the caller already limited `len` to `span`, so the result fits in i32.
    i64::from(pos).clamp(lo, hi) as i32
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    /// Free-form context the user writes for this session.
    pub context_note: String,
    /// Provider id used to answer; `None` means local model.
    pub provider_id: Option<String>,
    /// When true, nothing from this workspace may be sent to a cloud provider.
    pub local_only: bool,
}

/// How well the overlay is hidden from screen sharing on this system.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CaptureProtection {
    /// The window is left out of captures entirely.
    Full,
    /// The system only supports blacking the window out, so viewers see a black box.
    BlackBox,
    /// Depends on how the sharing app captures the screen. See `docs/COMPATIBILITY.md`.
    BestEffort,
    /// Could not be determined.
    Unknown,
}

/// First Windows 10 build (version 2004) that supports `WDA_EXCLUDEFROMCAPTURE`.
const WINDOWS_EXCLUDE_FROM_CAPTURE_BUILD: u32 = 19041;

/// Protection level for a Windows build number. Builds before 19041 can only use
/// `WDA_MONITOR`, which shows a black box in the capture. A build of 0 means the
/// number could not be read.
pub fn protection_for_windows_build(build: u32) -> CaptureProtection {
    match build {
        0 => CaptureProtection::Unknown,
        b if b >= WINDOWS_EXCLUDE_FROM_CAPTURE_BUILD => CaptureProtection::Full,
        _ => CaptureProtection::BlackBox,
    }
}

/// Actions a global hotkey can trigger. Serialized in snake_case as the payload
/// of the `hotkey` event the overlay listens to.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HotkeyAction {
    PlayPause,
    SpeedUp,
    SpeedDown,
    ToggleVisibility,
    ToggleClickThrough,
}

impl HotkeyAction {
    pub const ALL: [HotkeyAction; 5] = [
        Self::PlayPause,
        Self::SpeedUp,
        Self::SpeedDown,
        Self::ToggleVisibility,
        Self::ToggleClickThrough,
    ];

    /// Default shortcut in Tauri accelerator syntax. Cmd on macOS, Ctrl elsewhere.
    pub fn default_accelerator(self) -> &'static str {
        match self {
            Self::PlayPause => "CommandOrControl+Alt+Space",
            Self::SpeedUp => "CommandOrControl+Alt+Up",
            Self::SpeedDown => "CommandOrControl+Alt+Down",
            Self::ToggleVisibility => "CommandOrControl+Alt+O",
            // Not C: Cmd+Alt+C opens "Inspect element" in Chrome and would be stolen globally.
            Self::ToggleClickThrough => "CommandOrControl+Alt+T",
        }
    }
}

/// Events emitted from the Rust side to the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppEvent {
    Transcript {
        speaker: Speaker,
        text: String,
        is_final: bool,
    },
    QuestionDetected {
        text: String,
    },
    AnswerToken {
        token: String,
    },
    AnswerDone {
        sources: Vec<String>,
    },
    Error {
        message: String,
    },
    /// A model download advanced. `downloaded` counts bytes of the file so far.
    ModelProgress {
        id: String,
        downloaded: u64,
        total: u64,
    },
}

impl AppEvent {
    /// Name of the Tauri event this is sent under. Matches the `type` tag in the payload.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Transcript { .. } => "transcript",
            Self::QuestionDetected { .. } => "question_detected",
            Self::AnswerToken { .. } => "answer_token",
            Self::AnswerDone { .. } => "answer_done",
            Self::Error { .. } => "error",
            Self::ModelProgress { .. } => "model_progress",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Speaker {
    /// Microphone input.
    Me,
    /// System audio loopback (the other side of the call).
    Remote,
}

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("config error: {0}")]
    Config(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_round_trips_through_json() {
        let cfg = OverlayConfig {
            font_size: 48,
            opacity: 0.5,
            scroll_speed: 90.0,
            mirror: true,
            reading_line: 0.6,
            click_through: true,
            active_script: Some("s3e8".into()),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert_eq!(serde_json::from_str::<OverlayConfig>(&json).unwrap(), cfg);
    }

    #[test]
    fn missing_fields_use_defaults() {
        let old = r#"{"font_size":40,"mirror":true}"#;
        let cfg: OverlayConfig = serde_json::from_str(old).unwrap();
        assert_eq!(cfg.font_size, 40);
        assert!(cfg.mirror);
        assert_eq!(cfg.scroll_speed, OverlayConfig::default().scroll_speed);
        assert_eq!(cfg.reading_line, OverlayConfig::default().reading_line);
        assert!(!cfg.click_through);
    }

    #[test]
    fn clamped_forces_values_into_range() {
        let cfg = OverlayConfig {
            font_size: 500,
            opacity: -1.0,
            scroll_speed: 9999.0,
            mirror: false,
            reading_line: 2.0,
            click_through: false,
            active_script: None,
        }
        .clamped();
        assert_eq!(cfg.font_size, OverlayConfig::FONT_SIZE.1);
        assert_eq!(cfg.opacity, OverlayConfig::OPACITY.0);
        assert_eq!(cfg.scroll_speed, OverlayConfig::SCROLL_SPEED.1);
        assert_eq!(cfg.reading_line, OverlayConfig::READING_LINE.1);
    }

    #[test]
    fn clamped_replaces_non_finite_with_default() {
        let cfg = OverlayConfig {
            opacity: f32::NAN,
            scroll_speed: f32::INFINITY,
            ..OverlayConfig::default()
        }
        .clamped();
        assert_eq!(cfg.opacity, OverlayConfig::default().opacity);
        // Infinity is not finite, so it takes the default rather than the upper bound.
        assert_eq!(cfg.scroll_speed, OverlayConfig::default().scroll_speed);
    }

    #[test]
    fn clamped_keeps_valid_config_unchanged() {
        let cfg = OverlayConfig::default();
        assert_eq!(cfg.clamped(), cfg);
    }

    fn area(x: i32, y: i32, width: u32, height: u32) -> WindowState {
        WindowState {
            x,
            y,
            width,
            height,
        }
    }

    const SCREEN: WindowState = WindowState {
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
    };

    #[test]
    fn window_inside_a_monitor_is_unchanged() {
        let w = area(100, 100, 720, 260);
        assert_eq!(w.fit_to_monitors(&[SCREEN]), w);
    }

    #[test]
    fn window_on_an_unplugged_monitor_moves_onto_the_remaining_one() {
        let w = area(2500, 100, 720, 260);
        assert_eq!(w.fit_to_monitors(&[SCREEN]), area(1200, 100, 720, 260));
    }

    #[test]
    fn window_mostly_visible_keeps_its_position() {
        // 520 of 720 px are on screen.
        let w = area(-200, 100, 720, 260);
        assert_eq!(w.fit_to_monitors(&[SCREEN]), w);
    }

    #[test]
    fn window_mostly_offscreen_is_pulled_in() {
        // Only 120 of 720 px are on screen.
        let w = area(-600, 100, 720, 260);
        assert_eq!(w.fit_to_monitors(&[SCREEN]), area(0, 100, 720, 260));
    }

    #[test]
    fn window_larger_than_the_monitor_is_shrunk() {
        let w = area(50, 50, 3000, 2000);
        assert_eq!(w.fit_to_monitors(&[SCREEN]), area(0, 0, 1920, 1080));
    }

    #[test]
    fn window_below_the_minimum_size_is_grown() {
        let w = area(100, 100, 100, 50);
        let fitted = w.fit_to_monitors(&[SCREEN]);
        assert_eq!(fitted.width, WindowState::MIN_WIDTH);
        assert_eq!(fitted.height, WindowState::MIN_HEIGHT);
    }

    #[test]
    fn no_monitors_leaves_the_window_unchanged() {
        let w = area(5000, 5000, 720, 260);
        assert_eq!(w.fit_to_monitors(&[]), w);
    }

    #[test]
    fn partly_visible_window_goes_to_the_monitor_it_overlaps_most() {
        let second = area(1920, 0, 1920, 1080);
        // Less than half visible, and the visible part is on the second monitor.
        let w = area(3500, 900, 720, 260);
        assert_eq!(
            w.fit_to_monitors(&[SCREEN, second]),
            area(3120, 820, 720, 260)
        );
    }

    #[test]
    fn window_with_no_overlap_goes_to_the_first_monitor() {
        let second = area(1920, 0, 1920, 1080);
        let w = area(9000, 9000, 720, 260);
        assert_eq!(
            w.fit_to_monitors(&[SCREEN, second]),
            area(1200, 820, 720, 260)
        );
    }

    #[test]
    fn monitor_left_of_the_origin_is_handled() {
        let left = area(-1920, 0, 1920, 1080);
        // 140 of 720 px overlap the left monitor, so it is pulled onto that one.
        let w = area(-2500, 10, 720, 260);
        assert_eq!(
            w.fit_to_monitors(&[SCREEN, left]),
            area(-1920, 10, 720, 260)
        );
    }

    #[test]
    fn windows_builds_from_2004_hide_the_window_fully() {
        // 19041 is Windows 10 2004, the first build that supports it.
        assert_eq!(protection_for_windows_build(19041), CaptureProtection::Full);
        assert_eq!(protection_for_windows_build(19045), CaptureProtection::Full);
        // Windows 11 starts at 22000.
        assert_eq!(protection_for_windows_build(22000), CaptureProtection::Full);
        assert_eq!(protection_for_windows_build(26100), CaptureProtection::Full);
    }

    #[test]
    fn older_windows_builds_show_a_black_box() {
        assert_eq!(
            protection_for_windows_build(19040),
            CaptureProtection::BlackBox
        );
        // Windows 10 1909 and 1809.
        assert_eq!(
            protection_for_windows_build(18363),
            CaptureProtection::BlackBox
        );
        assert_eq!(
            protection_for_windows_build(17763),
            CaptureProtection::BlackBox
        );
    }

    #[test]
    fn an_unreadable_windows_build_is_unknown_not_a_warning() {
        assert_eq!(protection_for_windows_build(0), CaptureProtection::Unknown);
    }

    #[test]
    fn capture_protection_serializes_as_snake_case() {
        let json = serde_json::to_string(&CaptureProtection::BlackBox).unwrap();
        assert_eq!(json, r#""black_box""#);
        let json = serde_json::to_string(&CaptureProtection::BestEffort).unwrap();
        assert_eq!(json, r#""best_effort""#);
    }

    #[test]
    fn hotkey_actions_serialize_as_snake_case() {
        let json = serde_json::to_string(&HotkeyAction::ToggleVisibility).unwrap();
        assert_eq!(json, r#""toggle_visibility""#);
        let back: HotkeyAction = serde_json::from_str(r#""play_pause""#).unwrap();
        assert_eq!(back, HotkeyAction::PlayPause);
        let json = serde_json::to_string(&HotkeyAction::ToggleClickThrough).unwrap();
        assert_eq!(json, r#""toggle_click_through""#);
    }

    #[test]
    fn default_accelerators_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for action in HotkeyAction::ALL {
            assert!(
                seen.insert(action.default_accelerator()),
                "duplicate accelerator for {action:?}"
            );
        }
    }

    #[test]
    fn event_names_match_the_type_tag_in_the_payload() {
        let events = [
            AppEvent::Transcript {
                speaker: Speaker::Me,
                text: "hi".into(),
                is_final: true,
            },
            AppEvent::QuestionDetected {
                text: "why?".into(),
            },
            AppEvent::AnswerToken { token: "a".into() },
            AppEvent::AnswerDone { sources: vec![] },
            AppEvent::Error {
                message: "oops".into(),
            },
            AppEvent::ModelProgress {
                id: "m".into(),
                downloaded: 1,
                total: 2,
            },
        ];
        for event in &events {
            let json: serde_json::Value = serde_json::to_value(event).unwrap();
            assert_eq!(json["type"], event.name(), "{event:?}");
        }
    }

    #[test]
    fn transcript_event_carries_speaker_and_text() {
        let json = serde_json::to_string(&AppEvent::Transcript {
            speaker: Speaker::Remote,
            text: "xin chào".into(),
            is_final: true,
        })
        .unwrap();
        assert_eq!(
            json,
            r#"{"type":"transcript","speaker":"remote","text":"xin chào","is_final":true}"#
        );
    }

    #[test]
    fn events_serialize_with_type_tag() {
        let e = AppEvent::AnswerToken { token: "hi".into() };
        let json = serde_json::to_string(&e).unwrap();
        assert_eq!(json, r#"{"type":"answer_token","token":"hi"}"#);
    }
}
