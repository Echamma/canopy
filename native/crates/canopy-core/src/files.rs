//! A space's Files folder: everything the apps share. Imports come from here, exports land here.

use crate::{Error, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Image,
    Video,
    Audio,
    Project,
    Archive,
    File,
}

const TYPES: &[(&str, &str)] = &[
    ("png", "image/png"), ("jpg", "image/jpeg"), ("jpeg", "image/jpeg"), ("webp", "image/webp"), ("gif", "image/gif"), ("svg", "image/svg+xml"),
    ("mp4", "video/mp4"), ("webm", "video/webm"), ("mov", "video/quicktime"), ("mkv", "video/x-matroska"), ("ogv", "video/ogg"),
    ("mp3", "audio/mpeg"), ("wav", "audio/wav"), ("m4a", "audio/mp4"), ("ogg", "audio/ogg"), ("flac", "audio/flac"),
    ("zip", "application/zip"), ("json", "application/json"), ("forma", "application/x-forma-project"), ("srt", "text/plain"), ("txt", "text/plain"),
];

fn extension(name: &str) -> String {
    Path::new(name).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

pub fn type_of(name: &str) -> &'static str {
    let ext = extension(name);
    TYPES.iter().find(|(e, _)| *e == ext).map(|(_, t)| *t).unwrap_or("application/octet-stream")
}

pub fn kind_of(name: &str) -> Kind {
    let lower = name.to_lowercase();
    if lower.ends_with(".folio.json") || lower.ends_with(".forma") || lower.ends_with(".streamcuts.json") {
        return Kind::Project;
    }
    let mime = type_of(name);
    if mime.starts_with("image/") {
        Kind::Image
    } else if mime.starts_with("video/") {
        Kind::Video
    } else if mime.starts_with("audio/") {
        Kind::Audio
    } else if mime == "application/zip" {
        Kind::Archive
    } else {
        Kind::File
    }
}

/// Turns any name into one Windows and Linux both accept, keeping its extension.
pub fn safe_file_name(name: &str) -> String {
    let cleaned: String = name.chars().map(|c| if "<>:\"/\\|?*".contains(c) || (c as u32) < 0x20 { '-' } else { c }).collect();
    let cleaned = cleaned.trim_end_matches(['.', ' ']).trim();
    let cleaned: String = cleaned.chars().take(150).collect();
    if cleaned.is_empty() || cleaned.chars().all(|c| c == '.') { "file".into() } else { cleaned }
}

/// Resolves a name from the shell or an app to a path inside the folder, refusing anything that would leave it.
pub fn resolve_inside(dir: &Path, name: &str) -> Result<PathBuf> {
    let file_name = Path::new(name).file_name().map(|n| n.to_string_lossy().into_owned());
    if name.is_empty() || name == "." || name == ".." || file_name.as_deref() != Some(name) {
        return Err(Error("Invalid file name.".into()));
    }
    Ok(dir.join(name))
}

fn candidates(name: &str) -> impl Iterator<Item = String> {
    let safe = safe_file_name(name);
    let ext = if safe.to_lowercase().ends_with(".folio.json") {
        ".folio.json".to_string()
    } else if safe.to_lowercase().ends_with(".streamcuts.json") {
        ".streamcuts.json".to_string()
    } else {
        Path::new(&safe).extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default()
    };
    let stem = safe[..safe.len() - ext.len()].to_string();
    let stem = if stem.is_empty() { "file".to_string() } else { stem };
    std::iter::once(safe).chain((2..).map(move |n| format!("{stem} ({n}){ext}")))
}

/// The name a new file gets in the folder: the one asked for, or "name (2).ext" and so on.
pub fn unique_name(dir: &Path, name: &str) -> String {
    candidates(name).find(|candidate| !dir.join(candidate).exists()).expect("names never run out")
}

#[derive(Debug, Clone, PartialEq)]
pub struct FileEntry {
    pub name: String,
    pub size: u64,
    pub modified_ms: u64,
    pub kind: Kind,
}

fn hidden(name: &str) -> bool {
    let lower = name.to_lowercase();
    name.starts_with('.') || [".tmp", ".crdownload", ".part", ".download"].iter().any(|e| lower.ends_with(e))
}

/// The folder's files, newest first, leaving out hidden files and ones still being written.
pub fn list_files(dir: &Path) -> Vec<FileEntry> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let mut files: Vec<FileEntry> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let meta = entry.metadata().ok()?;
            if !meta.is_file() || hidden(&name) {
                return None;
            }
            let modified_ms = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_millis() as u64;
            Some(FileEntry { kind: kind_of(&name), size: meta.len(), modified_ms, name })
        })
        .collect();
    files.sort_by(|a, b| b.modified_ms.cmp(&a.modified_ms).then_with(|| a.name.cmp(&b.name)));
    files
}

fn temporary(dir: &Path) -> PathBuf {
    dir.join(format!(".{}-{}.tmp", std::process::id(), crate::now_ms() ^ (rand_bits() as u64)))
}
fn rand_bits() -> u32 {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u64(crate::now_ms());
    hasher.finish() as u32
}

/// Copies a file from anywhere into the folder; returns the name it was given.
pub fn copy_into(dir: &Path, source: &Path) -> Result<String> {
    if !source.is_file() {
        return Err(Error("Only files can be added.".into()));
    }
    fs::create_dir_all(dir)?;
    let name = unique_name(dir, &source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
    let temp = temporary(dir);
    let result = fs::copy(source, &temp).and_then(|_| fs::rename(&temp, dir.join(&name)));
    let _ = fs::remove_file(&temp);
    result?;
    Ok(name)
}

/// Writes bytes an app produced (an export) into the folder; returns the name it was given.
pub fn write_into(dir: &Path, name: &str, data: &[u8]) -> Result<String> {
    fs::create_dir_all(dir)?;
    let unique = unique_name(dir, name);
    let temp = temporary(dir);
    let result = fs::write(&temp, data).and_then(|_| fs::rename(&temp, dir.join(&unique)));
    let _ = fs::remove_file(&temp);
    result?;
    Ok(unique)
}

/// Writes JSON (or any text) so a reader never sees half a file.
pub fn write_atomic(file: &Path, contents: &[u8]) -> Result<()> {
    let dir = file.parent().ok_or_else(|| Error("Invalid path.".into()))?;
    fs::create_dir_all(dir)?;
    let temp = temporary(dir);
    let result = fs::write(&temp, contents).and_then(|_| fs::rename(&temp, file));
    let _ = fs::remove_file(&temp);
    Ok(result?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("canopy-core-files-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn names_are_safe_and_unique_keeping_double_extensions() {
        let dir = folder("names");
        assert_eq!(safe_file_name("a/b:c?.png"), "a-b-c-.png");
        assert_eq!(safe_file_name("..."), "file");
        fs::write(dir.join("cover.png"), "").unwrap();
        fs::write(dir.join("story.folio.json"), "").unwrap();
        assert_eq!(unique_name(&dir, "cover.png"), "cover (2).png");
        assert_eq!(unique_name(&dir, "story.folio.json"), "story (2).folio.json");
        assert_eq!(unique_name(&dir, "new.mp4"), "new.mp4");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn resolve_inside_refuses_names_that_leave_the_folder() {
        let dir = Path::new("C:\\space\\Files");
        assert_eq!(resolve_inside(dir, "clip.mp4").unwrap(), dir.join("clip.mp4"));
        for name in ["..", "../secret.txt", "sub/clip.mp4", "sub\\clip.mp4", "", "."] {
            assert!(resolve_inside(dir, name).is_err(), "{name} should be refused");
        }
    }

    #[test]
    fn list_files_hides_partial_and_hidden_files() {
        let dir = folder("list");
        fs::write(dir.join("a.png"), "x").unwrap();
        fs::write(dir.join("b.mp4"), "xx").unwrap();
        for hidden in [".secret", "export.mp4.crdownload", "x.tmp"] {
            fs::write(dir.join(hidden), "").unwrap();
        }
        let mut names: Vec<_> = list_files(&dir).into_iter().map(|f| f.name).collect();
        names.sort();
        assert_eq!(names, ["a.png", "b.mp4"]);
        assert!(list_files(&dir.join("missing")).is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn copy_and_write_never_overwrite() {
        let dir = folder("copy");
        let source_dir = folder("copy-source");
        let source = source_dir.join("clip.mp4");
        fs::write(&source, "video").unwrap();
        assert_eq!(copy_into(&dir, &source).unwrap(), "clip.mp4");
        assert_eq!(copy_into(&dir, &source).unwrap(), "clip (2).mp4");
        assert_eq!(write_into(&dir, "clip.mp4", b"other").unwrap(), "clip (3).mp4");
        assert_eq!(fs::read_to_string(dir.join("clip.mp4")).unwrap(), "video");
        fs::remove_dir_all(dir).unwrap();
        fs::remove_dir_all(source_dir).unwrap();
    }

    #[test]
    fn kinds_group_what_the_sidebar_shows() {
        assert_eq!(kind_of("a.folio.json"), Kind::Project);
        assert_eq!(kind_of("a.streamcuts.json"), Kind::Project);
        assert_eq!(kind_of("a.WAV"), Kind::Audio);
        assert_eq!(kind_of("a.mkv"), Kind::Video);
        assert_eq!(kind_of("a.zip"), Kind::Archive);
        assert_eq!(kind_of("notes.txt"), Kind::File);
    }
}
