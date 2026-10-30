//! Platform warning banners pinned above everything else.

use eframe::egui;

use crate::theme;

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
    let p = theme::palette(ui);
    egui::Panel::top(egui::Id::new("banners"))
        .frame(egui::Frame::new().fill(p.bg).inner_margin(egui::Margin {
            left: 14,
            right: 14,
            top: 2,
            bottom: 4,
        }))
        .show_separator_line(false)
        .show(ui, |ui| {
            for banner in banners {
                let color = match banner.severity {
                    Severity::Warning => p.warn,
                    Severity::Error => p.error,
                };
                egui::Frame::new()
                    .fill(color.gamma_multiply(0.10))
                    .stroke(egui::Stroke::new(1.0, color.gamma_multiply(0.45)))
                    .corner_radius(8)
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.colored_label(color, format!("⚠  {}", banner.text));
                    });
            }
        });
}
