// Canopy, native: one window for your apps, with shared spaces and files.
#![windows_subsystem = "windows"]

mod shell;

fn main() -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default().with_title("Canopy").with_inner_size([1480.0, 920.0]).with_min_inner_size([1100.0, 700.0]);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../../../../assets/canopy.png")) {
        viewport = viewport.with_icon(icon);
    }
    eframe::run_native(
        "Canopy",
        eframe::NativeOptions { viewport, centered: true, ..Default::default() },
        Box::new(|creation| {
            canopy_ui::install(&creation.egui_ctx);
            let mut shell = shell::Shell::new();
            shell.open_from_args();
            Ok(Box::new(shell))
        }),
    )
}
