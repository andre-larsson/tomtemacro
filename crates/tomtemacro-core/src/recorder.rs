//! Turns a stream of captured events into a clean macro: throttles the
//! mouse-move firehose, strips configured hotkey chords, trims the stop
//! chord's tail, drops key events that arrived with an unrecognized
//! (garbled/phantom) keycode, and converts capture times into relative delays.

use std::time::{Duration, Instant};

use crate::model::{EventKind, Key, MacroEvent};

#[derive(Debug, Clone)]
pub struct RecordConfig {
    /// Key events matching these are never recorded (the configured global
    /// hotkeys — usually consumed by the OS registration before we see
    /// them, but stripped here as a belt-and-braces defense).
    pub strip_keys: Vec<Key>,
    /// Mouse moves are downsampled to at most this rate. High-Hz gaming mice
    /// otherwise produce megabytes of moves per minute of recording.
    pub mouse_move_max_hz: u32,
    /// Trailing *key* events this close to the stop request are dropped, so
    /// the keystroke that stopped the recording never lands in the file.
    pub trim_tail: Duration,
}

impl Default for RecordConfig {
    fn default() -> Self {
        Self {
            strip_keys: Vec::new(),
            mouse_move_max_hz: 125,
            trim_tail: Duration::from_millis(200),
        }
    }
}

pub struct Recorder {
    config: RecordConfig,
    events: Vec<(Instant, EventKind)>,
    /// Newest throttled-away move; flushed before the next non-move event so
    /// clicks land at the true final cursor position.
    pending_move: Option<(Instant, EventKind)>,
    last_move_kept: Option<Instant>,
    /// Count of key events dropped for carrying an unrecognized keycode.
    dropped_unknown: usize,
}

impl Recorder {
    pub fn new(config: RecordConfig) -> Self {
        Self {
            config,
            events: Vec::new(),
            pending_move: None,
            last_move_kept: None,
            dropped_unknown: 0,
        }
    }

    /// Feed one captured event. Returns true if it was kept (drives the
    /// live event counter).
    pub fn push(&mut self, at: Instant, kind: EventKind) -> bool {
        match kind {
            EventKind::KeyPress(k) | EventKind::KeyRelease(k)
                if self.config.strip_keys.contains(&k) =>
            {
                false
            }
            // Some machines flood the low-level keyboard hook with phantom key
            // events carrying an unmapped/garbled keycode (surfaced as
            // `Unknown`). They don't correspond to real keystrokes, so drop
            // them rather than bake junk into the macro; `dropped_unknown`
            // drives the GUI's "keys were mangled" notice.
            EventKind::KeyPress(Key::Unknown(_)) | EventKind::KeyRelease(Key::Unknown(_)) => {
                self.dropped_unknown += 1;
                false
            }
            EventKind::MouseMove { .. } => {
                let min_gap = if self.config.mouse_move_max_hz == 0 {
                    Duration::MAX
                } else {
                    Duration::from_secs_f64(1.0 / f64::from(self.config.mouse_move_max_hz))
                };
                match self.last_move_kept {
                    Some(prev) if at.duration_since(prev) < min_gap => {
                        self.pending_move = Some((at, kind));
                        false
                    }
                    _ => {
                        self.pending_move = None;
                        self.last_move_kept = Some(at);
                        self.events.push((at, kind));
                        true
                    }
                }
            }
            _ => {
                if let Some(pending) = self.pending_move.take() {
                    self.last_move_kept = Some(pending.0);
                    self.events.push(pending);
                }
                self.events.push((at, kind));
                true
            }
        }
    }

    /// How many key events were dropped for carrying an unrecognized keycode
    /// (see `push`). Non-zero means this machine mangled some keystrokes.
    pub fn dropped_unknown(&self) -> usize {
        self.dropped_unknown
    }

    /// Finalize: trim the stop chord's tail and convert to relative delays.
    pub fn finish(mut self, stopped_at: Instant) -> Vec<MacroEvent> {
        while let Some((at, kind)) = self.events.last() {
            let in_window = stopped_at.saturating_duration_since(*at) <= self.config.trim_tail;
            let is_key = matches!(kind, EventKind::KeyPress(_) | EventKind::KeyRelease(_));
            if in_window && is_key {
                self.events.pop();
            } else {
                break;
            }
        }

        let mut out = Vec::with_capacity(self.events.len());
        let mut prev: Option<Instant> = None;
        for (at, kind) in self.events {
            let delay_us = prev.map_or(0, |p| at.duration_since(p).as_micros() as u64);
            out.push(MacroEvent { delay_us, kind });
            prev = Some(at);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::MouseButton;

    fn ms(base: Instant, offset: u64) -> Instant {
        base + Duration::from_millis(offset)
    }

    #[test]
    fn strips_configured_hotkeys() {
        let mut rec = Recorder::new(RecordConfig {
            strip_keys: vec![Key::F7],
            ..Default::default()
        });
        let t0 = Instant::now();
        assert!(rec.push(ms(t0, 0), EventKind::KeyPress(Key::KeyA)));
        assert!(!rec.push(ms(t0, 10), EventKind::KeyPress(Key::F7)));
        assert!(!rec.push(ms(t0, 20), EventKind::KeyRelease(Key::F7)));
        assert!(rec.push(ms(t0, 30), EventKind::KeyRelease(Key::KeyA)));
        let events = rec.finish(ms(t0, 1000));
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn throttles_moves_but_keeps_position_before_click() {
        let mut rec = Recorder::new(RecordConfig {
            mouse_move_max_hz: 100, // min gap 10 ms
            ..Default::default()
        });
        let t0 = Instant::now();
        rec.push(ms(t0, 0), EventKind::MouseMove { x: 0.0, y: 0.0 });
        // 1 ms apart: all throttled away...
        for i in 1..=5 {
            let kept = rec.push(
                ms(t0, i),
                EventKind::MouseMove {
                    x: f64::from(i as u32),
                    y: 0.0,
                },
            );
            assert!(!kept, "move {i} should be throttled");
        }
        // ...but the click must be preceded by the freshest position (x=5).
        rec.push(ms(t0, 6), EventKind::ButtonPress(MouseButton::Left));
        let events = rec.finish(ms(t0, 1000));
        let kinds: Vec<_> = events.iter().map(|e| e.kind).collect();
        assert_eq!(kinds.len(), 3);
        assert_eq!(kinds[0], EventKind::MouseMove { x: 0.0, y: 0.0 });
        assert_eq!(kinds[1], EventKind::MouseMove { x: 5.0, y: 0.0 });
        assert_eq!(kinds[2], EventKind::ButtonPress(MouseButton::Left));
    }

    #[test]
    fn trims_trailing_keys_near_stop_but_not_mouse() {
        let mut rec = Recorder::new(RecordConfig::default()); // trim 200 ms
        let t0 = Instant::now();
        rec.push(ms(t0, 0), EventKind::KeyPress(Key::KeyA));
        rec.push(ms(t0, 50), EventKind::KeyRelease(Key::KeyA));
        rec.push(ms(t0, 500), EventKind::ButtonPress(MouseButton::Left));
        rec.push(ms(t0, 550), EventKind::ButtonRelease(MouseButton::Left));
        // Stop chord fragments 20 ms before the stop request:
        rec.push(ms(t0, 880), EventKind::KeyPress(Key::ControlLeft));
        rec.push(ms(t0, 890), EventKind::KeyRelease(Key::ControlLeft));
        let events = rec.finish(ms(t0, 900));
        assert_eq!(events.len(), 4, "trailing ctrl press/release trimmed");
        assert!(matches!(events[3].kind, EventKind::ButtonRelease(_)));
    }

    #[test]
    fn unknown_key_events_are_dropped() {
        // Phantom key events with a garbled keycode are dropped, not recorded
        // (matches how a healthy tool copes with a machine that mangles keys).
        let mut rec = Recorder::new(RecordConfig::default());
        let t0 = Instant::now();
        assert!(!rec.push(ms(t0, 0), EventKind::KeyPress(Key::Unknown(215))));
        assert!(rec.push(ms(t0, 10), EventKind::KeyPress(Key::KeyD)));
        assert!(rec.push(ms(t0, 40), EventKind::KeyRelease(Key::KeyD)));
        assert!(!rec.push(ms(t0, 50), EventKind::KeyRelease(Key::Unknown(215))));
        assert_eq!(rec.dropped_unknown(), 2);
        let kinds: Vec<_> = rec.finish(ms(t0, 1000)).iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            vec![
                EventKind::KeyPress(Key::KeyD),
                EventKind::KeyRelease(Key::KeyD),
            ]
        );
    }

    #[test]
    fn dropping_unknowns_keeps_real_keys_and_modifiers() {
        // A real chord recorded alongside phantom presses: only the junk goes.
        let mut rec = Recorder::new(RecordConfig::default());
        let t0 = Instant::now();
        rec.push(ms(t0, 0), EventKind::KeyPress(Key::ControlLeft));
        rec.push(ms(t0, 10), EventKind::KeyPress(Key::Unknown(215)));
        rec.push(ms(t0, 20), EventKind::KeyPress(Key::KeyD));
        rec.push(ms(t0, 40), EventKind::KeyRelease(Key::KeyD));
        rec.push(ms(t0, 90), EventKind::KeyRelease(Key::ControlLeft));
        assert_eq!(rec.dropped_unknown(), 1);
        let kinds: Vec<_> = rec.finish(ms(t0, 2000)).iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            vec![
                EventKind::KeyPress(Key::ControlLeft),
                EventKind::KeyPress(Key::KeyD),
                EventKind::KeyRelease(Key::KeyD),
                EventKind::KeyRelease(Key::ControlLeft),
            ]
        );
    }

    #[test]
    fn phantom_flood_is_fully_dropped() {
        // A stuck phantom key auto-repeating at ~10 Hz: nothing survives.
        let mut rec = Recorder::new(RecordConfig::default());
        let t0 = Instant::now();
        for i in 0..5 {
            assert!(!rec.push(ms(t0, i * 90), EventKind::KeyPress(Key::Unknown(215))));
        }
        assert_eq!(rec.dropped_unknown(), 5);
        assert!(rec.finish(ms(t0, 1000)).is_empty());
    }

    #[test]
    fn delays_are_relative_and_first_is_zero() {
        let mut rec = Recorder::new(RecordConfig::default());
        let t0 = Instant::now();
        rec.push(ms(t0, 100), EventKind::KeyPress(Key::KeyB));
        rec.push(ms(t0, 130), EventKind::KeyRelease(Key::KeyB));
        rec.push(ms(t0, 200), EventKind::KeyPress(Key::KeyC));
        let events = rec.finish(ms(t0, 5000));
        assert_eq!(events[0].delay_us, 0);
        assert_eq!(events[1].delay_us, 30_000);
        assert_eq!(events[2].delay_us, 70_000);
    }
}
