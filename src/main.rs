// No console window on Windows release builds.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod config;
mod curve;
mod engine;
mod system;
mod ui;

use eframe::egui;

/// Window icon (same artwork as the .exe icon).
fn app_icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")).unwrap_or_default()
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Goodnight")
            .with_app_id("goodnight")
            .with_inner_size([470.0, 720.0])
            .with_min_inner_size([420.0, 560.0])
            .with_icon(app_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "Goodnight",
        options,
        Box::new(|_cc| Ok(Box::new(ui::GoodnightApp::new()))),
    )
}
