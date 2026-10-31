//! The interface between Canopy and its apps. Apps are libraries: Canopy shows each one in a pane of its window,
//! and each app's own exe shows it alone (see [`crate::run_standalone`]). Both go through the same trait.

use crate::theme::Accent;
use canopy_core::files::{Kind, kind_of};
use canopy_core::spaces::SpacePaths;
use std::path::PathBuf;

/// A file handed to an app: dragged from Canopy's sidebar, sent from a menu, or dropped from Explorer.
#[derive(Debug, Clone, PartialEq)]
pub struct SharedFile {
    pub path: PathBuf,
    pub name: String,
    pub kind: Kind,
}

impl SharedFile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        Self { kind: kind_of(&name), name, path }
    }
}

/// The payload of a drag inside a Canopy window, so an app can tell where a file was dropped on it.
#[derive(Debug, Clone)]
pub struct DraggedFiles(pub Vec<SharedFile>);

/// A project an app keeps in a space, for Canopy's sidebar.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub id: String,
    pub name: String,
    pub saved_at: u64,
    pub detail: String,
}

/// What an app did with files it was handed.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Receipt {
    pub accepted: usize,
    pub skipped: Vec<String>,
    /// Why nothing was taken, when there is a reason worth saying ("Open a project first").
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpaceInfo {
    pub id: String,
    pub name: String,
    pub dir: PathBuf,
}

/// Where an app keeps its things this run.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    /// The space it runs in, or `None` when it runs on its own.
    pub space: Option<SpaceInfo>,
    /// Visible projects (`<space>\StreamCuts`, or `Documents\StreamCuts Projects` on its own).
    pub projects_dir: PathBuf,
    /// Private data (caches, recovery copies).
    pub data_dir: PathBuf,
    /// Where exports go (the space's Files, or `Documents\StreamCuts Exports` on its own).
    pub exports_dir: PathBuf,
}

/// What an app can ask of whoever shows it.
pub trait Host {
    /// A short message for the person ("Exported clip.mp4").
    fn toast(&mut self, text: String);
    /// A file the app just finished writing to its exports folder. Canopy offers to send it on to the other apps.
    fn exported(&mut self, path: PathBuf);
}

pub trait CanopyApp {
    /// Draws the app filling `ui`'s rectangle (a pane, or the whole window on its own).
    fn ui(&mut self, ui: &mut egui::Ui, host: &mut dyn Host);
    /// Takes files from the space or from Explorer.
    fn receive(&mut self, files: Vec<SharedFile>, host: &mut dyn Host) -> Receipt;
    /// Opens one of the projects [`AppDescriptor::documents`] listed.
    fn open_document(&mut self, id: &str, host: &mut dyn Host);
    /// Saves open work. Canopy calls it before switching spaces or closing.
    fn save(&mut self) -> Result<(), String>;
    /// What the window or tab should say about the open work (a project name), if anything.
    fn title(&self) -> String {
        String::new()
    }
}

/// Everything Canopy needs to know about an app before it opens.
pub struct AppDescriptor {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub accent: Accent,
    pub initial: &'static str,
    /// The app's visible projects folder inside a space (`StreamCuts` → `<space>\StreamCuts`).
    pub folder: &'static str,
    /// The window icon for the app's own exe (PNG).
    pub icon_png: &'static [u8],
    pub accepts: fn(&SharedFile) -> bool,
    pub documents: fn(&Place) -> Vec<Document>,
    pub create: fn(Place, &egui::Context) -> Box<dyn CanopyApp>,
}

impl AppDescriptor {
    pub fn place_in_space(&self, space: &SpaceInfo) -> Place {
        let paths = SpacePaths::new(&space.dir);
        Place { space: Some(space.clone()), projects_dir: paths.projects(self.folder), data_dir: paths.app_data(self.id), exports_dir: paths.files() }
    }

    /// On its own an app keeps projects in Documents and private data in AppData, like any desktop app.
    pub fn standalone_place(&self) -> Place {
        let documents = dirs::document_dir().unwrap_or_else(std::env::temp_dir);
        let data = dirs::data_dir().unwrap_or_else(std::env::temp_dir);
        Place {
            space: None,
            projects_dir: documents.join(format!("{} Projects", self.name)),
            data_dir: data.join(format!("{}-native", self.id)),
            exports_dir: documents.join(format!("{} Exports", self.name)),
        }
    }
}
