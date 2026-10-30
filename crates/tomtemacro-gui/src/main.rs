#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // no console window on Windows

mod app;
mod banners;
mod hotkeys;
mod settings;
mod tabs;
mod theme;

fn main() -> eframe::Result {
    env_logger::init();
    // Window/taskbar icon only; the Windows exe icon comes from build.rs and
    // the macOS dock icon from the .app bundle's .icns.
    let icon = eframe::icon_data::from_png_bytes(include_bytes!(
        "../../../assets/icon/png/tomtemacro-256.png"
    ))
    .expect("embedded icon PNG is valid");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            // Wide enough for the Macros tab's list + editor + cheat sheet.
            .with_inner_size([840.0, 580.0])
            .with_min_inner_size([460.0, 400.0])
            .with_title("TomteMacro")
            .with_icon(icon),
        ..Default::default()
    };
    eframe::run_native(
        "TomteMacro",
        options,
        Box::new(|cc| Ok(Box::new(app::TomteApp::new(cc)))),
    )
}

use eframe::egui;
