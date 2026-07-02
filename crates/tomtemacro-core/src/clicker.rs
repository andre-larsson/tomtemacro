//! The auto-clicker: a drift-free press loop — mouse button or keyboard key —
//! with optional humanized jitter.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::inject::{InjectError, Injector};
use crate::model::{EventKind, Key, MouseButton};
use crate::timing;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClickKind {
    Single,
    Double,
}

/// What gets pressed on every tick of the loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClickTarget {
    Button(MouseButton),
    Key(Key),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ClickPosition {
    FollowCursor,
    Fixed { x: i32, y: i32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Jitter {
    /// ± fraction of the base interval (0.10 = up to ±10 % per click).
    pub interval_frac: f32,
    /// Clicks land uniformly within this radius of the target position.
    pub pos_radius_px: u16,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClickerConfig {
    pub interval: Duration,
    pub target: ClickTarget,
    pub click_kind: ClickKind,
    pub position: ClickPosition,
    pub jitter: Option<Jitter>,
    /// Stop after this many clicks (a double click counts as one).
    pub limit: Option<u64>,
}

/// Gap between the two presses of a double click (or key double-tap).
const DOUBLE_CLICK_GAP: Duration = Duration::from_millis(30);
/// Jitter never pushes an interval below this.
const MIN_INTERVAL: Duration = Duration::from_micros(500);

/// Run the click loop until the limit is reached or `stop` is set.
/// `clicks_done` is updated after every click for live UI counters.
/// Returns the number of clicks performed.
pub fn run(
    injector: &mut dyn Injector,
    config: &ClickerConfig,
    stop: &AtomicBool,
    clicks_done: &AtomicU64,
) -> Result<u64, InjectError> {
    let mut rng = fastrand::Rng::new();
    let mut count: u64 = 0;
    let mut deadline = Instant::now();
    loop {
        if let Some(limit) = config.limit {
            if count >= limit {
                break;
            }
        }
        if !timing::wait_until(deadline, stop) {
            break;
        }
        click_once(injector, config, &mut rng)?;
        count += 1;
        clicks_done.store(count, Ordering::Relaxed);

        // Absolute scheduling: advance from the previous deadline, not from
        // `now`, so injection latency doesn't accumulate as drift...
        deadline += jittered_interval(config.interval, config.jitter, &mut rng);
        // ...but if one click took longer than a whole interval, reschedule
        // from now instead of firing a catch-up burst.
        let now = Instant::now();
        if deadline < now {
            deadline = now;
        }
    }
    Ok(count)
}

fn click_once(
    injector: &mut dyn Injector,
    config: &ClickerConfig,
    rng: &mut fastrand::Rng,
) -> Result<(), InjectError> {
    if let ClickPosition::Fixed { x, y } = config.position {
        let (x, y) = jittered_position(x, y, config.jitter, rng);
        injector.inject(&EventKind::MouseMove {
            x: f64::from(x),
            y: f64::from(y),
        })?;
    }
    press_release(injector, config.target)?;
    if config.click_kind == ClickKind::Double {
        spin_sleep::sleep(DOUBLE_CLICK_GAP);
        press_release(injector, config.target)?;
    }
    Ok(())
}

fn press_release(injector: &mut dyn Injector, target: ClickTarget) -> Result<(), InjectError> {
    match target {
        ClickTarget::Button(button) => {
            injector.inject(&EventKind::ButtonPress(button))?;
            injector.inject(&EventKind::ButtonRelease(button))
        }
        ClickTarget::Key(key) => {
            injector.inject(&EventKind::KeyPress(key))?;
            injector.inject(&EventKind::KeyRelease(key))
        }
    }
}

fn jittered_interval(base: Duration, jitter: Option<Jitter>, rng: &mut fastrand::Rng) -> Duration {
    let Some(jitter) = jitter else { return base };
    if jitter.interval_frac <= 0.0 {
        return base;
    }
    let factor = 1.0 + f64::from(jitter.interval_frac) * (rng.f64() * 2.0 - 1.0);
    base.mul_f64(factor.max(0.0)).max(MIN_INTERVAL)
}

fn jittered_position(
    x: i32,
    y: i32,
    jitter: Option<Jitter>,
    rng: &mut fastrand::Rng,
) -> (i32, i32) {
    let Some(jitter) = jitter else { return (x, y) };
    if jitter.pos_radius_px == 0 {
        return (x, y);
    }
    // sqrt(u) makes the offset uniform over the disk, not clustered center.
    let radius = f64::from(jitter.pos_radius_px) * rng.f64().sqrt();
    let angle = rng.f64() * std::f64::consts::TAU;
    (
        x + (radius * angle.cos()).round() as i32,
        y + (radius * angle.sin()).round() as i32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    /// Records the wall-clock time of every button press instead of touching
    /// the OS — lets the cadence be verified headless.
    struct MockInjector {
        presses: Vec<Instant>,
        moves: Vec<(f64, f64)>,
        kinds: Vec<EventKind>,
    }

    impl MockInjector {
        fn new() -> Self {
            Self {
                presses: Vec::new(),
                moves: Vec::new(),
                kinds: Vec::new(),
            }
        }
    }

    impl Injector for MockInjector {
        fn inject(&mut self, kind: &EventKind) -> Result<(), InjectError> {
            self.kinds.push(*kind);
            match kind {
                EventKind::ButtonPress(_) => self.presses.push(Instant::now()),
                EventKind::MouseMove { x, y } => self.moves.push((*x, *y)),
                _ => {}
            }
            Ok(())
        }

        fn cursor_location(&mut self) -> Result<(i32, i32), InjectError> {
            Ok((0, 0))
        }
    }

    fn base_config() -> ClickerConfig {
        ClickerConfig {
            interval: Duration::from_millis(20),
            target: ClickTarget::Button(MouseButton::Left),
            click_kind: ClickKind::Single,
            position: ClickPosition::FollowCursor,
            jitter: None,
            limit: Some(25),
        }
    }

    #[test]
    fn respects_limit_and_cadence() {
        let mut mock = MockInjector::new();
        let stop = AtomicBool::new(false);
        let counter = AtomicU64::new(0);
        let count = run(&mut mock, &base_config(), &stop, &counter).unwrap();

        assert_eq!(count, 25);
        assert_eq!(mock.presses.len(), 25);
        assert_eq!(counter.load(Ordering::Relaxed), 25);

        let errors_us: Vec<i64> = mock
            .presses
            .windows(2)
            .map(|w| w[1].duration_since(w[0]).as_micros() as i64 - 20_000)
            .collect();
        let mean_abs_us = errors_us.iter().map(|e| e.abs()).sum::<i64>() / errors_us.len() as i64;
        // < 1 ms on real hardware. Hosted CI runners (macOS especially) have
        // contended schedulers that stall for tens of ms — only sanity-check
        // there; precision is properly measured locally and by examples/clicker.
        let limit_us = if std::env::var_os("CI").is_some() {
            20_000
        } else {
            5_000
        };
        assert!(
            mean_abs_us < limit_us,
            "mean interval error {mean_abs_us} µs"
        );
    }

    #[test]
    fn stop_flag_interrupts_promptly() {
        let mut mock = MockInjector::new();
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let counter = AtomicU64::new(0);
        let mut config = base_config();
        config.interval = Duration::from_secs(10);
        config.limit = None;

        let s = stop.clone();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            s.store(true, Ordering::Relaxed);
        });
        let begin = Instant::now();
        let count = run(&mut mock, &config, &stop, &counter).unwrap();
        let elapsed = begin.elapsed();

        assert_eq!(count, 1); // the first click fires immediately
        assert!(elapsed < Duration::from_millis(500), "took {elapsed:?}");
        handle.join().unwrap();
    }

    #[test]
    fn key_target_taps_the_key() {
        let mut mock = MockInjector::new();
        let stop = AtomicBool::new(false);
        let counter = AtomicU64::new(0);
        let mut config = base_config();
        config.interval = Duration::from_millis(1);
        config.target = ClickTarget::Key(Key::KeyE);
        config.limit = Some(3);

        let count = run(&mut mock, &config, &stop, &counter).unwrap();
        assert_eq!(count, 3);
        let expected = [
            EventKind::KeyPress(Key::KeyE),
            EventKind::KeyRelease(Key::KeyE),
        ]
        .repeat(3);
        assert_eq!(mock.kinds, expected);
    }

    #[test]
    fn double_click_presses_twice_per_click() {
        let mut mock = MockInjector::new();
        let stop = AtomicBool::new(false);
        let counter = AtomicU64::new(0);
        let mut config = base_config();
        config.click_kind = ClickKind::Double;
        config.limit = Some(3);

        let count = run(&mut mock, &config, &stop, &counter).unwrap();
        assert_eq!(count, 3);
        assert_eq!(mock.presses.len(), 6);
    }

    #[test]
    fn fixed_position_jitter_stays_within_radius() {
        let mut mock = MockInjector::new();
        let stop = AtomicBool::new(false);
        let counter = AtomicU64::new(0);
        let mut config = base_config();
        config.interval = Duration::from_millis(1);
        config.position = ClickPosition::Fixed { x: 500, y: 400 };
        config.jitter = Some(Jitter {
            interval_frac: 0.5,
            pos_radius_px: 10,
        });
        config.limit = Some(50);

        run(&mut mock, &config, &stop, &counter).unwrap();
        assert_eq!(mock.moves.len(), 50);
        for (x, y) in &mock.moves {
            let dist = ((x - 500.0).powi(2) + (y - 400.0).powi(2)).sqrt();
            // Rounding can push the offset just past the nominal radius.
            assert!(dist <= 11.0, "click at ({x}, {y}) is {dist:.1} px out");
        }
    }
}
