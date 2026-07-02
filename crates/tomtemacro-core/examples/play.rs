//! Phase-3 proof: replay a recorded macro from the CLI.
//!
//! ```text
//! cargo run --example play -- file.ron [--speed 2.0] [--repeat N|inf]
//! ```
//!
//! Press Enter to stop early.

use std::sync::Arc;

use tomtemacro_core::engine::{Command, EngineHandle, Status};
use tomtemacro_core::player::{PlaybackOptions, Repeat};
use tomtemacro_core::storage;

fn main() {
    env_logger::init();
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: play <file.ron> [--speed X] [--repeat N|inf]");
        std::process::exit(2);
    };
    let mut options = PlaybackOptions::default();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--speed" => {
                options.speed = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .expect("--speed expects a number");
            }
            "--repeat" => {
                let v = args.next().expect("--repeat expects N or 'inf'");
                options.repeat = if v == "inf" {
                    Repeat::Infinite
                } else {
                    Repeat::Times(v.parse().expect("--repeat expects N or 'inf'"))
                };
            }
            other => {
                eprintln!("unknown argument '{other}'");
                std::process::exit(2);
            }
        }
    }

    let file = storage::load(std::path::Path::new(&path)).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });
    println!(
        "playing '{}' ({} events, {:.2} s at 1.0x) at {}x — Enter stops",
        file.meta.name,
        file.events.len(),
        file.duration_us() as f64 / 1e6,
        options.speed,
    );

    let engine = EngineHandle::spawn(None);
    engine.send(Command::PlayMacro {
        file: Arc::new(file),
        options,
    });

    let stopper = {
        let shared = engine.shared.clone();
        std::thread::spawn(move || {
            let _ = std::io::stdin().read_line(&mut String::new());
            shared.request_stop();
        })
    };

    loop {
        match engine.status.recv() {
            Ok(Status::Finished { reason, .. }) => {
                println!("finished: {reason:?}");
                break;
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    drop(stopper); // detached; process exit cleans it up
}
