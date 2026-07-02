//! Conversions between the core model and backend (enigo/rdev) types.
//!
//! Injection strategy per platform:
//! - **Linux/X11**: every key is injected as a *raw X11 keycode* (the fixed
//!   evdev-based table below, the same one the capture layer uses). The
//!   active keyboard layout then applies, so recording `å` on a Swedish
//!   layout replays as `å` — true physical-key semantics.
//! - **Windows/macOS (v1)**: named enigo keys for specials, `Key::Unicode`
//!   with the QWERTY character for printables. This is *character* semantics
//!   for printables; refined to scancode/virtual-keycode tables in the
//!   cross-platform hardening phase.

use crate::model::{Key, MouseButton};

/// How to inject a [`Key`] on the current platform.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InjectKey {
    /// A named enigo key; enigo picks the per-platform code.
    Named(enigo::Key),
    /// A raw platform keycode (X11 keycode on Linux, scancode on Windows).
    Raw(u16),
    /// Cannot be injected on this platform (logged and skipped).
    Uninjectable,
}

pub fn button_to_enigo(button: MouseButton) -> Option<enigo::Button> {
    match button {
        MouseButton::Left => Some(enigo::Button::Left),
        MouseButton::Right => Some(enigo::Button::Right),
        MouseButton::Middle => Some(enigo::Button::Middle),
        MouseButton::Other(_) => None,
    }
}

#[cfg(target_os = "linux")]
pub fn key_to_inject(key: Key) -> InjectKey {
    match x11_keycode(key) {
        Some(code) => InjectKey::Raw(code),
        None => InjectKey::Uninjectable,
    }
}

#[cfg(not(target_os = "linux"))]
pub fn key_to_inject(key: Key) -> InjectKey {
    named_key(key)
}

/// Fixed X11 keycode for a key (standard evdev mapping: evdev code + 8).
/// This must stay consistent with the capture layer's table so that
/// record → replay round-trips exactly; the Xvfb smoke test verifies that.
#[cfg(target_os = "linux")]
pub fn x11_keycode(key: Key) -> Option<u16> {
    use Key::*;
    let code: u16 = match key {
        Escape => 9,
        Num1 => 10,
        Num2 => 11,
        Num3 => 12,
        Num4 => 13,
        Num5 => 14,
        Num6 => 15,
        Num7 => 16,
        Num8 => 17,
        Num9 => 18,
        Num0 => 19,
        Minus => 20,
        Equal => 21,
        Backspace => 22,
        Tab => 23,
        KeyQ => 24,
        KeyW => 25,
        KeyE => 26,
        KeyR => 27,
        KeyT => 28,
        KeyY => 29,
        KeyU => 30,
        KeyI => 31,
        KeyO => 32,
        KeyP => 33,
        LeftBracket => 34,
        RightBracket => 35,
        Return => 36,
        ControlLeft => 37,
        KeyA => 38,
        KeyS => 39,
        KeyD => 40,
        KeyF => 41,
        KeyG => 42,
        KeyH => 43,
        KeyJ => 44,
        KeyK => 45,
        KeyL => 46,
        SemiColon => 47,
        Quote => 48,
        BackQuote => 49,
        ShiftLeft => 50,
        BackSlash => 51,
        KeyZ => 52,
        KeyX => 53,
        KeyC => 54,
        KeyV => 55,
        KeyB => 56,
        KeyN => 57,
        KeyM => 58,
        Comma => 59,
        Dot => 60,
        Slash => 61,
        ShiftRight => 62,
        KpMultiply => 63,
        Alt => 64,
        Space => 65,
        CapsLock => 66,
        F1 => 67,
        F2 => 68,
        F3 => 69,
        F4 => 70,
        F5 => 71,
        F6 => 72,
        F7 => 73,
        F8 => 74,
        F9 => 75,
        F10 => 76,
        NumLock => 77,
        ScrollLock => 78,
        Kp7 => 79,
        Kp8 => 80,
        Kp9 => 81,
        KpMinus => 82,
        Kp4 => 83,
        Kp5 => 84,
        Kp6 => 85,
        KpPlus => 86,
        Kp1 => 87,
        Kp2 => 88,
        Kp3 => 89,
        Kp0 => 90,
        KpDelete => 91,
        IntlBackslash => 94,
        F11 => 95,
        F12 => 96,
        KpReturn => 104,
        ControlRight => 105,
        KpDivide => 106,
        PrintScreen => 107,
        AltGr => 108,
        Home => 110,
        UpArrow => 111,
        PageUp => 112,
        LeftArrow => 113,
        RightArrow => 114,
        End => 115,
        DownArrow => 116,
        PageDown => 117,
        Insert => 118,
        Delete => 119,
        Pause => 127,
        MetaLeft => 133,
        MetaRight => 134,
        // The laptop Fn key never reaches the X server as an injectable code.
        Function => return None,
        Unknown(code) => u16::try_from(code).ok()?,
    };
    Some(code)
}

/// Named/Unicode mapping used on Windows and macOS in v1.
#[cfg(not(target_os = "linux"))]
fn named_key(key: Key) -> InjectKey {
    use enigo::Key as E;
    use Key::*;
    let named = match key {
        Alt => E::Alt,
        Backspace => E::Backspace,
        CapsLock => E::CapsLock,
        ControlLeft => E::LControl,
        ControlRight => E::RControl,
        Delete => E::Delete,
        DownArrow => E::DownArrow,
        End => E::End,
        Escape => E::Escape,
        F1 => E::F1,
        F2 => E::F2,
        F3 => E::F3,
        F4 => E::F4,
        F5 => E::F5,
        F6 => E::F6,
        F7 => E::F7,
        F8 => E::F8,
        F9 => E::F9,
        F10 => E::F10,
        F11 => E::F11,
        F12 => E::F12,
        Home => E::Home,
        LeftArrow => E::LeftArrow,
        MetaLeft | MetaRight => E::Meta,
        PageDown => E::PageDown,
        PageUp => E::PageUp,
        Return | KpReturn => E::Return,
        RightArrow => E::RightArrow,
        ShiftLeft => E::LShift,
        ShiftRight => E::RShift,
        Space => E::Space,
        Tab => E::Tab,
        UpArrow => E::UpArrow,
        PrintScreen => E::PrintScr,
        Kp0 => E::Numpad0,
        Kp1 => E::Numpad1,
        Kp2 => E::Numpad2,
        Kp3 => E::Numpad3,
        Kp4 => E::Numpad4,
        Kp5 => E::Numpad5,
        Kp6 => E::Numpad6,
        Kp7 => E::Numpad7,
        Kp8 => E::Numpad8,
        Kp9 => E::Numpad9,
        KpMinus => E::Subtract,
        KpPlus => E::Add,
        KpMultiply => E::Multiply,
        KpDivide => E::Divide,
        KpDelete => E::Decimal,
        #[cfg(target_os = "windows")]
        NumLock => E::Numlock,
        #[cfg(target_os = "windows")]
        Insert => E::Insert,
        #[cfg(target_os = "windows")]
        Pause => E::Pause,
        // AltGr is the right Alt on Windows (VK_RMENU); absent on macOS.
        #[cfg(target_os = "windows")]
        AltGr => return InjectKey::Raw(0xE038), // extended scancode for RAlt
        Unknown(code) => match u16::try_from(code) {
            Ok(code) => return InjectKey::Raw(code),
            Err(_) => return InjectKey::Uninjectable,
        },
        other => match qwerty_char(other) {
            Some(c) => E::Unicode(c),
            None => return InjectKey::Uninjectable,
        },
    };
    InjectKey::Named(named)
}

/// The character on the QWERTY layout for printable-position keys.
#[cfg(not(target_os = "linux"))]
fn qwerty_char(key: Key) -> Option<char> {
    use Key::*;
    Some(match key {
        Num1 => '1',
        Num2 => '2',
        Num3 => '3',
        Num4 => '4',
        Num5 => '5',
        Num6 => '6',
        Num7 => '7',
        Num8 => '8',
        Num9 => '9',
        Num0 => '0',
        Minus => '-',
        Equal => '=',
        KeyQ => 'q',
        KeyW => 'w',
        KeyE => 'e',
        KeyR => 'r',
        KeyT => 't',
        KeyY => 'y',
        KeyU => 'u',
        KeyI => 'i',
        KeyO => 'o',
        KeyP => 'p',
        LeftBracket => '[',
        RightBracket => ']',
        KeyA => 'a',
        KeyS => 's',
        KeyD => 'd',
        KeyF => 'f',
        KeyG => 'g',
        KeyH => 'h',
        KeyJ => 'j',
        KeyK => 'k',
        KeyL => 'l',
        SemiColon => ';',
        Quote => '\'',
        BackQuote => '`',
        BackSlash => '\\',
        IntlBackslash => '<',
        KeyZ => 'z',
        KeyX => 'x',
        KeyC => 'c',
        KeyV => 'v',
        KeyB => 'b',
        KeyN => 'n',
        KeyM => 'm',
        Comma => ',',
        Dot => '.',
        Slash => '/',
        _ => return None,
    })
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn every_key_except_function_is_injectable_on_linux() {
        // Spot-check the table's shape: contiguous rows map contiguously.
        assert_eq!(x11_keycode(Key::KeyQ), Some(24));
        assert_eq!(x11_keycode(Key::KeyP), Some(33));
        assert_eq!(x11_keycode(Key::KeyA), Some(38));
        assert_eq!(x11_keycode(Key::KeyM), Some(58));
        assert_eq!(x11_keycode(Key::F12), Some(96));
        assert_eq!(x11_keycode(Key::Function), None);
        assert_eq!(x11_keycode(Key::Unknown(200)), Some(200));
    }
}
