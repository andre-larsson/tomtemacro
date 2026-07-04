//! Input injection behind a trait, so the enigo backend can be swapped
//! (e.g. for a future uinput/Wayland backend) and mocked in tests.

use enigo::{Axis, Coordinate, Direction, Enigo, Keyboard, Mouse, Settings};

use crate::convert::{button_to_enigo, key_to_inject, InjectKey};
use crate::model::{EventKind, Key};

#[derive(Debug, thiserror::Error)]
pub enum InjectError {
    #[error("failed to initialize injection backend: {0}")]
    Init(String),
    #[error("failed to inject event: {0}")]
    Inject(String),
}

pub trait Injector {
    fn inject(&mut self, kind: &EventKind) -> Result<(), InjectError>;
    /// Current cursor position in global virtual-desktop pixels.
    fn cursor_location(&mut self) -> Result<(i32, i32), InjectError>;
    /// Size of the main display, used to stamp recordings so playback on a
    /// different screen layout can warn. Optional for mock backends.
    fn main_display(&mut self) -> Result<(i32, i32), InjectError> {
        Err(InjectError::Inject("display size unavailable".into()))
    }
    /// Type a string as keystrokes. The default expands each character to a
    /// (shifted) physical-key tap via the QWERTY table, paced so slow
    /// applications keep up; backends with native text input may override.
    fn type_text(&mut self, text: &str) -> Result<(), InjectError> {
        for c in text.chars() {
            let Some((key, shift)) = crate::script::char_to_key(c) else {
                log::warn!("skipping untypeable character {c:?}");
                continue;
            };
            if shift {
                self.inject(&EventKind::KeyPress(Key::ShiftLeft))?;
            }
            self.inject(&EventKind::KeyPress(key))?;
            self.inject(&EventKind::KeyRelease(key))?;
            if shift {
                self.inject(&EventKind::KeyRelease(Key::ShiftLeft))?;
            }
            std::thread::sleep(std::time::Duration::from_micros(
                crate::script::TYPE_CHAR_PACE_US,
            ));
        }
        Ok(())
    }
}

pub struct EnigoInjector {
    enigo: Enigo,
}

impl EnigoInjector {
    /// Construct on the thread that will use it — the underlying OS handles
    /// are not guaranteed to be transferable between threads on all
    /// platforms.
    pub fn new() -> Result<Self, InjectError> {
        Enigo::new(&Settings::default())
            .map(|enigo| Self { enigo })
            .map_err(|e| InjectError::Init(e.to_string()))
    }
}

impl Injector for EnigoInjector {
    fn inject(&mut self, kind: &EventKind) -> Result<(), InjectError> {
        let result = match *kind {
            EventKind::MouseMove { x, y } => {
                self.enigo
                    .move_mouse(x.round() as i32, y.round() as i32, Coordinate::Abs)
            }
            EventKind::ButtonPress(b) | EventKind::ButtonRelease(b) => {
                let Some(button) = button_to_enigo(b) else {
                    log::warn!("skipping uninjectable mouse button {b:?}");
                    return Ok(());
                };
                let direction = if matches!(kind, EventKind::ButtonPress(_)) {
                    Direction::Press
                } else {
                    Direction::Release
                };
                self.enigo.button(button, direction)
            }
            EventKind::Wheel { dx, dy } => {
                // Model convention: dy > 0 scrolls up. enigo: positive = down.
                let mut result = Ok(());
                if dy != 0 {
                    result = self.enigo.scroll(-dy, Axis::Vertical);
                }
                if result.is_ok() && dx != 0 {
                    result = self.enigo.scroll(dx, Axis::Horizontal);
                }
                result
            }
            EventKind::KeyPress(k) | EventKind::KeyRelease(k) => {
                let direction = if matches!(kind, EventKind::KeyPress(_)) {
                    Direction::Press
                } else {
                    Direction::Release
                };
                match key_to_inject(k) {
                    InjectKey::Named(key) => self.enigo.key(key, direction),
                    InjectKey::Raw(code) => self.enigo.raw(code, direction),
                    InjectKey::Uninjectable => {
                        log::warn!("skipping uninjectable key {k:?}");
                        return Ok(());
                    }
                }
            }
        };
        result.map_err(|e| InjectError::Inject(e.to_string()))
    }

    fn cursor_location(&mut self) -> Result<(i32, i32), InjectError> {
        self.enigo
            .location()
            .map_err(|e| InjectError::Inject(e.to_string()))
    }

    fn main_display(&mut self) -> Result<(i32, i32), InjectError> {
        self.enigo
            .main_display()
            .map_err(|e| InjectError::Inject(e.to_string()))
    }

    /// Native text input beats per-key expansion off Linux: the platform
    /// backends there inject Unicode characters directly, and holding Shift
    /// around such an event would not capitalize it. On Linux the trait
    /// default keeps the raw-keycode physical-key semantics.
    #[cfg(not(target_os = "linux"))]
    fn type_text(&mut self, text: &str) -> Result<(), InjectError> {
        self.enigo
            .text(text)
            .map_err(|e| InjectError::Inject(e.to_string()))
    }
}
