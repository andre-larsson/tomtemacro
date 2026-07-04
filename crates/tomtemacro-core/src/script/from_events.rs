//! Flat recorded/legacy events → script statements.
//!
//! Readability transforms applied here (and only here — playback never
//! sees this path): press/release pairs collapse into `click`/`press`
//! with the intra-pair gap folded into the following wait, and delays are
//! rounded to whole milliseconds (recording precision is bounded by the
//! move throttle anyway; zero-ms waits are dropped).

use crate::model::{EventKind, MacroEvent};

use super::{Instr, Stmt};

/// Press/release pairs further apart than this stay explicit — the user
/// probably held the button on purpose.
const COLLAPSE_MAX_GAP_US: u64 = 500_000;

pub fn from_events(events: &[MacroEvent]) -> Vec<Stmt> {
    let mut out: Vec<Stmt> = Vec::new();
    let mut pending_us: u64 = 0;
    let mut i = 0;
    while i < events.len() {
        let event = &events[i];
        pending_us += event.delay_us;

        // Adjacent press+release of the same target collapse into a tap.
        let released_by_next = |kind: EventKind| {
            events
                .get(i + 1)
                .is_some_and(|next| next.kind == kind && next.delay_us <= COLLAPSE_MAX_GAP_US)
        };

        let mut collapsed_gap_us = 0;
        let instr = match event.kind {
            EventKind::MouseMove { x, y } => Some(Instr::Move {
                x: x.round() as i32,
                y: y.round() as i32,
            }),
            EventKind::ButtonPress(button)
                if released_by_next(EventKind::ButtonRelease(button)) =>
            {
                collapsed_gap_us = events[i + 1].delay_us;
                i += 1;
                Some(Instr::Click {
                    button,
                    at: None,
                    double: false,
                })
            }
            EventKind::ButtonPress(button) => Some(Instr::MouseDown(button)),
            EventKind::ButtonRelease(button) => Some(Instr::MouseUp(button)),
            EventKind::KeyPress(key) if released_by_next(EventKind::KeyRelease(key)) => {
                collapsed_gap_us = events[i + 1].delay_us;
                i += 1;
                Some(Instr::KeyTap(key))
            }
            EventKind::KeyPress(key) => Some(Instr::KeyDown(key)),
            EventKind::KeyRelease(key) => Some(Instr::KeyUp(key)),
            EventKind::Wheel { dx, dy } => {
                flush_wait(&mut out, &mut pending_us);
                if dy != 0 {
                    out.push(Instr::Scroll { dx: 0, dy }.into());
                }
                if dx != 0 {
                    out.push(Instr::Scroll { dx, dy: 0 }.into());
                }
                None
            }
        };
        if let Some(instr) = instr {
            flush_wait(&mut out, &mut pending_us);
            out.push(instr.into());
        }
        // The gap inside a collapsed tap lands in the next wait, so the
        // overall timeline is preserved.
        pending_us += collapsed_gap_us;
        i += 1;
    }
    out
}

fn flush_wait(out: &mut Vec<Stmt>, pending_us: &mut u64) {
    let ms = (*pending_us + 500) / 1000;
    *pending_us = 0;
    if ms > 0 {
        out.push(
            Instr::Wait {
                min_us: ms * 1000,
                max_us: ms * 1000,
            }
            .into(),
        );
    }
}
