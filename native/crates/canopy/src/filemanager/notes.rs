//! Titles and descriptions for files, so a file carries what to post with it. Files Video Chat rendered keep theirs
//! in Video Chat's own project (`state.json`, its `rendered` list): they're read from there and edits go back there,
//! so Video Chat and the File Manager show the same words. Every other file's notes are in `<profile>\file-notes.json`,
//! by path.

use crate::videochat::{engine, model::Renders};
use canopy_ui::canopy_core::files::write_atomic;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Note {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
}

impl Note {
    pub fn is_empty(&self) -> bool {
        self.title.trim().is_empty() && self.description.trim().is_empty()
    }

    /// What goes on the clipboard for a post: the description, or the title when there's no description; both when
    /// `with_title`.
    pub fn caption(&self, with_title: bool) -> String {
        let (title, description) = (self.title.trim(), self.description.trim());
        match (with_title, title.is_empty(), description.is_empty()) {
            (_, _, true) => title.to_owned(),
            (true, false, false) => format!("{title}\n\n{description}"),
            _ => description.to_owned(),
        }
    }
}

/// A file Video Chat rendered: where its words live.
#[derive(Debug, Clone, PartialEq)]
pub struct Rendered {
    pub path: PathBuf,
    pub state: PathBuf,
    pub download_url: String,
    pub project: String,
    pub short: bool,
    pub note: Note,
}

#[derive(Default, Serialize, Deserialize)]
struct Saved {
    #[serde(default)]
    files: HashMap<String, Note>,
}

pub struct Notes {
    file: PathBuf,
    saved: Saved,
}

/// Paths are compared as Windows does: case doesn't matter, and either slash will do.
pub fn key(path: &Path) -> String {
    path.to_string_lossy().replace('/', "\\").to_lowercase()
}

impl Notes {
    pub fn load(file: PathBuf) -> Self {
        let saved = std::fs::read(&file).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Self { file, saved }
    }

    /// The usual store: `<profile>\file-notes.json` (`CANOPY_PROFILE`, or `%APPDATA%\Canopy`).
    pub fn open() -> Self {
        let profile = std::env::var_os("CANOPY_PROFILE")
            .map(PathBuf::from)
            .or_else(|| dirs::data_dir().map(|d| d.join("Canopy")))
            .unwrap_or_else(std::env::temp_dir);
        Self::load(profile.join("file-notes.json"))
    }

    pub fn get(&self, path: &Path) -> Option<&Note> {
        self.saved.files.get(&key(path))
    }

    pub fn set(&mut self, path: &Path, note: Note) -> Result<(), String> {
        if note.is_empty() {
            self.saved.files.remove(&key(path));
        } else {
            self.saved.files.insert(key(path), note);
        }
        self.save()
    }

    /// A file renamed or moved keeps its notes.
    pub fn moved(&mut self, from: &Path, to: &Path) -> Result<(), String> {
        match self.saved.files.remove(&key(from)) {
            Some(note) => {
                self.saved.files.insert(key(to), note);
                self.save()
            }
            None => Ok(()),
        }
    }

    fn save(&self) -> Result<(), String> {
        if let Some(parent) = self.file.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_vec_pretty(&self.saved).map_err(|e| e.to_string())?;
        write_atomic(&self.file, &json).map_err(|e| e.to_string())
    }
}

/// Every file Video Chat has rendered that's still on disk, newest project first, with its title and description.
pub fn video_chat_renders(folder: &Path) -> Vec<Rendered> {
    let jobs = engine::jobs_dir(folder);
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(&jobs) else { return found };
    let mut dirs: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter(|e| e.path().join("state.json").is_file())
        .map(|e| (e.path().join("state.json").metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH), e.path()))
        .collect();
    dirs.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, dir) in dirs {
        let state_path = dir.join("state.json");
        let Some(state) = std::fs::read(&state_path).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()) else { continue };
        let Some(renders) = state.get("rendered").filter(|r| !r.is_null()).and_then(|r| serde_json::from_value::<Renders>(r.clone()).ok()) else { continue };
        let project = std::fs::read(dir.join("job.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .and_then(|v| v.get("sourceFileName").and_then(Value::as_str).map(|n| crate::videochat::model::without_extension(n).to_owned()))
            .unwrap_or_default();
        for (video, short) in renders.shorts.iter().map(|v| (v, true)).chain(renders.longer_videos.iter().map(|v| (v, false))) {
            let Some(path) = engine::rendered_path(folder, &video.download_url) else { continue };
            found.push(Rendered {
                path,
                state: state_path.clone(),
                download_url: video.download_url.clone(),
                project: project.clone(),
                short,
                note: Note { title: video.title.clone(), description: video.description.clone().unwrap_or_default() },
            });
        }
    }
    found
}

/// Writes a rendered file's title and description back into its Video Chat project.
pub fn save_rendered(rendered: &Rendered, note: &Note) -> Result<(), String> {
    let bytes = std::fs::read(&rendered.state).map_err(|e| e.to_string())?;
    let mut state: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let mut changed = false;
    for list in ["shorts", "longerVideos"] {
        if let Some(items) = state.pointer_mut(&format!("/rendered/{list}")).and_then(Value::as_array_mut) {
            for item in items.iter_mut().filter(|i| i.get("downloadUrl").and_then(Value::as_str) == Some(rendered.download_url.as_str())) {
                item["title"] = note.title.clone().into();
                item["description"] = note.description.clone().into();
                changed = true;
            }
        }
    }
    if !changed {
        return Err("Video Chat's project no longer has that clip.".into());
    }
    let json = serde_json::to_vec_pretty(&state).map_err(|e| e.to_string())?;
    write_atomic(&rendered.state, &json).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_keep_by_path_and_follow_a_rename() {
        let dir = std::env::temp_dir().join(format!("file-notes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut notes = Notes::load(dir.join("file-notes.json"));
        let note = Note { title: "Three nights".into(), description: "Wildest quote? #travel".into() };
        notes.set(Path::new("D:/Clips/a.mp4"), note.clone()).unwrap();
        let again = Notes::load(dir.join("file-notes.json"));
        assert_eq!(again.get(Path::new("d:\\clips\\A.MP4")), Some(&note));
        notes.moved(Path::new("D:/Clips/a.mp4"), Path::new("D:/Clips/b.mp4")).unwrap();
        assert!(notes.get(Path::new("D:/Clips/a.mp4")).is_none());
        assert_eq!(note.caption(false), "Wildest quote? #travel");
        assert_eq!(note.caption(true), "Three nights\n\nWildest quote? #travel");
        assert_eq!(Note { title: "Only".into(), description: String::new() }.caption(false), "Only");
        notes.set(Path::new("D:/Clips/b.mp4"), Note::default()).unwrap();
        assert!(Notes::load(dir.join("file-notes.json")).get(Path::new("D:/Clips/b.mp4")).is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn video_chat_renders_carry_their_words_and_take_edits() {
        let folder = std::env::temp_dir().join(format!("file-notes-vc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let job = engine::jobs_dir(&folder).join("abc");
        std::fs::create_dir_all(&job).unwrap();
        std::fs::write(job.join("short-1.mp4"), b"x").unwrap();
        std::fs::write(job.join("job.json"), br#"{"sourceFileName":"VOD 11.mp4"}"#).unwrap();
        let url = "/api/long-to-short/jobs/abc/clips/short-1.mp4";
        let state = serde_json::json!({"messages": [], "rendered": {"shorts": [{"downloadUrl": url, "title": "Countdown", "description": "Wait #x"}], "longerVideos": [
            {"downloadUrl": "/api/long-to-short/jobs/abc/clips/gone.mp4", "title": "Gone"}]}, "other": 1});
        std::fs::write(job.join("state.json"), state.to_string()).unwrap();
        let found = video_chat_renders(&folder);
        assert_eq!(found.len(), 1, "a missing file isn't listed");
        assert_eq!((found[0].project.as_str(), found[0].note.title.as_str(), found[0].short), ("VOD 11", "Countdown", true));
        save_rendered(&found[0], &Note { title: "New".into(), description: "Better #y".into() }).unwrap();
        let back: Value = serde_json::from_slice(&std::fs::read(job.join("state.json")).unwrap()).unwrap();
        assert_eq!(back["rendered"]["shorts"][0]["title"], "New");
        assert_eq!(back["rendered"]["shorts"][0]["description"], "Better #y");
        assert_eq!(back["other"], 1);
        std::fs::remove_dir_all(folder).unwrap();
    }
}
