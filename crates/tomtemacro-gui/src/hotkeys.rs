//! Global hotkeys: OS-registered so the chords are consumed (they never
//! leak into the focused app) and they keep working while the capture gate
//! is closed during playback.

use global_hotkey::hotkey::{Code, HotKey};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use tomtemacro_core::model::Key;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    ToggleClicker,
    ToggleRecord,
    PlayMacro,
    StopAll,
}

pub struct Binding {
    pub action: Action,
    pub hotkey: HotKey,
    /// Core-model key stripped from recordings (belt-and-braces; the OS
    /// registration usually consumes the chord before capture sees it).
    pub strip_key: Key,
    pub label: &'static str,
}

pub struct Hotkeys {
    // Held so the OS registrations live as long as the app.
    _manager: GlobalHotKeyManager,
    bindings: Vec<Binding>,
}

impl Hotkeys {
    /// Register the default bindings. Returns Err if the OS refuses (e.g.
    /// another app grabbed a chord) — the GUI still works, buttons only.
    pub fn register_defaults() -> Result<Self, String> {
        let manager = GlobalHotKeyManager::new().map_err(|e| e.to_string())?;
        let bindings = vec![
            Binding {
                action: Action::ToggleClicker,
                hotkey: HotKey::new(None, Code::F6),
                strip_key: Key::F6,
                label: "F6",
            },
            Binding {
                action: Action::ToggleRecord,
                hotkey: HotKey::new(None, Code::F7),
                strip_key: Key::F7,
                label: "F7",
            },
            Binding {
                action: Action::PlayMacro,
                hotkey: HotKey::new(None, Code::F8),
                strip_key: Key::F8,
                label: "F8",
            },
            Binding {
                action: Action::StopAll,
                hotkey: HotKey::new(None, Code::F9),
                strip_key: Key::F9,
                label: "F9",
            },
        ];
        for binding in &bindings {
            manager
                .register(binding.hotkey)
                .map_err(|e| format!("{} ({})", e, binding.label))?;
        }
        Ok(Self {
            _manager: manager,
            bindings,
        })
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

    pub fn label_for(&self, action: Action) -> &'static str {
        self.bindings
            .iter()
            .find(|b| b.action == action)
            .map_or("", |b| b.label)
    }

    /// Keys the recorder should strip from recordings.
    pub fn strip_keys(&self) -> Vec<Key> {
        self.bindings.iter().map(|b| b.strip_key).collect()
    }
}
