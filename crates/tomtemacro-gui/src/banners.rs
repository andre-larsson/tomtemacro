//! Platform warning banners pinned above everything else.

use eframe::egui;

pub enum Severity {
    Warning,
    Error,
}

pub struct Banner {
    pub severity: Severity,
    pub text: String,
}

pub fn show(ui: &mut egui::Ui, banners: &[Banner]) {
    if banners.is_empty() {
        return;
    }
    egui::Panel::top(egui::Id::new("banners")).show(ui, |ui| {
        for banner in banners {
            let color = match banner.severity {
                Severity::Warning => ui.visuals().warn_fg_color,
                Severity::Error => ui.visuals().error_fg_color,
            };
            ui.colored_label(color, format!("⚠ {}", banner.text));
        }
    });
}
