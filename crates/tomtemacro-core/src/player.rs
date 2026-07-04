//! Macro playback: walks the script AST against absolute deadlines so
//! timing drift never accumulates, at any speed, with instant stop.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::clicker::DOUBLE_CLICK_GAP;
use crate::inject::{InjectError, Injector};
use crate::model::EventKind;
use crate::script::{Instr, Script, Stmt};
use crate::timing;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaybackOptions {
    /// 1.0 = recorded speed, 2.0 = twice as fast.
    pub speed: f64,
    pub repeat: Repeat,
}

impl Default for PlaybackOptions {
    fn default() -> Self {
        Self {
            speed: 1.0,
            repeat: Repeat::Times(1),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repeat {
    Times(u32),
    Infinite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayOutcome {
    Completed,
    Stopped,
}

/// Play a script until the repeat count is exhausted or `stop` is set.
/// `iterations_done` is updated after each full pass for live UI counters.
///
/// Only waits advance the timeline; injections in between run back-to-back.
/// Ranged waits are resampled on every encounter, relative moves resolve
/// against the cursor position at that moment.
pub fn run_script(
    injector: &mut dyn Injector,
    script: &Script,
    options: &PlaybackOptions,
    stop: &AtomicBool,
    iterations_done: &AtomicU64,
) -> Result<PlayOutcome, InjectError> {
    let speed = options.speed.max(0.01);
    let mut done: u64 = 0;
    loop {
        match options.repeat {
            Repeat::Times(n) if done >= u64::from(n) => return Ok(PlayOutcome::Completed),
            _ => {}
        }

        let mut clock = Clock {
            start: Instant::now(),
            cumulative_us: 0,
            speed,
        };
        if exec_block(injector, &script.body, &mut clock, stop)? == Flow::Stopped {
            return Ok(PlayOutcome::Stopped);
        }

        done += 1;
        iterations_done.store(done, Ordering::Relaxed);
    }
}

#[derive(PartialEq, Eq)]
enum Flow {
    Ran,
    Stopped,
}

/// Macro time within one iteration. Deadlines are absolute against `start`
/// so timing drift never accumulates.
struct Clock {
    start: Instant,
    cumulative_us: u64,
    speed: f64,
}

impl Clock {
    /// Advance macro time by `us` and sleep until the new deadline.
    /// Returns false if stopped while waiting.
    fn advance(&mut self, us: u64, stop: &AtomicBool) -> bool {
        self.cumulative_us += us;
        let deadline =
            self.start + Duration::from_secs_f64(self.cumulative_us as f64 / 1e6 / self.speed);
        timing::wait_until(deadline, stop)
    }
}

fn exec_block(
    injector: &mut dyn Injector,
    body: &[Stmt],
    clock: &mut Clock,
    stop: &AtomicBool,
) -> Result<Flow, InjectError> {
    for stmt in body {
        // Wait-free stretches (dense clicks, long repeats) must stay stoppable.
        if stop.load(Ordering::Relaxed) {
            return Ok(Flow::Stopped);
        }
        match &stmt.instr {
            Instr::Nop => {}
            Instr::Move { x, y } => injector.inject(&EventKind::MouseMove {
                x: f64::from(*x),
                y: f64::from(*y),
            })?,
            Instr::MoveRel { dx, dy } => {
                let (x, y) = injector.cursor_location()?;
                injector.inject(&EventKind::MouseMove {
                    x: f64::from(x + dx),
                    y: f64::from(y + dy),
                })?;
            }
            Instr::Click { button, at, double } => {
                if let Some((x, y)) = at {
                    injector.inject(&EventKind::MouseMove {
                        x: f64::from(*x),
                        y: f64::from(*y),
                    })?;
                }
                injector.inject(&EventKind::ButtonPress(*button))?;
                injector.inject(&EventKind::ButtonRelease(*button))?;
                if *double {
                    if !clock.advance(DOUBLE_CLICK_GAP.as_micros() as u64, stop) {
                        return Ok(Flow::Stopped);
                    }
                    injector.inject(&EventKind::ButtonPress(*button))?;
                    injector.inject(&EventKind::ButtonRelease(*button))?;
                }
            }
            Instr::MouseDown(button) => injector.inject(&EventKind::ButtonPress(*button))?,
            Instr::MouseUp(button) => injector.inject(&EventKind::ButtonRelease(*button))?,
            Instr::Scroll { dx, dy } => injector.inject(&EventKind::Wheel { dx: *dx, dy: *dy })?,
            Instr::KeyTap(key) => {
                injector.inject(&EventKind::KeyPress(*key))?;
                injector.inject(&EventKind::KeyRelease(*key))?;
            }
            Instr::KeyDown(key) => injector.inject(&EventKind::KeyPress(*key))?,
            Instr::KeyUp(key) => injector.inject(&EventKind::KeyRelease(*key))?,
            Instr::Type(text) => {
                // Typing takes real wall time the deadline scheme doesn't
                // know about; bill it to the clock afterwards so following
                // waits keep their intended length.
                let before = Instant::now();
                injector.type_text(text)?;
                clock.cumulative_us += (before.elapsed().as_secs_f64() * 1e6 * clock.speed) as u64;
            }
            Instr::Wait { min_us, max_us } => {
                if !clock.advance(fastrand::u64(*min_us..=*max_us), stop) {
                    return Ok(Flow::Stopped);
                }
            }
            Instr::Repeat { count, body, .. } => {
                for _ in 0..*count {
                    if exec_block(injector, body, clock, stop)? == Flow::Stopped {
                        return Ok(Flow::Stopped);
                    }
                }
            }
        }
    }
    Ok(Flow::Ran)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Key;

    struct TimestampingInjector {
        seen: Vec<(Instant, EventKind)>,
        cursor: (i32, i32),
    }

    impl TimestampingInjector {
        fn new() -> Self {
            Self {
                seen: Vec::new(),
                cursor: (0, 0),
            }
        }
        fn kinds(&self) -> Vec<EventKind> {
            self.seen.iter().map(|(_, kind)| *kind).collect()
        }
    }

    impl Injector for TimestampingInjector {
        fn inject(&mut self, kind: &EventKind) -> Result<(), InjectError> {
            if let EventKind::MouseMove { x, y } = kind {
                self.cursor = (*x as i32, *y as i32);
            }
            self.seen.push((Instant::now(), *kind));
            Ok(())
        }
        fn cursor_location(&mut self) -> Result<(i32, i32), InjectError> {
            Ok(self.cursor)
        }
    }

    // --- run_script ---

    fn script_of(text: &str) -> Script {
        crate::script::parse(text).expect("test script parses")
    }

    fn play(
        injector: &mut TimestampingInjector,
        text: &str,
        options: &PlaybackOptions,
    ) -> PlayOutcome {
        let stop = AtomicBool::new(false);
        let iterations = AtomicU64::new(0);
        run_script(injector, &script_of(text), options, &stop, &iterations).unwrap()
    }

    #[test]
    fn script_plays_with_scripted_rhythm() {
        let mut injector = TimestampingInjector::new();
        let begin = Instant::now();
        let outcome = play(
            &mut injector,
            "press a\nwait 40ms\npress b\nwait 40ms\npress c\n",
            &PlaybackOptions::default(),
        );
        let elapsed = begin.elapsed();

        assert_eq!(outcome, PlayOutcome::Completed);
        assert_eq!(injector.seen.len(), 6); // three press+release taps
        assert!(
            (Duration::from_millis(75)..Duration::from_millis(150)).contains(&elapsed),
            "expected ≈80 ms, took {elapsed:?}"
        );
        // Tap of B (index 2) lands ≈40 ms after tap of A (index 0).
        let gap = injector.seen[2].0.duration_since(injector.seen[0].0);
        assert!(
            (Duration::from_millis(35)..Duration::from_millis(60)).contains(&gap),
            "expected ≈40 ms between taps, got {gap:?}"
        );
    }

    #[test]
    fn script_double_speed_halves_wall_time() {
        let mut injector = TimestampingInjector::new();
        let options = PlaybackOptions {
            speed: 2.0,
            repeat: Repeat::Times(1),
        };
        let begin = Instant::now();
        play(&mut injector, "wait 40ms\npress a\nwait 40ms\n", &options);
        let elapsed = begin.elapsed();
        assert!(
            elapsed < Duration::from_millis(70),
            "expected ≈40 ms at 2x, took {elapsed:?}"
        );
    }

    #[test]
    fn nested_repeats_multiply_in_order() {
        let mut injector = TimestampingInjector::new();
        play(
            &mut injector,
            "repeat 2\n  press a\n  repeat 3\n    press b\n  end\nend\n",
            &PlaybackOptions::default(),
        );
        let taps: Vec<Key> = injector
            .kinds()
            .into_iter()
            .filter_map(|kind| match kind {
                EventKind::KeyPress(key) => Some(key),
                _ => None,
            })
            .collect();
        assert_eq!(
            taps,
            vec![
                Key::KeyA,
                Key::KeyB,
                Key::KeyB,
                Key::KeyB,
                Key::KeyA,
                Key::KeyB,
                Key::KeyB,
                Key::KeyB,
            ]
        );
    }

    #[test]
    fn relative_moves_resolve_against_live_cursor() {
        let mut injector = TimestampingInjector::new();
        injector.cursor = (100, 100);
        play(
            &mut injector,
            "move +10 -20\nmoverel -5 -5\nmove 50 60\nmove +1 +2\n",
            &PlaybackOptions::default(),
        );
        let moves: Vec<(i32, i32)> = injector
            .kinds()
            .into_iter()
            .filter_map(|kind| match kind {
                EventKind::MouseMove { x, y } => Some((x as i32, y as i32)),
                _ => None,
            })
            .collect();
        assert_eq!(moves, vec![(110, 80), (105, 75), (50, 60), (51, 62)]);
    }

    #[test]
    fn click_at_moves_then_clicks_and_doubleclick_presses_twice() {
        let mut injector = TimestampingInjector::new();
        play(
            &mut injector,
            "click left at 10 20\ndoubleclick right\n",
            &PlaybackOptions::default(),
        );
        let kinds = injector.kinds();
        assert!(matches!(kinds[0], EventKind::MouseMove { .. }));
        assert!(matches!(kinds[1], EventKind::ButtonPress(_)));
        assert!(matches!(kinds[2], EventKind::ButtonRelease(_)));
        // Doubleclick: two full press/release pairs.
        assert_eq!(
            kinds[3..],
            [
                EventKind::ButtonPress(crate::model::MouseButton::Right),
                EventKind::ButtonRelease(crate::model::MouseButton::Right),
                EventKind::ButtonPress(crate::model::MouseButton::Right),
                EventKind::ButtonRelease(crate::model::MouseButton::Right),
            ]
        );
    }

    #[test]
    fn ranged_waits_stay_inside_the_envelope() {
        let mut injector = TimestampingInjector::new();
        let options = PlaybackOptions {
            speed: 1.0,
            repeat: Repeat::Times(5),
        };
        let begin = Instant::now();
        play(&mut injector, "wait 10ms..30ms\npress a\n", &options);
        let elapsed = begin.elapsed();
        assert!(
            elapsed >= Duration::from_millis(45),
            "five ≥10 ms waits took only {elapsed:?}"
        );
        assert!(
            elapsed < Duration::from_millis(400),
            "five ≤30 ms waits took {elapsed:?}"
        );
    }

    #[test]
    fn type_expands_to_shifted_taps() {
        let mut injector = TimestampingInjector::new();
        play(&mut injector, "type \"Ab\"\n", &PlaybackOptions::default());
        assert_eq!(
            injector.kinds(),
            vec![
                EventKind::KeyPress(Key::ShiftLeft),
                EventKind::KeyPress(Key::KeyA),
                EventKind::KeyRelease(Key::KeyA),
                EventKind::KeyRelease(Key::ShiftLeft),
                EventKind::KeyPress(Key::KeyB),
                EventKind::KeyRelease(Key::KeyB),
            ]
        );
    }

    #[test]
    fn infinite_playback_stops_on_flag() {
        let mut injector = TimestampingInjector::new();
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let iterations = AtomicU64::new(0);
        let options = PlaybackOptions {
            speed: 1.0,
            repeat: Repeat::Infinite,
        };
        let s = stop.clone();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(60));
            s.store(true, Ordering::Relaxed);
        });
        let outcome = run_script(
            &mut injector,
            &script_of("press a\nwait 20ms\n"),
            &options,
            &stop,
            &iterations,
        )
        .unwrap();
        assert_eq!(outcome, PlayOutcome::Stopped);
        handle.join().unwrap();
    }

    #[test]
    fn stop_lands_mid_repeat() {
        let mut injector = TimestampingInjector::new();
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let iterations = AtomicU64::new(0);
        let script = script_of("repeat 100000\n  press a\n  wait 10ms\nend\n");
        let s = stop.clone();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            s.store(true, Ordering::Relaxed);
        });
        let begin = Instant::now();
        let outcome = run_script(
            &mut injector,
            &script,
            &PlaybackOptions::default(),
            &stop,
            &iterations,
        )
        .unwrap();
        handle.join().unwrap();
        assert_eq!(outcome, PlayOutcome::Stopped);
        assert!(
            begin.elapsed() < Duration::from_millis(500),
            "stop should land promptly"
        );
    }
}
