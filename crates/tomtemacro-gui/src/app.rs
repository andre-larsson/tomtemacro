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
use tomtemacro_core::inject::{EnigoInjector, Injector};
use tomtemacro_core::platform::{self, SessionKind};
use tomtemacro_core::recorder::RecordConfig;
use tomtemacro_core::storage::MacroStore;

use crate::banners::{self, Banner, Severity};
use crate::hotkeys::{Action, Hotkeys};
use crate::settings::{self, Settings, ThemeChoice};
use crate::tabs::clicker::ClickerUi;
use crate::tabs::macros::MacrosUi;
use crate::tabs::settings_tab::SettingsUi;
use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Clicker,
    Macros,
    Settings,
}

/// The macro library from settings: a custom folder if one is set, else the
/// platform default location.
fn open_store(settings: &Settings) -> (Option<MacroStore>, Option<String>) {
    match &settings.macro_dir {
        Some(dir) => (Some(MacroStore::at(dir.clone())), None),
        None => match MacroStore::open_default() {
            Ok(s) => (Some(s), None),
            Err(e) => (None, Some(e.to_string())),
        },
    }
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
    settings: Settings,
    tab: Tab,
    clicker: ClickerUi,
    macros: MacrosUi,
    settings_ui: SettingsUi,
    last_finish: Option<(Mode, FinishReason, Instant)>,
    logo: egui::TextureHandle,
}

impl TomteApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let settings = settings::load();
        theme::install(&cc.egui_ctx);
        cc.egui_ctx.set_theme(settings.theme.preference());
        let logo = {
            let icon = eframe::icon_data::from_png_bytes(include_bytes!(
                "../../../assets/icon/png/tomtemacro-64.png"
            ))
            .expect("embedded icon PNG is valid");
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [icon.width as usize, icon.height as usize],
                &icon.rgba,
            );
            cc.egui_ctx
                .load_texture("logo", image, egui::TextureOptions::LINEAR)
        };

        let ctx = cc.egui_ctx.clone();
        let engine = EngineHandle::spawn(Some(Box::new(move || ctx.request_repaint())));
        let capture_errors = RdevCapture.start(engine.shared.clone(), engine.capture_sender());
        crate::tabs::settings_tab::apply_anti_sleep(&engine.shared, &settings.anti_sleep);
        let (hotkeys, hotkey_error) = match Hotkeys::register(&settings.hotkeys) {
            Ok(h) => (Some(h), None),
            Err(e) => (None, Some(e)),
        };
        let (store, store_error) = open_store(&settings);

        let mut macros = MacrosUi::from_settings(&settings.playback);
        macros.current_screen = EnigoInjector::new()
            .ok()
            .and_then(|mut e| e.main_display().ok())
            .and_then(|(w, h)| Some((u32::try_from(w).ok()?, u32::try_from(h).ok()?)));
        if let Some(last) = &settings.last_open_macro {
            if last.exists() {
                macros.open_path(last);
            }
        }

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
            clicker: ClickerUi::from_settings(&settings.clicker),
            macros,
            settings_ui: SettingsUi::default(),
            settings,
            // Start-tab override for tests/screenshots.
            tab: match std::env::var("TOMTE_START_TAB").as_deref() {
                Ok("macros" | "recorder" | "library") => Tab::Macros,
                Ok("settings") => Tab::Settings,
                _ => Tab::Clicker,
            },
            last_finish: None,
            logo,
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
            Action::ToggleClicker => {
                // The hotkey starts at once; drop any on-screen countdown.
                self.clicker.cancel_countdown();
                match self.engine.shared.mode() {
                    Mode::Idle => self.engine.toggle_clicker(self.clicker.config()),
                    Mode::Clicking => self.engine.request_stop(),
                    _ => {}
                }
            }
            Action::ToggleRecord => match self.engine.shared.mode() {
                Mode::Recording => self.engine.request_stop(),
                Mode::Idle => {
                    self.engine.send(Command::StartRecording(RecordConfig {
                        strip_keys: self.strip_keys(),
                        ..Default::default()
                    }));
                    self.tab = Tab::Macros;
                }
                _ => {}
            },
            Action::PlayMacro => {
                if self.engine.shared.mode() == Mode::Idle {
                    if let Some(script) = self.macros.play_target() {
                        self.engine.send(Command::PlayMacro {
                            script,
                            options: self.macros.playback_options(),
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
        // Checked here rather than in the tab so switching tabs mid-countdown
        // doesn't stall it.
        if self.clicker.countdown_due() && self.engine.shared.mode() == Mode::Idle {
            self.engine.toggle_clicker(self.clicker.config());
        }

        while let Ok(status) = self.engine.status.try_recv() {
            match status {
                Status::RecordingFinished {
                    script,
                    dropped_unknown,
                } => {
                    self.macros.append_recording(*script, dropped_unknown);
                    self.tab = Tab::Macros;
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
                text: format!("The engine stopped: {e}"),
            });
        }
        if let Some(e) = &self.capture_error {
            banners.push(Banner {
                severity: Severity::Warning,
                text: format!("Input capture is unavailable, so recording is off: {e}"),
            });
        }
        if let Some(e) = &self.hotkey_error {
            banners.push(Banner {
                severity: Severity::Warning,
                text: format!("Global hotkeys are unavailable — use the buttons instead: {e}"),
            });
        }
        if let Some(e) = &self.store_error {
            banners.push(Banner {
                severity: Severity::Warning,
                text: format!("The macro library is unavailable: {e}"),
            });
        }
        banners
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        egui::Panel::top(egui::Id::new("header"))
            .frame(egui::Frame::new().fill(p.bg).inner_margin(egui::Margin {
                left: 14,
                right: 14,
                top: 10,
                bottom: 6,
            }))
            .show_separator_line(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::Image::new(egui::load::SizedTexture::new(
                        self.logo.id(),
                        [30.0, 30.0],
                    )));
                    // The name goes first when space runs out; the tabs matter more.
                    if ui.available_width() > 560.0 {
                        ui.label(egui::RichText::new("TomteMacro").size(20.0).color(p.text));
                        ui.add_space(10.0);
                    }
                    theme::segmented(
                        ui,
                        &mut self.tab,
                        &[
                            (Tab::Clicker, "🖱 Auto-clicker"),
                            (Tab::Macros, "📜 Macros"),
                            (Tab::Settings, "⚙ Settings"),
                        ],
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let dark = ui.visuals().dark_mode;
                        let (icon, tip, next) = if dark {
                            ("☀", "Switch to the light theme", ThemeChoice::Light)
                        } else {
                            ("🌙", "Switch to the dark theme", ThemeChoice::Dark)
                        };
                        if ui
                            .add(
                                egui::Button::new(egui::RichText::new(icon).size(18.0))
                                    .frame(false),
                            )
                            .on_hover_text(tip)
                            .clicked()
                        {
                            self.settings.theme = next;
                            ui.ctx().set_theme(next.preference());
                        }
                    });
                });
            });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        egui::Panel::bottom(egui::Id::new("status"))
            .frame(
                egui::Frame::new()
                    .fill(p.bg)
                    .inner_margin(egui::Margin::symmetric(14, 5)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let relaxed = std::sync::atomic::Ordering::Relaxed;
                    let shared = &self.engine.shared;
                    match shared.mode() {
                        Mode::Idle => match self.last_finish {
                            // Show how the last activity ended for a few seconds.
                            Some((mode, reason, at)) if at.elapsed() < Duration::from_secs(4) => {
                                let color = match reason {
                                    FinishReason::Completed => p.green,
                                    FinishReason::Stopped => p.weak,
                                    FinishReason::Failed => p.error,
                                };
                                theme::status_dot(ui, color, finish_text(mode, reason));
                            }
                            _ => theme::status_dot(ui, p.weak, "Ready"),
                        },
                        Mode::Clicking => {
                            let word = crate::tabs::clicker::press_word(self.clicker.target);
                            theme::status_dot(
                                ui,
                                p.green,
                                format!("Clicking · {} {word}", shared.clicks_done.load(relaxed)),
                            );
                        }
                        Mode::Recording => {
                            theme::status_dot(
                                ui,
                                p.red,
                                format!(
                                    "Recording · {} events",
                                    shared.events_recorded.load(relaxed)
                                ),
                            );
                        }
                        Mode::Playing => {
                            theme::status_dot(
                                ui,
                                p.green,
                                format!(
                                    "Playing · round {}",
                                    shared.playback_iteration.load(relaxed) + 1
                                ),
                            );
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Anti-sleep toggle — reachable from every tab.
                        let armed = self.settings.anti_sleep.enabled;
                        let label = if armed {
                            egui::RichText::new("☕ awake").color(p.green)
                        } else {
                            egui::RichText::new("☕").size(16.0).color(p.weak)
                        };
                        let coffee = ui
                            .add(egui::Button::selectable(armed, label))
                            .on_hover_text(if armed {
                                format!(
                                    "Anti-sleep is on — nudges the mouse after {} s idle. \
                                     Click to turn it off.",
                                    self.settings.anti_sleep.interval_secs
                                )
                            } else {
                                "Anti-sleep is off — click to keep the computer awake".to_owned()
                            });
                        if coffee.clicked() {
                            self.settings.anti_sleep.enabled = !armed;
                            crate::tabs::settings_tab::apply_anti_sleep(
                                shared,
                                &self.settings.anti_sleep,
                            );
                        }
                        // Live input readout (capture-fed; hidden when capture is down).
                        if self.capture_error.is_none() {
                            let pos = match shared.cursor() {
                                Some((x, y)) => format!("{x}, {y}"),
                                None => "–".into(),
                            };
                            use tomtemacro_core::engine::LastPress;
                            let last = match shared.last_press() {
                                Some(LastPress::Button(button)) => format!(
                                    " · {} click",
                                    tomtemacro_core::script::button_name(button)
                                ),
                                Some(LastPress::Key(key)) => {
                                    format!(" · {}", tomtemacro_core::script::key_name(key))
                                }
                                None => String::new(),
                            };
                            ui.add_space(6.0);
                            ui.weak(format!("🖱 {pos}{last}"))
                                .on_hover_text("mouse position and the last button or key pressed");
                        }
                    });
                });
            });
    }
}

/// Human wording for how an activity ended.
fn finish_text(mode: Mode, reason: FinishReason) -> String {
    let what = match mode {
        Mode::Clicking => "Clicking",
        Mode::Playing => "Playback",
        Mode::Recording => "Recording",
        Mode::Idle => "Activity",
    };
    match reason {
        FinishReason::Completed => format!("{what} finished"),
        FinishReason::Stopped => format!("{what} stopped"),
        FinishReason::Failed => format!("{what} failed — see the log"),
    }
}

impl eframe::App for TomteApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        // Baseline tick so live counters move even without engine pings.
        ctx.request_repaint_after(Duration::from_millis(100));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.header(ui);
        banners::show(ui, &self.banners());
        self.status_bar(ui);

        let frame = egui::Frame::new()
            .fill(theme::palette(ui).bg)
            .inner_margin(egui::Margin {
                left: 14,
                right: 14,
                top: 6,
                bottom: 8,
            });
        egui::CentralPanel::default().frame(frame).show(ui, |ui| {
            match self.tab {
                Tab::Clicker => {
                    let hotkey = self
                        .hotkeys
                        .as_ref()
                        .map_or("", |h| h.label_for(Action::ToggleClicker))
                        .to_owned();
                    crate::tabs::clicker::show(ui, &mut self.clicker, &self.engine, &hotkey);
                }
                Tab::Macros => {
                    let (record_key, play_key, stop_key) = match &self.hotkeys {
                        Some(h) => (
                            h.label_for(Action::ToggleRecord).to_owned(),
                            h.label_for(Action::PlayMacro).to_owned(),
                            h.label_for(Action::StopAll).to_owned(),
                        ),
                        None => (String::new(), String::new(), String::new()),
                    };
                    let strip = self.strip_keys();
                    crate::tabs::macros::show(
                        ui,
                        &mut self.macros,
                        &self.engine,
                        self.store.as_ref(),
                        strip,
                        &record_key,
                        &play_key,
                        &stop_key,
                    );
                }
                Tab::Settings => {
                    let dir_changed = crate::tabs::settings_tab::show(
                        ui,
                        &mut self.settings_ui,
                        &mut self.settings.hotkeys,
                        &mut self.hotkeys,
                        &mut self.settings.anti_sleep,
                        &mut self.settings.macro_dir,
                        &mut self.settings.theme,
                        &self.engine.shared,
                        platform::os_label(self.session),
                    );
                    if dir_changed {
                        let (store, store_error) = open_store(&self.settings);
                        self.store = store;
                        self.store_error = store_error;
                        self.macros.refresh();
                    }
                    // A successful rebind clears an old registration failure.
                    if self.hotkeys.is_some() {
                        self.hotkey_error = None;
                    }
                }
            }
        });
    }
}

impl Drop for TomteApp {
    fn drop(&mut self) {
        self.settings.clicker = self.clicker.to_settings();
        self.settings.playback = self.macros.playback_settings();
        self.settings.last_open_macro = self.macros.selected.clone();
        settings::save(&self.settings);
    }
}
