#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // no console window on Windows

mod app;
mod banners;
mod hotkeys;
mod tabs;

fn main() -> eframe::Result {
    env_logger::init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([560.0, 480.0])
            .with_min_inner_size([420.0, 360.0])
            .with_title("TomteMacro"),
        ..Default::default()
    };
    eframe::run_native(
        "TomteMacro",
        options,
        Box::new(|cc| Ok(Box::new(app::TomteApp::new(cc)))),
    )
}

use eframe::egui;
