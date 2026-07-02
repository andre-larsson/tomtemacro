//! Runtime smoke tests that need a real or virtual X server.
//!
//! `#[ignore]`d by default so `cargo test` stays headless-safe; CI runs them
//! under `xvfb-run -a cargo test -- --ignored`. The cursor round-trip is also
//! safe on a live desktop (it moves the pointer and puts it back — no clicks).

#![cfg(target_os = "linux")]

use tomtemacro_core::inject::{EnigoInjector, Injector};
use tomtemacro_core::model::EventKind;

#[test]
#[ignore = "needs an X server"]
fn cursor_move_round_trips() {
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
