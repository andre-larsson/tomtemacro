//! Macros tab: the library list, a text editor for the macro script
//! language, record/play/save controls, regex find & replace, the tidy
//! tool, and the cheat-sheet panel.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use eframe::egui;
use tomtemacro_core::engine::{Command, EngineHandle, Mode};
use tomtemacro_core::model::Key;
use tomtemacro_core::player::{PlaybackOptions, Repeat};
use tomtemacro_core::recorder::RecordConfig;
use tomtemacro_core::script::{self, ParseError, Script};
use tomtemacro_core::storage::{self, MacroStore, SCRIPT_EXT};

use crate::settings::PlaybackSettings;
use crate::theme;

/// Widget id of the editor, needed to drive its selection from find-next.
const EDITOR_ID: &str = "macro-editor";
/// Above this many lines the syntax highlighter steps aside so huge
/// recordings stay editable.
const HIGHLIGHT_MAX_LINES: usize = 5_000;

struct Entry {
    path: PathBuf,
    name: String,
    legacy_ron: bool,
    screen: Option<(u32, u32)>,
    load_error: Option<String>,
}

/// What to do once the unsaved-changes dialog resolves.
enum PendingAction {
    Open(PathBuf),
    New,
}

pub struct MacrosUi {
    entries: Vec<Entry>,
    loaded: bool,

    /// Path of the macro in the editor; `None` = untitled buffer.
    pub selected: Option<PathBuf>,
    buffer: String,
    dirty: bool,
    /// File name (pre-slug) used when saving an untitled buffer.
    untitled_name: String,
    parse_cache: Option<(u64, Result<Arc<Script>, ParseError>)>,
    highlight_cache: Option<(u64, egui::text::LayoutJob)>,

    /// Fallback play-hotkey target when the buffer isn't playable.
    pub playable: Option<Arc<Script>>,
    speed: f64,
    repeat_times: u32,
    repeat_infinite: bool,

    show_find: bool,
    find_pattern: String,
    replace_with: String,
    regex_cache: Option<(String, Result<regex::Regex, String>)>,
    /// Byte offset the next find-next starts from (wraps).
    find_from: usize,

    show_cheatsheet: bool,
    pending: Option<PendingAction>,
    renaming: Option<usize>,
    rename_field: String,
    confirm_delete: Option<usize>,
    notice: Option<String>,
    /// Main-display size at startup, for the geometry-mismatch hint.
    pub current_screen: Option<(u32, u32)>,
}

impl Default for MacrosUi {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            loaded: false,
            selected: None,
            buffer: String::new(),
            dirty: false,
            untitled_name: "new-macro".into(),
            parse_cache: None,
            highlight_cache: None,
            playable: None,
            speed: 1.0,
            repeat_times: 1,
            repeat_infinite: true,
            show_find: false,
            find_pattern: String::new(),
            replace_with: String::new(),
            regex_cache: None,
            find_from: 0,
            show_cheatsheet: false,
            pending: None,
            renaming: None,
            rename_field: String::new(),
            confirm_delete: None,
            notice: None,
            current_screen: None,
        }
    }
}

impl MacrosUi {
    pub fn from_settings(s: &PlaybackSettings) -> Self {
        Self {
            speed: s.speed,
            repeat_times: s.repeat_times,
            repeat_infinite: s.repeat_infinite,
            ..Default::default()
        }
    }

    pub fn playback_settings(&self) -> PlaybackSettings {
        PlaybackSettings {
            speed: self.speed,
            repeat_times: self.repeat_times,
            repeat_infinite: self.repeat_infinite,
        }
    }

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

    /// Force a library re-scan on the next frame.
    pub fn refresh(&mut self) {
        self.loaded = false;
    }

    fn reload(&mut self, store: &MacroStore) {
        self.entries.clear();
        self.renaming = None;
        self.confirm_delete = None;
        match store.list() {
            Ok(paths) => {
                for path in paths {
                    let legacy_ron = path.extension().is_some_and(|ext| ext == "ron");
                    match storage::load_script(&path) {
                        Ok(loaded) => self.entries.push(Entry {
                            name: loaded.meta.name.clone(),
                            screen: loaded.meta.screen.map(|s| (s.width, s.height)),
                            path,
                            legacy_ron,
                            load_error: None,
                        }),
                        Err(e) => self.entries.push(Entry {
                            name: path
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_default(),
                            screen: None,
                            path,
                            legacy_ron,
                            load_error: Some(e.to_string()),
                        }),
                    }
                }
            }
            Err(e) => self.notice = Some(format!("Could not list macros: {e}")),
        }
        self.loaded = true;
    }

    /// Load a macro into the editor: raw text for `.tomte` (the file is the
    /// source of truth), a formatted conversion for legacy `.ron`.
    pub fn open_path(&mut self, path: &Path) -> bool {
        let text = if path.extension().is_some_and(|ext| ext == SCRIPT_EXT) {
            std::fs::read_to_string(path).map_err(|e| e.to_string())
        } else {
            storage::load_script(path)
                .map(|loaded| script::format(&loaded))
                .map_err(|e| e.to_string())
        };
        match text {
            Ok(text) => {
                self.selected = Some(path.to_path_buf());
                self.buffer = text;
                self.dirty = false;
                self.find_from = 0;
                true
            }
            Err(e) => {
                self.notice = Some(format!("Could not open: {e}"));
                false
            }
        }
    }

    fn request_open(&mut self, path: PathBuf) {
        if self.dirty {
            self.pending = Some(PendingAction::Open(path));
        } else {
            self.open_path(&path);
        }
    }

    fn request_new(&mut self) {
        if self.dirty {
            self.pending = Some(PendingAction::New);
        } else {
            self.new_buffer();
        }
    }

    fn new_buffer(&mut self) {
        self.selected = None;
        self.buffer = format!("# tomte-macro v{}\n\n", script::TEXT_VERSION);
        self.dirty = false;
        self.untitled_name = "new-macro".into();
        self.find_from = 0;
    }

    /// A finished recording lands in the editor: a fresh buffer if nothing
    /// is open, appended under a `# recorded …` marker otherwise.
    pub fn append_recording(&mut self, recorded: Script, dropped_unknown: usize) {
        let stats = recorded.stats();
        if self.selected.is_none() && self.buffer.trim().is_empty() {
            self.buffer = script::format(&recorded);
            self.untitled_name = "new-macro".into();
        } else {
            if !self.buffer.is_empty() && !self.buffer.ends_with('\n') {
                self.buffer.push('\n');
            }
            self.buffer
                .push_str(&format!("\n# recorded {}\n", recorded.meta.created_utc));
            self.buffer.push_str(&script::format_body(&recorded.body));
        }
        self.dirty = true;
        let mut notice = format!(
            "Recording added — {} steps. ✨ Tidy strips the extra mouse moves.",
            stats.instructions
        );
        if dropped_unknown > 0 {
            notice.push_str(&format!(
                " · ⚠ {dropped_unknown} key event(s) with unrecognized codes were \
                 dropped — this machine mangled some keystrokes (a phantom or \
                 garbled key); the rest recorded fine"
            ));
        }
        self.notice = Some(notice);
    }

    fn parse_current(&mut self) -> Result<Arc<Script>, ParseError> {
        let mut hasher = DefaultHasher::new();
        self.buffer.hash(&mut hasher);
        let hash = hasher.finish();
        if self.parse_cache.as_ref().is_none_or(|(h, _)| *h != hash) {
            self.parse_cache = Some((hash, script::parse(&self.buffer).map(Arc::new)));
        }
        self.parse_cache.as_ref().expect("just filled").1.clone()
    }

    /// What the play hotkey plays: the buffer if it parses to something
    /// executable, else whatever played last.
    pub fn play_target(&mut self) -> Option<Arc<Script>> {
        if let Ok(parsed) = self.parse_current() {
            if parsed.stats().instructions > 0 {
                return Some(parsed);
            }
        }
        self.playable.clone()
    }

    fn compiled_regex(&mut self) -> Result<regex::Regex, String> {
        if self
            .regex_cache
            .as_ref()
            .is_none_or(|(pattern, _)| pattern != &self.find_pattern)
        {
            let compiled = regex::Regex::new(&self.find_pattern).map_err(|e| e.to_string());
            self.regex_cache = Some((self.find_pattern.clone(), compiled));
        }
        self.regex_cache.as_ref().expect("just filled").1.clone()
    }
}

#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    state: &mut MacrosUi,
    engine: &EngineHandle,
    store: Option<&MacroStore>,
    strip_keys: Vec<Key>,
    record_key: &str,
    play_key: &str,
    stop_key: &str,
) {
    if !state.loaded {
        if let Some(store) = store {
            state.reload(store);
        } else {
            state.loaded = true;
        }
    }

    unsaved_dialog(ui, state, store);

    let p = theme::palette(ui);
    let side_card = egui::Frame::new()
        .fill(p.card)
        .stroke(egui::Stroke::new(1.0, p.border))
        .corner_radius(12)
        .inner_margin(egui::Margin::same(10));
    egui::Panel::left(egui::Id::new("macro-list"))
        .resizable(true)
        .default_size(220.0)
        .show_separator_line(false)
        .frame(side_card.outer_margin(egui::Margin {
            right: 12,
            ..Default::default()
        }))
        .show(ui, |ui| list_ui(ui, state, engine, store));

    let show_cheatsheet = &mut state.show_cheatsheet;
    egui::Panel::right(egui::Id::new("cheatsheet"))
        .resizable(true)
        .default_size(250.0)
        .show_separator_line(false)
        .frame(side_card.outer_margin(egui::Margin {
            left: 12,
            ..Default::default()
        }))
        .show_collapsible(ui, show_cheatsheet, super::cheatsheet::show);

    // The side panels only constrain the parent's *cursor*, and the next
    // allocation wins the full width back — anything wide (notably the
    // editor with long lines) would then lay out and paint straight across
    // the cheat-sheet panel, which was drawn earlier this frame and thus
    // ends up hidden underneath. Put the central content in a child `Ui`
    // hard-bounded (and clipped) to the space the panels left.
    let content_rect = ui.available_rect_before_wrap();
    let mut content_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(content_rect)
            .layout(*ui.layout()),
    );
    content_ui.set_clip_rect(content_ui.clip_rect().intersect(content_rect));
    let ui = &mut content_ui;

    toolbar(
        ui, state, engine, store, strip_keys, record_key, play_key, stop_key,
    );
    ui.add_enabled_ui(engine.shared.mode() == Mode::Idle, |ui| {
        options_row(ui, state);
    });
    if state.show_find {
        find_bar(ui, state);
    }
    if state.selected.is_none() {
        ui.horizontal(|ui| {
            ui.weak("Name");
            ui.add(egui::TextEdit::singleline(&mut state.untitled_name).desired_width(180.0));
            ui.weak("not saved yet");
        });
    }
    let has_steps = matches!(state.parse_current(), Ok(s) if s.stats().instructions > 0);
    match engine.shared.mode() {
        _ if has_steps => status_line(ui, state),
        Mode::Idle => {
            welcome_card(ui, record_key);
            ui.add_space(4.0);
        }
        Mode::Recording => {
            ui.weak("Go ahead — your steps land in the editor when you stop.");
        }
        _ => {}
    }
    if let Some(notice) = state.notice.clone() {
        ui.weak(notice);
    }
    ui.add_space(2.0);
    editor(ui, state);
}

/// First-run guidance shown while the editor holds no runnable steps.
fn welcome_card(ui: &mut egui::Ui, record_key: &str) {
    let p = theme::palette(ui);
    theme::card(ui, |ui| {
        ui.label(
            egui::RichText::new("Record your first macro")
                .size(17.0)
                .color(p.text),
        );
        ui.add_space(2.0);
        let anywhere = if record_key.is_empty() {
            String::new()
        } else {
            format!(" — or {record_key} from any app")
        };
        step(ui, 1, &format!("Press ⏺ Record{anywhere}."));
        step(ui, 2, "Do the clicks and typing you want repeated.");
        step(
            ui,
            3,
            "Stop the recording. Your steps appear below as a script you can edit.",
        );
        ui.add_space(2.0);
        ui.weak(
            "Then press ▶ Play. Rather write it yourself? Type commands below — \
             📖 Cheat sheet lists them all.",
        );
    });
}

/// A numbered step: the number in a soft red circle, then the text.
fn step(ui: &mut egui::Ui, n: u32, text: &str) {
    let p = theme::palette(ui);
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
        ui.painter()
            .circle_filled(rect.center(), 11.0, p.accent_soft);
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            n.to_string(),
            egui::FontId::proportional(13.0),
            p.accent_text,
        );
        ui.label(text);
    });
}

// --- left panel: the library list ---

fn list_ui(
    ui: &mut egui::Ui,
    state: &mut MacrosUi,
    engine: &EngineHandle,
    store: Option<&MacroStore>,
) {
    let p = theme::palette(ui);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Your macros").size(15.5).color(p.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            if let Some(store) = store {
                if ui
                    .add(egui::Button::new("📂").frame_when_inactive(false))
                    .on_hover_text("Open the macro folder")
                    .clicked()
                {
                    reveal_dir(store.dir());
                }
            }
            if ui
                .add(egui::Button::new("⟳").frame_when_inactive(false))
                .on_hover_text("Refresh the list")
                .clicked()
            {
                state.refresh();
            }
        });
    });
    if ui
        .add_sized(
            [ui.available_width(), 30.0],
            egui::Button::new("+  New macro"),
        )
        .clicked()
    {
        state.request_new();
    }
    ui.add_space(4.0);

    let Some(store) = store else {
        ui.weak("The macro library is unavailable — see the warning above.");
        return;
    };
    if state.entries.is_empty() {
        ui.weak("No macros yet. Record one, or start typing in the editor.");
        return;
    }

    let idle = engine.shared.mode() == Mode::Idle;
    let mut do_open: Option<PathBuf> = None;
    let mut do_play: Option<PathBuf> = None;
    let mut do_rename: Option<(usize, String)> = None;
    let mut do_delete: Option<usize> = None;

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            for (i, entry) in state.entries.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    // Rename/delete stay out of sight until the row is pointed at.
                    let hovered = ui.rect_contains_pointer(ui.max_rect());
                    if let Some(err) = &entry.load_error {
                        ui.colored_label(p.error, "✖");
                        ui.label(&entry.name).on_hover_text(err);
                        if ui
                            .add(egui::Button::new("🗑").frame_when_inactive(false))
                            .on_hover_text("Delete the broken file")
                            .clicked()
                        {
                            do_delete = Some(i);
                        }
                        return;
                    }

                    if ui
                        .add_enabled(
                            idle,
                            egui::Button::new(egui::RichText::new("▶").color(p.green))
                                .small()
                                .frame_when_inactive(false),
                        )
                        .on_hover_text("Play this macro")
                        .clicked()
                    {
                        do_play = Some(entry.path.clone());
                    }

                    if state.renaming == Some(i) {
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut state.rename_field)
                                .desired_width(ui.available_width() - 64.0),
                        );
                        let commit = response.lost_focus()
                            && ui.input(|inp| inp.key_pressed(egui::Key::Enter));
                        if ui.small_button("✔").clicked() || commit {
                            do_rename = Some((i, state.rename_field.trim().to_string()));
                        }
                        if ui.small_button("✖").clicked() {
                            state.renaming = None;
                        }
                        return;
                    }

                    let is_open = state.selected.as_deref() == Some(entry.path.as_path());
                    let confirming = state.confirm_delete == Some(i);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if hovered || confirming {
                            if confirming {
                                if ui
                                    .add(egui::Button::new(
                                        egui::RichText::new("Delete?").color(p.error),
                                    ))
                                    .clicked()
                                {
                                    do_delete = Some(i);
                                }
                            } else if ui
                                .add(egui::Button::new("🗑").small().frame_when_inactive(false))
                                .on_hover_text("Delete")
                                .clicked()
                            {
                                state.confirm_delete = Some(i);
                            }
                            if ui
                                .add(egui::Button::new("✏").small().frame_when_inactive(false))
                                .on_hover_text("Rename")
                                .clicked()
                            {
                                state.renaming = Some(i);
                                state.rename_field = entry.name.clone();
                            }
                        }
                        if let (Some((sw, sh)), Some((w, h))) = (entry.screen, state.current_screen)
                        {
                            if (sw, sh) != (w, h) {
                                ui.colored_label(p.warn, "⚠").on_hover_text(format!(
                                    "Recorded on a {sw}×{sh} screen, this one is {w}×{h} — \
                                     absolute positions may be off"
                                ));
                            }
                        }
                        if entry.legacy_ron {
                            ui.weak("ron").on_hover_text(
                                "Old format — opening shows the converted script; \
                                 saving converts the file to .tomte",
                            );
                        }
                        ui.with_layout(
                            egui::Layout::left_to_right(egui::Align::Center)
                                .with_main_align(egui::Align::Min)
                                .with_main_justify(true),
                            |ui| {
                                let name = egui::Button::selectable(is_open, entry.name.as_str())
                                    .truncate();
                                if ui.add(name).clicked() && !is_open {
                                    do_open = Some(entry.path.clone());
                                }
                            },
                        );
                    });
                });
            }
        });

    if let Some(path) = do_open {
        state.request_open(path);
    }
    if let Some(path) = do_play {
        match storage::load_script(&path) {
            Ok(loaded) => {
                let script = Arc::new(loaded);
                state.playable = Some(script.clone());
                engine.send(Command::PlayMacro {
                    script,
                    options: state.playback_options(),
                });
            }
            Err(e) => state.notice = Some(format!("Could not play: {e}")),
        }
    }
    if let Some((i, new_name)) = do_rename {
        if !new_name.is_empty() {
            let old_path = state.entries[i].path.clone();
            match store.rename(&old_path, &new_name) {
                Ok(new_path) => {
                    if state.selected.as_deref() == Some(old_path.as_path()) {
                        // Re-open so the buffer picks up the new name directive.
                        state.open_path(&new_path);
                    }
                    state.notice = Some(format!("Renamed to “{new_name}”"));
                }
                Err(e) => state.notice = Some(format!("Rename failed: {e}")),
            }
        }
        state.refresh();
    }
    if let Some(i) = do_delete {
        let path = state.entries[i].path.clone();
        match store.delete(&path) {
            Ok(()) => {
                if state.selected.as_deref() == Some(path.as_path()) {
                    state.new_buffer();
                }
                state.notice = Some("Deleted".into());
            }
            Err(e) => state.notice = Some(format!("Delete failed: {e}")),
        }
        state.refresh();
    }
}

// --- toolbar and editor ---

#[allow(clippy::too_many_arguments)]
fn toolbar(
    ui: &mut egui::Ui,
    state: &mut MacrosUi,
    engine: &EngineHandle,
    store: Option<&MacroStore>,
    strip_keys: Vec<Key>,
    record_key: &str,
    play_key: &str,
    stop_key: &str,
) {
    let relaxed = std::sync::atomic::Ordering::Relaxed;
    let p = theme::palette(ui);
    match engine.shared.mode() {
        Mode::Idle => {
            // Wrapped: when the cheat sheet squeezes the middle column, the
            // buttons flow onto a second row instead of overflowing (which
            // would widen the Ui and push the editor under the panel).
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add(theme::accent_button("⏺  Record", p.red).right_text(key_text(record_key)))
                    .on_hover_text(
                        "Record your mouse and keyboard. New steps are added to the \
                         end of the open macro.",
                    )
                    .clicked()
                {
                    engine.send(Command::StartRecording(RecordConfig {
                        strip_keys,
                        ..Default::default()
                    }));
                }
                let parsed = state.parse_current();
                let playable = matches!(&parsed, Ok(s) if s.stats().instructions > 0);
                if ui
                    .add_enabled(
                        playable,
                        theme::accent_button("▶  Play", p.green).right_text(key_text(play_key)),
                    )
                    .on_disabled_hover_text("Record or write some steps first")
                    .clicked()
                {
                    if let Ok(script) = parsed {
                        state.playable = Some(script.clone());
                        engine.send(Command::PlayMacro {
                            script,
                            options: state.playback_options(),
                        });
                    }
                }
                let savable = state.dirty
                    || state.selected.is_none()
                    || state
                        .selected
                        .as_ref()
                        .is_some_and(|p| p.extension().is_some_and(|ext| ext == "ron"));
                if ui
                    .add_enabled(
                        savable && !state.buffer.trim().is_empty(),
                        egui::Button::new("💾 Save"),
                    )
                    .clicked()
                {
                    save(state, store);
                }
                ui.add_space(8.0);
                // Secondary tools: frameless until hovered or switched on.
                if ui
                    .add(egui::Button::new("✨ Tidy").frame_when_inactive(false))
                    .on_hover_text(
                        "Remove all mouse moves except those directly before a click \
                         or mousedown (reformats the script)",
                    )
                    .clicked()
                {
                    tidy(state);
                }
                if ui
                    .add(egui::Button::selectable(state.show_find, "🔍 Find"))
                    .on_hover_text("Find & replace with regular expressions")
                    .clicked()
                {
                    state.show_find = !state.show_find;
                }
                if ui
                    .add(egui::Button::selectable(
                        state.show_cheatsheet,
                        "📖 Cheat sheet",
                    ))
                    .on_hover_text("Every command the macro language knows")
                    .clicked()
                {
                    state.show_cheatsheet = !state.show_cheatsheet;
                }
            });
        }
        Mode::Recording => {
            ui.horizontal(|ui| {
                if ui
                    .add(
                        theme::accent_button("⏹  Stop recording", p.red)
                            .right_text(key_text(record_key)),
                    )
                    .clicked()
                {
                    engine.request_stop();
                }
                let events = engine.shared.events_recorded.load(relaxed);
                theme::status_dot(ui, p.red, format!("Recording — {events} events so far"));
            });
        }
        Mode::Playing => {
            ui.horizontal(|ui| {
                if ui
                    .add(theme::accent_button("⏹  Stop", p.red).right_text(key_text(stop_key)))
                    .clicked()
                {
                    engine.request_stop();
                }
                let iteration = engine.shared.playback_iteration.load(relaxed);
                theme::status_dot(ui, p.green, format!("Playing — round {}", iteration + 1));
            });
        }
        Mode::Clicking => {
            ui.weak("The auto-clicker is running — stop it first.");
        }
    }
}

/// A hotkey shown dimmed inside a filled button.
fn key_text(key: &str) -> egui::RichText {
    egui::RichText::new(key)
        .size(12.0)
        .color(egui::Color32::from_white_alpha(200))
}

fn options_row(ui: &mut egui::Ui, state: &mut MacrosUi) {
    // The segmented control is one nested row that `horizontal_wrapped`
    // can't break, so stack the two groups explicitly when space is short.
    let narrow = ui.available_width() < 600.0;
    let speed = |ui: &mut egui::Ui, state: &mut MacrosUi| {
        ui.weak("Speed");
        ui.add(
            egui::Slider::new(&mut state.speed, 0.25..=4.0)
                .logarithmic(true)
                .suffix("×"),
        );
    };
    let repeat = |ui: &mut egui::Ui, state: &mut MacrosUi| {
        ui.weak("Repeat");
        theme::segmented(
            ui,
            &mut state.repeat_infinite,
            &[(true, "Until stopped"), (false, "A set number")],
        );
        if !state.repeat_infinite {
            ui.add(
                egui::DragValue::new(&mut state.repeat_times)
                    .range(1..=1_000_000)
                    .suffix(" times"),
            );
        }
    };
    if narrow {
        ui.horizontal(|ui| speed(ui, state));
        ui.horizontal(|ui| repeat(ui, state));
    } else {
        ui.horizontal(|ui| {
            speed(ui, state);
            ui.add_space(10.0);
            repeat(ui, state);
        });
    }
}

fn save(state: &mut MacrosUi, store: Option<&MacroStore>) -> bool {
    let Some(store) = store else {
        state.notice = Some("The macro library is unavailable, so nothing can be saved".into());
        return false;
    };
    let mut converted = false;
    let result = match state.selected.clone() {
        Some(path) if path.extension().is_some_and(|ext| ext == SCRIPT_EXT) => {
            storage::save_text(&state.buffer, &path).map(|()| path)
        }
        Some(ron_path) => {
            // Legacy file: write a .tomte next to it, then retire the .ron.
            converted = true;
            let name = state
                .entries
                .iter()
                .find(|e| e.path == ron_path)
                .map(|e| e.name.clone())
                .unwrap_or_else(|| {
                    ron_path
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default()
                });
            store
                .save_new_script(&name, &state.buffer)
                .and_then(|new_path| store.delete(&ron_path).map(|()| new_path))
        }
        None => {
            let name = state.untitled_name.trim();
            let name = if name.is_empty() { "unnamed" } else { name };
            store.save_new_script(name, &state.buffer)
        }
    };
    match result {
        Ok(path) => {
            state.notice = Some(if converted {
                format!("✔ Converted to {}", path.display())
            } else {
                format!("✔ Saved to {}", path.display())
            });
            state.selected = Some(path);
            state.dirty = false;
            state.refresh();
            true
        }
        Err(e) => {
            state.notice = Some(format!("Save failed: {e}"));
            false
        }
    }
}

fn tidy(state: &mut MacrosUi) {
    match state.parse_current() {
        Ok(parsed) => {
            let mut tidied = (*parsed).clone();
            let removed = script::strip_moves(&mut tidied.body);
            if removed == 0 {
                state.notice = Some("Already tidy — no extra mouse moves".into());
                return;
            }
            state.buffer = script::format(&tidied);
            state.dirty = true;
            state.find_from = 0;
            state.notice = Some(format!("✨ Removed {removed} mouse moves"));
        }
        Err(e) => state.notice = Some(format!("Fix the script first — {e}")),
    }
}

fn find_bar(ui: &mut egui::Ui, state: &mut MacrosUi) {
    ui.horizontal(|ui| {
        ui.label("🔍");
        ui.add(
            egui::TextEdit::singleline(&mut state.find_pattern)
                .hint_text("regex, e.g. wait \\d+ms")
                .desired_width(170.0),
        );
        ui.add(
            egui::TextEdit::singleline(&mut state.replace_with)
                .hint_text("replace with ($1 for groups)")
                .desired_width(170.0),
        );
        if state.find_pattern.is_empty() {
            return;
        }
        match state.compiled_regex() {
            Err(e) => {
                ui.colored_label(ui.visuals().error_fg_color, "bad pattern")
                    .on_hover_text(e);
            }
            Ok(re) => {
                let count = re.find_iter(&state.buffer).count();
                if ui
                    .add_enabled(count > 0, egui::Button::new("find next"))
                    .clicked()
                {
                    find_next(ui.ctx(), state, &re);
                }
                if ui
                    .add_enabled(count > 0, egui::Button::new("replace all"))
                    .clicked()
                {
                    let replaced = re
                        .replace_all(&state.buffer, state.replace_with.as_str())
                        .into_owned();
                    if replaced != state.buffer {
                        state.buffer = replaced;
                        state.dirty = true;
                        state.find_from = 0;
                    }
                    state.notice = Some(format!(
                        "Replaced {count} match{}",
                        if count == 1 { "" } else { "es" }
                    ));
                }
                ui.weak(format!(
                    "{count} match{}",
                    if count == 1 { "" } else { "es" }
                ));
            }
        }
    });
}

fn find_next(ctx: &egui::Context, state: &mut MacrosUi, re: &regex::Regex) {
    let start = state.find_from.min(state.buffer.len());
    let hit = re
        .find_at(&state.buffer, start)
        .or_else(|| re.find(&state.buffer)); // wrap around
    let Some(hit) = hit else { return };
    // Guarantee forward progress even on empty matches.
    state.find_from = hit.end().max(hit.start() + 1);

    // Regex offsets are bytes; egui cursors are char indices.
    let char_start = state.buffer[..hit.start()].chars().count();
    let char_end = char_start + state.buffer[hit.start()..hit.end()].chars().count();
    let id = egui::Id::new(EDITOR_ID);
    if let Some(mut edit_state) = egui::TextEdit::load_state(ctx, id) {
        edit_state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(char_start),
                egui::text::CCursor::new(char_end),
            )));
        egui::TextEdit::store_state(ctx, id, edit_state);
        ctx.memory_mut(|memory| memory.request_focus(id));
    }
}

fn status_line(ui: &mut egui::Ui, state: &mut MacrosUi) {
    let p = theme::palette(ui);
    match state.parse_current() {
        Ok(parsed) => {
            let stats = parsed.stats();
            ui.horizontal(|ui| {
                ui.weak(format!(
                    "✔ {} steps · ≈ {:.1} s per round at 1×",
                    stats.instructions,
                    stats.nominal_us as f64 / 1e6,
                ));
                if state.dirty {
                    ui.label(egui::RichText::new("· unsaved changes").color(p.accent_text));
                }
            });
        }
        Err(e) => {
            ui.colored_label(p.error, format!("✖ {e}"));
        }
    }
}

fn editor(ui: &mut egui::Ui, state: &mut MacrosUi) {
    let mut changed = false;
    {
        let MacrosUi {
            buffer,
            highlight_cache,
            ..
        } = state;
        let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
            let mut job = highlighted_job(ui, buf.as_str(), highlight_cache);
            job.wrap.max_width = wrap_width;
            ui.fonts_mut(|fonts| fonts.layout_job(job))
        };
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                let response = ui.add(
                    egui::TextEdit::multiline(buffer)
                        .id(egui::Id::new(EDITOR_ID))
                        .code_editor()
                        .hint_text(
                            "Recorded steps show up here — or type your own, e.g.  click left",
                        )
                        .desired_width(f32::INFINITY)
                        .desired_rows(16)
                        .layouter(&mut layouter),
                );
                changed = response.changed();
            });
    }
    if changed {
        state.dirty = true;
    }
}

// --- unsaved-changes dialog ---

fn unsaved_dialog(ui: &mut egui::Ui, state: &mut MacrosUi, store: Option<&MacroStore>) {
    if state.pending.is_none() {
        return;
    }
    // None = keep asking; Some(true) = proceed; Some(false) = stay put.
    let mut decision: Option<bool> = None;
    let response = egui::Modal::new(egui::Id::new("unsaved-macro")).show(ui.ctx(), |ui| {
        let p = theme::palette(ui);
        ui.label(
            egui::RichText::new("Save your changes?")
                .size(17.0)
                .color(p.text),
        );
        ui.label("The macro in the editor has changes that are not saved yet.");
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui
                .add(theme::accent_button("💾 Save first", p.green))
                .clicked()
            {
                decision = Some(save(state, store));
            }
            if ui.button("Discard changes").clicked() {
                decision = Some(true);
            }
            if ui.button("Cancel").clicked() {
                decision = Some(false);
            }
        });
    });
    if decision.is_none() && response.should_close() {
        decision = Some(false);
    }
    match decision {
        Some(true) => match state.pending.take() {
            Some(PendingAction::Open(path)) => {
                state.open_path(&path);
            }
            Some(PendingAction::New) => state.new_buffer(),
            None => {}
        },
        Some(false) => state.pending = None,
        None => {}
    }
}

// --- syntax highlighting ---

fn highlighted_job(
    ui: &egui::Ui,
    text: &str,
    cache: &mut Option<(u64, egui::text::LayoutJob)>,
) -> egui::text::LayoutJob {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    ui.visuals().dark_mode.hash(&mut hasher);
    let hash = hasher.finish();
    if let Some((cached_hash, job)) = cache {
        if *cached_hash == hash {
            return job.clone();
        }
    }
    let job = build_job(ui, text);
    *cache = Some((hash, job.clone()));
    job
}

fn build_job(ui: &egui::Ui, text: &str) -> egui::text::LayoutJob {
    use egui::text::{LayoutJob, TextFormat};

    let font = egui::TextStyle::Monospace.resolve(ui.style());
    let visuals = ui.visuals();
    let normal = TextFormat::simple(font.clone(), visuals.text_color());
    let comment = TextFormat::simple(font.clone(), visuals.weak_text_color());
    let keyword = TextFormat::simple(font.clone(), visuals.hyperlink_color);
    let string = TextFormat::simple(font, theme::palette(ui).string);

    let mut job = LayoutJob::default();
    if text.lines().count() > HIGHLIGHT_MAX_LINES {
        job.append(text, 0.0, normal);
        return job;
    }
    // split_inclusive keeps the newlines so offsets stay aligned.
    for line in text.split_inclusive('\n') {
        append_line(&mut job, line, &normal, &comment, &keyword, &string);
    }
    if job.text.is_empty() {
        job.append("", 0.0, normal);
    }
    job
}

/// One line: leading whitespace, keyword, arguments with quoted strings,
/// then (outside quotes) a `#` comment to the end.
fn append_line(
    job: &mut egui::text::LayoutJob,
    line: &str,
    normal: &egui::text::TextFormat,
    comment: &egui::text::TextFormat,
    keyword: &egui::text::TextFormat,
    string: &egui::text::TextFormat,
) {
    #[derive(PartialEq)]
    enum State {
        Indent,
        Word,
        Rest,
        InString,
        Comment,
    }
    let mut state = State::Indent;
    let mut span_start = 0;
    let mut escaped = false;

    let fmt_of = |state: &State| match state {
        State::Indent | State::Rest => normal,
        State::Word => keyword,
        State::InString => string,
        State::Comment => comment,
    };

    for (i, c) in line.char_indices() {
        let next = match state {
            State::Indent if c == '#' => Some(State::Comment),
            State::Indent if !c.is_whitespace() => Some(State::Word),
            State::Word if c.is_whitespace() => Some(State::Rest),
            State::Rest if c == '"' => Some(State::InString),
            State::Rest if c == '#' => Some(State::Comment),
            State::InString => {
                if escaped {
                    escaped = false;
                    None
                } else if c == '\\' {
                    escaped = true;
                    None
                } else if c == '"' {
                    // Include the closing quote in the string span.
                    let end = i + c.len_utf8();
                    job.append(&line[span_start..end], 0.0, string.clone());
                    span_start = end;
                    state = State::Rest;
                    continue;
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some(next) = next {
            if i > span_start {
                job.append(&line[span_start..i], 0.0, fmt_of(&state).clone());
                span_start = i;
            }
            state = next;
        }
    }
    if span_start < line.len() {
        job.append(&line[span_start..], 0.0, fmt_of(&state).clone());
    }
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
