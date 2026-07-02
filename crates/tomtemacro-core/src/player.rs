//! Macro playback: replays events against absolute deadlines so timing
//! drift never accumulates, at any speed, with instant stop.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::inject::{InjectError, Injector};
use crate::model::MacroFile;
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

/// Replay `macro_file` until the repeat count is exhausted or `stop` is set.
/// `iterations_done` is updated after each full pass for live UI counters.
pub fn run(
    injector: &mut dyn Injector,
    macro_file: &MacroFile,
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

        let start = Instant::now();
        let mut cumulative_us: u64 = 0;
        for event in &macro_file.events {
            cumulative_us += event.delay_us;
            let deadline = start + Duration::from_secs_f64(cumulative_us as f64 / 1e6 / speed);
            if !timing::wait_until(deadline, stop) {
                return Ok(PlayOutcome::Stopped);
            }
            injector.inject(&event.kind)?;
        }

        done += 1;
        iterations_done.store(done, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EventKind, Key, MacroEvent, MacroFile, MacroMeta};

    struct TimestampingInjector {
        seen: Vec<(Instant, EventKind)>,
    }

    impl Injector for TimestampingInjector {
        fn inject(&mut self, kind: &EventKind) -> Result<(), InjectError> {
            self.seen.push((Instant::now(), *kind));
            Ok(())
        }
        fn cursor_location(&mut self) -> Result<(i32, i32), InjectError> {
            Ok((0, 0))
        }
    }

    fn three_key_macro() -> MacroFile {
        // A(0 ms) .. B(+40 ms) .. C(+40 ms) → 80 ms total at 1.0x.
        MacroFile::new(
            MacroMeta::default(),
            vec![
                MacroEvent {
                    delay_us: 0,
                    kind: EventKind::KeyPress(Key::KeyA),
                },
                MacroEvent {
                    delay_us: 40_000,
                    kind: EventKind::KeyPress(Key::KeyB),
                },
                MacroEvent {
                    delay_us: 40_000,
                    kind: EventKind::KeyPress(Key::KeyC),
                },
            ],
        )
    }

    #[test]
    fn plays_in_order_with_recorded_rhythm() {
        let mut injector = TimestampingInjector { seen: Vec::new() };
        let stop = AtomicBool::new(false);
        let iterations = AtomicU64::new(0);
        let begin = Instant::now();
        let outcome = run(
            &mut injector,
            &three_key_macro(),
            &PlaybackOptions::default(),
            &stop,
            &iterations,
        )
        .unwrap();
        let elapsed = begin.elapsed();

        assert_eq!(outcome, PlayOutcome::Completed);
        assert_eq!(injector.seen.len(), 3);
        assert_eq!(iterations.load(Ordering::Relaxed), 1);
        assert!(
            (Duration::from_millis(75)..Duration::from_millis(150)).contains(&elapsed),
            "expected ≈80 ms, took {elapsed:?}"
        );
        let gap = injector.seen[1].0.duration_since(injector.seen[0].0);
        assert!(
            (Duration::from_millis(35)..Duration::from_millis(60)).contains(&gap),
            "expected ≈40 ms between events, got {gap:?}"
        );
    }

    #[test]
    fn double_speed_halves_wall_time() {
        let mut injector = TimestampingInjector { seen: Vec::new() };
        let stop = AtomicBool::new(false);
        let iterations = AtomicU64::new(0);
        let options = PlaybackOptions {
            speed: 2.0,
            repeat: Repeat::Times(1),
        };
        let begin = Instant::now();
        run(
            &mut injector,
            &three_key_macro(),
            &options,
            &stop,
            &iterations,
        )
        .unwrap();
        let elapsed = begin.elapsed();
        assert!(
            elapsed < Duration::from_millis(70),
            "expected ≈40 ms at 2x, took {elapsed:?}"
        );
    }

    #[test]
    fn repeats_n_times() {
        let mut injector = TimestampingInjector { seen: Vec::new() };
        let stop = AtomicBool::new(false);
        let iterations = AtomicU64::new(0);
        let options = PlaybackOptions {
            speed: 10.0,
            repeat: Repeat::Times(3),
        };
        run(
            &mut injector,
            &three_key_macro(),
            &options,
            &stop,
            &iterations,
        )
        .unwrap();
        assert_eq!(injector.seen.len(), 9);
        assert_eq!(iterations.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn infinite_playback_stops_on_flag() {
        let mut injector = TimestampingInjector { seen: Vec::new() };
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
        let outcome = run(
            &mut injector,
            &three_key_macro(),
            &options,
            &stop,
            &iterations,
        )
        .unwrap();
        assert_eq!(outcome, PlayOutcome::Stopped);
        handle.join().unwrap();
    }
}
