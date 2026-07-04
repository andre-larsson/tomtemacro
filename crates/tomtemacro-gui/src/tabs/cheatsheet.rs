//! Condensed macro-language reference shown in a side panel next to the
//! editor. The full reference lives in `docs/macro-language.md`.

use eframe::egui;

const COMMANDS: &[(&str, &str)] = &[
    ("move 812 344", "move the mouse to absolute pixels"),
    ("move +10 -20", "move relative to the current position"),
    (
        "moverel -10 -20",
        "relative move when both deltas are negative",
    ),
    ("click left", "click a button: left, right, or middle"),
    ("click left at 812 344", "move there, then click"),
    ("doubleclick left", "two clicks, 30 ms apart"),
    ("mousedown left", "hold a button …"),
    ("mouseup left", "… and release it (drags)"),
    ("scroll up 3", "scroll up/down/left/right N notches"),
    ("press enter", "tap a key (press + release)"),
    ("keydown ctrl", "hold a key …"),
    ("keyup ctrl", "… and release it (chords)"),
    ("type \"hello!\"", "type text (printable ASCII)"),
    ("wait 500", "pause 500 ms — also 250ms, 2s, 8.5ms"),
    ("wait 100ms..300ms", "random pause, new roll every time"),
    ("repeat 10 … end", "repeat the lines in between (nestable)"),
    ("# note to self", "comment — also allowed after a command"),
];

pub fn show(ui: &mut egui::Ui) {
    ui.add_space(4.0);
    ui.heading("Cheat sheet");
    ui.add_space(4.0);
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            for (syntax, what) in COMMANDS {
                ui.monospace(*syntax);
                ui.weak(*what);
                ui.add_space(6.0);
            }
            ui.separator();
            ui.weak(
                "Key names: a–z, 0–9, f1–f12, enter, space, tab, esc, backspace, \
                 delete, insert, home, end, pageup, pagedown, up, down, left, right, \
                 ctrl, rctrl, shift, rshift, alt, altgr, meta, kp0–kp9, kpenter, \
                 minus, equal, comma, dot, slash, semicolon, quote, …",
            );
            ui.add_space(4.0);
            ui.weak("Full reference: docs/macro-language.md in the repository.");
        });
}
