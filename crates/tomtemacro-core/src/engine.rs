//! The engine: a long-lived controller thread that owns the injector and
//! runs one activity at a time (clicking, and from phase 3 recording and
//! playback).
//!
//! Concurrency contract:
//! - **Commands** (start/stop/shutdown) arrive on a channel.
//! - **Continuous telemetry** (mode, live counters) lives in [`SharedState`]
//!   atomics so the GUI can render it every frame without message churn.
//! - **Stopping** is a shared atomic, not (only) a message: activities poll
//!   it every few milliseconds, so a stop issued from a global hotkey lands
//!   mid-interval even while the engine is deep in a click loop.
//! - Start commands that arrive while an activity is running are dropped
//!   when it finishes — a queued-up "start" firing the moment you stop
//!   would be surprising (and is one half of the injected-events-triggering-
//!   our-own-hotkeys defense; the recorder's chord-stripping is the other).

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use crossbeam_channel::{unbounded, Receiver, Sender};

use crate::clicker::{self, ClickerConfig};
use crate::inject::{EnigoInjector, InjectError, Injector};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Mode {
    Idle = 0,
    Recording = 1,
    Playing = 2,
    Clicking = 3,
}

impl Mode {
    fn from_u8(value: u8) -> Self {
        match value {
            1 => Mode::Recording,
            2 => Mode::Playing,
            3 => Mode::Clicking,
            _ => Mode::Idle,
        }
    }
}

/// Lock-free state shared between the engine, the GUI, and hotkey handlers.
#[derive(Debug, Default)]
pub struct SharedState {
    mode: AtomicU8,
    /// Set to request the current activity to stop. Cleared by the engine
    /// when a new activity starts.
    stop: AtomicBool,
    pub clicks_done: AtomicU64,
    pub events_recorded: AtomicU64,
    pub playback_iteration: AtomicU64,
}

impl SharedState {
    pub fn mode(&self) -> Mode {
        Mode::from_u8(self.mode.load(Ordering::Relaxed))
    }

    fn set_mode(&self, mode: Mode) {
        self.mode.store(mode as u8, Ordering::Relaxed);
    }

    /// Ask the running activity to stop. Safe from any thread.
    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    pub fn stop_flag(&self) -> &AtomicBool {
        &self.stop
    }
}

#[derive(Debug)]
pub enum Command {
    StartClicker(ClickerConfig),
    /// No-op if idle; otherwise equivalent to `SharedState::request_stop`.
    StopActivity,
    Shutdown,
}

#[derive(Debug)]
pub enum Status {
    ModeChanged(Mode),
    Finished {
        mode: Mode,
        reason: FinishReason,
    },
    /// Engine hit an unrecoverable error and shut down.
    Fatal(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinishReason {
    /// Ran to its natural end (e.g. click limit reached).
    Completed,
    /// Stopped by request.
    Stopped,
    /// The activity failed mid-run (details arrive as a `Fatal` only if the
    /// engine can't continue; injection errors end the activity, not the
    /// engine).
    Failed,
}

/// Called after every status push — the GUI passes a closure that requests a
/// repaint so status changes render immediately.
pub type Wake = Box<dyn Fn() + Send>;

pub struct EngineHandle {
    pub shared: Arc<SharedState>,
    commands: Sender<Command>,
    pub status: Receiver<Status>,
    thread: Option<JoinHandle<()>>,
}

impl EngineHandle {
    /// Spawn with the real enigo injector.
    pub fn spawn(wake: Option<Wake>) -> Self {
        Self::spawn_with(EnigoInjector::new, wake)
    }

    /// Spawn with a custom injector factory. The factory runs *on the engine
    /// thread* — OS input handles aren't reliably transferable across
    /// threads on every platform.
    pub fn spawn_with<F, I>(make_injector: F, wake: Option<Wake>) -> Self
    where
        F: FnOnce() -> Result<I, InjectError> + Send + 'static,
        I: Injector + 'static,
    {
        let (command_tx, command_rx) = unbounded();
        let (status_tx, status_rx) = unbounded();
        let shared = Arc::new(SharedState::default());
        let shared_for_thread = shared.clone();
        let thread = std::thread::Builder::new()
            .name("tomte-engine".into())
            .spawn(move || {
                engine_main(
                    command_rx,
                    status_tx,
                    shared_for_thread,
                    make_injector,
                    wake,
                );
            })
            .expect("failed to spawn engine thread");
        Self {
            shared,
            commands: command_tx,
            status: status_rx,
            thread: Some(thread),
        }
    }

    pub fn send(&self, command: Command) {
        // A send only fails after the engine died; Fatal was already pushed.
        let _ = self.commands.send(command);
    }

    /// Stop whatever is running, without shutting the engine down.
    pub fn request_stop(&self) {
        self.shared.request_stop();
        self.send(Command::StopActivity);
    }

    /// Convenience for hotkey handlers: start if idle, stop if busy.
    pub fn toggle_clicker(&self, config: ClickerConfig) {
        if self.shared.mode() == Mode::Idle {
            self.send(Command::StartClicker(config));
        } else {
            self.request_stop();
        }
    }
}

impl Drop for EngineHandle {
    fn drop(&mut self) {
        self.shared.request_stop();
        let _ = self.commands.send(Command::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn engine_main<F, I>(
    commands: Receiver<Command>,
    status: Sender<Status>,
    shared: Arc<SharedState>,
    make_injector: F,
    wake: Option<Wake>,
) where
    F: FnOnce() -> Result<I, InjectError>,
    I: Injector,
{
    let push = |s: Status| {
        let _ = status.send(s);
        if let Some(wake) = &wake {
            wake();
        }
    };

    let mut injector = match make_injector() {
        Ok(injector) => injector,
        Err(e) => {
            push(Status::Fatal(format!("injection backend: {e}")));
            return;
        }
    };

    loop {
        let Ok(command) = commands.recv() else {
            break; // all senders dropped
        };
        match command {
            Command::Shutdown => break,
            Command::StopActivity => {} // already idle
            Command::StartClicker(config) => {
                run_activity(Mode::Clicking, &shared, &push, |shared| {
                    shared.clicks_done.store(0, Ordering::Relaxed);
                    let hit_limit = config.limit.is_some();
                    match clicker::run(
                        &mut injector,
                        &config,
                        shared.stop_flag(),
                        &shared.clicks_done,
                    ) {
                        Ok(n) if hit_limit && Some(n) == config.limit => FinishReason::Completed,
                        Ok(_) => FinishReason::Stopped,
                        Err(e) => {
                            log::error!("clicker failed: {e}");
                            FinishReason::Failed
                        }
                    }
                });
                if drain_stale_starts(&commands) {
                    break; // shutdown arrived while we were busy
                }
            }
        }
    }
    shared.set_mode(Mode::Idle);
}

/// Standard bracket around any activity: clear the stop flag, flip the mode,
/// announce both transitions, and report how the activity ended.
fn run_activity(
    mode: Mode,
    shared: &SharedState,
    push: &impl Fn(Status),
    body: impl FnOnce(&SharedState) -> FinishReason,
) {
    shared.stop.store(false, Ordering::Relaxed);
    shared.set_mode(mode);
    push(Status::ModeChanged(mode));

    let reason = body(shared);

    shared.set_mode(Mode::Idle);
    push(Status::Finished { mode, reason });
    push(Status::ModeChanged(Mode::Idle));
}

/// Drop start commands that queued up while an activity was running (a
/// queued "start" firing the moment you stop would be surprising). A stale
/// stop is harmless to drop too — we're idle. Returns true if a shutdown
/// was among them.
fn drain_stale_starts(commands: &Receiver<Command>) -> bool {
    while let Ok(command) = commands.try_recv() {
        match command {
            Command::Shutdown => return true,
            other => log::debug!("dropping stale command queued during activity: {other:?}"),
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clicker::{ClickKind, ClickPosition};
    use crate::model::{EventKind, MouseButton};
    use std::time::Duration;

    struct NullInjector;

    impl Injector for NullInjector {
        fn inject(&mut self, _: &EventKind) -> Result<(), InjectError> {
            Ok(())
        }
        fn cursor_location(&mut self) -> Result<(i32, i32), InjectError> {
            Ok((0, 0))
        }
    }

    fn config(limit: Option<u64>) -> ClickerConfig {
        ClickerConfig {
            interval: Duration::from_millis(5),
            button: MouseButton::Left,
            click_kind: ClickKind::Single,
            position: ClickPosition::FollowCursor,
            jitter: None,
            limit,
        }
    }

    fn recv_status(engine: &EngineHandle) -> Status {
        engine
            .status
            .recv_timeout(Duration::from_secs(5))
            .expect("status in time")
    }

    #[test]
    fn limited_clicker_completes() {
        let engine = EngineHandle::spawn_with(|| Ok(NullInjector), None);
        engine.send(Command::StartClicker(config(Some(10))));

        assert!(matches!(
            recv_status(&engine),
            Status::ModeChanged(Mode::Clicking)
        ));
        assert!(matches!(
            recv_status(&engine),
            Status::Finished {
                mode: Mode::Clicking,
                reason: FinishReason::Completed
            }
        ));
        assert!(matches!(
            recv_status(&engine),
            Status::ModeChanged(Mode::Idle)
        ));
        assert_eq!(engine.shared.clicks_done.load(Ordering::Relaxed), 10);
    }

    #[test]
    fn infinite_clicker_stops_on_request() {
        let engine = EngineHandle::spawn_with(|| Ok(NullInjector), None);
        engine.send(Command::StartClicker(config(None)));
        assert!(matches!(
            recv_status(&engine),
            Status::ModeChanged(Mode::Clicking)
        ));

        std::thread::sleep(Duration::from_millis(30));
        let begin = std::time::Instant::now();
        engine.request_stop();
        assert!(matches!(
            recv_status(&engine),
            Status::Finished {
                reason: FinishReason::Stopped,
                ..
            }
        ));
        assert!(begin.elapsed() < Duration::from_millis(200));
    }

    #[test]
    fn starts_queued_during_activity_are_dropped() {
        let engine = EngineHandle::spawn_with(|| Ok(NullInjector), None);
        engine.send(Command::StartClicker(config(None)));
        assert!(matches!(
            recv_status(&engine),
            Status::ModeChanged(Mode::Clicking)
        ));

        // Queue a second start while the first is still running, then stop.
        engine.send(Command::StartClicker(config(None)));
        engine.request_stop();

        // Finished + Idle for the first activity...
        assert!(matches!(recv_status(&engine), Status::Finished { .. }));
        assert!(matches!(
            recv_status(&engine),
            Status::ModeChanged(Mode::Idle)
        ));
        // ...and the queued start must NOT begin a second one.
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(engine.shared.mode(), Mode::Idle);
        assert!(engine.status.try_recv().is_err());
    }

    #[test]
    fn toggle_starts_then_stops() {
        let engine = EngineHandle::spawn_with(|| Ok(NullInjector), None);
        engine.toggle_clicker(config(None));
        assert!(matches!(
            recv_status(&engine),
            Status::ModeChanged(Mode::Clicking)
        ));
        std::thread::sleep(Duration::from_millis(20));
        engine.toggle_clicker(config(None));
        assert!(matches!(
            recv_status(&engine),
            Status::Finished {
                reason: FinishReason::Stopped,
                ..
            }
        ));
    }
}
