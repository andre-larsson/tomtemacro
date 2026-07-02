//! Phase-1 proof: a CLI auto-clicker.
//!
//! ```text
//! cargo run --example clicker -- [--interval-ms N] [--count N]
//!     [--button left|right|middle] [--double] [--x N --y N]
//!     [--jitter FRAC] [--jitter-px N]
//! ```
//!
//! Press Enter to stop early. Reports the measured cadence at the end.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tomtemacro_core::clicker::{self, ClickKind, ClickPosition, ClickerConfig, Jitter};
use tomtemacro_core::inject::{EnigoInjector, InjectError, Injector};
use tomtemacro_core::model::{EventKind, MouseButton};

/// Wraps the real injector and timestamps every button press so the cadence
/// can be measured.
struct MeasuringInjector {
    inner: EnigoInjector,
    presses: Arc<Mutex<Vec<Instant>>>,
}

impl Injector for MeasuringInjector {
    fn inject(&mut self, kind: &EventKind) -> Result<(), InjectError> {
        if matches!(kind, EventKind::ButtonPress(_)) {
            self.presses.lock().unwrap().push(Instant::now());
        }
        self.inner.inject(kind)
    }

    fn cursor_location(&mut self) -> Result<(i32, i32), InjectError> {
        self.inner.cursor_location()
    }
}

fn main() {
    env_logger::init();
    let config = match parse_args() {
        Ok(config) => config,
        Err(msg) => {
            eprintln!("{msg}");
            std::process::exit(2);
        }
    };

    println!(
        "clicking every {:?}{} — press Enter to stop",
        config.interval,
        config
            .limit
            .map(|n| format!(", {n} times"))
            .unwrap_or_default()
    );

    let stop = Arc::new(AtomicBool::new(false));
    let stop_on_enter = stop.clone();
    std::thread::spawn(move || {
        let _ = std::io::stdin().read_line(&mut String::new());
        stop_on_enter.store(true, Ordering::Relaxed);
    });

    let presses = Arc::new(Mutex::new(Vec::new()));
    let mut injector = MeasuringInjector {
        inner: EnigoInjector::new().expect("failed to init injection backend"),
        presses: presses.clone(),
    };

    let counter = AtomicU64::new(0);
    let begin = Instant::now();
    let count =
        clicker::run(&mut injector, &config, &stop, &counter).expect("injection failed mid-run");
    let elapsed = begin.elapsed();

    let presses = presses.lock().unwrap();
    let intervals: Vec<f64> = presses
        .windows(2)
        .map(|w| w[1].duration_since(w[0]).as_secs_f64() * 1e3)
        .collect();
    println!("performed {count} clicks in {elapsed:.2?}");
    if !intervals.is_empty() && config.jitter.is_none() {
        let target = config.interval.as_secs_f64() * 1e3;
        let mean_err =
            intervals.iter().map(|ms| (ms - target).abs()).sum::<f64>() / intervals.len() as f64;
        let max_err = intervals
            .iter()
            .map(|ms| (ms - target).abs())
            .fold(0.0, f64::max);
        println!(
            "cadence: target {target:.3} ms, mean error {mean_err:.3} ms, max {max_err:.3} ms"
        );
    }
}

fn parse_args() -> Result<ClickerConfig, String> {
    let mut interval_ms: u64 = 100;
    let mut count: Option<u64> = None;
    let mut button = MouseButton::Left;
    let mut double = false;
    let mut x: Option<i32> = None;
    let mut y: Option<i32> = None;
    let mut jitter_frac: f32 = 0.0;
    let mut jitter_px: u16 = 0;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = |name: &str| args.next().ok_or_else(|| format!("{name} expects a value"));
        match arg.as_str() {
            "--interval-ms" => interval_ms = parse(&value("--interval-ms")?)?,
            "--count" => count = Some(parse(&value("--count")?)?),
            "--button" => {
                button = match value("--button")?.as_str() {
                    "left" => MouseButton::Left,
                    "right" => MouseButton::Right,
                    "middle" => MouseButton::Middle,
                    other => return Err(format!("unknown button '{other}'")),
                }
            }
            "--double" => double = true,
            "--x" => x = Some(parse(&value("--x")?)?),
            "--y" => y = Some(parse(&value("--y")?)?),
            "--jitter" => jitter_frac = parse(&value("--jitter")?)?,
            "--jitter-px" => jitter_px = parse(&value("--jitter-px")?)?,
            other => return Err(format!("unknown argument '{other}' (see source for usage)")),
        }
    }

    let position = match (x, y) {
        (Some(x), Some(y)) => ClickPosition::Fixed { x, y },
        (None, None) => ClickPosition::FollowCursor,
        _ => return Err("--x and --y must be given together".into()),
    };
    let jitter = (jitter_frac > 0.0 || jitter_px > 0).then_some(Jitter {
        interval_frac: jitter_frac,
        pos_radius_px: jitter_px,
    });

    Ok(ClickerConfig {
        interval: Duration::from_millis(interval_ms.max(1)),
        button,
        click_kind: if double {
            ClickKind::Double
        } else {
            ClickKind::Single
        },
        position,
        jitter,
        limit: count,
    })
}

fn parse<T: std::str::FromStr>(s: &str) -> Result<T, String> {
    s.parse().map_err(|_| format!("invalid value '{s}'"))
}
