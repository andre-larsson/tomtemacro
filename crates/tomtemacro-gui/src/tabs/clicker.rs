//! Auto-clicker tab: configure and drive the click engine.

use std::time::{Duration, Instant};

use eframe::egui;
use tomtemacro_core::clicker::{ClickKind, ClickPosition, ClickerConfig, Jitter};
use tomtemacro_core::engine::{EngineHandle, Mode};
use tomtemacro_core::inject::{EnigoInjector, Injector};
use tomtemacro_core::model::MouseButton;

pub struct ClickerUi {
    pub interval_ms: u64,
    pub button: MouseButton,
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
            button: MouseButton::Left,
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
            button: s.button,
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
            button: self.button,
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
            button: self.button,
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
        ui.label("Click every");
        ui.add(
            egui::DragValue::new(&mut state.interval_ms)
                .range(1..=3_600_000)
                .suffix(" ms"),
        );
        ui.label("with");
        egui::ComboBox::from_id_salt("button")
            .selected_text(button_name(state.button))
            .show_ui(ui, |ui| {
                for b in [MouseButton::Left, MouseButton::Middle, MouseButton::Right] {
                    ui.selectable_value(&mut state.button, b, button_name(b));
                }
            });
        ui.checkbox(&mut state.double, "double-click");
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

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.checkbox(&mut state.limit_enabled, "Stop after");
        if state.limit_enabled {
            ui.add(egui::DragValue::new(&mut state.limit).range(1..=u64::MAX));
            ui.label("clicks");
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
                    egui::Button::new(format!("⏹  Stop — {clicks} clicks  ({hotkey})")),
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
