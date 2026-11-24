//! Settings every app shares, whatever space is open and whether an app runs inside Canopy or alone: the keys for
//! stock services (GIPHY, Pixabay, Freesound) and the person's own sound, picture and video folders. Canopy's
//! Settings (the gear at the top right) edits them; StreamCuts' Codex reads them, looking in the folders before it
//! uses a key. They live in Canopy's profile folder: `%APPDATA%\Canopy\settings.json`, or `CANOPY_PROFILE`.

use crate::Result;
use crate::files::write_atomic;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub giphy_key: String,
    pub pixabay_key: String,
    pub freesound_key: String,
    pub sounds: Option<PathBuf>,
    pub pictures: Option<PathBuf>,
    pub videos: Option<PathBuf>,
    /// Whatever a newer version wrote, kept.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Canopy's profile folder: `CANOPY_PROFILE`, else `%APPDATA%\Canopy`.
pub fn profile_dir() -> PathBuf {
    std::env::var_os("CANOPY_PROFILE")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("Canopy")))
        .unwrap_or_else(|| std::env::temp_dir().join("Canopy"))
}

pub fn file() -> PathBuf {
    profile_dir().join("settings.json")
}

impl Settings {
    /// The saved settings, or empty ones.
    pub fn load() -> Self {
        Self::load_from(&file())
    }

    pub fn load_from(path: &Path) -> Self {
        std::fs::read(path).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(&file())
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        write_atomic(path, &serde_json::to_vec_pretty(self)?)
    }

    /// Fills what's empty here from `other` (keys and folders set somewhere before there was one place for them).
    /// Whether anything changed.
    pub fn adopt(&mut self, other: &Settings) -> bool {
        let before = self.clone();
        for (mine, theirs) in [
            (&mut self.giphy_key, &other.giphy_key),
            (&mut self.pixabay_key, &other.pixabay_key),
            (&mut self.freesound_key, &other.freesound_key),
        ] {
            if mine.trim().is_empty() && !theirs.trim().is_empty() {
                *mine = theirs.trim().to_owned();
            }
        }
        for (mine, theirs) in [(&mut self.sounds, &other.sounds), (&mut self.pictures, &other.pictures), (&mut self.videos, &other.videos)] {
            if mine.is_none() && theirs.is_some() {
                *mine = theirs.clone();
            }
        }
        *self != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_only_fill_what_is_empty() {
        let dir = std::env::temp_dir().join(format!("canopy-settings-{}", std::process::id()));
        let path = dir.join("settings.json");
        assert_eq!(Settings::load_from(&path), Settings::default());
        let mut settings = Settings { pixabay_key: "mine".into(), pictures: Some(PathBuf::from("D:/Pictures")), ..Settings::default() };
        settings.extra.insert("later".into(), 1.into());
        settings.save_to(&path).unwrap();
        assert_eq!(Settings::load_from(&path), settings);
        let old = Settings { giphy_key: " g ".into(), pixabay_key: "theirs".into(), sounds: Some(PathBuf::from("D:/Sounds")), ..Settings::default() };
        assert!(settings.adopt(&old));
        assert_eq!((settings.giphy_key.as_str(), settings.pixabay_key.as_str()), ("g", "mine"));
        assert_eq!(settings.sounds, Some(PathBuf::from("D:/Sounds")));
        assert!(!settings.adopt(&old), "nothing more to take");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
