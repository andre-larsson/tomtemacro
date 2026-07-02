//! Macro library tab: browse, play, rename, and delete saved macros.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use eframe::egui;
use tomtemacro_core::engine::{Command, EngineHandle, Mode};
use tomtemacro_core::model::MacroFile;
use tomtemacro_core::player::PlaybackOptions;
use tomtemacro_core::storage::{self, MacroStore};

struct Entry {
    path: PathBuf,
    file: Arc<MacroFile>,
    load_error: Option<String>,
}

#[derive(Default)]
pub struct LibraryUi {
    entries: Vec<Entry>,
    loaded: bool,
    renaming: Option<usize>,
    rename_field: String,
    confirm_delete: Option<usize>,
    notice: Option<String>,
    /// Main-display size at startup, for the geometry-mismatch hint.
    pub current_screen: Option<(u32, u32)>,
}

impl LibraryUi {
    /// Force a re-scan on the next frame (after save/delete/rename).
    pub fn mark_dirty(&mut self) {
        self.loaded = false;
    }

    fn reload(&mut self, store: &MacroStore) {
        self.entries.clear();
        self.renaming = None;
        self.confirm_delete = None;
        match store.list() {
            Ok(paths) => {
                for path in paths {
                    match storage::load(&path) {
                        Ok(file) => self.entries.push(Entry {
                            path,
                            file: Arc::new(file),
                            load_error: None,
                        }),
                        Err(e) => self.entries.push(Entry {
                            path,
                            file: Arc::new(MacroFile::new(Default::default(), Vec::new())),
                            load_error: Some(e.to_string()),
                        }),
                    }
                }
            }
            Err(e) => self.notice = Some(format!("could not list macros: {e}")),
        }
        self.loaded = true;
    }
}

pub fn show(
    ui: &mut egui::Ui,
    state: &mut LibraryUi,
    engine: &EngineHandle,
    store: Option<&MacroStore>,
    options: PlaybackOptions,
    play_key: &str,
) -> Option<Arc<MacroFile>> {
    let Some(store) = store else {
        ui.label("macro library unavailable — see the warning above");
        return None;
    };
    if !state.loaded {
        state.reload(store);
    }

    let mut played = None;
    ui.horizontal(|ui| {
        ui.heading("Saved macros");
        if ui.small_button("⟳ refresh").clicked() {
            state.mark_dirty();
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("📂 open folder").clicked() {
                reveal_dir(store.dir());
            }
        });
    });
    ui.add_space(4.0);

    if state.entries.is_empty() {
        ui.weak("nothing here yet — record something in the Recorder tab");
        return None;
    }

    let idle = engine.shared.mode() == Mode::Idle;
    let mut do_delete: Option<usize> = None;
    let mut do_rename: Option<(usize, String)> = None;

    egui::ScrollArea::vertical().show(ui, |ui| {
        for (i, entry) in state.entries.iter().enumerate() {
            ui.horizontal(|ui| {
                if let Some(err) = &entry.load_error {
                    ui.colored_label(ui.visuals().error_fg_color, "✖");
                    ui.label(
                        entry
                            .path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                    )
                    .on_hover_text(err);
                    if ui
                        .small_button("🗑")
                        .on_hover_text("delete broken file")
                        .clicked()
                    {
                        do_delete = Some(i);
                    }
                    return;
                }

                // Play
                if ui
                    .add_enabled(idle, egui::Button::new("▶"))
                    .on_hover_text(format!("play ({play_key} plays the selected macro)"))
                    .clicked()
                {
                    engine.send(Command::PlayMacro {
                        file: entry.file.clone(),
                        options,
                    });
                    played = Some(entry.file.clone());
                }

                // Name (or rename editor)
                if state.renaming == Some(i) {
                    let response = ui.text_edit_singleline(&mut state.rename_field);
                    let commit =
                        response.lost_focus() && ui.input(|inp| inp.key_pressed(egui::Key::Enter));
                    if ui.small_button("✔").clicked() || commit {
                        do_rename = Some((i, state.rename_field.trim().to_string()));
                    }
                    if ui.small_button("✖").clicked() {
                        state.renaming = None;
                    }
                } else {
                    ui.label(&entry.file.meta.name);
                    ui.weak(format!(
                        "{} events · {:.1} s · {}",
                        entry.file.events.len(),
                        entry.file.duration_us() as f64 / 1e6,
                        entry.file.meta.os,
                    ));
                    if let (Some(screen), Some((w, h))) =
                        (entry.file.meta.screen, state.current_screen)
                    {
                        if (screen.width, screen.height) != (w, h) {
                            ui.colored_label(ui.visuals().warn_fg_color, "⚠")
                                .on_hover_text(format!(
                                    "recorded on a {}×{} screen, this one is {w}×{h} — \
                                     absolute positions may be off",
                                    screen.width, screen.height
                                ));
                        }
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Delete (two-step confirm)
                    if state.confirm_delete == Some(i) {
                        if ui.small_button("really delete?").clicked() {
                            do_delete = Some(i);
                        }
                    } else if ui.small_button("🗑").clicked() {
                        state.confirm_delete = Some(i);
                    }
                    // Rename
                    if state.renaming != Some(i) && ui.small_button("✏").clicked() {
                        state.renaming = Some(i);
                        state.rename_field = entry.file.meta.name.clone();
                    }
                });
            });
            ui.separator();
        }
    });

    if let Some((i, new_name)) = do_rename {
        if !new_name.is_empty() {
            match store.rename(&state.entries[i].path, &new_name) {
                Ok(_) => state.notice = Some(format!("renamed to “{new_name}”")),
                Err(e) => state.notice = Some(format!("rename failed: {e}")),
            }
        }
        state.mark_dirty();
    }
    if let Some(i) = do_delete {
        match store.delete(&state.entries[i].path) {
            Ok(()) => state.notice = Some("deleted".into()),
            Err(e) => state.notice = Some(format!("delete failed: {e}")),
        }
        state.mark_dirty();
    }

    if let Some(notice) = &state.notice {
        ui.weak(notice);
    }
    played
}

/// Best-effort "open the macros folder in the file manager".
fn reveal_dir(dir: &Path) {
    let _ = std::fs::create_dir_all(dir);
    #[cfg(target_os = "linux")]
    let cmd = "xdg-open";
    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(target_os = "windows")]
    let cmd = "explorer";
    let _ = std::process::Command::new(cmd).arg(dir).spawn();
}
