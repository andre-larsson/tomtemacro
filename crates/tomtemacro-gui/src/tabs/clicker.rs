//! Auto-clicker tab: configure and drive the click engine.

use std::time::{Duration, Instant};

use eframe::egui;
use tomtemacro_core::clicker::{ClickKind, ClickPosition, ClickTarget, ClickerConfig, Jitter};
use tomtemacro_core::engine::{EngineHandle, Mode};
use tomtemacro_core::inject::{EnigoInjector, Injector};
use tomtemacro_core::model::{Key, MouseButton};

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
    ui.horizontal(|ui| {
        ui.label("Press every");
        ui.add(
            egui::DragValue::new(&mut state.interval_ms)
                .range(1..=3_600_000)
                .suffix(" ms"),
        );
        ui.label("with");
        egui::ComboBox::from_id_salt("target")
            .selected_text(target_label(state.target))
            .show_ui(ui, |ui| {
                for b in [MouseButton::Left, MouseButton::Middle, MouseButton::Right] {
                    let t = ClickTarget::Button(b);
                    ui.selectable_value(&mut state.target, t, target_label(t));
                }
                ui.separator();
                for &k in Key::ALL {
                    ui.selectable_value(&mut state.target, ClickTarget::Key(k), key_label(k));
                }
            });
        let double_label = match state.target {
            ClickTarget::Button(_) => "double-click",
            ClickTarget::Key(_) => "double-tap",
        };
        ui.checkbox(&mut state.double, double_label);
    });

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label("Position:");
        ui.radio_value(&mut state.fixed_position, false, "follow cursor");
        ui.radio_value(&mut state.fixed_position, true, "fixed");
    });
    if state.fixed_position {
        ui.horizontal(|ui| {
            ui.label("x");
            ui.add(egui::DragValue::new(&mut state.x));
            ui.label("y");
            ui.add(egui::DragValue::new(&mut state.y));
            match state.pick_deadline {
                None => {
                    if ui.button("Pick current position in 3 s").clicked() {
                        state.pick_deadline = Some(Instant::now() + Duration::from_secs(3));
                    }
                }
                Some(deadline) => {
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        if let Ok((x, y)) =
                            EnigoInjector::new().and_then(|mut e| e.cursor_location())
                        {
                            state.x = x;
                            state.y = y;
                        }
                        state.pick_deadline = None;
                    } else {
                        ui.label(format!(
                            "move the cursor… {:.0} s",
                            left.as_secs_f32().ceil()
                        ));
                    }
                }
            }
        });
    }

    ui.add_space(6.0);
    ui.checkbox(&mut state.jitter_enabled, "Humanized jitter");
    if state.jitter_enabled {
        ui.horizontal(|ui| {
            ui.label("timing ±");
            ui.add(
                egui::Slider::new(&mut state.jitter_frac, 0.0..=0.5)
                    .custom_formatter(|v, _| format!("{:.0} %", v * 100.0)),
            );
        });
        ui.horizontal(|ui| {
            ui.label("position ±");
            ui.add(egui::Slider::new(&mut state.jitter_px, 0..=25).suffix(" px"));
        });
    }

    let press_word = match state.target {
        ClickTarget::Button(_) => "clicks",
        ClickTarget::Key(_) => "presses",
    };

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.checkbox(&mut state.limit_enabled, "Stop after");
        if state.limit_enabled {
            ui.add(egui::DragValue::new(&mut state.limit).range(1..=u64::MAX));
            ui.label(press_word);
        }
    });

    ui.add_space(12.0);
    match engine.shared.mode() {
        Mode::Idle => {
            if ui
                .add_sized(
                    [200.0, 32.0],
                    egui::Button::new(format!("▶  Start  ({hotkey})")),
                )
                .clicked()
            {
                engine.toggle_clicker(state.config());
            }
        }
        Mode::Clicking => {
            let clicks = engine
                .shared
                .clicks_done
                .load(std::sync::atomic::Ordering::Relaxed);
            if ui
                .add_sized(
                    [200.0, 32.0],
                    egui::Button::new(format!("⏹  Stop — {clicks} {press_word}  ({hotkey})")),
                )
                .clicked()
            {
                engine.request_stop();
            }
        }
        other => {
            ui.add_enabled(false, egui::Button::new(format!("engine busy: {other:?}")));
        }
    }
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
    match target {
        ClickTarget::Button(b) => format!("{} click", button_name(b)),
        ClickTarget::Key(k) => format!("{} key", key_label(k)),
    }
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
