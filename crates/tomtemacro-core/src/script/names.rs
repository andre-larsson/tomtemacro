//! Canonical script names for keys, buttons, and typeable characters.
//!
//! These names are on-disk syntax — renaming an entry breaks existing
//! macros. `key_name` and `parse_key` share one table so they can't drift.

use crate::model::{Key, MouseButton};

/// One entry per named `Key` variant (everything except `Unknown`).
/// Names are lowercase with no separators; arrows get the bare direction
/// words since key verbs and mouse verbs never share an argument slot.
const KEY_NAMES: &[(Key, &str)] = &[
    (Key::KeyA, "a"),
    (Key::KeyB, "b"),
    (Key::KeyC, "c"),
    (Key::KeyD, "d"),
    (Key::KeyE, "e"),
    (Key::KeyF, "f"),
    (Key::KeyG, "g"),
    (Key::KeyH, "h"),
    (Key::KeyI, "i"),
    (Key::KeyJ, "j"),
    (Key::KeyK, "k"),
    (Key::KeyL, "l"),
    (Key::KeyM, "m"),
    (Key::KeyN, "n"),
    (Key::KeyO, "o"),
    (Key::KeyP, "p"),
    (Key::KeyQ, "q"),
    (Key::KeyR, "r"),
    (Key::KeyS, "s"),
    (Key::KeyT, "t"),
    (Key::KeyU, "u"),
    (Key::KeyV, "v"),
    (Key::KeyW, "w"),
    (Key::KeyX, "x"),
    (Key::KeyY, "y"),
    (Key::KeyZ, "z"),
    (Key::Num0, "0"),
    (Key::Num1, "1"),
    (Key::Num2, "2"),
    (Key::Num3, "3"),
    (Key::Num4, "4"),
    (Key::Num5, "5"),
    (Key::Num6, "6"),
    (Key::Num7, "7"),
    (Key::Num8, "8"),
    (Key::Num9, "9"),
    (Key::F1, "f1"),
    (Key::F2, "f2"),
    (Key::F3, "f3"),
    (Key::F4, "f4"),
    (Key::F5, "f5"),
    (Key::F6, "f6"),
    (Key::F7, "f7"),
    (Key::F8, "f8"),
    (Key::F9, "f9"),
    (Key::F10, "f10"),
    (Key::F11, "f11"),
    (Key::F12, "f12"),
    (Key::ControlLeft, "ctrl"),
    (Key::ControlRight, "rctrl"),
    (Key::ShiftLeft, "shift"),
    (Key::ShiftRight, "rshift"),
    (Key::Alt, "alt"),
    (Key::AltGr, "altgr"),
    (Key::MetaLeft, "meta"),
    (Key::MetaRight, "rmeta"),
    (Key::Return, "enter"),
    (Key::Space, "space"),
    (Key::Tab, "tab"),
    (Key::Escape, "esc"),
    (Key::Backspace, "backspace"),
    (Key::Delete, "delete"),
    (Key::Insert, "insert"),
    (Key::Home, "home"),
    (Key::End, "end"),
    (Key::PageUp, "pageup"),
    (Key::PageDown, "pagedown"),
    (Key::UpArrow, "up"),
    (Key::DownArrow, "down"),
    (Key::LeftArrow, "left"),
    (Key::RightArrow, "right"),
    (Key::Minus, "minus"),
    (Key::Equal, "equal"),
    (Key::LeftBracket, "lbracket"),
    (Key::RightBracket, "rbracket"),
    (Key::SemiColon, "semicolon"),
    (Key::Quote, "quote"),
    (Key::BackQuote, "backquote"),
    (Key::BackSlash, "backslash"),
    (Key::IntlBackslash, "intlbackslash"),
    (Key::Comma, "comma"),
    (Key::Dot, "dot"),
    (Key::Slash, "slash"),
    (Key::Kp0, "kp0"),
    (Key::Kp1, "kp1"),
    (Key::Kp2, "kp2"),
    (Key::Kp3, "kp3"),
    (Key::Kp4, "kp4"),
    (Key::Kp5, "kp5"),
    (Key::Kp6, "kp6"),
    (Key::Kp7, "kp7"),
    (Key::Kp8, "kp8"),
    (Key::Kp9, "kp9"),
    (Key::KpReturn, "kpenter"),
    (Key::KpPlus, "kpplus"),
    (Key::KpMinus, "kpminus"),
    (Key::KpMultiply, "kpmultiply"),
    (Key::KpDivide, "kpdivide"),
    (Key::KpDelete, "kpdelete"),
    (Key::PrintScreen, "printscreen"),
    (Key::ScrollLock, "scrolllock"),
    (Key::Pause, "pause"),
    (Key::NumLock, "numlock"),
    (Key::CapsLock, "capslock"),
    (Key::Function, "fn"),
];

pub fn key_name(key: Key) -> String {
    if let Key::Unknown(code) = key {
        return format!("unknown-{code}");
    }
    KEY_NAMES
        .iter()
        .find(|(k, _)| *k == key)
        .map_or_else(|| format!("{key:?}").to_lowercase(), |(_, n)| (*n).into())
}

pub fn parse_key(name: &str) -> Option<Key> {
    let name = name.to_ascii_lowercase();
    if let Some(code) = name.strip_prefix("unknown-") {
        return code.parse().ok().map(Key::Unknown);
    }
    // A few forgiving aliases for hand-written macros.
    let name = match name.as_str() {
        "escape" => "esc",
        "return" => "enter",
        "del" => "delete",
        "ins" => "insert",
        "lctrl" => "ctrl",
        "lshift" => "shift",
        "lmeta" | "win" => "meta",
        other => other,
    };
    KEY_NAMES.iter().find(|(_, n)| *n == name).map(|(k, _)| *k)
}

pub fn button_name(button: MouseButton) -> String {
    match button {
        MouseButton::Left => "left".into(),
        MouseButton::Right => "right".into(),
        MouseButton::Middle => "middle".into(),
        MouseButton::Other(code) => format!("button-{code}"),
    }
}

pub fn parse_button(name: &str) -> Option<MouseButton> {
    match name.to_ascii_lowercase().as_str() {
        "left" => Some(MouseButton::Left),
        "right" => Some(MouseButton::Right),
        "middle" => Some(MouseButton::Middle),
        other => other
            .strip_prefix("button-")?
            .parse()
            .ok()
            .map(MouseButton::Other),
    }
}

/// Physical key (+ shift) that produces `c` on a QWERTY layout. Covers
/// printable ASCII; everything else is untypeable via `type "…"`.
/// Mirrors the reverse mapping in `convert::qwerty_char` (non-Linux inject).
pub fn char_to_key(c: char) -> Option<(Key, bool)> {
    if c.is_ascii_lowercase() || c.is_ascii_uppercase() {
        let key = parse_key(&c.to_ascii_lowercase().to_string())?;
        return Some((key, c.is_ascii_uppercase()));
    }
    if c.is_ascii_digit() {
        return Some((parse_key(&c.to_string())?, false));
    }
    let (key, shift) = match c {
        ' ' => (Key::Space, false),
        '-' => (Key::Minus, false),
        '=' => (Key::Equal, false),
        '[' => (Key::LeftBracket, false),
        ']' => (Key::RightBracket, false),
        ';' => (Key::SemiColon, false),
        '\'' => (Key::Quote, false),
        '`' => (Key::BackQuote, false),
        '\\' => (Key::BackSlash, false),
        ',' => (Key::Comma, false),
        '.' => (Key::Dot, false),
        '/' => (Key::Slash, false),
        '!' => (Key::Num1, true),
        '@' => (Key::Num2, true),
        '#' => (Key::Num3, true),
        '$' => (Key::Num4, true),
        '%' => (Key::Num5, true),
        '^' => (Key::Num6, true),
        '&' => (Key::Num7, true),
        '*' => (Key::Num8, true),
        '(' => (Key::Num9, true),
        ')' => (Key::Num0, true),
        '_' => (Key::Minus, true),
        '+' => (Key::Equal, true),
        '{' => (Key::LeftBracket, true),
        '}' => (Key::RightBracket, true),
        ':' => (Key::SemiColon, true),
        '"' => (Key::Quote, true),
        '~' => (Key::BackQuote, true),
        '|' => (Key::BackSlash, true),
        '<' => (Key::Comma, true),
        '>' => (Key::Dot, true),
        '?' => (Key::Slash, true),
        _ => return None,
    };
    Some((key, shift))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_named_key_round_trips() {
        for &key in Key::ALL {
            let name = key_name(key);
            assert_eq!(parse_key(&name), Some(key), "name {name:?}");
        }
        // Capture-only / escape-hatch variants outside Key::ALL.
        assert_eq!(parse_key(&key_name(Key::Function)), Some(Key::Function));
        assert_eq!(key_name(Key::Unknown(238)), "unknown-238");
        assert_eq!(parse_key("unknown-238"), Some(Key::Unknown(238)));
    }

    #[test]
    fn key_names_are_unique() {
        let mut names: Vec<&str> = KEY_NAMES.iter().map(|(_, n)| *n).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), KEY_NAMES.len());
    }

    #[test]
    fn aliases_and_case_are_forgiving() {
        assert_eq!(parse_key("Escape"), Some(Key::Escape));
        assert_eq!(parse_key("RETURN"), Some(Key::Return));
        assert_eq!(parse_key("del"), Some(Key::Delete));
        assert_eq!(parse_key("win"), Some(Key::MetaLeft));
        assert_eq!(parse_key("bogus"), None);
    }

    #[test]
    fn buttons_round_trip() {
        for b in [
            MouseButton::Left,
            MouseButton::Right,
            MouseButton::Middle,
            MouseButton::Other(8),
        ] {
            assert_eq!(parse_button(&button_name(b)), Some(b));
        }
        assert_eq!(parse_button("nope"), None);
    }

    #[test]
    fn all_printable_ascii_is_typeable() {
        for c in ' '..='~' {
            assert!(char_to_key(c).is_some(), "char {c:?}");
        }
        assert_eq!(char_to_key('A'), Some((Key::KeyA, true)));
        assert_eq!(char_to_key('?'), Some((Key::Slash, true)));
        assert_eq!(char_to_key('é'), None);
        assert_eq!(char_to_key('\n'), None);
    }
}
