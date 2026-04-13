#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(confusable_idents, mixed_script_confusables)]
#![allow(dead_code, unused_imports, unused_variables)]

mod app;
mod cymatics;
mod theremin;
mod gravity;
mod fluiddrum;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 900.0])
            .with_title("Synesthesia"),
        ..Default::default()
    };
    eframe::run_native(
        "Synesthesia",
        options,
        Box::new(|cc| Ok(Box::new(app::SoundArtApp::new(cc)))),
    )
}
