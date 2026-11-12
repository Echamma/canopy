//! The File Manager: browse the space's files, Video Chat's clips and downloads and any folder, give each file a title
//! and a description, and drag files out of Canopy into a browser's upload page (TikTok) or any other app, as from
//! Explorer. A dragged file's description goes on the clipboard, ready to paste as its caption.

mod dragout;
pub mod notes;

use crate::videochat::engine;
use canopy_ui::canopy_core::files::{Kind, copy_into, kind_of, recycle, safe_file_name, unique_name, write_atomic};
use canopy_ui::{Accent, AppDescriptor, CanopyApp, Document, Host, Icon, Mood, Place, Receipt, SharedFile, color, font, icon_button, muted, primary_button, quiet_button, section_label};
use egui::{Align, Align2, Color32, Layout, Rect, RichText, Sense, Stroke, StrokeKind, TextureHandle, TextureOptions, Ui, UiBuilder, pos2, vec2};
use notes::{Note, Notes, Rendered};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant, SystemTime};
use streamcuts::ffmpeg::Frame;

pub const DESCRIPTOR: AppDescriptor = AppDescriptor {
    id: "filemanager",
    name: "File Manager",
    description: "Your files with titles and descriptions; drag them out to post",
    accent: Accent::Sky,
    initial: "FM",
    folder: "Files",
    icon_png: include_bytes!("../../../../../assets/canopy.png"),
    accepts,
    documents,
    create,
};

fn accepts(_: &SharedFile) -> bool {
    true
}

fn documents(_: &Place) -> Vec<Document> {
    Vec::new()
}

fn create(place: Place, ctx: &egui::Context) -> Box<dyn CanopyApp> {
    Box::new(FileManager::new(place, ctx))
}

/// What goes on the clipboard when a file is dragged out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OnDrag {
    #[default]
    Description,
    TitleAndDescription,
    Nothing,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Prefs {
    #[serde(default)]
    pinned: Vec<PathBuf>,
    #[serde(default)]
    on_drag: OnDrag,
    #[serde(default)]
    by_name: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum Location {
    Folder(PathBuf),
    VideoChatClips,
}

#[derive(Debug, Clone)]
struct Entry {
    path: PathBuf,
    name: String,
    folder: bool,
    size: u64,
    modified: SystemTime,
    kind: Kind,
    /// Index into the Video Chat renders, for a file Video Chat made.
    rendered: Option<usize>,
}

struct Thumb {
    texture: TextureHandle,
    frame: Frame,
}

pub struct FileManager {
    place: Place,
    ctx: egui::Context,
    notes: Notes,
    prefs: Prefs,
    prefs_file: PathBuf,
    location: Location,
    history: Vec<Location>,
    entries: Vec<Entry>,
    listed_at: Instant,
    renders: Vec<Rendered>,
    renders_at: Instant,
    selected: Vec<PathBuf>,
    anchor: Option<PathBuf>,
    search: String,
    /// The selected file's words as typed, saved a moment after the last change.
    draft: Option<(PathBuf, Note, Option<Instant>)>,
    thumbs: HashMap<PathBuf, Option<Thumb>>,
    asked: HashSet<PathBuf>,
    thumb_jobs: Sender<PathBuf>,
    thumb_done: Receiver<(PathBuf, Option<Frame>)>,
    rename: Option<(PathBuf, String)>,
    /// The file whose rename dialog has had its name selected, so that happens once as it opens.
    rename_selected: Option<PathBuf>,
    toasts: Vec<String>,
    /// This pane has the keyboard and no dialog is up.
    focused: bool,
    /// The row the arrow keys are on, and a row to scroll to once.
    cursor: Option<PathBuf>,
    reveal: Option<PathBuf>,
    /// Only this kind of file is listed (folders always are).
    filter: Option<Kind>,
    focus_search: bool,
}

fn profile() -> PathBuf {
    std::env::var_os("CANOPY_PROFILE")
        .map(PathBuf::from)
        .or_else(|| dirs::data_dir().map(|d| d.join("Canopy")))
        .unwrap_or_else(std::env::temp_dir)
}

/// Thumbnails, one at a time off the UI thread: a video's frame a second in, or a picture scaled down.
fn thumbnailer(ctx: egui::Context) -> (Sender<PathBuf>, Receiver<(PathBuf, Option<Frame>)>) {
    let (jobs, queue) = channel::<PathBuf>();
    let (done, results) = channel();
    std::thread::spawn(move || {
        while let Ok(path) = queue.recv() {
            let frame = match kind_of(&path.to_string_lossy()) {
                Kind::Video => streamcuts::ffmpeg::thumbnail(&path, 1.0).or_else(|_| streamcuts::ffmpeg::thumbnail(&path, 0.0)).ok(),
                Kind::Image => image::open(&path).ok().map(|image| {
                    let small = image.thumbnail(192, 108).to_rgba8();
                    Frame { time: 0.0, size: [small.width() as usize, small.height() as usize], rgba: small.into_raw() }
                }),
                _ => None,
            };
            if done.send((path, frame)).is_err() {
                break;
            }
            ctx.request_repaint();
        }
    });
    (jobs, results)
}

impl FileManager {
    pub fn new(place: Place, ctx: &egui::Context) -> Self {
        let prefs_file = profile().join("file-manager.json");
        let prefs: Prefs = std::fs::read(&prefs_file).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        let (thumb_jobs, thumb_done) = thumbnailer(ctx.clone());
        let home = match &place.space {
            Some(_) => place.exports_dir.clone(),
            None => dirs::video_dir().or_else(dirs::document_dir).unwrap_or_else(std::env::temp_dir),
        };
        let long_ago = Instant::now() - Duration::from_secs(600);
        Self {
            place,
            ctx: ctx.clone(),
            notes: Notes::open(),
            prefs,
            prefs_file,
            location: Location::Folder(home),
            history: Vec::new(),
            entries: Vec::new(),
            listed_at: long_ago,
            renders: Vec::new(),
            renders_at: long_ago,
            selected: Vec::new(),
            anchor: None,
            search: String::new(),
            draft: None,
            thumbs: HashMap::new(),
            asked: HashSet::new(),
            thumb_jobs,
            thumb_done,
            rename: None,
            rename_selected: None,
            focused: false,
            cursor: None,
            reveal: None,
            filter: None,
            focus_search: false,
            toasts: Vec::new(),
        }
    }

    fn save_prefs(&self) {
        if let Ok(json) = serde_json::to_vec_pretty(&self.prefs) {
            let _ = std::fs::create_dir_all(self.prefs_file.parent().unwrap_or(Path::new(".")));
            let _ = write_atomic(&self.prefs_file, &json);
        }
    }

    /// Video Chat's downloads folder: the one set in Video Chat, else its own.
    fn video_chat_downloads(folder: &Path) -> PathBuf {
        let own = std::env::var_os("VIDEO_CHAT_DATA").map(PathBuf::from).unwrap_or_else(|| folder.join("backend").join("data")).join("downloads");
        std::fs::read(own.join("settings.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
            .and_then(|v| v.get("folder").and_then(|f| f.as_str()).filter(|f| !f.is_empty()).map(PathBuf::from))
            .filter(|f| f.is_dir())
            .unwrap_or(own)
    }

    fn go(&mut self, location: Location) {
        if location != self.location {
            self.history.push(std::mem::replace(&mut self.location, location));
            self.selected.clear();
            self.anchor = None;
            self.commit_draft();
            self.draft = None;
            self.listed_at = Instant::now() - Duration::from_secs(600);
        }
    }

    fn refresh(&mut self) {
        if self.renders_at.elapsed() > Duration::from_secs(4) {
            self.renders_at = Instant::now();
            self.renders = engine::folder().map(|f| notes::video_chat_renders(&f)).unwrap_or_default();
        }
        if self.listed_at.elapsed() < Duration::from_secs(2) {
            return;
        }
        self.listed_at = Instant::now();
        let by_path: HashMap<String, usize> = self.renders.iter().enumerate().map(|(i, r)| (notes::key(&r.path), i)).collect();
        let mut entries: Vec<Entry> = match &self.location {
            Location::VideoChatClips => self.renders.iter().enumerate().filter_map(|(i, r)| {
                let meta = std::fs::metadata(&r.path).ok()?;
                Some(Entry {
                    name: r.path.file_name()?.to_string_lossy().into_owned(),
                    path: r.path.clone(),
                    folder: false,
                    size: meta.len(),
                    modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                    kind: Kind::Video,
                    rendered: Some(i),
                })
            }).collect(),
            Location::Folder(dir) => std::fs::read_dir(dir)
                .map(|items| {
                    items.flatten().filter_map(|item| {
                        let name = item.file_name().to_string_lossy().into_owned();
                        let meta = item.metadata().ok()?;
                        // Hidden and system things stay out of the way, as in Explorer.
                        if name.starts_with('.') || name.eq_ignore_ascii_case("desktop.ini") || name.eq_ignore_ascii_case("thumbs.db") {
                            return None;
                        }
                        let path = item.path();
                        Some(Entry {
                            rendered: by_path.get(&notes::key(&path)).copied(),
                            kind: kind_of(&name),
                            folder: meta.is_dir(),
                            size: if meta.is_dir() { 0 } else { meta.len() },
                            modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                            name,
                            path,
                        })
                    }).collect()
                })
                .unwrap_or_default(),
        };
        let by_name = self.prefs.by_name;
        entries.sort_by(|a, b| {
            b.folder.cmp(&a.folder).then_with(|| if by_name { a.name.to_lowercase().cmp(&b.name.to_lowercase()) } else { b.modified.cmp(&a.modified) })
        });
        self.entries = entries;
        let still: HashSet<&PathBuf> = self.entries.iter().map(|e| &e.path).collect();
        self.selected.retain(|p| still.contains(p));
    }

    /// A file's words: Video Chat's for a clip it made, else the File Manager's own.
    fn note(&self, entry: &Entry) -> Note {
        if let Some((path, note, _)) = &self.draft {
            if path == &entry.path {
                return note.clone();
            }
        }
        match entry.rendered.and_then(|i| self.renders.get(i)) {
            Some(rendered) => rendered.note.clone(),
            None => self.notes.get(&entry.path).cloned().unwrap_or_default(),
        }
    }

    fn save_note(&mut self, path: &Path, note: &Note) -> Result<(), String> {
        let rendered = self.renders.iter().position(|r| notes::key(&r.path) == notes::key(path));
        match rendered {
            Some(i) => {
                notes::save_rendered(&self.renders[i], note)?;
                self.renders[i].note = note.clone();
                Ok(())
            }
            None => self.notes.set(path, note.clone()),
        }
    }

    /// Saves the words being typed, if they changed.
    fn commit_draft(&mut self) {
        if let Some((path, note, Some(_))) = self.draft.clone() {
            match self.save_note(&path, &note) {
                Ok(()) => {
                    if let Some(draft) = &mut self.draft {
                        draft.2 = None;
                    }
                }
                Err(error) => self.toasts.push(format!("Couldn't save the title and description: {error}")),
            }
        }
    }

    fn take_thumbs(&mut self) {
        while let Ok((path, frame)) = self.thumb_done.try_recv() {
            let thumb = frame.map(|frame| Thumb {
                texture: self.ctx.load_texture(format!("fm-{}", path.display()), egui::ColorImage::from_rgba_unmultiplied(frame.size, &frame.rgba), TextureOptions::LINEAR),
                frame,
            });
            self.thumbs.insert(path, thumb);
        }
    }

    fn thumb(&mut self, entry: &Entry) -> Option<&Thumb> {
        if !matches!(entry.kind, Kind::Video | Kind::Image) {
            return None;
        }
        if self.asked.insert(entry.path.clone()) {
            let _ = self.thumb_jobs.send(entry.path.clone());
        }
        self.thumbs.get(&entry.path).and_then(Option::as_ref)
    }

    /// Drags the selected files (or `entry`, if it isn't selected) out of Canopy, their caption on the clipboard.
    fn drag_out(&mut self, entry: &Entry) {
        self.commit_draft();
        let files: Vec<Entry> = if self.selected.contains(&entry.path) {
            self.entries.iter().filter(|e| self.selected.contains(&e.path) && !e.folder).cloned().collect()
        } else {
            vec![entry.clone()]
        };
        if files.is_empty() {
            return;
        }
        let caption = match self.prefs.on_drag {
            OnDrag::Nothing => String::new(),
            mode => files.iter().map(|f| self.note(f).caption(mode == OnDrag::TitleAndDescription)).filter(|c| !c.is_empty()).collect::<Vec<_>>().join("\n\n"),
        };
        if !caption.is_empty() {
            set_clipboard(&self.ctx, &caption);
        }
        let image = self.thumbs.get(&files[0].path).and_then(Option::as_ref).and_then(|t| dragout::png(&t.frame.rgba, t.frame.size));
        match dragout::drag_files(files.iter().map(|f| f.path.clone()).collect(), image) {
            Ok(true) if !caption.is_empty() => self.toasts.push("Dropped. Its description is on the clipboard: paste it with Ctrl+V".into()),
            Ok(_) => {}
            Err(error) => self.toasts.push(format!("Couldn't drag it out: {error}")),
        }
    }

    fn click(&mut self, entry: &Entry, ctrl: bool, shift: bool) {
        if shift {
            if let Some(anchor) = self.anchor.clone() {
                let a = self.entries.iter().position(|e| e.path == anchor);
                let b = self.entries.iter().position(|e| e.path == entry.path);
                if let (Some(a), Some(b)) = (a, b) {
                    self.selected = self.entries[a.min(b)..=a.max(b)].iter().map(|e| e.path.clone()).collect();
                    return;
                }
            }
        }
        if ctrl {
            if let Some(i) = self.selected.iter().position(|p| p == &entry.path) {
                self.selected.remove(i);
            } else {
                self.selected.push(entry.path.clone());
            }
        } else {
            self.selected = vec![entry.path.clone()];
        }
        self.anchor = Some(entry.path.clone());
        self.cursor = Some(entry.path.clone());
    }

    /// Moves the selected files to the Recycle Bin. Folders and the clips Video Chat keeps track of stay.
    fn recycle_selected(&mut self) {
        self.commit_draft();
        let chosen: Vec<Entry> = self.entries.iter().filter(|e| self.selected.contains(&e.path)).cloned().collect();
        let (mut moved, mut kept) = (0, 0);
        for entry in &chosen {
            if entry.folder || entry.rendered.is_some() || recycle(&entry.path).is_err() {
                kept += 1;
            } else {
                moved += 1;
            }
        }
        match (moved, kept) {
            (0, 0) => return,
            (0, _) => self.toasts.push("Folders and Video Chat's clips aren't deleted from here".into()),
            (1, _) => self.toasts.push(format!("Moved {} to the Recycle Bin", chosen.iter().find(|e| !e.path.exists()).map(|e| e.name.as_str()).unwrap_or("it"))),
            (n, _) => self.toasts.push(format!("Moved {n} files to the Recycle Bin")),
        }
        self.draft = None;
        self.selected.clear();
        self.listed_at = Instant::now() - Duration::from_secs(600);
    }

    fn up(&mut self) {
        if let Location::Folder(dir) = &self.location {
            if let Some(parent) = dir.parent().map(Path::to_path_buf) {
                self.go(Location::Folder(parent));
            }
        }
    }

    fn open(&mut self, entry: &Entry) {
        if entry.folder {
            self.go(Location::Folder(entry.path.clone()));
        } else {
            open_with_default(&entry.path);
        }
    }

    fn rename_file(&mut self, from: &Path, name: &str) -> Result<PathBuf, String> {
        let mut name = safe_file_name(name.trim());
        // A file renamed without an extension keeps its own.
        if !from.is_dir() && Path::new(&name).extension().is_none() {
            if let Some(extension) = from.extension() {
                name = format!("{name}.{}", extension.to_string_lossy());
            }
        }
        let to = from.with_file_name(&name);
        if to == from {
            return Ok(to);
        }
        let same_file = from.file_name().is_some_and(|old| old.to_string_lossy().to_lowercase() == name.to_lowercase());
        if to.exists() && !same_file {
            return Err(format!("There's already a {name} here."));
        }
        std::fs::rename(from, &to).map_err(|e| e.to_string())?;
        self.notes.moved(from, &to)?;
        Ok(to)
    }
}

fn open_with_default(path: &Path) {
    let _ = std::process::Command::new("explorer").arg(path).spawn();
}

/// Puts text on the clipboard now (the drag that follows runs before egui would).
fn set_clipboard(ctx: &egui::Context, text: &str) {
    #[cfg(windows)]
    if windows_clipboard(text) {
        return;
    }
    ctx.copy_text(text.to_owned());
}

/// Puts `text` on the Windows clipboard as Unicode text, so emoji and accents survive. False if it couldn't.
#[cfg(windows)]
fn windows_clipboard(text: &str) -> bool {
    const CF_UNICODETEXT: u32 = 13;
    const GMEM_MOVEABLE: u32 = 0x0002;
    #[link(name = "user32")]
    unsafe extern "system" {
        fn OpenClipboard(owner: isize) -> i32;
        fn EmptyClipboard() -> i32;
        fn SetClipboardData(format: u32, memory: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        fn CloseClipboard() -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GlobalAlloc(flags: u32, bytes: usize) -> *mut std::ffi::c_void;
        fn GlobalLock(memory: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        fn GlobalUnlock(memory: *mut std::ffi::c_void) -> i32;
        fn GlobalFree(memory: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    }
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: the standard clipboard sequence. The memory is ours until SetClipboardData takes it; if that fails we
    // free it. Copying `wide` stays within the size allocated.
    unsafe {
        if OpenClipboard(0) == 0 {
            return false;
        }
        EmptyClipboard();
        let memory = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2);
        let mut ok = false;
        if !memory.is_null() {
            let target = GlobalLock(memory) as *mut u16;
            if !target.is_null() {
                std::ptr::copy_nonoverlapping(wide.as_ptr(), target, wide.len());
                GlobalUnlock(memory);
                ok = !SetClipboardData(CF_UNICODETEXT, memory).is_null();
            }
            if !ok {
                GlobalFree(memory);
            }
        }
        CloseClipboard();
        ok
    }
}

#[cfg(all(test, windows))]
mod tests {
    /// Writes to the real clipboard: run alone with `--ignored`, and save the clipboard first.
    #[test]
    #[ignore]
    fn puts_unicode_text_on_the_clipboard() {
        assert!(super::windows_clipboard("Wait for it 🎮 #fyp"));
    }
}

fn size_text(entry: &Entry) -> String {
    if entry.folder { "Folder".into() } else { canopy_ui::format_size(entry.size) }
}

/// "6 Nov 19:51" (this year) or "6 Nov 2025".
fn date_text(time: SystemTime) -> String {
    let seconds = time.duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0) + crate::videochat::local_offset_seconds();
    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0) + crate::videochat::local_offset_seconds();
    let (year, month, day) = crate::videochat::model::civil(seconds.div_euclid(86_400));
    let (this_year, _, _) = crate::videochat::model::civil(now.div_euclid(86_400));
    let months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let minutes = seconds.rem_euclid(86_400) / 60;
    if year == this_year {
        format!("{day} {} {:02}:{:02}", months[month as usize - 1], minutes / 60, minutes % 60)
    } else {
        format!("{day} {} {year}", months[month as usize - 1])
    }
}

impl CanopyApp for FileManager {
    fn ui(&mut self, ui: &mut Ui, host: &mut dyn Host) {
        self.take_thumbs();
        self.refresh();
        if self.draft.as_ref().is_some_and(|d| d.2.is_some_and(|at| at.elapsed() > Duration::from_millis(700))) {
            self.commit_draft();
        } else if self.draft.as_ref().is_some_and(|d| d.2.is_some()) {
            self.ctx.request_repaint_after(Duration::from_millis(750));
        }
        for toast in self.toasts.drain(..) {
            host.toast(toast);
        }
        self.ctx.request_repaint_after(Duration::from_secs(2));
        self.focused = host.focused() && self.rename.is_none();
        let rect = ui.max_rect();
        let places_w = 210.0;
        let details_w = if rect.width() > 900.0 { 330.0 } else { 270.0 };
        let left = Rect::from_min_size(rect.min, vec2(places_w, rect.height()));
        let right = Rect::from_min_max(pos2(rect.right() - details_w, rect.top()), rect.max);
        let middle = Rect::from_min_max(pos2(left.right() + 6.0, rect.top()), pos2(right.left() - 6.0, rect.bottom()));
        card(ui, left, |ui| self.places(ui));
        card(ui, middle, |ui| self.list(ui));
        card(ui, right, |ui| self.details(ui, host));
        self.rename_dialog(ui);
        if host.focused() && self.rename.is_none() && !ui.ctx().memory(|m| m.focused().is_some()) {
            let (back, all, delete_hint, delete, find, up, reload) = ui.input_mut(|i| (
                i.consume_key(egui::Modifiers::NONE, egui::Key::Backspace),
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::A),
                i.consume_key(egui::Modifiers::NONE, egui::Key::F2),
                i.consume_key(egui::Modifiers::NONE, egui::Key::Delete),
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::F),
                i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowUp),
                i.consume_key(egui::Modifiers::NONE, egui::Key::F5),
            ));
            if delete {
                self.recycle_selected();
            }
            if find {
                self.focus_search = true;
            }
            if up {
                self.up();
            }
            if reload {
                self.listed_at = Instant::now() - Duration::from_secs(600);
            }
            if back {
                if let Some(previous) = self.history.pop() {
                    self.location = previous;
                    self.selected.clear();
                    self.listed_at = Instant::now() - Duration::from_secs(600);
                }
            }
            if all {
                self.selected = self.entries.iter().filter(|e| !e.folder).map(|e| e.path.clone()).collect();
            }
            if delete_hint {
                if let [path] = self.selected.as_slice() {
                    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    self.rename = Some((path.clone(), name));
                }
            }
        }
    }

    /// Files dropped on the File Manager are copied into the folder it shows.
    fn receive(&mut self, files: Vec<SharedFile>, _host: &mut dyn Host) -> Receipt {
        let mut receipt = Receipt::default();
        let Location::Folder(dir) = self.location.clone() else {
            receipt.note = Some("Open a folder to copy files into it".into());
            return receipt;
        };
        for file in files {
            if file.path.parent() == Some(dir.as_path()) {
                continue; // Dropped back where it came from.
            }
            match copy_into(&dir, &file.path) {
                Ok(_) => receipt.accepted += 1,
                Err(_) => receipt.skipped.push(file.name),
            }
        }
        receipt.note = Some(match receipt.accepted {
            0 => "Nothing to copy: it's already here".into(),
            1 => "Copied it here".into(),
            n => format!("Copied {n} files here"),
        });
        self.listed_at = Instant::now() - Duration::from_secs(600);
        receipt
    }

    fn open_document(&mut self, _: &str, _: &mut dyn Host) {}

    fn save(&mut self) -> Result<(), String> {
        self.commit_draft();
        Ok(())
    }
}

fn card(ui: &mut Ui, rect: Rect, contents: impl FnOnce(&mut Ui)) {
    ui.painter().rect_filled(rect, 10, color::VEIL);
    ui.painter().rect_stroke(rect, 10, Stroke::new(1.0, color::HAIRLINE), StrokeKind::Inside);
    ui.scope_builder(UiBuilder::new().max_rect(rect.shrink(10.0)), |ui| {
        ui.set_clip_rect(rect.shrink(4.0));
        contents(ui)
    });
}

impl FileManager {
    fn places(&mut self, ui: &mut Ui) {
        let mut go = None;
        let mut unpin = None;
        let place_row = |ui: &mut Ui, label: &str, icon: Icon, at: Location, current: &Location, go: &mut Option<Location>| -> egui::Response {
            let selected = &at == current;
            let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
            if selected {
                ui.painter().rect_filled(rect, 6, color::SELECT_FILL);
            } else if response.hovered() {
                ui.painter().rect_filled(rect, 6, color::VEIL_HOVER);
            }
            canopy_ui::icon(ui.painter(), Rect::from_center_size(pos2(rect.left() + 14.0, rect.center().y), vec2(15.0, 15.0)), icon, if selected { color::SELECT_INK } else { color::INK_SOFT });
            ui.painter().with_clip_rect(rect).text(pos2(rect.left() + 30.0, rect.center().y), Align2::LEFT_CENTER, label, font::ui(13.0), if selected { color::INK } else { color::INK_SOFT });
            if response.clicked() {
                *go = Some(at);
            }
            response
        };
        egui::ScrollArea::vertical().id_salt("fm-places").auto_shrink(false).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            section_label(ui, "Canopy");
            if self.place.space.is_some() {
                place_row(ui, "This space's Files", Icon::Folder, Location::Folder(self.place.exports_dir.clone()), &self.location, &mut go);
            }
            if let Some(folder) = engine::folder() {
                place_row(ui, "Video Chat clips", Icon::Video, Location::VideoChatClips, &self.location, &mut go).on_hover_text("Everything Video Chat rendered, with its titles and descriptions");
                place_row(ui, "Video Chat downloads", Icon::Export, Location::Folder(Self::video_chat_downloads(&folder)), &self.location, &mut go);
            }
            ui.add_space(8.0);
            section_label(ui, "This PC");
            for (label, dir) in [("Videos", dirs::video_dir()), ("Downloads", dirs::download_dir()), ("Desktop", dirs::desktop_dir()), ("Documents", dirs::document_dir()), ("Pictures", dirs::picture_dir())] {
                if let Some(dir) = dir.filter(|d| d.is_dir()) {
                    place_row(ui, label, Icon::Folder, Location::Folder(dir), &self.location, &mut go);
                }
            }
            ui.add_space(8.0);
            section_label(ui, "Pinned");
            if self.prefs.pinned.is_empty() {
                ui.label(muted("Pin a folder you post from (the pin button above the list).").font(font::ui(12.0)));
            }
            for dir in self.prefs.pinned.clone() {
                let label = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| dir.display().to_string());
                place_row(ui, &label, Icon::Folder, Location::Folder(dir.clone()), &self.location, &mut go).on_hover_text(dir.display().to_string()).context_menu(|ui| {
                    if ui.button("Unpin").clicked() {
                        unpin = Some(dir.clone());
                        ui.close();
                    }
                });
            }
            ui.add_space(6.0);
            if ui.small_button("Open a folder…").clicked() {
                if let Some(dir) = rfd::FileDialog::new().set_title("Open a folder").pick_folder() {
                    go = Some(Location::Folder(dir));
                }
            }
        });
        if let Some(dir) = unpin {
            self.prefs.pinned.retain(|p| p != &dir);
            self.save_prefs();
        }
        if let Some(location) = go {
            self.go(location);
        }
    }

    fn list(&mut self, ui: &mut Ui) {
        // The bar: back, up, where, pin, sort and search.
        let mut go = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if ui.add_enabled_ui(!self.history.is_empty(), |ui| icon_button(ui, Icon::SkipBack, "Back (Backspace)", false)).inner.clicked() {
                if let Some(previous) = self.history.pop() {
                    self.location = previous;
                    self.selected.clear();
                    self.listed_at = Instant::now() - Duration::from_secs(600);
                }
            }
            let parent = match &self.location {
                Location::Folder(dir) => dir.parent().map(Path::to_path_buf),
                _ => None,
            };
            if ui.add_enabled(parent.is_some(), egui::Button::new("Up").small()).on_hover_text("Up a folder (Alt+Up)").clicked() {
                go = parent.map(Location::Folder);
            }
            let title = match &self.location {
                Location::Folder(dir) => dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| dir.display().to_string()),
                Location::VideoChatClips => "Video Chat clips".into(),
            };
            let label = ui.label(RichText::new(title).font(font::display(16.0)).color(color::INK));
            if let Location::Folder(dir) = &self.location {
                label.on_hover_text(dir.display().to_string());
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if let Location::Folder(dir) = self.location.clone() {
                    if ui.small_button("Show in Explorer").clicked() {
                        open_with_default(&dir);
                    }
                    if ui.small_button("New folder").clicked() {
                        let folder = dir.join(unique_name(&dir, "New folder"));
                        match std::fs::create_dir(&folder) {
                            Ok(()) => {
                                self.rename = Some((folder.clone(), folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
                                self.listed_at = Instant::now() - Duration::from_secs(600);
                            }
                            Err(error) => self.toasts.push(format!("Couldn't make a folder here: {error}")),
                        }
                    }
                    let pinned = self.prefs.pinned.contains(&dir);
                    if ui.small_button(if pinned { "Unpin" } else { "Pin" }).on_hover_text(if pinned { "Take this folder off the left" } else { "Keep this folder on the left, under Pinned" }).clicked() {
                        if pinned {
                            self.prefs.pinned.retain(|p| p != &dir);
                        } else {
                            self.prefs.pinned.push(dir);
                        }
                        self.save_prefs();
                    }
                }
                let sort = if self.prefs.by_name { "By name" } else { "Newest first" };
                if ui.small_button(sort).on_hover_text("Change the order").clicked() {
                    self.prefs.by_name = !self.prefs.by_name;
                    self.save_prefs();
                    self.listed_at = Instant::now() - Duration::from_secs(600);
                }
            });
        });
        let search = ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Search names, titles and descriptions (Ctrl+F)").desired_width(f32::INFINITY).margin(vec2(8.0, 5.0)));
        if std::mem::take(&mut self.focus_search) {
            search.request_focus();
        }
        // Show only one kind of file, and how much is here.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (label, kind) in [("All", None), ("Videos", Some(Kind::Video)), ("Pictures", Some(Kind::Image)), ("Sound", Some(Kind::Audio))] {
                if ui.selectable_label(self.filter == kind, label).clicked() {
                    self.filter = kind;
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let count = match (self.entries.len(), self.selected.len()) {
                    (all, 0) => format!("{all} item{}", if all == 1 { "" } else { "s" }),
                    (all, chosen) => format!("{chosen} of {all} selected"),
                };
                ui.label(muted(count).font(font::ui(12.0)));
            });
        });
        ui.add_space(4.0);
        if let Some(location) = go {
            self.go(location);
            return;
        }
        let query = self.search.trim().to_lowercase();
        let shown: Vec<Entry> = self
            .entries
            .clone()
            .into_iter()
            .filter(|e| {
                if !e.folder && self.filter.is_some_and(|kind| e.kind != kind) {
                    return false;
                }
                if query.is_empty() {
                    return true;
                }
                let note = self.note(e);
                format!("{}\n{}\n{}", e.name, note.title, note.description).to_lowercase().contains(&query)
            })
            .collect();
        if shown.is_empty() {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.label(muted(if query.is_empty() {
                    if self.location == Location::VideoChatClips { "Video Chat hasn't rendered anything yet." } else { "This folder is empty." }
                } else {
                    "Nothing matches."
                }));
            });
            return;
        }
        let mut clicked = None;
        let mut opened = None;
        let mut dragged = None;
        let mut renamed = None;
        let mut copied = None;
        let mut duplicated = None;
        let mut recycled = None;
        // Arrows walk the list (Shift selects along the way) and Enter opens.
        if self.focused && !ui.ctx().memory(|m| m.focused().is_some()) {
            let (step, extend, enter) = ui.input(|i| {
                let step = if i.modifiers.alt { 0 } else { i.key_pressed(egui::Key::ArrowDown) as i32 - i.key_pressed(egui::Key::ArrowUp) as i32 };
                (step, i.modifiers.shift, i.key_pressed(egui::Key::Enter))
            });
            if step != 0 {
                let at = self.cursor.as_ref().and_then(|cursor| shown.iter().position(|e| &e.path == cursor));
                let next = match at {
                    Some(at) => (at as i32 + step).clamp(0, shown.len() as i32 - 1) as usize,
                    None => if step > 0 { 0 } else { shown.len() - 1 },
                };
                let entry = shown[next].clone();
                if extend && self.anchor.is_some() {
                    let anchor = self.anchor.clone();
                    self.click(&entry, false, true);
                    self.anchor = anchor;
                    self.cursor = Some(entry.path.clone());
                } else {
                    self.click(&entry, false, false);
                }
                self.reveal = Some(entry.path);
            }
            if enter {
                opened = shown.iter().find(|e| self.selected.contains(&e.path)).cloned();
            }
        }
        egui::ScrollArea::vertical().id_salt("fm-list").auto_shrink(false).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 3.0;
            for entry in &shown {
                let note = self.note(entry);
                let selected = self.selected.contains(&entry.path);
                let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 54.0), Sense::click_and_drag());
                if self.reveal.as_ref() == Some(&entry.path) {
                    response.scroll_to_me(None);
                    self.reveal = None;
                }
                if !ui.is_rect_visible(rect) {
                    continue;
                }
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 8, if selected { color::SELECT_FILL } else if response.hovered() { color::VEIL_HOVER } else { color::VEIL });
                if selected {
                    painter.rect_stroke(rect, 8, Stroke::new(1.0, color::SELECT_LINE), StrokeKind::Inside);
                }
                let thumb_rect = Rect::from_min_size(rect.min + vec2(6.0, 5.0), vec2(78.0, 44.0));
                painter.rect_filled(thumb_rect, 4, Color32::from_black_alpha(90));
                match self.thumb(entry) {
                    Some(thumb) => {
                        let [w, h] = thumb.frame.size;
                        let aspect = w as f32 / h.max(1) as f32;
                        let fit = if aspect > thumb_rect.width() / thumb_rect.height() { vec2(thumb_rect.width(), thumb_rect.width() / aspect) } else { vec2(thumb_rect.height() * aspect, thumb_rect.height()) };
                        painter.image(thumb.texture.id(), Rect::from_center_size(thumb_rect.center(), fit), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    }
                    None => {
                        let icon = match (entry.folder, entry.kind) {
                            (true, _) => Icon::Folder,
                            (_, Kind::Video) => Icon::Video,
                            (_, Kind::Image) => Icon::Image,
                            (_, Kind::Audio) => Icon::Audio,
                            (_, Kind::Archive) => Icon::Archive,
                            (_, Kind::Project) => Icon::Project,
                            _ => Icon::File,
                        };
                        canopy_ui::icon(&painter, Rect::from_center_size(thumb_rect.center(), vec2(20.0, 20.0)), icon, color::INK_SOFT);
                    }
                }
                let text_left = thumb_rect.right() + 10.0;
                let headline = if note.title.trim().is_empty() { entry.name.clone() } else { note.title.trim().to_owned() };
                painter.text(pos2(text_left, rect.top() + 16.0), Align2::LEFT_CENTER, &headline, font::ui(14.0), color::INK);
                let mut detail = vec![size_text(entry), date_text(entry.modified)];
                if !note.title.trim().is_empty() {
                    detail.insert(0, entry.name.clone());
                }
                if entry.rendered.is_some() {
                    detail.push("from Video Chat".into());
                }
                painter.text(pos2(text_left, rect.top() + 33.0), Align2::LEFT_CENTER, detail.join(" · "), font::ui(12.0), color::INK_MUTED);
                if !note.description.trim().is_empty() {
                    let line = note.description.lines().next().unwrap_or_default();
                    painter.text(pos2(text_left, rect.top() + 47.0), Align2::LEFT_CENTER, line, font::ui(11.0), color::SELECT_INK);
                }
                if !note.is_empty() {
                    // A mark for "has words to post".
                    canopy_ui::icon(&painter, Rect::from_center_size(pos2(rect.right() - 16.0, rect.center().y), vec2(14.0, 14.0)), Icon::Text, color::SELECT_INK);
                }
                if response.drag_started() && !entry.folder {
                    dragged = Some(entry.clone());
                } else if response.double_clicked() {
                    opened = Some(entry.clone());
                } else if response.clicked() {
                    clicked = Some((entry.clone(), ui.input(|i| (i.modifiers.command, i.modifiers.shift))));
                }
                let hover = if entry.folder { "Double-click to open".to_owned() } else { "Drag it into TikTok, a browser or any app. Double-click opens it.".to_owned() };
                response.on_hover_text_at_pointer(hover).context_menu(|ui| {
                    if !entry.folder {
                        if ui.button("Copy description").clicked() {
                            copied = Some(note.caption(false));
                            ui.close();
                        }
                        if ui.button("Copy title and description").clicked() {
                            copied = Some(note.caption(true));
                            ui.close();
                        }
                        ui.separator();
                    }
                    if ui.button("Open").clicked() {
                        opened = Some(entry.clone());
                        ui.close();
                    }
                    if ui.button("Show in Explorer").clicked() {
                        let _ = std::process::Command::new("explorer").arg(format!("/select,{}", entry.path.display())).spawn();
                        ui.close();
                    }
                    if ui.button("Copy path").clicked() {
                        copied = Some(entry.path.display().to_string());
                        ui.close();
                    }
                    if entry.rendered.is_none() && ui.button("Rename…").clicked() {
                        renamed = Some(entry.clone());
                        ui.close();
                    }
                    if !entry.folder && ui.button("Duplicate").clicked() {
                        duplicated = Some(entry.clone());
                        ui.close();
                    }
                    if !entry.folder && entry.rendered.is_none() && ui.button("Move to the Recycle Bin").clicked() {
                        recycled = Some(entry.clone());
                        ui.close();
                    }
                });
            }
        });
        if let Some((entry, (ctrl, shift))) = clicked {
            self.click(&entry, ctrl, shift);
        }
        if let Some(entry) = opened {
            self.open(&entry);
        }
        if let Some(entry) = renamed {
            self.rename = Some((entry.path.clone(), entry.name.clone()));
        }
        if let Some(entry) = duplicated {
            match entry.path.parent().map(|dir| copy_into(dir, &entry.path)) {
                Some(Ok(copy)) => self.toasts.push(format!("Made {copy}")),
                Some(Err(error)) => self.toasts.push(error.0),
                None => {}
            }
            self.listed_at = Instant::now() - Duration::from_secs(600);
        }
        if let Some(entry) = recycled {
            // The file that was right-clicked goes, with the rest of the selection when it's part of it.
            if !self.selected.contains(&entry.path) {
                self.selected = vec![entry.path.clone()];
            }
            self.recycle_selected();
        }
        if let Some(text) = copied.filter(|t| !t.is_empty()) {
            set_clipboard(&self.ctx, &text);
            self.toasts.push("Copied".into());
        }
        if let Some(entry) = dragged {
            if !self.selected.contains(&entry.path) {
                self.selected = vec![entry.path.clone()];
            }
            self.drag_out(&entry);
        }
    }

    fn details(&mut self, ui: &mut Ui, host: &mut dyn Host) {
        let chosen: Vec<Entry> = self.entries.iter().filter(|e| self.selected.contains(&e.path)).cloned().collect();
        egui::ScrollArea::vertical().id_salt("fm-details").auto_shrink(false).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            match chosen.as_slice() {
                [] => {
                    section_label(ui, "Posting");
                    ui.add(egui::Label::new(muted("Pick a file to give it a title and a description. Then drag it into TikTok (or anywhere): its description goes on the clipboard, so paste it as the caption.")).wrap());
                    ui.add_space(8.0);
                    section_label(ui, "Keys");
                    ui.add(egui::Label::new(muted("Arrows move and Enter opens. F2 renames, Delete moves to the Recycle Bin, Ctrl+F searches, Alt+Up goes up a folder, Backspace goes back.").font(font::ui(12.0))).wrap());
                }
                [entry] if !entry.folder => self.one(ui, entry.clone(), host),
                [entry] => {
                    ui.label(RichText::new(&entry.name).font(font::display(16.0)));
                    ui.label(muted("A folder. Double-click to open it."));
                }
                many => {
                    let files = many.iter().filter(|e| !e.folder).count();
                    ui.label(RichText::new(format!("{files} files")).font(font::display(16.0)));
                    let size: u64 = many.iter().map(|e| e.size).sum();
                    ui.label(muted(canopy_ui::format_size(size)));
                    ui.add(egui::Label::new(muted("Drag any of them to drag them all. Their descriptions go on the clipboard together.")).wrap());
                }
            }
            ui.add_space(12.0);
            section_label(ui, "When I drag a file out, copy");
            let before = self.prefs.on_drag;
            ui.radio_value(&mut self.prefs.on_drag, OnDrag::Description, "its description");
            ui.radio_value(&mut self.prefs.on_drag, OnDrag::TitleAndDescription, "its title and description");
            ui.radio_value(&mut self.prefs.on_drag, OnDrag::Nothing, "nothing");
            if self.prefs.on_drag != before {
                self.save_prefs();
            }
        });
    }

    fn one(&mut self, ui: &mut Ui, entry: Entry, host: &mut dyn Host) {
        let width = ui.available_width();
        if let Some(thumb) = self.thumb(&entry) {
            let [w, h] = thumb.frame.size;
            let height = (width * h as f32 / w.max(1) as f32).min(220.0);
            let id = thumb.texture.id();
            let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());
            ui.painter().image(id, rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            if response.drag_started() {
                self.drag_out(&entry);
            } else if response.double_clicked() {
                open_with_default(&entry.path);
            }
            response.on_hover_text("Drag it out to post it; double-click to play it");
        }
        ui.add(egui::Label::new(RichText::new(&entry.name).font(font::bold(14.0)).color(color::INK)).wrap());
        let mut detail = format!("{} · {}", size_text(&entry), date_text(entry.modified));
        if let Some(rendered) = entry.rendered.and_then(|i| self.renders.get(i)) {
            detail.push_str(&format!(" · {} from {}", if rendered.short { "Short" } else { "chapter" }, rendered.project));
        }
        ui.label(muted(detail).font(font::ui(12.0)));
        if entry.rendered.is_some() {
            ui.label(muted("Its words are Video Chat's: changes here show there too.").font(font::ui(11.0)));
        }
        // The words, edited in a draft that saves a moment after the last change.
        if self.draft.as_ref().is_none_or(|d| d.0 != entry.path) {
            self.commit_draft();
            let note = self.note(&entry);
            self.draft = Some((entry.path.clone(), note, None));
        }
        let Some((_, note, changed)) = &mut self.draft else { return };
        ui.add_space(4.0);
        section_label(ui, "Title");
        if ui.add(egui::TextEdit::singleline(&mut note.title).hint_text("What it's called").desired_width(f32::INFINITY).margin(vec2(8.0, 6.0))).changed() {
            *changed = Some(Instant::now());
        }
        ui.horizontal(|ui| {
            section_label(ui, "Description");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let count = note.description.chars().count();
                ui.label(RichText::new(format!("{count} / 4000")).font(font::ui(11.0)).color(if count > 4000 { color::DANGER } else { color::INK_FAINT }))
                    .on_hover_text("TikTok captions hold up to 4,000 characters, hashtags included");
            });
        });
        if ui.add(egui::TextEdit::multiline(&mut note.description).hint_text("The caption: what to say, and its #hashtags").desired_rows(7).desired_width(f32::INFINITY)).changed() {
            *changed = Some(Instant::now());
        }
        let caption = note.clone();
        let mut copy = None;
        ui.horizontal_wrapped(|ui| {
            if quiet_button(ui, "Copy description", None).clicked() {
                copy = Some(caption.caption(false));
            }
            if ui.small_button("Copy title").clicked() {
                copy = Some(caption.title.trim().to_owned());
            }
            if ui.small_button("Copy both").clicked() {
                copy = Some(caption.caption(true));
            }
        });
        if let Some(text) = copy {
            if text.is_empty() {
                host.toast("There's nothing to copy yet".into());
            } else {
                set_clipboard(&self.ctx, &text);
                host.toast("Copied".into());
            }
        }
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            if primary_button(ui, if matches!(entry.kind, Kind::Video | Kind::Audio) { "Play" } else { "Open" }, Mood::Ember).clicked() {
                open_with_default(&entry.path);
            }
            if quiet_button(ui, "Show in Explorer", Some(Icon::Folder)).clicked() {
                let _ = std::process::Command::new("explorer").arg(format!("/select,{}", entry.path.display())).spawn();
            }
            if entry.rendered.is_none() && ui.small_button("Rename…").clicked() {
                self.rename = Some((entry.path.clone(), entry.name.clone()));
            }
        });
        ui.add_space(4.0);
        ui.add(egui::Label::new(muted("Drag the picture or the row into TikTok's upload page, Explorer or any app.").font(font::ui(12.0))).wrap());
    }

    fn rename_dialog(&mut self, ui: &mut Ui) {
        let Some((path, mut name)) = self.rename.clone() else { return };
        let mut done = None;
        let modal = egui::Modal::new(ui.scope_id().with("fm-rename")).show(ui.ctx(), |ui| {
            ui.set_width(380.0);
            ui.label(RichText::new("Rename").font(font::display(20.0)));
            let id = ui.scope_id().with("fm-rename-field");
            let field = ui.add(egui::TextEdit::singleline(&mut name).id(id).desired_width(f32::INFINITY).margin(vec2(10.0, 8.0)));
            // As the dialog opens, select the name without its extension: typing replaces it, as in Explorer.
            if self.rename_selected.as_ref() != Some(&path) {
                self.rename_selected = Some(path.clone());
                let stem = if path.is_dir() { name.chars().count() } else { Path::new(&name).file_stem().map_or(0, |s| s.to_string_lossy().chars().count()) };
                let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
                state.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(stem))));
                state.store(ui.ctx(), id);
            }
            // The field keeps the keyboard while the dialog is up, so Enter is read from the keys.
            field.request_focus();
            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            ui.add_space(8.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if primary_button(ui, "Rename", Mood::Ember).clicked() || enter {
                    done = Some(true);
                }
                if quiet_button(ui, "Back", None).clicked() {
                    done = Some(false);
                }
            });
        });
        self.rename = Some((path.clone(), name.clone()));
        if modal.should_close() || done == Some(false) {
            self.rename = None;
            self.rename_selected = None;
        } else if done == Some(true) {
            self.rename = None;
            self.rename_selected = None;
            self.commit_draft();
            match self.rename_file(&path, &name) {
                Ok(to) => {
                    self.selected = vec![to];
                    self.draft = None;
                    self.listed_at = Instant::now() - Duration::from_secs(600);
                }
                Err(error) => self.toasts.push(format!("Couldn't rename it: {error}")),
            }
        }
    }
}
