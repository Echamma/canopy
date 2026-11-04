#![windows_subsystem = "windows"]

fn main() -> eframe::Result {
    canopy_ui::run_standalone(&canopy::moodboard::DESCRIPTOR)
}
