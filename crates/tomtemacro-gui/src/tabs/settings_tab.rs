//! Settings tab: theme, hotkey rebinding, anti-sleep, and the macro folder.

use std::path::PathBuf;

use eframe::egui;
use tomtemacro_core::engine::SharedState;
use tomtemacro_core::storage::MacroStore;

use crate::hotkeys::{Hotkeys, KEY_CHOICES};
use crate::settings::{AntiSleepSettings, HotkeySettings, ThemeChoice};
use crate::theme;

#[derive(Default)]
pub struct SettingsUi {
    /// Edited-but-not-applied hotkey names; None = mirror current settings.
    draft: Option<HotkeySettings>,
    pub notice: Option<String>,
    /// Edited-but-not-applied macro folder; None = mirror current settings.
    dir_draft: Option<String>,
    dir_notice: Option<String>,
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

/// Returns true when the macro folder changed and the store must be
/// reopened by the caller.
#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    state: &mut SettingsUi,
    current: &mut HotkeySettings,
    hotkeys: &mut Option<Hotkeys>,
    anti_sleep: &mut AntiSleepSettings,
    macro_dir: &mut Option<PathBuf>,
    theme_choice: &mut ThemeChoice,
    shared: &SharedState,
    os_label: &str,
) -> bool {
    let mut dir_changed = false;
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            theme::centered_column(ui, 600.0, |ui| {
                ui.add_space(6.0);
                theme::section_title(ui, "Appearance");
                theme::card(ui, |ui| appearance_section(ui, theme_choice));
                ui.add_space(12.0);
                theme::section_title(ui, "Global hotkeys");
                theme::card(ui, |ui| hotkey_section(ui, state, current, hotkeys));
                ui.add_space(12.0);
                theme::section_title(ui, "Keep awake");
                theme::card(ui, |ui| anti_sleep_section(ui, anti_sleep, shared));
                ui.add_space(12.0);
                theme::section_title(ui, "Macro folder");
                dir_changed = theme::card(ui, |ui| macro_folder_section(ui, state, macro_dir));
                ui.add_space(10.0);
                ui.vertical_centered(|ui| {
                    ui.weak(format!(
                        "TomteMacro {} · {os_label} · clicker and playback settings \
                         are saved when you quit",
                        env!("CARGO_PKG_VERSION")
                    ));
                });
                ui.add_space(6.0);
            });
        });
    dir_changed
}

fn appearance_section(ui: &mut egui::Ui, theme_choice: &mut ThemeChoice) {
    ui.horizontal(|ui| {
        ui.label("Theme");
        ui.add_space(8.0);
        let changed = theme::segmented(
            ui,
            theme_choice,
            &[
                (ThemeChoice::Light, "☀ Light"),
                (ThemeChoice::Dark, "🌙 Dark"),
                (ThemeChoice::System, "Match system"),
            ],
        );
        if changed {
            ui.ctx().set_theme(theme_choice.preference());
        }
    });
}

fn hotkey_section(
    ui: &mut egui::Ui,
    state: &mut SettingsUi,
    current: &mut HotkeySettings,
    hotkeys: &mut Option<Hotkeys>,
) {
    ui.weak("Work from any app, even when TomteMacro is in the background.");
    ui.add_space(4.0);

    let draft = state.draft.get_or_insert_with(|| current.clone());
    let rows: [(&str, &mut String); 4] = [
        ("Start / stop the auto-clicker", &mut draft.toggle_clicker),
        ("Start / stop recording", &mut draft.toggle_record),
        ("Play the open macro", &mut draft.play_macro),
        ("Stop everything", &mut draft.stop_all),
    ];
    egui::Grid::new("hotkey-grid")
        .num_columns(2)
        .spacing([24.0, 8.0])
        .show(ui, |ui| {
            for (label, value) in rows {
                ui.label(label);
                egui::ComboBox::from_id_salt(label)
                    .width(80.0)
                    .selected_text(value.as_str())
                    .show_ui(ui, |ui| {
                        for choice in KEY_CHOICES {
                            ui.selectable_value(value, choice.to_string(), choice);
                        }
                    });
                ui.end_row();
            }
        });

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
        ui.colored_label(ui.visuals().warn_fg_color, "Each action needs its own key.");
    }

    // Apply/Revert only appear once there is something to apply.
    let changed = draft != *current;
    if changed {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let p = theme::palette(ui);
            if ui
                .add_enabled(!has_duplicates, theme::accent_button("Apply", p.green))
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
                        state.notice = Some("✔ Hotkeys updated".into());
                    }
                    Err(e) => state.notice = Some(format!("Could not apply: {e}")),
                }
            }
            if ui.button("Revert").clicked() {
                state.draft = None;
            }
        });
    }

    if let Some(notice) = &state.notice {
        ui.weak(notice);
    }
}

fn anti_sleep_section(ui: &mut egui::Ui, anti_sleep: &mut AntiSleepSettings, shared: &SharedState) {
    let mut changed = ui
        .checkbox(&mut anti_sleep.enabled, "Keep the computer awake")
        .changed();
    ui.add_enabled_ui(anti_sleep.enabled, |ui| {
        ui.horizontal(|ui| {
            ui.weak("after");
            changed |= ui
                .add(
                    egui::DragValue::new(&mut anti_sleep.interval_secs)
                        .range(5..=600)
                        .suffix(" s"),
                )
                .changed();
            ui.weak("without input");
        });
    });
    if changed {
        apply_anti_sleep(shared, anti_sleep);
    }
    ui.weak(
        "Nudges the mouse one pixel and straight back once you have been idle \
         that long. It never fires while you are active, recording or playing. \
         The ☕ in the status bar toggles it too.",
    );
}

/// The platform-default macro folder, if one can be determined.
fn default_macro_dir() -> Option<PathBuf> {
    MacroStore::open_default()
        .ok()
        .map(|s| s.dir().to_path_buf())
}

fn macro_folder_section(
    ui: &mut egui::Ui,
    state: &mut SettingsUi,
    macro_dir: &mut Option<PathBuf>,
) -> bool {
    ui.weak(
        "Where recordings are saved and where the macro list looks. \
         Existing files are not moved — copy them over if you want them along.",
    );
    ui.add_space(4.0);

    let default_dir = default_macro_dir();
    let effective = macro_dir
        .clone()
        .or_else(|| default_dir.clone())
        .unwrap_or_default();
    let draft = state
        .dir_draft
        .get_or_insert_with(|| effective.display().to_string());

    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(draft)
                .desired_width((ui.available_width() - 90.0).max(120.0)),
        );
        if ui
            .button("Default")
            .on_hover_text(match &default_dir {
                Some(dir) => format!("reset to {}", dir.display()),
                None => "no platform default available".into(),
            })
            .clicked()
        {
            if let Some(dir) = &default_dir {
                *draft = dir.display().to_string();
            }
        }
    });

    let mut changed = false;
    let draft_path = PathBuf::from(draft.trim());
    let draft_differs = !draft.trim().is_empty() && draft_path != effective;
    if draft_differs {
        ui.horizontal(|ui| {
            let p = theme::palette(ui);
            if ui.add(theme::accent_button("Apply", p.green)).clicked() {
                match std::fs::create_dir_all(&draft_path) {
                    Ok(()) => {
                        // Storing the default as None keeps the file portable
                        // (and tracking the platform default if it ever moves).
                        *macro_dir =
                            (Some(&draft_path) != default_dir.as_ref()).then(|| draft_path.clone());
                        crate::settings::save(&crate::settings::Settings {
                            macro_dir: macro_dir.clone(),
                            ..crate::settings::load()
                        });
                        state.dir_notice =
                            Some(format!("✔ Macros now live in {}", draft_path.display()));
                        changed = true;
                    }
                    Err(e) => {
                        state.dir_notice = Some(format!("Can’t use that folder: {e}"));
                    }
                }
            }
            if ui.button("Revert").clicked() {
                state.dir_draft = None;
                state.dir_notice = None;
            }
        });
    }
    if let Some(notice) = &state.dir_notice {
        ui.weak(notice);
    }
    changed
}
