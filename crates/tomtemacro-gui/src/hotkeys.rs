//! Global hotkeys: OS-registered so the chords are consumed (they never
//! leak into the focused app) and they keep working while the capture gate
//! is closed during playback. Bindings are configurable (F1–F12) and can be
//! re-registered live from the Settings tab.

use global_hotkey::hotkey::{Code, HotKey};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use tomtemacro_core::model::Key;

use crate::settings::HotkeySettings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    ToggleClicker,
    ToggleRecord,
    PlayMacro,
    StopAll,
}

pub const KEY_CHOICES: [&str; 12] = [
    "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12",
];

fn parse_key(name: &str) -> Option<(Code, Key)> {
    Some(match name {
        "F1" => (Code::F1, Key::F1),
        "F2" => (Code::F2, Key::F2),
        "F3" => (Code::F3, Key::F3),
        "F4" => (Code::F4, Key::F4),
        "F5" => (Code::F5, Key::F5),
        "F6" => (Code::F6, Key::F6),
        "F7" => (Code::F7, Key::F7),
        "F8" => (Code::F8, Key::F8),
        "F9" => (Code::F9, Key::F9),
        "F10" => (Code::F10, Key::F10),
        "F11" => (Code::F11, Key::F11),
        "F12" => (Code::F12, Key::F12),
        _ => return None,
    })
}

struct Binding {
    action: Action,
    hotkey: HotKey,
    /// Core-model key stripped from recordings (belt-and-braces; the OS
    /// registration usually consumes the chord before capture sees it).
    strip_key: Key,
    label: String,
}

pub struct Hotkeys {
    manager: GlobalHotKeyManager,
    bindings: Vec<Binding>,
}

impl Hotkeys {
    pub fn register(config: &HotkeySettings) -> Result<Self, String> {
        let manager = GlobalHotKeyManager::new().map_err(|e| e.to_string())?;
        let mut hotkeys = Self {
            manager,
            bindings: Vec::new(),
        };
        hotkeys.rebind(config)?;
        Ok(hotkeys)
    }

    /// Swap all bindings. On any failure the old chords are already
    /// unregistered — the error tells the user which chord was refused.
    pub fn rebind(&mut self, config: &HotkeySettings) -> Result<(), String> {
        for binding in self.bindings.drain(..) {
            let _ = self.manager.unregister(binding.hotkey);
        }
        let wanted = [
            (Action::ToggleClicker, &config.toggle_clicker),
            (Action::ToggleRecord, &config.toggle_record),
            (Action::PlayMacro, &config.play_macro),
            (Action::StopAll, &config.stop_all),
        ];
        for (action, name) in wanted {
            let (code, strip_key) =
                parse_key(name).ok_or_else(|| format!("unknown key '{name}'"))?;
            let hotkey = HotKey::new(None, code);
            self.manager
                .register(hotkey)
                .map_err(|e| format!("could not grab {name}: {e}"))?;
            self.bindings.push(Binding {
                action,
                hotkey,
                strip_key,
                label: name.clone(),
            });
        }
        Ok(())
    }

    /// Non-blocking: all actions whose chords were pressed since last frame.
    pub fn pressed(&self) -> Vec<Action> {
        let mut actions = Vec::new();
        while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if event.state() != HotKeyState::Pressed {
                continue;
            }
            if let Some(b) = self.bindings.iter().find(|b| b.hotkey.id() == event.id()) {
                actions.push(b.action);
            }
        }
        actions
    }

    pub fn label_for(&self, action: Action) -> &str {
        self.bindings
            .iter()
            .find(|b| b.action == action)
            .map_or("", |b| b.label.as_str())
    }

    /// Keys the recorder should strip from recordings.
    pub fn strip_keys(&self) -> Vec<Key> {
        self.bindings.iter().map(|b| b.strip_key).collect()
    }
}
