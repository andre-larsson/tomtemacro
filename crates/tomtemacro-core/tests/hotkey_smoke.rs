//! End-to-end hotkey test on a real X server: injects an F6 keypress with
//! enigo and asserts that the OS-registered hotkey fires and toggles the
//! engine. Uses a null injector for the engine so no clicks reach the
//! desktop; the injected F6 is consumed by our own grab, so it doesn't leak
//! into whatever app has focus either.

#![cfg(target_os = "linux")]

use std::time::Duration;

use global_hotkey::hotkey::{Code, HotKey};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use tomtemacro_core::clicker::{ClickKind, ClickPosition, ClickerConfig};
use tomtemacro_core::engine::{EngineHandle, Mode};
use tomtemacro_core::inject::{EnigoInjector, InjectError, Injector};
use tomtemacro_core::model::{EventKind, Key, MouseButton};

struct NullInjector;

impl Injector for NullInjector {
    fn inject(&mut self, _: &EventKind) -> Result<(), InjectError> {
        Ok(())
    }
    fn cursor_location(&mut self) -> Result<(i32, i32), InjectError> {
        Ok((0, 0))
    }
}

fn tap_f6(injector: &mut EnigoInjector) {
    injector
        .inject(&EventKind::KeyPress(Key::F6))
        .expect("press F6");
    std::thread::sleep(Duration::from_millis(20));
    injector
        .inject(&EventKind::KeyRelease(Key::F6))
        .expect("release F6");
}

fn wait_for_mode(engine: &EngineHandle, want: Mode) {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        if engine.shared.mode() == want {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("engine never reached {want:?}");
}

#[test]
#[ignore = "needs an X server"]
fn injected_f6_toggles_clicker_via_global_hotkey() {
    let manager = GlobalHotKeyManager::new().expect("hotkey manager");
    let toggle = HotKey::new(None, Code::F6);
    manager.register(toggle).expect("register F6");

    let engine = EngineHandle::spawn_with(|| Ok(NullInjector), None);
    let config = ClickerConfig {
        interval: Duration::from_millis(50),
        button: MouseButton::Left,
        click_kind: ClickKind::Single,
        position: ClickPosition::FollowCursor,
        jitter: None,
        limit: None,
    };

    let mut keys = EnigoInjector::new().expect("injection backend");
    let hotkeys = GlobalHotKeyEvent::receiver();

    // First tap: hotkey fires, engine starts clicking (into the null sink).
    tap_f6(&mut keys);
    let event = hotkeys
        .recv_timeout(Duration::from_secs(2))
        .expect("hotkey event for injected F6");
    assert_eq!(event.id(), toggle.id());
    assert_eq!(event.state(), HotKeyState::Pressed);
    engine.toggle_clicker(config);
    wait_for_mode(&engine, Mode::Clicking);

    // Second tap: stop. Drain the release/press pair down to the next press.
    tap_f6(&mut keys);
    loop {
        let event = hotkeys
            .recv_timeout(Duration::from_secs(2))
            .expect("second hotkey event");
        if event.state() == HotKeyState::Pressed {
            break;
        }
    }
    engine.toggle_clicker(config);
    wait_for_mode(&engine, Mode::Idle);

    manager.unregister(toggle).expect("unregister F6");
}
