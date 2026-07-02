//! The main application: an egui dashboard over the background engine.
//!
//! Threading: the engine and capture listener live on their own threads
//! (spawned here); every frame drains the hotkey/status channels and reads
//! the shared atomics. The engine pings `request_repaint` on status changes;
//! a 100 ms repaint baseline keeps live counters ticking between them.

use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use eframe::egui;
use tomtemacro_core::capture::{CaptureError, InputCapture, RdevCapture};
use tomtemacro_core::engine::{Command, EngineHandle, FinishReason, Mode, Status};
use tomtemacro_core::platform::{self, SessionKind};
use tomtemacro_core::recorder::RecordConfig;
use tomtemacro_core::storage::MacroStore;

use crate::banners::{self, Banner, Severity};
use crate::hotkeys::{Action, Hotkeys};
use crate::tabs::clicker::ClickerUi;
use crate::tabs::recorder::RecorderUi;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Clicker,
    Recorder,
}

pub struct TomteApp {
    engine: EngineHandle,
    capture_errors: Receiver<CaptureError>,
    capture_error: Option<String>,
    hotkeys: Option<Hotkeys>,
    hotkey_error: Option<String>,
    store: Option<MacroStore>,
    store_error: Option<String>,
    fatal: Option<String>,
    session: SessionKind,
    tab: Tab,
    clicker: ClickerUi,
    recorder: RecorderUi,
    last_finish: Option<(Mode, FinishReason, Instant)>,
}

impl TomteApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let ctx = cc.egui_ctx.clone();
        let engine = EngineHandle::spawn(Some(Box::new(move || ctx.request_repaint())));
        let capture_errors = RdevCapture.start(engine.shared.clone(), engine.capture_sender());
        let (hotkeys, hotkey_error) = match Hotkeys::register_defaults() {
            Ok(h) => (Some(h), None),
            Err(e) => (None, Some(e)),
        };
        let (store, store_error) = match MacroStore::open_default() {
            Ok(s) => (Some(s), None),
            Err(e) => (None, Some(e.to_string())),
        };
        Self {
            engine,
            capture_errors,
            capture_error: None,
            hotkeys,
            hotkey_error,
            store,
            store_error,
            fatal: None,
            session: platform::detect_session(),
            tab: Tab::Clicker,
            clicker: ClickerUi::default(),
            recorder: RecorderUi::default(),
            last_finish: None,
        }
    }

    fn strip_keys(&self) -> Vec<tomtemacro_core::model::Key> {
        self.hotkeys
            .as_ref()
            .map(Hotkeys::strip_keys)
            .unwrap_or_default()
    }

    fn handle_action(&mut self, action: Action) {
        match action {
            Action::ToggleClicker => match self.engine.shared.mode() {
                Mode::Idle => self.engine.toggle_clicker(self.clicker.config()),
                Mode::Clicking => self.engine.request_stop(),
                _ => {}
            },
            Action::ToggleRecord => match self.engine.shared.mode() {
                Mode::Recording => self.engine.request_stop(),
                Mode::Idle => {
                    self.engine.send(Command::StartRecording(RecordConfig {
                        strip_keys: self.strip_keys(),
                        ..Default::default()
                    }));
                    self.tab = Tab::Recorder;
                }
                _ => {}
            },
            Action::PlayMacro => {
                if self.engine.shared.mode() == Mode::Idle {
                    if let Some(file) = &self.recorder.playable {
                        self.engine.send(Command::PlayMacro {
                            file: file.clone(),
                            options: self.recorder.playback_options(),
                        });
                    }
                }
            }
            Action::StopAll => self.engine.request_stop(),
        }
    }

    fn poll(&mut self) {
        let actions = self
            .hotkeys
            .as_ref()
            .map(Hotkeys::pressed)
            .unwrap_or_default();
        for action in actions {
            self.handle_action(action);
        }

        while let Ok(status) = self.engine.status.try_recv() {
            match status {
                Status::RecordingFinished(file) => {
                    self.recorder.take_finished(*file);
                    self.tab = Tab::Recorder;
                }
                Status::Finished { mode, reason } => {
                    self.last_finish = Some((mode, reason, Instant::now()));
                }
                Status::Fatal(message) => self.fatal = Some(message),
                Status::ModeChanged(_) => {}
            }
        }

        if let Ok(err) = self.capture_errors.try_recv() {
            self.capture_error = Some(err.to_string());
        }
    }

    fn banners(&self) -> Vec<Banner> {
        let mut banners = Vec::new();
        if self.session == SessionKind::Wayland {
            banners.push(Banner {
                severity: Severity::Error,
                text: "Wayland session detected — global recording and playback do not work \
                       on Wayland yet. Log into an X11/Xorg session to use TomteMacro."
                    .into(),
            });
        }
        if let Some(e) = &self.fatal {
            banners.push(Banner {
                severity: Severity::Error,
                text: format!("engine stopped: {e}"),
            });
        }
        if let Some(e) = &self.capture_error {
            banners.push(Banner {
                severity: Severity::Warning,
                text: format!("input capture unavailable (recording disabled): {e}"),
            });
        }
        if let Some(e) = &self.hotkey_error {
            banners.push(Banner {
                severity: Severity::Warning,
                text: format!("global hotkeys unavailable (use the buttons): {e}"),
            });
        }
        if let Some(e) = &self.store_error {
            banners.push(Banner {
                severity: Severity::Warning,
                text: format!("macro library unavailable: {e}"),
            });
        }
        banners
    }

    fn status_bar(&self, ui: &mut egui::Ui) {
        egui::Panel::bottom(egui::Id::new("status")).show(ui, |ui| {
            ui.horizontal(|ui| {
                let relaxed = std::sync::atomic::Ordering::Relaxed;
                let shared = &self.engine.shared;
                match shared.mode() {
                    Mode::Idle => {
                        // Show how the last activity ended for a few seconds.
                        match self.last_finish {
                            Some((mode, reason, at)) if at.elapsed() < Duration::from_secs(4) => {
                                ui.weak(format!("idle — {mode:?} {reason:?}"));
                            }
                            _ => {
                                ui.weak("idle");
                            }
                        }
                    }
                    Mode::Clicking => {
                        ui.colored_label(
                            egui::Color32::from_rgb(0x4c, 0xaf, 0x50),
                            format!("clicking — {}", shared.clicks_done.load(relaxed)),
                        );
                    }
                    Mode::Recording => {
                        ui.colored_label(
                            ui.visuals().error_fg_color,
                            format!(
                                "● recording — {} events",
                                shared.events_recorded.load(relaxed)
                            ),
                        );
                    }
                    Mode::Playing => {
                        ui.colored_label(
                            egui::Color32::from_rgb(0x42, 0xa5, 0xf5),
                            format!(
                                "▶ playing — iteration {}",
                                shared.playback_iteration.load(relaxed) + 1
                            ),
                        );
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.weak(platform::os_label(self.session));
                    if let Some(hk) = &self.hotkeys {
                        ui.weak(format!(
                            "{} click · {} record · {} play · {} stop",
                            hk.label_for(Action::ToggleClicker),
                            hk.label_for(Action::ToggleRecord),
                            hk.label_for(Action::PlayMacro),
                            hk.label_for(Action::StopAll),
                        ));
                    }
                });
            });
        });
    }
}

impl eframe::App for TomteApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        // Baseline tick so live counters move even without engine pings.
        ctx.request_repaint_after(Duration::from_millis(100));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        banners::show(ui, &self.banners());
        self.status_bar(ui);

        egui::CentralPanel::default_margins().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Clicker, "🖱 Clicker");
                ui.selectable_value(&mut self.tab, Tab::Recorder, "⏺ Recorder");
            });
            ui.separator();
            match self.tab {
                Tab::Clicker => {
                    let hotkey = self
                        .hotkeys
                        .as_ref()
                        .map_or("", |h| h.label_for(Action::ToggleClicker));
                    crate::tabs::clicker::show(ui, &mut self.clicker, &self.engine, hotkey);
                }
                Tab::Recorder => {
                    let (record_key, play_key, stop_key) = match &self.hotkeys {
                        Some(h) => (
                            h.label_for(Action::ToggleRecord),
                            h.label_for(Action::PlayMacro),
                            h.label_for(Action::StopAll),
                        ),
                        None => ("", "", ""),
                    };
                    let strip = self.strip_keys();
                    crate::tabs::recorder::show(
                        ui,
                        &mut self.recorder,
                        &self.engine,
                        self.store.as_ref(),
                        strip,
                        record_key,
                        play_key,
                        stop_key,
                    );
                }
            }
        });
    }
}
