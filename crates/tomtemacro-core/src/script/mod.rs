//! The macro script language: a human-friendly, line-oriented text format.
//!
//! This is the primary on-disk format for macros (`.tomte` files) and the
//! source of truth in the editor. Unlike the flat event model, scripts have
//! structure — `repeat` blocks, ranged waits, relative moves — that is
//! resolved at *play time*, so playback walks this AST instead of a
//! pre-expanded event list. See `docs/macro-language.md` for the reference.

mod format;
mod from_events;
mod names;
mod parse;
mod transform;

pub use format::{format, format_body};
pub use from_events::from_events;
pub use names::{button_name, char_to_key, key_name, parse_button, parse_key};
pub use parse::{parse, with_header_name};
pub use transform::strip_moves;

use crate::model::{Key, MacroFile, MacroMeta, MouseButton};

/// Version stamped as `# tomte-macro vN`; files marked newer are rejected.
pub const TEXT_VERSION: u32 = 1;

/// `repeat` blocks deeper than this are a parse error (keeps playback
/// recursion trivially bounded).
pub const MAX_REPEAT_DEPTH: usize = 32;

/// Pacing of `type "…"` injection, per character. Also the estimate used
/// for duration stats and playback-timeline accounting.
pub const TYPE_CHAR_PACE_US: u64 = 10_000;

#[derive(Debug, Clone, PartialEq)]
pub struct Script {
    pub meta: MacroMeta,
    pub body: Vec<Stmt>,
}

impl Script {
    /// Convert a flat recorded/legacy macro into a script.
    pub fn from_macro_file(file: &MacroFile) -> Self {
        Self {
            meta: file.meta.clone(),
            body: from_events(&file.events),
        }
    }

    pub fn stats(&self) -> ScriptStats {
        body_stats(&self.body)
    }
}

/// One line of a script: an instruction plus its trailing comment.
/// `Nop` with no comment is a blank line; `Nop` with a comment is a
/// full-line comment. Both are kept so edits and transforms round-trip.
#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub instr: Instr,
    pub comment: Option<String>,
}

impl Stmt {
    pub fn bare(instr: Instr) -> Self {
        Self {
            instr,
            comment: None,
        }
    }
}

impl From<Instr> for Stmt {
    fn from(instr: Instr) -> Self {
        Self::bare(instr)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Instr {
    /// Blank line or full-line comment (see [`Stmt`]).
    Nop,
    /// `move X Y` — absolute physical pixels (negative on multi-monitor).
    Move {
        x: i32,
        y: i32,
    },
    /// `move +DX -DY` / `moverel DX DY` — relative to the cursor at play time.
    MoveRel {
        dx: i32,
        dy: i32,
    },
    /// `click B [at X Y]` / `doubleclick B [at X Y]`.
    Click {
        button: MouseButton,
        at: Option<(i32, i32)>,
        double: bool,
    },
    /// `mousedown B` / `mouseup B`.
    MouseDown(MouseButton),
    MouseUp(MouseButton),
    /// `scroll up|down|left|right [N]` — model convention: dy>0 up, dx>0 right.
    /// Only one axis is ever nonzero.
    Scroll {
        dx: i32,
        dy: i32,
    },
    /// `press K` (tap), `keydown K`, `keyup K`.
    KeyTap(Key),
    KeyDown(Key),
    KeyUp(Key),
    /// `type "…"` — printable ASCII, validated at parse time.
    Type(String),
    /// `wait 500ms` (min == max) or `wait 100ms..300ms` (resampled per
    /// encounter). Microseconds.
    Wait {
        min_us: u64,
        max_us: u64,
    },
    /// `repeat N` … `end`.
    Repeat {
        count: u32,
        body: Vec<Stmt>,
        /// Trailing comment on the `end` line.
        end_comment: Option<String>,
    },
}

#[derive(Debug, thiserror::Error, Clone, PartialEq)]
#[error("line {line}: {message}")]
pub struct ParseError {
    /// 1-based line number.
    pub line: u32,
    pub message: String,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ScriptStats {
    /// Executable instructions, with `repeat` bodies multiplied out.
    pub instructions: u64,
    /// Rough duration at 1.0× speed: wait midpoints, doubleclick gaps, and
    /// `type` pacing, multiplied through `repeat` counts.
    pub nominal_us: u64,
}

fn body_stats(body: &[Stmt]) -> ScriptStats {
    let mut stats = ScriptStats::default();
    for stmt in body {
        match &stmt.instr {
            Instr::Nop => {}
            Instr::Wait { min_us, max_us } => {
                stats.instructions += 1;
                stats.nominal_us = stats.nominal_us.saturating_add(min_us.midpoint(*max_us));
            }
            Instr::Click { double: true, .. } => {
                stats.instructions += 1;
                stats.nominal_us = stats
                    .nominal_us
                    .saturating_add(crate::clicker::DOUBLE_CLICK_GAP.as_micros() as u64);
            }
            Instr::Type(text) => {
                stats.instructions += 1;
                stats.nominal_us = stats
                    .nominal_us
                    .saturating_add(text.chars().count() as u64 * TYPE_CHAR_PACE_US);
            }
            Instr::Repeat { count, body, .. } => {
                let inner = body_stats(body);
                stats.instructions = stats
                    .instructions
                    .saturating_add(inner.instructions.saturating_mul(u64::from(*count)));
                stats.nominal_us = stats
                    .nominal_us
                    .saturating_add(inner.nominal_us.saturating_mul(u64::from(*count)));
            }
            _ => stats.instructions += 1,
        }
    }
    stats
}
