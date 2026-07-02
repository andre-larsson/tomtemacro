//! Precise, interruptible waits.
//!
//! Everything schedules against *absolute* deadlines (`start + n * interval`),
//! never per-delta sleeps — absolute deadlines self-correct: a late event
//! shortens the next wait instead of accumulating drift.
//!
//! Waits are coarse-then-precise: native sleep in short chunks (so a stop
//! request is honored within a few ms), then a spin-sleep tail for the final
//! stretch to get millisecond precision even on Windows' ~15.6 ms timer.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;

/// Final stretch that is spin-slept for precision.
const SPIN_TAIL: Duration = Duration::from_millis(3);
/// How often the stop flag is polled during the coarse phase.
const CHECK_EVERY: Duration = Duration::from_millis(5);

/// Sleep until `deadline`, polling `stop` every few milliseconds.
/// Returns `true` if the deadline was reached, `false` if stopped first.
pub fn wait_until(deadline: Instant, stop: &AtomicBool) -> bool {
    loop {
        if stop.load(Ordering::Relaxed) {
            return false;
        }
        let now = Instant::now();
        if now >= deadline {
            return true;
        }
        let remaining = deadline - now;
        if remaining <= SPIN_TAIL {
            spin_sleep::sleep(remaining);
            return !stop.load(Ordering::Relaxed);
        }
        std::thread::sleep((remaining - SPIN_TAIL).min(CHECK_EVERY));
    }
}

/// Outcome of an interruptible wait.
pub enum Wait<T> {
    /// Deadline reached.
    Reached,
    /// The stop flag was set.
    Stopped,
    /// A message arrived before the deadline.
    Message(T),
    /// The channel disconnected (sender dropped) — treat as shutdown.
    Disconnected,
}

/// Sleep until `deadline`, waking immediately if a message arrives on `rx`
/// or (within a few ms) if `stop` is set. Used by the engine thread so a
/// STOP command interrupts playback mid-interval.
pub fn wait_until_or_msg<T>(deadline: Instant, rx: &Receiver<T>, stop: &AtomicBool) -> Wait<T> {
    loop {
        if stop.load(Ordering::Relaxed) {
            return Wait::Stopped;
        }
        let now = Instant::now();
        if now >= deadline {
            return Wait::Reached;
        }
        let remaining = deadline - now;
        if remaining <= SPIN_TAIL {
            spin_sleep::sleep(remaining);
            return if stop.load(Ordering::Relaxed) {
                Wait::Stopped
            } else {
                Wait::Reached
            };
        }
        // recv_timeout wakes instantly on a message; the timeout chunk keeps
        // the stop-flag poll responsive.
        match rx.recv_timeout((remaining - SPIN_TAIL).min(CHECK_EVERY)) {
            Ok(msg) => return Wait::Message(msg),
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return Wait::Disconnected,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn wait_reaches_deadline_with_ms_precision() {
        let stop = AtomicBool::new(false);
        let target = Instant::now() + Duration::from_millis(25);
        assert!(wait_until(target, &stop));
        let overshoot = Instant::now().duration_since(target);
        // < 100 µs on real hardware. Hosted CI runners have contended
        // schedulers that stall for tens of ms — only sanity-check there.
        let limit = if std::env::var_os("CI").is_some() {
            Duration::from_millis(50)
        } else {
            Duration::from_millis(5)
        };
        assert!(overshoot < limit, "overshot by {overshoot:?}");
    }

    #[test]
    fn wait_stops_quickly() {
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let deadline = Instant::now() + Duration::from_secs(10);
        let s = stop.clone();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            s.store(true, Ordering::Relaxed);
        });
        let begin = Instant::now();
        assert!(!wait_until(deadline, &stop));
        let elapsed = begin.elapsed();
        assert!(elapsed < Duration::from_millis(200), "took {elapsed:?}");
        handle.join().unwrap();
    }
}
