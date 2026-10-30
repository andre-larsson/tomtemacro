//! Look and feel: a warm palette lifted straight from the app icon (cream,
//! tomte red, forest green, slate), roomier spacing, and a handful of shared
//! widgets so every tab speaks the same visual language.

use eframe::egui::{
    self, Color32, CornerRadius, FontId, Margin, Shadow, Stroke, TextStyle, Theme, Vec2,
};

/// The colors one theme variant is built from.
pub struct Palette {
    /// Window background.
    pub bg: Color32,
    /// Raised surfaces: cards, text fields, popups.
    pub card: Color32,
    pub border: Color32,
    /// Outline of buttons, inputs and checkboxes — a touch stronger than
    /// `border` so controls read as clickable.
    pub edge: Color32,
    pub text: Color32,
    pub weak: Color32,
    /// Resting fill of ordinary buttons and inputs.
    pub widget: Color32,
    pub widget_hover: Color32,
    pub widget_active: Color32,
    /// Brand red (the hat): primary actions, recording.
    pub red: Color32,
    /// Brand green (the body): running/clicking, play.
    pub green: Color32,
    /// Selected tabs, segments and text selection.
    pub accent_soft: Color32,
    pub accent_text: Color32,
    pub warn: Color32,
    pub error: Color32,
    /// Syntax highlighting in the macro editor.
    pub keyword: Color32,
    pub string: Color32,
}

pub const LIGHT: Palette = Palette {
    bg: Color32::from_rgb(0xFB, 0xF7, 0xEF),
    card: Color32::from_rgb(0xFF, 0xFE, 0xFB),
    border: Color32::from_rgb(0xE7, 0xDE, 0xCC),
    edge: Color32::from_rgb(0xDB, 0xCF, 0xB8),
    text: Color32::from_rgb(0x2F, 0x3B, 0x45),
    weak: Color32::from_rgb(0x85, 0x7D, 0x71),
    widget: Color32::from_rgb(0xF2, 0xEB, 0xDD),
    widget_hover: Color32::from_rgb(0xEB, 0xE1, 0xCE),
    widget_active: Color32::from_rgb(0xE2, 0xD5, 0xBC),
    red: Color32::from_rgb(0xD2, 0x38, 0x2C),
    green: Color32::from_rgb(0x41, 0x60, 0x3C),
    accent_soft: Color32::from_rgb(0xF8, 0xDE, 0xD8),
    accent_text: Color32::from_rgb(0xA8, 0x2B, 0x21),
    warn: Color32::from_rgb(0xB0, 0x6A, 0x00),
    error: Color32::from_rgb(0xC0, 0x30, 0x25),
    keyword: Color32::from_rgb(0xB8, 0x32, 0x28),
    string: Color32::from_rgb(0x3F, 0x76, 0x2C),
};

pub const DARK: Palette = Palette {
    bg: Color32::from_rgb(0x1F, 0x1C, 0x19),
    card: Color32::from_rgb(0x29, 0x25, 0x21),
    border: Color32::from_rgb(0x3B, 0x35, 0x2F),
    edge: Color32::from_rgb(0x4B, 0x43, 0x3C),
    text: Color32::from_rgb(0xEE, 0xE7, 0xD9),
    weak: Color32::from_rgb(0xA0, 0x97, 0x89),
    widget: Color32::from_rgb(0x35, 0x30, 0x2B),
    widget_hover: Color32::from_rgb(0x41, 0x3A, 0x34),
    widget_active: Color32::from_rgb(0x4C, 0x44, 0x3D),
    red: Color32::from_rgb(0xDA, 0x47, 0x3A),
    green: Color32::from_rgb(0x55, 0x80, 0x4E),
    accent_soft: Color32::from_rgb(0x5A, 0x2E, 0x29),
    accent_text: Color32::from_rgb(0xF5, 0xB0, 0xA6),
    warn: Color32::from_rgb(0xE8, 0xA3, 0x3D),
    error: Color32::from_rgb(0xF0, 0x70, 0x5F),
    keyword: Color32::from_rgb(0xF0, 0x8A, 0x7E),
    string: Color32::from_rgb(0xA5, 0xC2, 0x61),
};

/// The palette matching whatever theme `ui` is currently drawn with.
pub fn palette(ui: &egui::Ui) -> &'static Palette {
    if ui.visuals().dark_mode {
        &DARK
    } else {
        &LIGHT
    }
}

/// Install both theme variants; the active one follows the theme preference.
pub fn install(ctx: &egui::Context) {
    ctx.set_style_of(Theme::Light, style(ctx.style_of(Theme::Light), &LIGHT));
    ctx.set_style_of(Theme::Dark, style(ctx.style_of(Theme::Dark), &DARK));
}

fn style(base: std::sync::Arc<egui::Style>, p: &Palette) -> egui::Style {
    let mut style = (*base).clone();

    style.text_styles = [
        (TextStyle::Small, FontId::proportional(11.0)),
        (TextStyle::Body, FontId::proportional(14.5)),
        (TextStyle::Button, FontId::proportional(14.5)),
        (TextStyle::Heading, FontId::proportional(19.0)),
        (TextStyle::Monospace, FontId::monospace(13.0)),
    ]
    .into();

    let s = &mut style.spacing;
    s.item_spacing = Vec2::new(8.0, 7.0);
    s.button_padding = Vec2::new(10.0, 4.0);
    s.interact_size = Vec2::new(40.0, 26.0);
    s.window_margin = Margin::same(14);
    s.menu_margin = Margin::same(8);
    s.combo_width = 140.0;
    s.icon_width = 16.0;
    s.icon_width_inner = 9.0;
    s.slider_width = 160.0;

    let v = &mut style.visuals;
    v.override_text_color = None;
    v.weak_text_color = Some(p.weak);
    v.hyperlink_color = p.keyword;
    v.panel_fill = p.bg;
    v.window_fill = p.card;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(12);
    v.menu_corner_radius = CornerRadius::same(8);
    v.window_shadow = Shadow {
        offset: [0, 8],
        blur: 24,
        spread: 0,
        color: Color32::from_black_alpha(if v.dark_mode { 90 } else { 28 }),
    };
    v.popup_shadow = Shadow {
        offset: [0, 4],
        blur: 12,
        spread: 0,
        color: Color32::from_black_alpha(if v.dark_mode { 80 } else { 22 }),
    };
    v.faint_bg_color = p.widget;
    v.extreme_bg_color = p.card;
    v.text_edit_bg_color = Some(p.card);
    v.code_bg_color = p.widget;
    v.warn_fg_color = p.warn;
    v.error_fg_color = p.error;
    v.selection.bg_fill = p.accent_soft;
    v.selection.stroke = Stroke::new(1.0, p.accent_text);
    v.text_cursor.stroke = Stroke::new(2.0, p.red);
    v.slider_trailing_fill = true;

    // Small enough that checkboxes stay square-ish (they share this radius).
    let radius = CornerRadius::same(5);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.bg;
    w.noninteractive.weak_bg_fill = p.bg;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    w.noninteractive.corner_radius = radius;

    w.inactive.bg_fill = p.widget;
    w.inactive.weak_bg_fill = p.widget;
    w.inactive.bg_stroke = Stroke::new(1.0, p.edge);
    w.inactive.fg_stroke = Stroke::new(1.0, p.text);
    w.inactive.corner_radius = radius;

    w.hovered.bg_fill = p.widget_hover;
    w.hovered.weak_bg_fill = p.widget_hover;
    w.hovered.bg_stroke = Stroke::new(1.0, p.edge);
    w.hovered.fg_stroke = Stroke::new(1.5, p.text);
    w.hovered.corner_radius = radius;
    w.hovered.expansion = 0.0;

    w.active.bg_fill = p.widget_active;
    w.active.weak_bg_fill = p.widget_active;
    w.active.bg_stroke = Stroke::new(1.0, p.red);
    w.active.fg_stroke = Stroke::new(1.5, p.text);
    w.active.corner_radius = radius;
    w.active.expansion = 0.0;

    w.open.bg_fill = p.widget_hover;
    w.open.weak_bg_fill = p.widget_hover;
    w.open.bg_stroke = Stroke::new(1.0, p.edge);
    w.open.fg_stroke = Stroke::new(1.0, p.text);
    w.open.corner_radius = radius;

    style
}

// --- shared widgets ---

/// A raised, rounded surface that groups related controls.
pub fn card<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let p = palette(ui);
    egui::Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(12)
        .inner_margin(Margin::symmetric(16, 14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui)
        })
        .inner
}

/// Lay `add_contents` out in a column at most `max_width` wide, centered
/// horizontally — short forms read better than ones stretched edge to edge.
pub fn centered_column<R>(
    ui: &mut egui::Ui,
    max_width: f32,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let width = ui.available_width().min(max_width);
    let pad = ((ui.available_width() - width) / 2.0).floor();
    ui.horizontal_top(|ui| {
        ui.add_space(pad);
        ui.vertical(|ui| {
            ui.set_width(width);
            add_contents(ui)
        })
        .inner
    })
    .inner
}

/// Muted section title used above cards.
pub fn section_title(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).size(13.0).color(palette(ui).weak));
}

/// A big, filled call-to-action button.
pub fn primary_button(text: impl Into<String>, fill: Color32) -> egui::Button<'static> {
    egui::Button::new(
        egui::RichText::new(text.into())
            .size(16.0)
            .color(Color32::WHITE),
    )
    .fill(fill)
    .corner_radius(10)
}

/// A medium filled button for toolbars.
pub fn accent_button(text: impl Into<String>, fill: Color32) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text.into()).color(Color32::WHITE))
        .fill(fill)
        .corner_radius(8)
}

/// One line of hint text with the hotkey drawn as a keycap, e.g.
/// "or press  F6  anywhere". A single label, so it centers like one.
pub fn hotkey_hint(ui: &mut egui::Ui, before: &str, key: &str, after: &str) {
    if key.is_empty() {
        return;
    }
    let p = palette(ui);
    let font = TextStyle::Body.resolve(ui.style());
    let weak = egui::TextFormat::simple(font.clone(), p.weak);
    let mut job = egui::text::LayoutJob::default();
    job.append(before, 0.0, weak.clone());
    job.append(
        &format!(" {key} "),
        6.0,
        egui::TextFormat {
            font_id: FontId::proportional(font.size - 1.0),
            color: p.text,
            background: p.widget_hover,
            expand_bg: 2.0,
            ..Default::default()
        },
    );
    job.append(after, 6.0, weak);
    ui.label(job);
}

/// A row of joined toggle buttons; exactly one option is active.
/// Returns true when the user picked a different option.
pub fn segmented<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    value: &mut T,
    options: &[(T, &str)],
) -> bool {
    let mut changed = false;
    let p = palette(ui);
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.horizontal(|ui| {
            let last = options.len().saturating_sub(1);
            for (i, (option, label)) in options.iter().enumerate() {
                let selected = *value == *option;
                let r = 8;
                let corners = CornerRadius {
                    nw: if i == 0 { r } else { 2 },
                    sw: if i == 0 { r } else { 2 },
                    ne: if i == last { r } else { 2 },
                    se: if i == last { r } else { 2 },
                };
                let text = egui::RichText::new(*label).color(if selected {
                    p.accent_text
                } else {
                    p.text
                });
                let mut button = egui::Button::new(text).corner_radius(corners);
                if selected {
                    button = button
                        .fill(p.accent_soft)
                        .stroke(Stroke::new(1.0, p.accent_text.gamma_multiply(0.35)));
                }
                if ui.add(button).clicked() && !selected {
                    *value = *option;
                    changed = true;
                }
            }
        });
    });
    changed
}

/// A small colored dot followed by text — used for live status.
pub fn status_dot(ui: &mut egui::Ui, color: Color32, text: impl Into<String>) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(9.0), egui::Sense::hover());
        ui.painter().circle_filled(rect.center(), 4.5, color);
        ui.label(egui::RichText::new(text.into()).color(color));
    });
}
