//! Phase-2 proof: engine driven by OS-registered global hotkeys.
//!
//! ```text
//! cargo run --example hotkeys
//! ```
//!
//! - **F6** toggles the auto-clicker (left click every 500 ms, follow cursor)
//! - **F9** stops whatever is running
//! - **Enter** quits
//!
//! Manual acceptance check: focus a text editor, press F6 — clicking starts
//! and no "F6" reaches the editor (the OS registration consumes the chord);
//! press F6 again — clicking stops well under 50 ms.

use std::time::Duration;

use crossbeam_channel::select;
use global_hotkey::hotkey::{Code, HotKey};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use tomtemacro_core::clicker::{ClickKind, ClickPosition, ClickTarget, ClickerConfig};
use tomtemacro_core::engine::EngineHandle;
use tomtemacro_core::model::MouseButton;

fn main() {
    env_logger::init();

    let manager = GlobalHotKeyManager::new().expect("hotkey manager");
    let toggle = HotKey::new(None, Code::F6);
    let stop = HotKey::new(None, Code::F9);
    manager.register(toggle).expect("register F6");
    manager.register(stop).expect("register F9");

    let engine = EngineHandle::spawn(None);
    let config = ClickerConfig {
        interval: Duration::from_millis(500),
        target: ClickTarget::Button(MouseButton::Left),
        click_kind: ClickKind::Single,
        position: ClickPosition::FollowCursor,
        jitter: None,
        limit: None,
    };

    let (quit_tx, quit_rx) = crossbeam_channel::bounded::<()>(1);
    std::thread::spawn(move || {
        let _ = std::io::stdin().read_line(&mut String::new());
        let _ = quit_tx.send(());
    });

    println!("F6 = toggle clicker, F9 = stop, Enter = quit");
    let hotkeys = GlobalHotKeyEvent::receiver();
    loop {
        select! {
            recv(hotkeys) -> event => {
                let Ok(event) = event else { break };
                if event.state() != HotKeyState::Pressed {
                    continue;
                }
                if event.id() == toggle.id() {
                    engine.toggle_clicker(config);
                } else if event.id() == stop.id() {
                    engine.request_stop();
                }
            }
            recv(engine.status) -> status => {
                if let Ok(status) = status {
                    println!("engine: {status:?}");
                }
            }
            recv(quit_rx) -> _ => break,
        }
    }
    println!("bye");
}
