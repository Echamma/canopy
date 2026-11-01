//! Runs one app in its own window: what each app's own exe does when it isn't inside Canopy.

use crate::app::{AppDescriptor, CanopyApp, Host, SharedFile};
use crate::paint::paint_ground;
use crate::theme::{Mood, color, font, install};
use egui::{Align2, Rect, UiBuilder, pos2, vec2};
use std::path::PathBuf;
use std::time::{Duration, Instant};

struct Toasts(Vec<(String, Instant)>);

impl Host for Toasts {
    fn toast(&mut self, text: String) {
        self.0.push((text, Instant::now()));
    }
    fn exported(&mut self, path: PathBuf) {
        let folder = path.parent().map(|p| p.display().to_string()).unwrap_or_default();
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        self.toast(format!("Saved {name} to {folder}"));
    }
}

struct Standalone {
    app: Box<dyn CanopyApp>,
    toasts: Toasts,
    title: String,
    name: &'static str,
}

impl eframe::App for Standalone {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let rect = ui.max_rect();
        paint_ground(ui.painter(), rect, Mood::Ember);
        let dropped: Vec<SharedFile> = ui.ctx().input(|input| input.raw.dropped_files.iter().map(|file| file.path().to_path_buf()).filter(|path| !path.as_os_str().is_empty()).map(SharedFile::new).collect());
        if !dropped.is_empty() {
            let position = ui.ctx().input(|input| input.pointer.hover_pos());
            let receipt = match position {
                Some(position) => self.app.receive_at(dropped, position, &mut self.toasts),
                None => self.app.receive(dropped, &mut self.toasts),
            };
            if let Some(note) = receipt.note {
                self.toasts.toast(note);
            } else if !receipt.skipped.is_empty() {
                self.toasts.toast(format!("{} can't open {}", self.name, receipt.skipped.join(", ")));
            }
        }
        ui.scope_builder(UiBuilder::new().max_rect(rect.shrink(6.0)), |ui| self.app.ui(ui, &mut self.toasts));

        let title = match self.app.title() {
            t if t.is_empty() => self.name.to_string(),
            t => format!("{t} — {}", self.name),
        };
        if title != self.title {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }

        self.toasts.0.retain(|(_, at)| at.elapsed() < Duration::from_secs(5));
        if let Some((text, _)) = self.toasts.0.last() {
            let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("standalone-toast")));
            let galley = painter.layout_no_wrap(text.clone(), font::ui(13.0), color::SELECT_INK);
            let size = galley.size() + vec2(24.0, 12.0);
            let toast = Rect::from_center_size(pos2(rect.center().x, rect.bottom() - 28.0), size);
            painter.rect_filled(toast, 99, Mood::Ember.pane_solid());
            painter.rect_filled(toast, 99, color::SELECT_FILL);
            painter.text(toast.center(), Align2::CENTER_CENTER, text, font::ui(13.0), color::SELECT_INK);
            ui.ctx().request_repaint_after(Duration::from_millis(250));
        }
    }

    fn on_exit(&mut self) {
        if let Err(error) = self.app.save() {
            eprintln!("{} couldn't save on close: {error}", self.name);
        }
    }
}

/// Opens the app alone, with its standalone folders (Documents and AppData). Files given on the command line open in it.
pub fn run_standalone(descriptor: &'static AppDescriptor) -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default().with_title(descriptor.name).with_inner_size([1440.0, 900.0]).with_min_inner_size([960.0, 640.0]);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(descriptor.icon_png) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions { viewport, centered: true, ..Default::default() };
    eframe::run_native(
        descriptor.name,
        options,
        Box::new(move |creation| {
            install(&creation.egui_ctx);
            let mut app = (descriptor.create)(descriptor.standalone_place(), &creation.egui_ctx);
            let mut toasts = Toasts(Vec::new());
            let opened: Vec<SharedFile> = std::env::args().skip(1).filter(|arg| !arg.starts_with('-')).map(SharedFile::new).filter(|file| file.path.is_file()).collect();
            if !opened.is_empty() {
                app.receive(opened, &mut toasts);
            }
            Ok(Box::new(Standalone { app, toasts, title: String::new(), name: descriptor.name }))
        }),
    )
}
