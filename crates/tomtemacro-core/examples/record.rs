//! Phase-3 proof: record a macro from the CLI.
//!
//! ```text
//! cargo run --example record -- out.tomte
//! ```
//!
//! Records global mouse + keyboard until you press Enter in this terminal,
//! then writes the macro to the given path. (The Enter keystroke lands more
//! than 200 ms before the write? No — it *is* the stop signal, and the
//! recorder's tail-trim drops it from the file.)

use std::sync::atomic::Ordering;

use tomtemacro_core::capture::{InputCapture, RdevCapture};
use tomtemacro_core::engine::{Command, EngineHandle, Status};
use tomtemacro_core::model::Key;
use tomtemacro_core::platform;
use tomtemacro_core::recorder::RecordConfig;
use tomtemacro_core::{script, storage};

fn main() {
    env_logger::init();
    let out = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: record <out.tomte>");
        std::process::exit(2);
    });

    let session = platform::detect_session();
    if !platform::input_supported(session) {
        eprintln!("global input capture is not supported in this session ({session:?})");
        std::process::exit(1);
    }

    let engine = EngineHandle::spawn(None);
    let capture_errors = RdevCapture.start(engine.shared.clone(), engine.capture_sender());

    engine.send(Command::StartRecording(RecordConfig {
        // Enter stops us via stdin; strip stray Returns near the end anyway.
        strip_keys: vec![Key::Return],
        ..Default::default()
    }));
    println!("recording… press Enter to stop");

    let _ = std::io::stdin().read_line(&mut String::new());
    if let Ok(err) = capture_errors.try_recv() {
        eprintln!("capture failed to start: {err}");
        std::process::exit(1);
    }
    engine.request_stop();

    loop {
        match engine.status.recv() {
            Ok(Status::RecordingFinished { script, .. }) => {
                let mut recorded = *script;
                recorded.meta.name = std::path::Path::new(&out)
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let stats = recorded.stats();
                storage::save_text(&script::format(&recorded), std::path::Path::new(&out))
                    .expect("write macro file");
                println!(
                    "saved {} instructions ({:.2} s, {} captured raw) to {out}",
                    stats.instructions,
                    stats.nominal_us as f64 / 1e6,
                    engine.shared.events_recorded.load(Ordering::Relaxed)
                );
                return;
            }
            Ok(_) => {}
            Err(_) => {
                eprintln!("engine exited without a recording");
                std::process::exit(1);
            }
        }
    }
}
