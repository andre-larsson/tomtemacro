//! Runtime smoke tests that need a real or virtual X server.
//!
//! `#[ignore]`d by default so `cargo test` stays headless-safe; CI runs them
//! under `xvfb-run -a cargo test -- --ignored`. The cursor round-trip is also
//! safe on a live desktop (it moves the pointer and puts it back — no clicks).

#![cfg(target_os = "linux")]

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tomtemacro_core::capture::{InputCapture, RdevCapture};
use tomtemacro_core::engine::{Command, EngineHandle, Status};
use tomtemacro_core::inject::{EnigoInjector, Injector};
use tomtemacro_core::model::{EventKind, Key, MouseButton};
use tomtemacro_core::player::{PlaybackOptions, Repeat};
use tomtemacro_core::recorder::RecordConfig;

/// These tests all drive the display's one cursor/keyboard, so they must not
/// run concurrently (the default test runner uses threads). A failed test
/// poisons the mutex; later tests still need the lock, hence `into_inner`.
static X_DISPLAY: Mutex<()> = Mutex::new(());

#[test]
#[ignore = "needs an X server"]
fn cursor_move_round_trips() {
    let _display = X_DISPLAY.lock().unwrap_or_else(PoisonError::into_inner);
    let mut injector = EnigoInjector::new().expect("X11 injection backend");
    let original = injector.cursor_location().expect("read cursor");

    let target = (137, 211);
    injector
        .inject(&EventKind::MouseMove {
            x: f64::from(target.0),
            y: f64::from(target.1),
        })
        .expect("inject move");
    // XTest applies synchronously, but give the server a moment anyway.
    std::thread::sleep(std::time::Duration::from_millis(30));
    let observed = injector.cursor_location().expect("read cursor");

    // Restore before asserting so a failure doesn't strand the pointer.
    injector
        .inject(&EventKind::MouseMove {
            x: f64::from(original.0),
            y: f64::from(original.1),
        })
        .expect("restore cursor");

    assert_eq!(observed, target, "injected move didn't land exactly");
}

fn wait_for_recording(engine: &EngineHandle) {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while engine.shared.mode() != tomtemacro_core::engine::Mode::Recording {
        assert!(
            std::time::Instant::now() < deadline,
            "never started recording"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn recording_finished(engine: &EngineHandle) -> tomtemacro_core::model::MacroFile {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let remaining = deadline
            .checked_duration_since(std::time::Instant::now())
            .expect("timed out waiting for RecordingFinished");
        match engine.status.recv_timeout(remaining) {
            Ok(Status::RecordingFinished(file)) => return *file,
            Ok(_) => {}
            Err(e) => panic!("engine status channel: {e}"),
        }
    }
}

/// Record real injected mouse moves through the actual rdev capture layer,
/// then replay them and check the cursor lands where the recording says.
/// Safe on a live desktop: moves only, no clicks or keys, cursor restored.
#[test]
#[ignore = "needs an X server"]
fn record_and_replay_mouse_moves() {
    let _display = X_DISPLAY.lock().unwrap_or_else(PoisonError::into_inner);
    let engine = EngineHandle::spawn(None);
    let capture_errors = RdevCapture.start(engine.shared.clone(), engine.capture_sender());

    let mut injector = EnigoInjector::new().expect("injection backend");
    let original = injector.cursor_location().expect("read cursor");

    engine.send(Command::StartRecording(RecordConfig {
        trim_tail: Duration::ZERO,
        ..Default::default()
    }));
    wait_for_recording(&engine);
    std::thread::sleep(Duration::from_millis(100)); // let XRecord settle

    let waypoints = [(320.0, 310.0), (420.0, 360.0), (520.0, 410.0)];
    for (x, y) in waypoints {
        injector
            .inject(&EventKind::MouseMove { x, y })
            .expect("inject move");
        std::thread::sleep(Duration::from_millis(60)); // > coalescing gap
    }
    engine.request_stop();

    if let Ok(err) = capture_errors.try_recv() {
        panic!("capture backend failed to start: {err}");
    }
    let file = recording_finished(&engine);
    let moves: Vec<(f64, f64)> = file
        .events
        .iter()
        .filter_map(|e| match e.kind {
            EventKind::MouseMove { x, y } => Some((x, y)),
            _ => None,
        })
        .collect();
    for (x, y) in waypoints {
        assert!(
            moves.contains(&(x, y)),
            "recorded moves {moves:?} missing waypoint ({x}, {y})"
        );
    }

    // Park the cursor elsewhere, replay, and check it followed the recording.
    injector
        .inject(&EventKind::MouseMove { x: 50.0, y: 50.0 })
        .expect("park cursor");
    engine.send(Command::PlayMacro {
        file: Arc::new(file),
        options: PlaybackOptions {
            speed: 4.0,
            repeat: Repeat::Times(1),
        },
    });
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        match engine.status.recv_timeout(
            deadline
                .checked_duration_since(std::time::Instant::now())
                .expect("timed out waiting for playback"),
        ) {
            // The recording's own Finished/ModeChanged may still be queued —
            // wait specifically for the *playback* to finish.
            Ok(Status::Finished {
                mode: tomtemacro_core::engine::Mode::Playing,
                ..
            }) => break,
            Ok(_) => {}
            Err(e) => panic!("engine status channel: {e}"),
        }
    }
    let after = injector.cursor_location().expect("read cursor");

    injector
        .inject(&EventKind::MouseMove {
            x: f64::from(original.0),
            y: f64::from(original.1),
        })
        .expect("restore cursor");

    assert_eq!(after, (520, 410), "replay should end at the last waypoint");
}

/// Full-kind round-trip (buttons, wheel, keys) — only safe on a throwaway
/// display. Set TOMTE_FULL_SMOKE=1 under `xvfb-run` (CI does).
#[test]
#[ignore = "needs a virtual X server; set TOMTE_FULL_SMOKE=1"]
fn full_event_kinds_round_trip() {
    if std::env::var_os("TOMTE_FULL_SMOKE").is_none() {
        eprintln!("skipping: TOMTE_FULL_SMOKE not set (needs a disposable display)");
        return;
    }

    let _display = X_DISPLAY.lock().unwrap_or_else(PoisonError::into_inner);
    let engine = EngineHandle::spawn(None);
    let capture_errors = RdevCapture.start(engine.shared.clone(), engine.capture_sender());
    let mut injector = EnigoInjector::new().expect("injection backend");

    engine.send(Command::StartRecording(RecordConfig {
        trim_tail: Duration::ZERO,
        ..Default::default()
    }));
    wait_for_recording(&engine);
    std::thread::sleep(Duration::from_millis(100));

    let script = [
        EventKind::MouseMove { x: 300.0, y: 300.0 },
        EventKind::ButtonPress(MouseButton::Left),
        EventKind::ButtonRelease(MouseButton::Left),
        EventKind::Wheel { dx: 0, dy: -1 },
        EventKind::KeyPress(Key::KeyE),
        EventKind::KeyRelease(Key::KeyE),
        EventKind::KeyPress(Key::LeftBracket), // å on Swedish layouts
        EventKind::KeyRelease(Key::LeftBracket),
    ];
    for kind in &script {
        injector.inject(kind).expect("inject");
        std::thread::sleep(Duration::from_millis(30));
    }
    engine.request_stop();

    if let Ok(err) = capture_errors.try_recv() {
        panic!("capture backend failed to start: {err}");
    }
    let file = recording_finished(&engine);
    let kinds: Vec<EventKind> = file.events.iter().map(|e| e.kind).collect();

    // The exact injected sequence must appear in order (extra MouseMoves
    // from the server are fine).
    let mut it = kinds.iter();
    for expected in &script {
        assert!(
            it.any(|k| k == expected),
            "captured stream missing {expected:?} in order; got {kinds:?}"
        );
    }
}
