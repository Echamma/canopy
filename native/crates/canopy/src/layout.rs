//! What the shell remembers between runs: for each space, the apps that were showing (`<space>\.canopy\canopy\layout.json`);
//! for this computer, the sidebar (`<profile>\shell.json`).

use canopy_core::files::write_atomic;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::path::Path;

pub const SIDEBAR_WIDTH: f32 = 250.0;

/// The panes of a space, with apps by their descriptor ids so the list survives apps being added or reordered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Layout {
    pub visible: Vec<String>,
    pub focused: Option<String>,
    pub split: bool,
    /// The left pane's share of the width when two apps show.
    pub ratio: f32,
}

impl Default for Layout {
    fn default() -> Self {
        Self { visible: Vec::new(), focused: None, split: false, ratio: 0.5 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Prefs {
    pub sidebar_width: f32,
    pub sidebar_hidden: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self { sidebar_width: SIDEBAR_WIDTH, sidebar_hidden: false }
    }
}

pub fn clamp_ratio(ratio: f32) -> f32 {
    if ratio.is_finite() { ratio.clamp(0.25, 0.75) } else { 0.5 }
}

pub fn clamp_sidebar(width: f32) -> f32 {
    if width.is_finite() { width.clamp(200.0, 480.0) } else { SIDEBAR_WIDTH }
}

/// Reads a remembered value; a missing or unreadable file gives the defaults.
pub fn load<T: DeserializeOwned + Default>(file: &Path) -> T {
    std::fs::read(file).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
}

/// Remembering is best effort: a folder that can't be written to just starts fresh next time.
pub fn save<T: Serialize>(file: &Path, value: &T) {
    if let Ok(json) = serde_json::to_vec_pretty(value) {
        let _ = write_atomic(file, &json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_layout_comes_back_as_saved_and_bad_files_give_defaults() {
        let dir = std::env::temp_dir().join(format!("canopy-layout-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join(".canopy").join("canopy").join("layout.json");
        assert_eq!(load::<Layout>(&file), Layout::default());
        let layout = Layout { visible: vec!["streamcuts".into(), "moodie".into()], focused: Some("moodie".into()), split: true, ratio: 0.4 };
        save(&file, &layout);
        assert_eq!(load::<Layout>(&file), layout);
        std::fs::write(&file, "{\"split\":true}").unwrap();
        assert_eq!(load::<Layout>(&file), Layout { split: true, ..Layout::default() });
        std::fs::write(&file, "not json").unwrap();
        assert_eq!(load::<Prefs>(&file), Prefs::default());
        assert_eq!((clamp_ratio(f32::NAN), clamp_ratio(0.9), clamp_sidebar(20.0)), (0.5, 0.75, 200.0));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
