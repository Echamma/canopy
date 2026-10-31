//! Canopy Night for egui, and the interface every Canopy app implements.
//!
//! An app is a library with an [`AppDescriptor`]. Canopy shows it as a pane beside the other apps; [`run_standalone`]
//! opens it in its own window, which is what each app's own exe does.

mod app;
mod paint;
mod standalone;
mod theme;
mod widgets;

pub use app::*;
pub use canopy_core;
pub use eframe::{self, egui};
pub use paint::*;
pub use standalone::run_standalone;
pub use theme::*;
pub use widgets::*;
