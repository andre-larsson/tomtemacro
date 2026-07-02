//! Settings tab: hotkey rebinding.

use eframe::egui;

use crate::hotkeys::{Hotkeys, KEY_CHOICES};
use crate::settings::HotkeySettings;

#[derive(Default)]
pub struct SettingsUi {
    /// Edited-but-not-applied hotkey names; None = mirror current settings.
    draft: Option<HotkeySettings>,
    pub notice: Option<String>,
}

pub fn show(
    ui: &mut egui::Ui,
    state: &mut SettingsUi,
    current: &mut HotkeySettings,
    hotkeys: &mut Option<Hotkeys>,
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
    ui.weak("Clicker and playback settings are saved automatically on exit.");
}
