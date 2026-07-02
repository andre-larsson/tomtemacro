//! Global input capture behind a trait — the isolation seam for the
//! project's biggest dependency risk (rdev is stale) and the future
//! evdev/uinput Wayland backend.

use std::sync::Arc;
use std::time::Instant;

use crossbeam_channel::{bounded, Receiver, Sender};

use crate::convert;
use crate::engine::{Mode, SharedState};
use crate::model::EventKind;

/// A captured event stamped with a monotonic capture time. (rdev exposes a
/// wall-clock `SystemTime`, which can jump; we stamp our own.)
pub type CaptureEvent = (Instant, EventKind);

#[derive(Debug, thiserror::Error)]
#[error("global input capture failed: {0}")]
pub struct CaptureError(pub String);

pub trait InputCapture {
    /// Start capturing for the lifetime of the process — there is
    /// deliberately no stop: the callback gates on `shared.mode()` being
    /// [`Mode::Recording`], which also guarantees our own injected events
    /// (mode `Playing`/`Clicking`) can never enter a recording.
    ///
    /// Startup errors surface on the returned channel (at most one message;
    /// on success nothing is ever sent).
    fn start(self, shared: Arc<SharedState>, tx: Sender<CaptureEvent>) -> Receiver<CaptureError>;
}

/// rdev-based capture: `SetWindowsHookEx` on Windows, `CGEventTap` on macOS,
/// XRecord on X11. Does not work on Wayland — see `platform::detect_session`.
pub struct RdevCapture;

impl InputCapture for RdevCapture {
    fn start(self, shared: Arc<SharedState>, tx: Sender<CaptureEvent>) -> Receiver<CaptureError> {
        let (err_tx, err_rx) = bounded(1);
        std::thread::Builder::new()
            .name("tomte-capture".into())
            .spawn(move || {
                // Blocks forever on success; returns only on startup failure.
                let result = rdev::listen(move |event| {
                    // Fast path: one relaxed atomic load per OS input event.
                    if shared.mode() != Mode::Recording {
                        return;
                    }
                    let kind = convert::rdev_to_core(&event.event_type);
                    let _ = tx.send((Instant::now(), kind));
                });
                if let Err(e) = result {
                    log::error!("input capture unavailable: {e:?}");
                    let _ = err_tx.send(CaptureError(format!("{e:?}")));
                }
            })
            .expect("failed to spawn capture thread");
        err_rx
    }
}
