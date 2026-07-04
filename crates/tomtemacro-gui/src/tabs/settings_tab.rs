//! Settings tab: hotkey rebinding and anti-sleep.

use eframe::egui;
use tomtemacro_core::engine::SharedState;

use crate::hotkeys::{Hotkeys, KEY_CHOICES};
use crate::settings::{AntiSleepSettings, HotkeySettings};

#[derive(Default)]
pub struct SettingsUi {
    /// Edited-but-not-applied hotkey names; None = mirror current settings.
    draft: Option<HotkeySettings>,
    pub notice: Option<String>,
}

/// Push the anti-sleep config to the engine — takes effect on its next
/// idle tick, no apply step needed.
pub fn apply_anti_sleep(shared: &SharedState, settings: &AntiSleepSettings) {
    shared.set_anti_sleep(
        settings
            .enabled
            .then(|| std::time::Duration::from_secs(u64::from(settings.interval_secs))),
    );
}

pub fn show(
    ui: &mut egui::Ui,
    state: &mut SettingsUi,
    current: &mut HotkeySettings,
    hotkeys: &mut Option<Hotkeys>,
    anti_sleep: &mut AntiSleepSettings,
    shared: &SharedState,
) {
    ui.heading("Global hotkeys");
    ui.add_space(4.0);

    let draft = state.draft.get_or_insert_with(|| current.clone());
    let rows: [(&str, &mut String); 4] = [
        ("Toggle auto-clicker", &mut draft.toggle_clicker),
        ("Start/stop recording", &mut draft.toggle_record),
        ("Play macro", &mut draft.play_macro),
        ("Stop everything", &mut draft.stop_all),
    ];
    for (label, value) in rows {
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt(label)
                .selected_text(value.as_str())
                .show_ui(ui, |ui| {
                    for choice in KEY_CHOICES {
                        ui.selectable_value(value, choice.to_string(), choice);
                    }
                });
            ui.label(label);
        });
    }

    let draft = state.draft.clone().expect("set above");
    let mut names = [
        &draft.toggle_clicker,
        &draft.toggle_record,
        &draft.play_macro,
        &draft.stop_all,
    ];
    names.sort();
    let has_duplicates = names.windows(2).any(|w| w[0] == w[1]);
    if has_duplicates {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            "each action needs a different key",
        );
    }

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let changed = draft != *current;
        if ui
            .add_enabled(changed && !has_duplicates, egui::Button::new("Apply"))
            .clicked()
        {
            let result = match hotkeys.as_mut() {
                Some(hk) => hk.rebind(&draft),
                None => match Hotkeys::register(&draft) {
                    Ok(hk) => {
                        *hotkeys = Some(hk);
                        Ok(())
                    }
                    Err(e) => Err(e),
                },
            };
            match result {
                Ok(()) => {
                    *current = draft.clone();
                    crate::settings::save(&crate::settings::Settings {
                        hotkeys: current.clone(),
                        ..crate::settings::load()
                    });
                    state.notice = Some("hotkeys updated".into());
                }
                Err(e) => state.notice = Some(format!("failed: {e}")),
            }
        }
        if changed && ui.button("Revert").clicked() {
            state.draft = None;
        }
    });

    if let Some(notice) = &state.notice {
        ui.weak(notice);
    }

    ui.add_space(16.0);
    ui.separator();
    ui.add_space(8.0);

    ui.heading("Anti-sleep");
    ui.add_space(4.0);
    let mut changed = ui
        .checkbox(&mut anti_sleep.enabled, "Keep the system awake")
        .changed();
    ui.add_enabled_ui(anti_sleep.enabled, |ui| {
        ui.horizontal(|ui| {
            ui.label("after");
            changed |= ui
                .add(
                    egui::DragValue::new(&mut anti_sleep.interval_secs)
                        .range(5..=600)
                        .suffix(" s"),
                )
                .changed();
            ui.label("without input");
        });
    });
    if changed {
        apply_anti_sleep(shared, anti_sleep);
    }
    ui.weak(
        "Nudges the mouse one pixel and straight back once you have been idle \
         that long. It never fires while you are active, or while recording \
         or playing — also toggled by the ☕ in the status bar.",
    );

    ui.add_space(16.0);
    ui.separator();
    ui.weak("Clicker and playback settings are saved automatically on exit.");
}
