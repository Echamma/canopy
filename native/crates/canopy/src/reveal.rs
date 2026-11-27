//! Showing things in Explorer. Every "Show in folder" / "Show in Explorer" / "Open" goes through here.
//!
//! Explorer reads its own command line: `explorer /select,"C:\a b\c.mp4"` selects the file, but
//! `explorer "/select,C:\a b\c.mp4"` (what `Command::arg` writes whenever the path has a space) is not a
//! switch Explorer knows, so it opens Documents instead. Paths with forward slashes, a `\\?\` prefix or
//! relative parts send it there too. So paths are cleaned up first and the command line is written raw.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Opens Explorer on the folder that holds `path`, with `path` selected. If `path` is gone, opens the
/// nearest folder that still exists.
pub fn select(path: impl AsRef<Path>) {
    let path = clean(path.as_ref());
    if path.exists() {
        explorer(&format!("/select,{}", quoted(&path)));
    } else if let Some(folder) = nearest_folder(&path) {
        explorer(&quoted(&folder));
    }
}

/// Opens a folder in Explorer, or a file in its default app. If `path` is gone, opens the nearest folder
/// that still exists.
pub fn open(path: impl AsRef<Path>) {
    let path = clean(path.as_ref());
    if path.exists() {
        explorer(&quoted(&path));
    } else if let Some(folder) = nearest_folder(&path) {
        explorer(&quoted(&folder));
    }
}

/// An absolute path with backslashes and without the `\\?\` prefix: the form Explorer understands.
pub fn clean(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    let text = text.strip_prefix(r"\\?\UNC\").map(|rest| format!(r"\\{rest}")).unwrap_or_else(|| {
        text.strip_prefix(r"\\?\").unwrap_or(&text).to_string()
    });
    let path = PathBuf::from(text.replace('/', "\\"));
    let path = std::path::absolute(&path).unwrap_or(path);
    // `absolute` keeps `..` on Windows only when it can't resolve it; drop `.` and fold `..` ourselves.
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    let text = out.to_string_lossy();
    // A trailing backslash would escape the closing quote; keep it only on a drive root ("C:\").
    if text.len() > 3 && text.ends_with('\\') {
        PathBuf::from(text.trim_end_matches('\\'))
    } else {
        out
    }
}

fn nearest_folder(path: &Path) -> Option<PathBuf> {
    path.ancestors().skip(1).find(|p| p.is_dir()).map(Path::to_path_buf)
}

fn quoted(path: &Path) -> String {
    format!("\"{}\"", path.display())
}

#[cfg(windows)]
fn explorer(args: &str) {
    use std::os::windows::process::CommandExt;
    let _ = Command::new("explorer.exe").raw_arg(args).spawn();
}

#[cfg(not(windows))]
fn explorer(args: &str) {
    let path = args.trim_start_matches("/select,").trim_matches('"').to_string();
    let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    let _ = Command::new(opener).arg(path).spawn();
}

#[cfg(all(test, windows))]
mod tests {
    use super::clean;
    use std::path::Path;

    #[test]
    fn explorer_gets_plain_absolute_backslash_paths() {
        assert_eq!(clean(Path::new("C:/Users/a b/Files/x.mp4")), Path::new(r"C:\Users\a b\Files\x.mp4"));
        assert_eq!(clean(Path::new(r"\\?\C:\Users\a b\x.mp4")), Path::new(r"C:\Users\a b\x.mp4"));
        assert_eq!(clean(Path::new(r"C:\Users\a b\.\Files\..\x.mp4")), Path::new(r"C:\Users\a b\x.mp4"));
        assert_eq!(clean(Path::new(r"C:\Users\a b\Files\")), Path::new(r"C:\Users\a b\Files"));
        assert!(clean(Path::new("relative/x.mp4")).is_absolute());
    }
}
