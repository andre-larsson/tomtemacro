//! Auto-clicker tab: configure and drive the click engine.

use std::time::{Duration, Instant};

use eframe::egui;
use tomtemacro_core::clicker::{ClickKind, ClickPosition, ClickTarget, ClickerConfig, Jitter};
use tomtemacro_core::engine::{EngineHandle, Mode};
use tomtemacro_core::inject::{EnigoInjector, Injector};
use tomtemacro_core::model::{Key, MouseButton};

use crate::theme;

pub struct ClickerUi {
    pub interval_ms: u64,
    pub target: ClickTarget,
    pub double: bool,
    pub fixed_position: bool,
    pub x: i32,
    pub y: i32,
    pick_deadline: Option<Instant>,
    pub jitter_enabled: bool,
    pub jitter_frac: f32,
    pub jitter_px: u16,
    pub limit_enabled: bool,
    pub limit: u64,
    /// When an on-screen Start fires; see [`ClickerUi::needs_countdown`].
    start_at: Option<Instant>,
}

impl Default for ClickerUi {
    fn default() -> Self {
        Self {
            interval_ms: 100,
            target: ClickTarget::Button(MouseButton::Left),
            double: false,
            fixed_position: false,
            x: 0,
            y: 0,
            pick_deadline: None,
            jitter_enabled: false,
            jitter_frac: 0.10,
            jitter_px: 3,
            limit_enabled: false,
            limit: 100,
            start_at: None,
        }
    }
}

impl ClickerUi {
    pub fn from_settings(s: &crate::settings::ClickerSettings) -> Self {
        Self {
            interval_ms: s.interval_ms,
            target: s.target,
            double: s.double,
            jitter_enabled: s.jitter_enabled,
            jitter_frac: s.jitter_frac,
            jitter_px: s.jitter_px,
            limit_enabled: s.limit_enabled,
            limit: s.limit,
            ..Default::default()
        }
    }

    pub fn to_settings(&self) -> crate::settings::ClickerSettings {
        crate::settings::ClickerSettings {
            interval_ms: self.interval_ms,
            target: self.target,
            double: self.double,
            jitter_enabled: self.jitter_enabled,
            jitter_frac: self.jitter_frac,
            jitter_px: self.jitter_px,
            limit_enabled: self.limit_enabled,
            limit: self.limit,
        }
    }

    /// The first click fires immediately, so starting from our own button
    /// would click the Stop button that replaces it (or send key presses to
    /// this window). The on-screen Start therefore counts down first, unless
    /// clicks go to a fixed spot. The hotkey always starts straight away.
    fn needs_countdown(&self) -> bool {
        !(self.fixed_position && matches!(self.target, ClickTarget::Button(_)))
    }

    pub fn cancel_countdown(&mut self) {
        self.start_at = None;
    }

    /// True once, when a countdown started from the button runs out.
    pub fn countdown_due(&mut self) -> bool {
        match self.start_at {
            Some(at) if Instant::now() >= at => {
                self.start_at = None;
                true
            }
            _ => false,
        }
    }

    pub fn config(&self) -> ClickerConfig {
        ClickerConfig {
            interval: Duration::from_millis(self.interval_ms.max(1)),
            target: self.target,
            click_kind: if self.double {
                ClickKind::Double
            } else {
                ClickKind::Single
            },
            position: if self.fixed_position {
                ClickPosition::Fixed {
                    x: self.x,
                    y: self.y,
                }
            } else {
                ClickPosition::FollowCursor
            },
            jitter: self.jitter_enabled.then_some(Jitter {
                interval_frac: self.jitter_frac,
                pos_radius_px: self.jitter_px,
            }),
            limit: self.limit_enabled.then_some(self.limit),
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut ClickerUi, engine: &EngineHandle, hotkey: &str) {
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            theme::centered_column(ui, 560.0, |ui| {
                ui.add_space(10.0);
                // Settings only apply when clicking starts, so lock them meanwhile.
                let idle = engine.shared.mode() == Mode::Idle;
                ui.add_enabled_ui(idle, |ui| settings_card(ui, state));
                ui.add_space(22.0);
                start_stop(ui, state, engine, hotkey);
            });
        });
}

/// "clicks" or "presses", depending on what the clicker drives.
pub fn press_word(target: ClickTarget) -> &'static str {
    match target {
        ClickTarget::Button(_) => "clicks",
        ClickTarget::Key(_) => "presses",
    }
}

/// Left-column label, pinned to the top so it lines up with the first
/// control when a row grows (fixed spot, humanize sliders).
fn row_label(ui: &mut egui::Ui, text: &str) {
    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
        ui.add_space(4.0);
        ui.add(egui::Label::new(egui::RichText::new(text).color(theme::palette(ui).weak)).extend());
    });
}

fn settings_card(ui: &mut egui::Ui, state: &mut ClickerUi) {
    let word = press_word(state.target);
    theme::card(ui, |ui| {
        egui::Grid::new("clicker-settings")
            .num_columns(2)
            .spacing([20.0, 14.0])
            .show(ui, |ui| {
                row_label(ui, "Press");
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("target")
                        .selected_text(target_label(state.target))
                        .show_ui(ui, |ui| {
                            for b in [MouseButton::Left, MouseButton::Middle, MouseButton::Right] {
                                let t = ClickTarget::Button(b);
                                ui.selectable_value(&mut state.target, t, target_label(t));
                            }
                            ui.separator();
                            for &k in Key::ALL {
                                ui.selectable_value(
                                    &mut state.target,
                                    ClickTarget::Key(k),
                                    key_label(k),
                                );
                            }
                        });
                    theme::segmented(ui, &mut state.double, &[(false, "Once"), (true, "Twice")]);
                });
                ui.end_row();

                row_label(ui, "Every");
                ui.horizontal(|ui| {
                    ui.add(
                        egui::DragValue::new(&mut state.interval_ms)
                            .range(1..=3_600_000)
                            .speed(5.0)
                            .suffix(" ms"),
                    )
                    .on_hover_text("drag, or click to type");
                    ui.weak(rate_hint(state.interval_ms, word));
                });
                ui.end_row();

                row_label(ui, "Where");
                ui.vertical(|ui| {
                    theme::segmented(
                        ui,
                        &mut state.fixed_position,
                        &[(false, "Wherever the cursor is"), (true, "A fixed spot")],
                    );
                    if state.fixed_position {
                        fixed_spot_row(ui, state);
                    }
                });
                ui.end_row();

                row_label(ui, "Until");
                ui.horizontal(|ui| {
                    theme::segmented(
                        ui,
                        &mut state.limit_enabled,
                        &[(false, "I stop it"), (true, "A set number")],
                    );
                    if state.limit_enabled {
                        ui.add(egui::DragValue::new(&mut state.limit).range(1..=u64::MAX));
                        ui.label(word);
                    }
                });
                ui.end_row();

                row_label(ui, "Feel");
                ui.vertical(|ui| {
                    ui.checkbox(&mut state.jitter_enabled, "Humanize")
                        .on_hover_text(
                            "vary the timing and position a little, like a person would",
                        );
                    if state.jitter_enabled {
                        ui.horizontal(|ui| {
                            ui.weak("timing ±");
                            ui.add(
                                egui::Slider::new(&mut state.jitter_frac, 0.0..=0.5)
                                    .custom_formatter(|v, _| format!("{:.0} %", v * 100.0)),
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.weak("position ±");
                            ui.add(egui::Slider::new(&mut state.jitter_px, 0..=25).suffix(" px"));
                        });
                    }
                });
                ui.end_row();
            });
    });
}

fn fixed_spot_row(ui: &mut egui::Ui, state: &mut ClickerUi) {
    ui.horizontal(|ui| {
        ui.weak("x");
        ui.add(egui::DragValue::new(&mut state.x));
        ui.weak("y");
        ui.add(egui::DragValue::new(&mut state.y));
        match state.pick_deadline {
            None => {
                if ui
                    .button("🎯 Pick")
                    .on_hover_text("then point at the spot — it is grabbed after 3 seconds")
                    .clicked()
                {
                    state.pick_deadline = Some(Instant::now() + Duration::from_secs(3));
                }
            }
            Some(deadline) => {
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    if let Ok((x, y)) = EnigoInjector::new().and_then(|mut e| e.cursor_location()) {
                        state.x = x;
                        state.y = y;
                    }
                    state.pick_deadline = None;
                } else {
                    ui.label(
                        egui::RichText::new(format!(
                            "point at the spot… {:.0}",
                            left.as_secs_f32().ceil()
                        ))
                        .color(theme::palette(ui).accent_text),
                    );
                }
            }
        }
    });
}

/// The interval as a rate people can picture: "≈ 10 clicks per second".
fn rate_hint(interval_ms: u64, word: &str) -> String {
    let trim = |v: f64| {
        let s = format!("{v:.1}");
        s.strip_suffix(".0").map(str::to_owned).unwrap_or(s)
    };
    if interval_ms < 1000 {
        let per_second = 1000.0 / interval_ms.max(1) as f64;
        format!("≈ {} {word} per second", trim(per_second))
    } else {
        format!("once every {} s", trim(interval_ms as f64 / 1000.0))
    }
}

fn start_stop(ui: &mut egui::Ui, state: &mut ClickerUi, engine: &EngineHandle, hotkey: &str) {
    let p = theme::palette(ui);
    let size = egui::vec2(260.0, 50.0);
    ui.vertical_centered(|ui| match engine.shared.mode() {
        Mode::Idle => match state.start_at {
            None => {
                if ui
                    .add(theme::primary_button("▶   Start clicking", p.green).min_size(size))
                    .clicked()
                {
                    if state.needs_countdown() {
                        state.start_at = Some(Instant::now() + Duration::from_secs(3));
                    } else {
                        engine.toggle_clicker(state.config());
                    }
                }
                theme::hotkey_hint(ui, "or press", hotkey, "anywhere — the same key stops it");
            }
            Some(at) => {
                let left = at.saturating_duration_since(Instant::now()).as_secs_f32();
                let label = format!("Starting in {:.0}…", left.ceil().max(1.0));
                if ui
                    .add(theme::primary_button(label, p.green.gamma_multiply(0.75)).min_size(size))
                    .on_hover_text("click to cancel")
                    .clicked()
                {
                    state.cancel_countdown();
                }
                ui.weak(match state.target {
                    ClickTarget::Button(_) => "Point at what you want clicked.",
                    ClickTarget::Key(_) => "Switch to the window that should get the key presses.",
                });
            }
        },
        Mode::Clicking => {
            let done = engine
                .shared
                .clicks_done
                .load(std::sync::atomic::Ordering::Relaxed);
            let word = press_word(state.target);
            if ui
                .add(
                    theme::primary_button(format!("⏹   Stop  ·  {done} {word}"), p.red)
                        .min_size(size),
                )
                .clicked()
            {
                engine.request_stop();
            }
            theme::hotkey_hint(ui, "or press", hotkey, "anywhere");
        }
        _ => {
            ui.add_enabled(
                false,
                theme::primary_button("▶   Start clicking", p.green).min_size(size),
            );
            ui.weak("Finish the recording or playback first.");
        }
    });
}

fn button_name(button: MouseButton) -> &'static str {
    match button {
        MouseButton::Left => "left",
        MouseButton::Middle => "middle",
        MouseButton::Right => "right",
        MouseButton::Other(_) => "other",
    }
}

fn target_label(target: ClickTarget) -> String {
    let label = match target {
        ClickTarget::Button(b) => format!("{} click", button_name(b)),
        ClickTarget::Key(k) => format!("{} key", key_label(k)),
    };
    // Sentence case: "Left click", "Page down key".
    let mut chars = label.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// Human name for a physical key, derived from the variant name:
/// `KeyE` → "E", `Num1` → "1", `Kp7` → "keypad 7", `PageDown` → "page down".
fn key_label(key: Key) -> String {
    let name = format!("{key:?}");
    if let Some(letter) = name.strip_prefix("Key") {
        return letter.to_string();
    }
    if let Some(digit) = name.strip_prefix("Num") {
        if digit.len() == 1 {
            return digit.to_string();
        }
    }
    if let Some(kp) = name.strip_prefix("Kp") {
        if !kp.is_empty() {
            return format!("keypad {}", kp.to_lowercase());
        }
    }
    if name.len() > 1 && name.starts_with('F') && name[1..].chars().all(|c| c.is_ascii_digit()) {
        return name;
    }
    // Remaining variants are CamelCase words: "PageDown" → "page down".
    let mut label = String::new();
    for c in name.chars() {
        if c.is_uppercase() && !label.is_empty() {
            label.push(' ');
        }
        label.push(c.to_ascii_lowercase());
    }
    label
}
