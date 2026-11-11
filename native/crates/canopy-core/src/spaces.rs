//! Spaces are Canopy's projects. Each is a folder you can see on disk:
//!   canopy-space.json   the space's name and color
//!   Files/              shared files: what every app can import, and where exports land
//!   Folio/, StreamCuts/ each app's visible projects for this space
//!   .canopy/<app>/      each app's private data for this space
//! A registry in Canopy's data folder (`%APPDATA%\Canopy\spaces.json`) remembers which spaces exist.

use crate::files::{safe_file_name, write_atomic};
use crate::{Error, Result, now_ms};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const COLORS: [&str; 5] = ["leaf", "sky", "clay", "berry", "moss"];
pub const MANIFEST: &str = "canopy-space.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Space {
    pub id: String,
    pub name: String,
    #[serde(default = "default_color")]
    pub color: String,
    #[serde(default)]
    pub created_at: u64,
    pub dir: PathBuf,
    #[serde(default)]
    pub last_opened_at: u64,
}

fn default_color() -> String {
    COLORS[0].into()
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    #[serde(default = "one")]
    version: u32,
    id: String,
    name: String,
    #[serde(default = "default_color")]
    color: String,
    #[serde(default)]
    created_at: u64,
}

fn one() -> u32 {
    1
}

#[derive(Serialize, Deserialize, Default)]
struct Registry {
    #[serde(default)]
    spaces: Vec<Space>,
}

/// Where a space keeps its things.
#[derive(Debug, Clone, PartialEq)]
pub struct SpacePaths {
    pub dir: PathBuf,
}

impl SpacePaths {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }
    pub fn manifest(&self) -> PathBuf {
        self.dir.join(MANIFEST)
    }
    pub fn files(&self) -> PathBuf {
        self.dir.join("Files")
    }
    /// An app's visible projects folder, like `Folio\` or `StreamCuts\`.
    pub fn projects(&self, app_name: &str) -> PathBuf {
        self.dir.join(app_name)
    }
    /// An app's private data for this space.
    pub fn app_data(&self, app_id: &str) -> PathBuf {
        self.dir.join(".canopy").join(app_id)
    }
}

fn clean_name(name: &str) -> Result<String> {
    let value: String = name.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(60).collect();
    if value.is_empty() { Err(Error("Give your space a name.".into())) } else { Ok(value) }
}

pub struct Spaces {
    registry_file: PathBuf,
    default_root: PathBuf,
}

impl Spaces {
    pub fn new(registry_file: impl Into<PathBuf>, default_root: impl Into<PathBuf>) -> Self {
        Self { registry_file: registry_file.into(), default_root: default_root.into() }
    }

    pub fn default_root(&self) -> &Path {
        &self.default_root
    }

    fn load(&self) -> Vec<Space> {
        fs::read(&self.registry_file)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Registry>(&bytes).ok())
            .map(|registry| registry.spaces)
            .unwrap_or_default()
    }

    fn save(&self, spaces: &[Space]) -> Result<()> {
        let json = serde_json::to_vec_pretty(&serde_json::json!({ "version": 1, "spaces": spaces }))?;
        write_atomic(&self.registry_file, &json)
    }

    fn prepare(dir: &Path) -> Result<()> {
        let paths = SpacePaths::new(dir);
        fs::create_dir_all(paths.files())?;
        fs::create_dir_all(paths.projects("Folio"))?;
        fs::create_dir_all(dir.join(".canopy"))?;
        Ok(())
    }

    /// Spaces, most recently opened first, each with whether its folder is still there.
    pub fn list(&self) -> Vec<(Space, bool)> {
        let mut spaces: Vec<_> = self.load().into_iter().map(|space| {
            let available = SpacePaths::new(&space.dir).manifest().is_file();
            (space, available)
        }).collect();
        spaces.sort_by(|a, b| b.0.last_opened_at.cmp(&a.0.last_opened_at));
        spaces
    }

    pub fn get(&self, id: &str) -> Option<Space> {
        self.load().into_iter().find(|space| space.id == id)
    }

    /// Makes a new space: its folder under the default root (or `dir`), its manifest, Files and Folio.
    pub fn create(&self, name: &str, color: &str, dir: Option<PathBuf>) -> Result<Space> {
        let name = clean_name(name)?;
        let color = if COLORS.contains(&color) { color.to_string() } else { default_color() };
        let folder = match dir {
            Some(dir) => dir,
            None => {
                let stem = safe_file_name(&name);
                let mut folder = self.default_root.join(&stem);
                let mut number = 2;
                while folder.exists() {
                    folder = self.default_root.join(format!("{stem} ({number})"));
                    number += 1;
                }
                folder
            }
        };
        if SpacePaths::new(&folder).manifest().exists() {
            return Err(Error("That folder already holds a space. Open it instead.".into()));
        }
        let now = now_ms();
        let space = Space { id: new_id(), name, color, created_at: now, dir: folder.clone(), last_opened_at: now };
        Self::prepare(&folder)?;
        let manifest = Manifest { version: 1, id: space.id.clone(), name: space.name.clone(), color: space.color.clone(), created_at: now };
        write_atomic(&SpacePaths::new(&folder).manifest(), &serde_json::to_vec_pretty(&manifest)?)?;
        let mut spaces = self.load();
        spaces.push(space.clone());
        self.save(&spaces)?;
        Ok(space)
    }

    /// Adds an existing space folder (one made on another computer, or taken off the list earlier).
    pub fn add(&self, dir: &Path) -> Result<Space> {
        let manifest: Manifest = fs::read(SpacePaths::new(dir).manifest())
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .ok_or_else(|| Error("That folder isn't a Canopy space.".into()))?;
        let mut spaces = self.load();
        if let Some(known) = spaces.iter_mut().find(|space| space.id == manifest.id) {
            known.dir = dir.to_path_buf();
            known.name = manifest.name;
            let known = known.clone();
            self.save(&spaces)?;
            return Ok(known);
        }
        let color = if COLORS.contains(&manifest.color.as_str()) { manifest.color } else { default_color() };
        let space = Space { id: manifest.id, name: manifest.name, color, created_at: manifest.created_at, dir: dir.to_path_buf(), last_opened_at: now_ms() };
        spaces.push(space.clone());
        self.save(&spaces)?;
        Self::prepare(dir)?;
        Ok(space)
    }

    /// Marks a space as just opened and makes sure its folders exist.
    pub fn touch(&self, id: &str) -> Result<Space> {
        let mut spaces = self.load();
        let space = spaces.iter_mut().find(|space| space.id == id).ok_or_else(|| Error("That space is no longer listed.".into()))?;
        if !SpacePaths::new(&space.dir).manifest().is_file() {
            return Err(Error(format!("The folder for {} wasn't found: {}", space.name, space.dir.display())));
        }
        space.last_opened_at = now_ms();
        let space = space.clone();
        self.save(&spaces)?;
        Self::prepare(&space.dir)?;
        Ok(space)
    }

    pub fn rename(&self, id: &str, name: &str) -> Result<Space> {
        let name = clean_name(name)?;
        let mut spaces = self.load();
        let space = spaces.iter_mut().find(|space| space.id == id).ok_or_else(|| Error("That space is no longer listed.".into()))?;
        space.name = name.clone();
        let space = space.clone();
        self.save(&spaces)?;
        let manifest_file = SpacePaths::new(&space.dir).manifest();
        if let Some(mut manifest) = fs::read(&manifest_file).ok().and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok()) {
            manifest["name"] = serde_json::Value::String(name);
            write_atomic(&manifest_file, &serde_json::to_vec_pretty(&manifest)?)?;
        }
        Ok(space)
    }

    /// Changes a space's leaf color, in the list and in its folder's manifest.
    pub fn recolor(&self, id: &str, color: &str) -> Result<Space> {
        if !COLORS.contains(&color) {
            return Err(Error("That isn't one of Canopy's colors.".into()));
        }
        let mut spaces = self.load();
        let space = spaces.iter_mut().find(|space| space.id == id).ok_or_else(|| Error("That space is no longer listed.".into()))?;
        space.color = color.to_string();
        let space = space.clone();
        self.save(&spaces)?;
        let manifest_file = SpacePaths::new(&space.dir).manifest();
        if let Some(mut manifest) = fs::read(&manifest_file).ok().and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok()) {
            manifest["color"] = serde_json::Value::String(color.to_string());
            write_atomic(&manifest_file, &serde_json::to_vec_pretty(&manifest)?)?;
        }
        Ok(space)
    }

    /// Takes a space off the list; its folder and files stay where they are.
    pub fn forget(&self, id: &str) -> Result<()> {
        let spaces: Vec<_> = self.load().into_iter().filter(|space| space.id != id).collect();
        self.save(&spaces)
    }
}

/// A random version-4 UUID, like crypto.randomUUID().
pub fn new_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut bytes = [0u8; 16];
    for (index, chunk) in bytes.chunks_mut(8).enumerate() {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u64(now_ms() ^ index as u64);
        hasher.write_u32(std::process::id());
        chunk.copy_from_slice(&hasher.finish().to_le_bytes());
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &hex[0..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..32])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(tag: &str) -> (PathBuf, Spaces) {
        let root = std::env::temp_dir().join(format!("canopy-core-spaces-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let spaces = Spaces::new(root.join("profile").join("spaces.json"), root.join("Canopy"));
        (root, spaces)
    }

    #[test]
    fn a_new_space_is_a_folder_with_manifest_files_and_folio() {
        let (root, spaces) = setup("new");
        let space = spaces.create("  Launch   week ", "clay", None).unwrap();
        assert_eq!(space.name, "Launch week");
        assert_eq!(space.dir, root.join("Canopy").join("Launch week"));
        let paths = SpacePaths::new(&space.dir);
        assert!(paths.files().is_dir() && paths.projects("Folio").is_dir());
        let manifest: serde_json::Value = serde_json::from_slice(&fs::read(paths.manifest()).unwrap()).unwrap();
        assert_eq!(manifest["name"], "Launch week");
        assert_eq!(manifest["id"], space.id.as_str());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn names_are_required_colors_fall_back_and_folders_stay_apart() {
        let (root, spaces) = setup("names");
        assert!(spaces.create("   ", "leaf", None).is_err());
        let first = spaces.create("Client", "purple", None).unwrap();
        let second = spaces.create("Client", "sky", None).unwrap();
        assert_eq!(first.color, "leaf");
        assert!(second.dir.ends_with("Client (2)"));
        assert_ne!(first.id, second.id);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn list_puts_recent_first_and_flags_missing_folders() {
        let (root, spaces) = setup("list");
        let a = spaces.create("A", "leaf", None).unwrap();
        let b = spaces.create("B", "leaf", None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        spaces.touch(&a.id).unwrap();
        let names: Vec<_> = spaces.list().into_iter().map(|(space, _)| space.name).collect();
        assert_eq!(names, ["A", "B"]);
        fs::remove_dir_all(&b.dir).unwrap();
        assert!(!spaces.list().into_iter().find(|(space, _)| space.id == b.id).unwrap().1);
        assert!(spaces.touch(&b.id).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_forgotten_space_comes_back_with_its_identity() {
        let (root, spaces) = setup("forget");
        let space = spaces.create("Portable", "moss", None).unwrap();
        spaces.forget(&space.id).unwrap();
        assert!(spaces.list().is_empty());
        assert_eq!(spaces.add(&space.dir).unwrap().id, space.id);
        assert!(spaces.add(space.dir.parent().unwrap()).is_err());
        assert!(spaces.create("Again", "leaf", Some(space.dir.clone())).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rename_updates_list_and_manifest() {
        let (root, spaces) = setup("rename");
        let space = spaces.create("Draft", "leaf", None).unwrap();
        spaces.rename(&space.id, "Final").unwrap();
        assert_eq!(spaces.get(&space.id).unwrap().name, "Final");
        let manifest: serde_json::Value = serde_json::from_slice(&fs::read(SpacePaths::new(&space.dir).manifest()).unwrap()).unwrap();
        assert_eq!(manifest["name"], "Final");
        assert!(spaces.recolor(&space.id, "purple").is_err());
        assert_eq!(spaces.recolor(&space.id, "berry").unwrap().color, "berry");
        let manifest: serde_json::Value = serde_json::from_slice(&fs::read(SpacePaths::new(&space.dir).manifest()).unwrap()).unwrap();
        assert_eq!((manifest["color"].as_str(), manifest["name"].as_str()), (Some("berry"), Some("Final")));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reads_the_registry_electron_canopy_writes() {
        let (root, spaces) = setup("electron");
        let dir = root.join("Canopy").join("Old");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(MANIFEST), r#"{"version":1,"id":"abc","name":"Old","color":"sky","createdAt":5}"#).unwrap();
        fs::create_dir_all(root.join("profile")).unwrap();
        let registry = serde_json::json!({"version": 1, "spaces": [{"id": "abc", "name": "Old", "color": "sky", "createdAt": 5, "dir": dir, "lastOpenedAt": 9}]});
        fs::write(root.join("profile").join("spaces.json"), registry.to_string()).unwrap();
        let listed = spaces.list();
        assert_eq!(listed.len(), 1);
        assert!(listed[0].1);
        assert_eq!(listed[0].0.color, "sky");
        fs::remove_dir_all(root).unwrap();
    }
}
