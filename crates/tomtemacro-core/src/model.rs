//! The stable event model: what a macro *is*, in memory and on disk.
//!
//! These are deliberately our own types rather than re-exports from
//! rdev/enigo — the on-disk format must survive swapping either dependency.

use serde::{Deserialize, Serialize};

/// Bump when the on-disk schema changes; `storage` rejects newer files and
/// migrates older ones.
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MacroFile {
    pub version: u32,
    pub meta: MacroMeta,
    pub events: Vec<MacroEvent>,
}

impl MacroFile {
    pub fn new(meta: MacroMeta, events: Vec<MacroEvent>) -> Self {
        Self {
            version: SCHEMA_VERSION,
            meta,
            events,
        }
    }

    /// Total duration of the macro at 1.0x speed.
    pub fn duration_us(&self) -> u64 {
        self.events.iter().map(|e| e.delay_us).sum()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MacroMeta {
    pub name: String,
    /// RFC 3339 UTC timestamp of when the macro was recorded.
    pub created_utc: String,
    /// e.g. "linux-x11", "windows", "macos" — informational.
    pub os: String,
    /// Monitor geometry at record time; mismatch at load time triggers a
    /// non-blocking warning (absolute coordinates may be off).
    pub screen: Option<ScreenInfo>,
    pub notes: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct ScreenInfo {
    pub width: u32,
    pub height: u32,
    pub scale: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct MacroEvent {
    /// Microseconds since the previous event (0 for the first). Relative
    /// delays keep files human-auditable and make speed scaling a division.
    pub delay_us: u64,
    pub kind: EventKind,
}

/// Coordinates are physical pixels in the OS's global virtual-desktop space,
/// exactly as the capture layer reports them (may be negative on
/// multi-monitor setups).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum EventKind {
    MouseMove { x: f64, y: f64 },
    ButtonPress(MouseButton),
    ButtonRelease(MouseButton),
    Wheel { dx: i32, dy: i32 },
    KeyPress(Key),
    KeyRelease(Key),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Other(u8),
}

/// Physical-position key identity (QWERTY-position semantics, mirroring the
/// capture layer's model): `KeyQ` is the key at the QWERTY "Q" position
/// regardless of the active layout. Replaying on a machine with a different
/// layout reproduces physical keys, not characters — documented v1 behavior.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Key {
    Alt,
    AltGr,
    Backspace,
    CapsLock,
    ControlLeft,
    ControlRight,
    Delete,
    DownArrow,
    End,
    Escape,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    Home,
    LeftArrow,
    MetaLeft,
    MetaRight,
    PageDown,
    PageUp,
    Return,
    RightArrow,
    ShiftLeft,
    ShiftRight,
    Space,
    Tab,
    UpArrow,
    PrintScreen,
    ScrollLock,
    Pause,
    NumLock,
    BackQuote,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,
    Num0,
    Minus,
    Equal,
    KeyQ,
    KeyW,
    KeyE,
    KeyR,
    KeyT,
    KeyY,
    KeyU,
    KeyI,
    KeyO,
    KeyP,
    LeftBracket,
    RightBracket,
    KeyA,
    KeyS,
    KeyD,
    KeyF,
    KeyG,
    KeyH,
    KeyJ,
    KeyK,
    KeyL,
    SemiColon,
    Quote,
    BackSlash,
    IntlBackslash,
    KeyZ,
    KeyX,
    KeyC,
    KeyV,
    KeyB,
    KeyN,
    KeyM,
    Comma,
    Dot,
    Slash,
    Insert,
    KpReturn,
    KpMinus,
    KpPlus,
    KpMultiply,
    KpDivide,
    Kp0,
    Kp1,
    Kp2,
    Kp3,
    Kp4,
    Kp5,
    Kp6,
    Kp7,
    Kp8,
    Kp9,
    KpDelete,
    /// The laptop `Fn` key — observable on some platforms, never injectable.
    Function,
    /// Platform-specific keycode we have no name for; passed through verbatim.
    Unknown(u32),
}

impl Key {
    /// Every named, injectable key, grouped for UI pickers (letters, digits,
    /// F-keys, editing, arrows, navigation, modifiers, punctuation, keypad,
    /// misc). Excludes `Function` (capture-only) and `Unknown`. Keep in sync
    /// when adding variants.
    pub const ALL: &'static [Key] = &[
        Key::KeyA,
        Key::KeyB,
        Key::KeyC,
        Key::KeyD,
        Key::KeyE,
        Key::KeyF,
        Key::KeyG,
        Key::KeyH,
        Key::KeyI,
        Key::KeyJ,
        Key::KeyK,
        Key::KeyL,
        Key::KeyM,
        Key::KeyN,
        Key::KeyO,
        Key::KeyP,
        Key::KeyQ,
        Key::KeyR,
        Key::KeyS,
        Key::KeyT,
        Key::KeyU,
        Key::KeyV,
        Key::KeyW,
        Key::KeyX,
        Key::KeyY,
        Key::KeyZ,
        Key::Num1,
        Key::Num2,
        Key::Num3,
        Key::Num4,
        Key::Num5,
        Key::Num6,
        Key::Num7,
        Key::Num8,
        Key::Num9,
        Key::Num0,
        Key::F1,
        Key::F2,
        Key::F3,
        Key::F4,
        Key::F5,
        Key::F6,
        Key::F7,
        Key::F8,
        Key::F9,
        Key::F10,
        Key::F11,
        Key::F12,
        Key::Space,
        Key::Return,
        Key::Tab,
        Key::Escape,
        Key::Backspace,
        Key::Delete,
        Key::Insert,
        Key::UpArrow,
        Key::DownArrow,
        Key::LeftArrow,
        Key::RightArrow,
        Key::Home,
        Key::End,
        Key::PageUp,
        Key::PageDown,
        Key::ShiftLeft,
        Key::ShiftRight,
        Key::ControlLeft,
        Key::ControlRight,
        Key::Alt,
        Key::AltGr,
        Key::MetaLeft,
        Key::MetaRight,
        Key::CapsLock,
        Key::Minus,
        Key::Equal,
        Key::LeftBracket,
        Key::RightBracket,
        Key::SemiColon,
        Key::Quote,
        Key::BackQuote,
        Key::BackSlash,
        Key::IntlBackslash,
        Key::Comma,
        Key::Dot,
        Key::Slash,
        Key::Kp0,
        Key::Kp1,
        Key::Kp2,
        Key::Kp3,
        Key::Kp4,
        Key::Kp5,
        Key::Kp6,
        Key::Kp7,
        Key::Kp8,
        Key::Kp9,
        Key::KpReturn,
        Key::KpPlus,
        Key::KpMinus,
        Key::KpMultiply,
        Key::KpDivide,
        Key::KpDelete,
        Key::PrintScreen,
        Key::ScrollLock,
        Key::Pause,
        Key::NumLock,
    ];
}
