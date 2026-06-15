// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod capture;
mod library;
mod model;
mod replay;
mod winutil;

use eframe::egui;

fn main() -> eframe::Result<()> {
    // Start the global input hooks before the GUI so the record hotkey works
    // immediately.
    let capture = capture::start();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([960.0, 600.0])
            .with_min_inner_size([700.0, 420.0])
            .with_title("Mackrey — Macro Recorder"),
        ..Default::default()
    };

    eframe::run_native(
        "Mackrey",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, capture)))),
    )
}
