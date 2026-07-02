//! App settings: a RON file in the per-platform config directory,
//! loaded at startup and written on exit and on explicit apply.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tomtemacro_core::clicker::ClickTarget;
use tomtemacro_core::model::MouseButton;

pub const SETTINGS_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    pub hotkeys: HotkeySettings,
    pub clicker: ClickerSettings,
    pub playback: PlaybackSettings,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HotkeySettings {
    /// Key names as shown in the UI — currently F1–F12.
    pub toggle_clicker: String,
    pub toggle_record: String,
    pub play_macro: String,
    pub stop_all: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClickerSettings {
    pub interval_ms: u64,
    pub target: ClickTarget,
    pub double: bool,
    pub jitter_enabled: bool,
    pub jitter_frac: f32,
    pub jitter_px: u16,
    pub limit_enabled: bool,
    pub limit: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlaybackSettings {
    pub speed: f64,
    pub repeat_times: u32,
    pub repeat_infinite: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            hotkeys: HotkeySettings::default(),
            clicker: ClickerSettings::default(),
            playback: PlaybackSettings::default(),
        }
    }
}

impl Default for HotkeySettings {
    fn default() -> Self {
        Self {
            toggle_clicker: "F6".into(),
            toggle_record: "F7".into(),
            play_macro: "F8".into(),
            stop_all: "F9".into(),
        }
    }
}

impl Default for ClickerSettings {
    fn default() -> Self {
        Self {
            interval_ms: 100,
            target: ClickTarget::Button(MouseButton::Left),
            double: false,
            jitter_enabled: false,
            jitter_frac: 0.10,
            jitter_px: 3,
            limit_enabled: false,
            limit: 100,
        }
    }
}

impl Default for PlaybackSettings {
    fn default() -> Self {
        Self {
            speed: 1.0,
            repeat_times: 1,
            repeat_infinite: false,
        }
    }
}

fn settings_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "tomtemacro")
        .map(|dirs| dirs.config_dir().join("settings.ron"))
}

/// Missing or unreadable settings fall back to defaults (logged, not fatal).
pub fn load() -> Settings {
    let Some(path) = settings_path() else {
        return Settings::default();
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => match ron::from_str(&text) {
            Ok(settings) => settings,
            Err(e) => {
                log::warn!("ignoring malformed {}: {e}", path.display());
                Settings::default()
            }
        },
        Err(_) => Settings::default(), // usually: first run
    }
}

pub fn save(settings: &Settings) {
    let Some(path) = settings_path() else { return };
    let Ok(body) = ron::ser::to_string_pretty(settings, ron::ser::PrettyConfig::default()) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(&path, body) {
        log::warn!("could not save settings to {}: {e}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A settings.ron written before `ClickerSettings::target` existed must
    /// still load: the removed `button` field is ignored, `target` defaults.
    #[test]
    fn pre_key_target_settings_still_load() {
        let old = r#"(
            version: 1,
            clicker: (interval_ms: 250, button: Right, double: true),
        )"#;
        let settings: Settings = ron::from_str(old).expect("old settings must parse");
        assert_eq!(settings.clicker.interval_ms, 250);
        assert!(settings.clicker.double);
        assert_eq!(
            settings.clicker.target,
            ClickTarget::Button(MouseButton::Left)
        );
    }
}
