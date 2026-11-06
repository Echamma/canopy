#![windows_subsystem = "windows"]

fn main() -> eframe::Result {
    let result = canopy_ui::run_standalone(&canopy::videochat::DESCRIPTOR);
    canopy::videochat::shutdown();
    result
}
