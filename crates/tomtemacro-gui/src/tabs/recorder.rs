//! Recorder tab: record, name, save, and replay macros.

use std::sync::Arc;

use eframe::egui;
use tomtemacro_core::engine::{Command, EngineHandle, Mode};
use tomtemacro_core::model::{Key, MacroFile};
use tomtemacro_core::player::{PlaybackOptions, Repeat};
use tomtemacro_core::recorder::RecordConfig;
use tomtemacro_core::storage::MacroStore;

pub struct RecorderUi {
    /// A finished recording waiting to be named and saved.
    pub pending: Option<MacroFile>,
    pub name: String,
    pub speed: f64,
    pub repeat_times: u32,
    pub repeat_infinite: bool,
    /// Most recently saved or recorded macro — the F8 play target.
    pub playable: Option<Arc<MacroFile>>,
    pub playable_name: String,
    pub notice: Option<String>,
}

impl Default for RecorderUi {
    fn default() -> Self {
        Self {
            pending: None,
            name: String::new(),
            speed: 1.0,
            repeat_times: 1,
            repeat_infinite: false,
            playable: None,
            playable_name: String::new(),
            notice: None,
        }
    }
}

impl RecorderUi {
    pub fn playback_options(&self) -> PlaybackOptions {
        PlaybackOptions {
            speed: self.speed,
            repeat: if self.repeat_infinite {
                Repeat::Infinite
            } else {
                Repeat::Times(self.repeat_times)
            },
        }
    }

    pub fn take_finished(&mut self, file: MacroFile) {
        self.playable = Some(Arc::new(file.clone()));
        self.playable_name = "unsaved recording".into();
        self.pending = Some(file);
        if self.name.is_empty() {
            self.name = "new-macro".into();
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    state: &mut RecorderUi,
    engine: &EngineHandle,
    store: Option<&MacroStore>,
    strip_keys: Vec<Key>,
    record_key: &str,
    play_key: &str,
    stop_key: &str,
) {
    match engine.shared.mode() {
        Mode::Idle => {
            ui.horizontal(|ui| {
                if ui
                    .add_sized(
                        [190.0, 32.0],
                        egui::Button::new(format!("⏺  Record  ({record_key})")),
                    )
                    .clicked()
                {
                    engine.send(Command::StartRecording(RecordConfig {
                        strip_keys,
                        ..Default::default()
                    }));
                }
                let can_play = state.playable.is_some();
                if ui
                    .add_enabled(
                        can_play,
                        egui::Button::new(format!("▶  Play  ({play_key})"))
                            .min_size([140.0, 32.0].into()),
                    )
                    .clicked()
                {
                    if let Some(file) = &state.playable {
                        engine.send(Command::PlayMacro {
                            file: file.clone(),
                            options: state.playback_options(),
                        });
                    }
                }
            });
        }
        Mode::Recording => {
            let events = engine
                .shared
                .events_recorded
                .load(std::sync::atomic::Ordering::Relaxed);
            ui.colored_label(
                ui.visuals().error_fg_color,
                format!("●  recording — {events} events"),
            );
            if ui
                .add_sized(
                    [190.0, 32.0],
                    egui::Button::new(format!("⏹  Stop recording  ({record_key})")),
                )
                .clicked()
            {
                engine.request_stop();
            }
        }
        Mode::Playing => {
            let iteration = engine
                .shared
                .playback_iteration
                .load(std::sync::atomic::Ordering::Relaxed);
            ui.label(format!("▶ playing — {iteration} iterations done"));
            if ui
                .add_sized(
                    [190.0, 32.0],
                    egui::Button::new(format!("⏹  Stop playback  ({stop_key})")),
                )
                .clicked()
            {
                engine.request_stop();
            }
        }
        Mode::Clicking => {
            ui.label("engine is busy auto-clicking — stop it first");
        }
    }

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label("Playback:");
        ui.add(
            egui::Slider::new(&mut state.speed, 0.25..=4.0)
                .logarithmic(true)
                .suffix("×"),
        );
        ui.checkbox(&mut state.repeat_infinite, "loop forever");
        if !state.repeat_infinite {
            ui.add(
                egui::DragValue::new(&mut state.repeat_times)
                    .range(1..=1_000_000)
                    .prefix("× "),
            );
        }
    });

    if let Some(pending) = state.pending.take() {
        ui.add_space(8.0);
        ui.separator();
        ui.label(format!(
            "New recording: {} events, {:.2} s",
            pending.events.len(),
            pending.duration_us() as f64 / 1e6
        ));
        let mut keep = true;
        ui.horizontal(|ui| {
            ui.label("Name");
            ui.text_edit_singleline(&mut state.name);
            let can_save = store.is_some() && !state.name.trim().is_empty();
            if ui
                .add_enabled(can_save, egui::Button::new("💾 Save"))
                .clicked()
            {
                let mut file = pending.clone();
                file.meta.name = state.name.trim().to_string();
                match store.expect("guarded by can_save").save_new(&file) {
                    Ok(path) => {
                        state.notice = Some(format!("saved to {}", path.display()));
                        state.playable = Some(Arc::new(file.clone()));
                        state.playable_name = file.meta.name;
                        keep = false;
                    }
                    Err(e) => state.notice = Some(format!("save failed: {e}")),
                }
            }
            if ui.button("🗑 Discard").clicked() {
                keep = false;
                state.notice = Some("recording discarded".into());
            }
        });
        if keep {
            state.pending = Some(pending);
        }
    }

    if state.pending.is_none() {
        if let Some(playable) = &state.playable {
            ui.add_space(8.0);
            ui.label(format!(
                "{play_key} plays: “{}” ({} events)",
                state.playable_name,
                playable.events.len()
            ));
        }
    }

    if let Some(notice) = &state.notice {
        ui.add_space(4.0);
        ui.weak(notice);
    }
}
