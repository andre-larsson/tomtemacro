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

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, LazyLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::{select, unbounded, Receiver, RecvTimeoutError, Sender};

use crate::capture::CaptureEvent;
use crate::clicker::{self, ClickerConfig};
use crate::inject::{EnigoInjector, InjectError, Injector};
use crate::model::{EventKind, Key, MacroMeta, MouseButton};
use crate::platform;
use crate::player::{self, PlayOutcome, PlaybackOptions};
use crate::recorder::{RecordConfig, Recorder};
use crate::script::{self, Script};
use crate::storage;

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

/// Time origin for the idle clock; forced at engine spawn, so the initial
/// `last_input_ms` of 0 reads as "no input since startup".
static EPOCH: LazyLock<Instant> = LazyLock::new(Instant::now);

fn epoch_ms() -> u64 {
    EPOCH.elapsed().as_millis() as u64
}

/// How often the idle engine wakes to check the anti-sleep deadline (and to
/// notice config changes in the shared atomics).
const ANTI_SLEEP_TICK: Duration = Duration::from_millis(250);

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
    // Live telemetry for the status-bar readout, fed by the capture layer
    // on every OS input event regardless of mode.
    cursor_x: AtomicI32,
    cursor_y: AtomicI32,
    cursor_seen: AtomicBool,
    /// 0 = none yet; see `encode_button`.
    last_button: AtomicU32,
    /// 0 = none yet; see `encode_key`.
    last_key: AtomicU32,
    /// What kind of press happened most recently: 0 = none, 1 = button,
    /// 2 = key. Racing a writer can pair a fresh kind with the previous
    /// press for one frame — fine for a live readout.
    last_press_kind: AtomicU8,
    /// Anti-sleep jiggle interval in ms; 0 = disarmed.
    anti_sleep_ms: AtomicU64,
    /// When input was last observed, in ms since [`EPOCH`].
    last_input_ms: AtomicU64,
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

    /// Record the mouse position (capture layer; every move, any mode).
    pub fn note_cursor(&self, x: i32, y: i32) {
        self.cursor_x.store(x, Ordering::Relaxed);
        self.cursor_y.store(y, Ordering::Relaxed);
        self.cursor_seen.store(true, Ordering::Relaxed);
    }

    /// Latest observed mouse position, if any input arrived yet. The two
    /// coordinates are separate atomics, so a reader racing a writer can see
    /// a position one event stale on one axis — fine for a live readout.
    pub fn cursor(&self) -> Option<(i32, i32)> {
        self.cursor_seen.load(Ordering::Relaxed).then(|| {
            (
                self.cursor_x.load(Ordering::Relaxed),
                self.cursor_y.load(Ordering::Relaxed),
            )
        })
    }

    /// Record a mouse-button press (capture layer, any mode).
    pub fn note_button(&self, button: MouseButton) {
        self.last_button
            .store(encode_button(button), Ordering::Relaxed);
        self.last_press_kind.store(1, Ordering::Relaxed);
    }

    /// Latest pressed mouse button, if any was observed yet.
    pub fn last_button(&self) -> Option<MouseButton> {
        decode_button(self.last_button.load(Ordering::Relaxed))
    }

    /// Record a keyboard key press (capture layer, any mode).
    pub fn note_key(&self, key: Key) {
        self.last_key.store(encode_key(key), Ordering::Relaxed);
        self.last_press_kind.store(2, Ordering::Relaxed);
    }

    /// Latest pressed keyboard key, if any was observed yet.
    pub fn last_key(&self) -> Option<Key> {
        decode_key(self.last_key.load(Ordering::Relaxed))
    }

    /// The most recent press of either kind, for the status-bar readout.
    pub fn last_press(&self) -> Option<LastPress> {
        match self.last_press_kind.load(Ordering::Relaxed) {
            1 => self.last_button().map(LastPress::Button),
            2 => self.last_key().map(LastPress::Key),
            _ => None,
        }
    }

    /// Arm (`Some(interval)`) or disarm (`None`) the anti-sleep jiggle.
    /// Takes effect on the engine's next idle tick — no command needed.
    pub fn set_anti_sleep(&self, interval: Option<Duration>) {
        let ms = interval.map_or(0, |d| (d.as_millis() as u64).max(1));
        self.anti_sleep_ms.store(ms, Ordering::Relaxed);
    }

    pub fn anti_sleep(&self) -> Option<Duration> {
        match self.anti_sleep_ms.load(Ordering::Relaxed) {
            0 => None,
            ms => Some(Duration::from_millis(ms)),
        }
    }

    /// Stamp "input happened now" — fed by the capture layer for every OS
    /// event in any mode, and by the anti-sleep jiggle itself so its cadence
    /// holds even where capture can't observe our synthetic events.
    pub fn note_input(&self) {
        self.last_input_ms.store(epoch_ms(), Ordering::Relaxed);
    }

    /// Time since the last observed input (or since startup, if none yet).
    pub fn idle_for(&self) -> Duration {
        let last = self.last_input_ms.load(Ordering::Relaxed);
        Duration::from_millis(epoch_ms().saturating_sub(last))
    }
}

fn encode_button(button: MouseButton) -> u32 {
    match button {
        MouseButton::Left => 1,
        MouseButton::Right => 2,
        MouseButton::Middle => 3,
        MouseButton::Other(code) => 0x100 + u32::from(code),
    }
}

fn decode_button(encoded: u32) -> Option<MouseButton> {
    match encoded {
        0 => None,
        1 => Some(MouseButton::Left),
        2 => Some(MouseButton::Right),
        3 => Some(MouseButton::Middle),
        other => Some(MouseButton::Other((other - 0x100) as u8)),
    }
}

/// The most recent input press, for live telemetry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LastPress {
    Button(MouseButton),
    Key(Key),
}

/// 0 = none; 1.. = index into [`Key::ALL`]; the variants outside `ALL` get
/// the tag bits below. Unknown codes at or above 2^30 can't round-trip and
/// decode as `None` — real platform keycodes are nowhere near that.
const KEY_FUNCTION: u32 = 0x4000_0000;
const KEY_UNKNOWN: u32 = 0x8000_0000;

fn encode_key(key: Key) -> u32 {
    match key {
        Key::Function => KEY_FUNCTION,
        Key::Unknown(code) if code < KEY_FUNCTION => KEY_UNKNOWN | code,
        Key::Unknown(_) => 0,
        named => Key::ALL
            .iter()
            .position(|k| *k == named)
            .map_or(0, |i| i as u32 + 1),
    }
}

fn decode_key(encoded: u32) -> Option<Key> {
    match encoded {
        0 => None,
        KEY_FUNCTION => Some(Key::Function),
        code if code & KEY_UNKNOWN != 0 => Some(Key::Unknown(code & !KEY_UNKNOWN)),
        index => Key::ALL.get(index as usize - 1).copied(),
    }
}

#[derive(Debug)]
pub enum Command {
    StartClicker(ClickerConfig),
    StartRecording(RecordConfig),
    PlayMacro {
        script: Arc<Script>,
        options: PlaybackOptions,
    },
    /// No-op if idle; otherwise equivalent to `SharedState::request_stop`.
    StopActivity,
    Shutdown,
}

#[derive(Debug)]
pub enum Status {
    ModeChanged(Mode),
    /// The recording that just stopped, with metadata stamped but no name
    /// yet — the frontend names and saves it. `dropped_unknown` counts key
    /// events discarded for carrying an unrecognized keycode.
    RecordingFinished {
        script: Box<Script>,
        dropped_unknown: usize,
    },
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
    capture_tx: Sender<CaptureEvent>,
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
        let (capture_tx, capture_rx) = unbounded();
        // Pin the idle-clock origin to engine startup, so "no input observed
        // yet" measures as idle-since-start.
        LazyLock::force(&EPOCH);
        let shared = Arc::new(SharedState::default());
        let shared_for_thread = shared.clone();
        let thread = std::thread::Builder::new()
            .name("tomte-engine".into())
            .spawn(move || {
                engine_main(
                    command_rx,
                    capture_rx,
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
            capture_tx,
            thread: Some(thread),
        }
    }

    /// Where a capture backend (see [`crate::capture::InputCapture`])
    /// delivers events for recording. Tests can feed synthetic events here.
    pub fn capture_sender(&self) -> Sender<CaptureEvent> {
        self.capture_tx.clone()
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
    capture: Receiver<CaptureEvent>,
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

    let mut jiggle_warned = false;
    loop {
        // A timed wait instead of a blocking recv: the timeout is the idle
        // tick that drives the anti-sleep jiggle (and notices the GUI arming
        // it through the shared atomics). Activities keep the thread inside
        // their own loops, so a jiggle can never interleave with one.
        let command = match commands.recv_timeout(ANTI_SLEEP_TICK) {
            Ok(command) => command,
            Err(RecvTimeoutError::Timeout) => {
                maybe_jiggle(&mut injector, &shared, &mut jiggle_warned);
                continue;
            }
            // All senders dropped — same as shutdown.
            Err(RecvTimeoutError::Disconnected) => break,
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
            Command::PlayMacro { script, options } => {
                run_activity(Mode::Playing, &shared, &push, |shared| {
                    shared.playback_iteration.store(0, Ordering::Relaxed);
                    match player::run_script(
                        &mut injector,
                        &script,
                        &options,
                        shared.stop_flag(),
                        &shared.playback_iteration,
                    ) {
                        Ok(PlayOutcome::Completed) => FinishReason::Completed,
                        Ok(PlayOutcome::Stopped) => FinishReason::Stopped,
                        Err(e) => {
                            log::error!("playback failed: {e}");
                            FinishReason::Failed
                        }
                    }
                });
                if drain_stale_starts(&commands) {
                    break;
                }
            }
            Command::StartRecording(config) => {
                let shutdown =
                    record_activity(&commands, &capture, &shared, &push, &mut injector, config);
                if shutdown || drain_stale_starts(&commands) {
                    break;
                }
            }
        }
    }
    shared.set_mode(Mode::Idle);
}

/// One idle tick of the anti-sleep feature: if armed and nothing (user or
/// engine) produced input for the configured interval, nudge the cursor one
/// pixel and put it straight back. The net displacement is zero and it only
/// ever fires after real idleness, so it cannot fight the user's hand.
fn maybe_jiggle(injector: &mut impl Injector, shared: &SharedState, warned: &mut bool) {
    let Some(interval) = shared.anti_sleep() else {
        return;
    };
    if shared.idle_for() < interval {
        return;
    }
    let result = (|| {
        let (x, y) = injector.cursor_location()?;
        // Nudge toward the screen interior so an edge clamp can't swallow
        // the move.
        let dx = if x > 0 { -1 } else { 1 };
        injector.inject(&EventKind::MouseMove {
            x: f64::from(x + dx),
            y: f64::from(y),
        })?;
        std::thread::sleep(Duration::from_millis(10));
        injector.inject(&EventKind::MouseMove {
            x: f64::from(x),
            y: f64::from(y),
        })
    })();
    match result {
        // The jiggle counts as input: one nudge per interval, even where
        // capture can't observe synthetic events (e.g. Wayland).
        Ok(()) => shared.note_input(),
        Err(e) if !*warned => {
            log::warn!("anti-sleep jiggle failed (will keep trying): {e}");
            *warned = true;
        }
        Err(_) => {}
    }
}

/// Recording can't use `run_activity`: it multiplexes the capture stream
/// with the command channel and produces a `Script`. Returns true if a
/// shutdown was requested mid-recording.
fn record_activity(
    commands: &Receiver<Command>,
    capture: &Receiver<CaptureEvent>,
    shared: &SharedState,
    push: &impl Fn(Status),
    injector: &mut impl Injector,
    config: RecordConfig,
) -> bool {
    // Anything stuck in the channel predates this recording.
    while capture.try_recv().is_ok() {}

    shared.events_recorded.store(0, Ordering::Relaxed);
    shared.stop.store(false, Ordering::Relaxed);
    shared.set_mode(Mode::Recording); // opens the capture gate
    push(Status::ModeChanged(Mode::Recording));

    let mut recorder = Recorder::new(config);
    let mut shutdown = false;
    let stopped_at = loop {
        if shared.stop.load(Ordering::Relaxed) {
            break Instant::now();
        }
        select! {
            recv(capture) -> event => {
                if let Ok((at, kind)) = event {
                    if recorder.push(at, kind) {
                        shared.events_recorded.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            recv(commands) -> command => match command {
                Ok(Command::StopActivity) => break Instant::now(),
                Ok(Command::Shutdown) | Err(_) => {
                    shutdown = true;
                    break Instant::now();
                }
                Ok(other) => log::debug!("ignoring {other:?} while recording"),
            }
        }
    };

    shared.set_mode(Mode::Idle); // closes the gate
                                 // Give events already past the gate check a moment to land, then drop
                                 // them — they arrived after the user asked to stop.
    while capture.try_recv().is_ok() {}

    let dropped_unknown = recorder.dropped_unknown();
    let events = recorder.finish(stopped_at);
    let session = platform::detect_session();
    let screen = injector.main_display().ok().and_then(|(w, h)| {
        Some(crate::model::ScreenInfo {
            width: u32::try_from(w).ok()?,
            height: u32::try_from(h).ok()?,
            scale: 1.0,
        })
    });
    let recorded = Script {
        meta: MacroMeta {
            name: String::new(),
            created_utc: storage::now_utc_rfc3339(),
            os: platform::os_label(session).to_string(),
            screen,
            notes: String::new(),
        },
        body: script::from_events(&events),
    };
    push(Status::RecordingFinished {
        script: Box::new(recorded),
        dropped_unknown,
    });
    push(Status::Finished {
        mode: Mode::Recording,
        reason: FinishReason::Completed,
    });
    push(Status::ModeChanged(Mode::Idle));
    shutdown
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
    use crate::clicker::{ClickKind, ClickPosition, ClickTarget};
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
            target: ClickTarget::Button(MouseButton::Left),
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
    fn recording_collects_fed_events() {
        use crate::model::Key;

        let engine = EngineHandle::spawn_with(|| Ok(NullInjector), None);
        let tx = engine.capture_sender();
        engine.send(Command::StartRecording(RecordConfig {
            trim_tail: Duration::ZERO, // don't trim the synthetic tail
            ..Default::default()
        }));
        assert!(matches!(
            recv_status(&engine),
            Status::ModeChanged(Mode::Recording)
        ));

        let t = std::time::Instant::now();
        // Two presses without releases: stays keydown + wait in the script.
        tx.send((t, EventKind::KeyPress(Key::KeyA))).unwrap();
        tx.send((
            t + Duration::from_millis(30),
            EventKind::KeyPress(Key::KeyB),
        ))
        .unwrap();
        std::thread::sleep(Duration::from_millis(50)); // let the engine drain
        engine.request_stop();

        let Status::RecordingFinished {
            script: recorded, ..
        } = recv_status(&engine)
        else {
            panic!("expected RecordingFinished first");
        };
        let instrs: Vec<_> = recorded.body.iter().map(|s| &s.instr).collect();
        assert_eq!(
            instrs,
            vec![
                &script::Instr::KeyDown(Key::KeyA),
                &script::Instr::Wait {
                    min_us: 30_000,
                    max_us: 30_000
                },
                &script::Instr::KeyDown(Key::KeyB),
            ]
        );
        assert!(!recorded.meta.created_utc.is_empty());
        assert!(matches!(
            recv_status(&engine),
            Status::Finished {
                mode: Mode::Recording,
                reason: FinishReason::Completed
            }
        ));
    }

    #[test]
    fn playback_replays_macro_through_engine() {
        use std::sync::Mutex;

        // An injector that counts events into a shared cell.
        struct CountingInjector(Arc<Mutex<Vec<EventKind>>>);
        impl Injector for CountingInjector {
            fn inject(&mut self, kind: &EventKind) -> Result<(), InjectError> {
                self.0.lock().unwrap().push(*kind);
                Ok(())
            }
            fn cursor_location(&mut self) -> Result<(i32, i32), InjectError> {
                Ok((0, 0))
            }
        }

        let seen = Arc::new(Mutex::new(Vec::new()));
        let seen_for_engine = seen.clone();
        let engine = EngineHandle::spawn_with(move || Ok(CountingInjector(seen_for_engine)), None);

        // A key tap injects press+release: 2 events per iteration.
        let played = Arc::new(script::parse("press x\nwait 5ms\n").unwrap());
        engine.send(Command::PlayMacro {
            script: played,
            options: crate::player::PlaybackOptions {
                speed: 1.0,
                repeat: crate::player::Repeat::Times(3),
            },
        });

        assert!(matches!(
            recv_status(&engine),
            Status::ModeChanged(Mode::Playing)
        ));
        assert!(matches!(
            recv_status(&engine),
            Status::Finished {
                mode: Mode::Playing,
                reason: FinishReason::Completed
            }
        ));
        assert_eq!(seen.lock().unwrap().len(), 6);
        assert_eq!(engine.shared.playback_iteration.load(Ordering::Relaxed), 3);
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

    /// Records every injected event; cursor parked at (100, 100).
    struct SpyInjector(Arc<std::sync::Mutex<Vec<EventKind>>>);

    impl Injector for SpyInjector {
        fn inject(&mut self, kind: &EventKind) -> Result<(), InjectError> {
            self.0.lock().unwrap().push(*kind);
            Ok(())
        }
        fn cursor_location(&mut self) -> Result<(i32, i32), InjectError> {
            Ok((100, 100))
        }
    }

    fn spy_engine() -> (EngineHandle, Arc<std::sync::Mutex<Vec<EventKind>>>) {
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen_for_engine = seen.clone();
        let engine = EngineHandle::spawn_with(move || Ok(SpyInjector(seen_for_engine)), None);
        (engine, seen)
    }

    fn injected_moves(seen: &std::sync::Mutex<Vec<EventKind>>) -> Vec<(i32, i32)> {
        seen.lock()
            .unwrap()
            .iter()
            .filter_map(|kind| match kind {
                EventKind::MouseMove { x, y } => Some((*x as i32, *y as i32)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn anti_sleep_jiggles_when_idle() {
        let (engine, seen) = spy_engine();
        engine
            .shared
            .set_anti_sleep(Some(Duration::from_millis(50)));

        // Generous deadline for busy CI runners; exits as soon as one full
        // nudge-and-return pair has landed.
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let moves = loop {
            let moves = injected_moves(&seen);
            if moves.len() >= 2 {
                break moves;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "no jiggle within 10 s"
            );
            std::thread::sleep(Duration::from_millis(25));
        };
        assert_eq!(moves[0], (99, 100), "1 px toward the screen interior");
        assert_eq!(moves[1], (100, 100), "and straight back");
    }

    #[test]
    fn anti_sleep_holds_off_while_input_is_recent() {
        let (engine, seen) = spy_engine();
        engine.shared.set_anti_sleep(Some(Duration::from_secs(600)));
        engine.shared.note_input();

        // Several idle ticks pass, but the idle clock is nowhere near the
        // interval — nothing may be injected.
        std::thread::sleep(Duration::from_millis(700));
        assert_eq!(injected_moves(&seen), Vec::<(i32, i32)>::new());
    }

    #[test]
    fn anti_sleep_never_fires_during_an_activity() {
        let (engine, seen) = spy_engine();
        // Arm only once the clicker is confirmed running, so the tiny
        // interval can't fire in an idle window before the activity starts.
        engine.send(Command::StartClicker(config(None)));
        assert!(matches!(
            recv_status(&engine),
            Status::ModeChanged(Mode::Clicking)
        ));
        engine
            .shared
            .set_anti_sleep(Some(Duration::from_millis(10)));

        std::thread::sleep(Duration::from_millis(400));
        engine.request_stop();
        engine.shared.set_anti_sleep(None); // no post-stop jiggle either
        assert!(matches!(recv_status(&engine), Status::Finished { .. }));

        // The follow-cursor clicker injects only button events; the engine
        // thread was inside the activity the whole time, so no mouse move
        // (= no jiggle) may appear.
        assert_eq!(injected_moves(&seen), Vec::<(i32, i32)>::new());
        assert!(!seen.lock().unwrap().is_empty(), "clicker did click");
    }

    #[test]
    fn anti_sleep_config_and_idle_clock_round_trip() {
        let shared = SharedState::default();
        assert_eq!(shared.anti_sleep(), None);
        shared.set_anti_sleep(Some(Duration::from_secs(60)));
        assert_eq!(shared.anti_sleep(), Some(Duration::from_secs(60)));
        shared.set_anti_sleep(None);
        assert_eq!(shared.anti_sleep(), None);

        shared.note_input();
        assert!(shared.idle_for() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(30));
        // Coarse bound — the clock has millisecond resolution.
        assert!(shared.idle_for() >= Duration::from_millis(20));
    }

    #[test]
    fn telemetry_starts_empty_and_round_trips() {
        let shared = SharedState::default();
        assert_eq!(shared.cursor(), None);
        assert_eq!(shared.last_button(), None);
        assert_eq!(shared.last_key(), None);
        assert_eq!(shared.last_press(), None);

        shared.note_cursor(-1920, 300);
        assert_eq!(shared.cursor(), Some((-1920, 300)));

        for button in [
            MouseButton::Left,
            MouseButton::Right,
            MouseButton::Middle,
            MouseButton::Other(0),
            MouseButton::Other(8),
            MouseButton::Other(255),
        ] {
            shared.note_button(button);
            assert_eq!(shared.last_button(), Some(button));
        }
    }

    #[test]
    fn every_key_round_trips_through_telemetry() {
        let shared = SharedState::default();
        for &key in Key::ALL {
            shared.note_key(key);
            assert_eq!(shared.last_key(), Some(key), "{key:?}");
        }
        for key in [Key::Function, Key::Unknown(0), Key::Unknown(215)] {
            shared.note_key(key);
            assert_eq!(shared.last_key(), Some(key), "{key:?}");
        }
    }

    #[test]
    fn last_press_tracks_the_most_recent_kind() {
        let shared = SharedState::default();
        shared.note_button(MouseButton::Left);
        assert_eq!(
            shared.last_press(),
            Some(LastPress::Button(MouseButton::Left))
        );
        shared.note_key(Key::KeyA);
        assert_eq!(shared.last_press(), Some(LastPress::Key(Key::KeyA)));
        shared.note_button(MouseButton::Right);
        assert_eq!(
            shared.last_press(),
            Some(LastPress::Button(MouseButton::Right))
        );
    }
}
