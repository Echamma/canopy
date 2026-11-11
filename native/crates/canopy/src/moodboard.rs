//! A space-local, native moodboard. Positions are in an unbounded world plane.
use canopy_ui::{
    Accent, AppDescriptor, CanopyApp, Document, Host, Place, Receipt, SharedFile, color, font,
};
use egui::{
    Align2, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, TextureHandle, TextureOptions, Ui,
    UiBuilder, Vec2, pos2, vec2,
};
use serde::{Deserialize, Serialize};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Instant;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use streamcuts::ffmpeg::{self, Frame, FrameStream};
use streamcuts::project::{MediaKind, Picture};

pub const DESCRIPTOR: AppDescriptor = AppDescriptor {
    id: "moodboard",
    name: "Moodie",
    description: "Collect images and notes on an endless canvas",
    accent: Accent::Leaf,
    initial: "Mo",
    folder: "Moodboard",
    icon_png: include_bytes!("../../../../assets/canopy.png"),
    accepts: accepts,
    documents,
    create,
};

const IMAGE_HEADER: f32 = 28.0;

// Project the pointer movement onto the image's diagonal so either axis can resize it.
fn resized_image(w: f32, h: f32, screen_delta: Vec2, zoom: f32) -> (f32, f32) {
    let ratio = (h - IMAGE_HEADER).max(1.0) / w.max(1.0);
    let delta = screen_delta / zoom.max(0.1);
    let width_delta = (delta.x + ratio * delta.y) / (1.0 + ratio * ratio);
    let width = (w + width_delta).clamp(120.0, 1600.0);
    (width, IMAGE_HEADER + width * ratio)
}

fn accepts(file: &SharedFile) -> bool {
    is_image(file) || is_video(file)
}
fn is_image(file: &SharedFile) -> bool {
    matches!(
        file.path
            .extension()
            .and_then(|x| x.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "png" | "jpg" | "jpeg" | "webp"
    )
}
fn is_video(file: &SharedFile) -> bool {
    matches!(
        file.path
            .extension()
            .and_then(|x| x.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "mp4" | "mov" | "webm" | "mkv" | "m4v" | "avi"
    )
}

#[cfg(windows)]
fn open_with_default_player(path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn ShellExecuteW(
            window: isize,
            operation: *const u16,
            file: *const u16,
            parameters: *const u16,
            directory: *const u16,
            show: i32,
        ) -> isize;
    }
    let verb: Vec<u16> = "open\0".encode_utf16().collect();
    let file: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    // ShellExecuteW reads the zero-terminated strings during this call and retains no pointer.
    let result = unsafe {
        ShellExecuteW(
            0,
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
        )
    };
    if result <= 32 {
        Err(format!("Windows couldn't open {}", path.display()))
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn open_with_default_player(path: &Path) -> Result<(), String> {
    std::process::Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[derive(Clone, Serialize, Deserialize)]
struct Card {
    id: u64,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    #[serde(flatten)]
    kind: CardKind,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum CardKind {
    Note {
        text: String,
    },
    Image {
        file: String,
        name: String,
    },
    Video {
        file: String,
        name: String,
        #[serde(default)]
        thumb: Option<String>,
        duration: f64,
        width: u32,
        height: u32,
    },
}

#[derive(Clone, Serialize, Deserialize)]
struct Board {
    version: u32,
    id: String,
    name: String,
    saved_at: u64,
    cards: Vec<Card>,
    /// Where the board was being looked at (pan x, pan y, zoom), so it reopens there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    view: Option<[f32; 3]>,
}

impl Board {
    fn new(name: String) -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        Self {
            version: 1,
            id: format!("{stamp}-{}", std::process::id()),
            name,
            saved_at: now(),
            cards: Vec::new(),
            view: None,
        }
    }
}

/// The pan and zoom a board was last seen at.
fn view_of(board: &Board) -> (Vec2, f32) {
    match board.view {
        Some([x, y, zoom]) if x.is_finite() && y.is_finite() && zoom.is_finite() => (vec2(x, y), zoom.clamp(0.1, 4.0)),
        _ => (Vec2::ZERO, 1.0),
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn board_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.moodboard.json"))
}
fn read_board(path: &Path) -> Result<Board, String> {
    let board: Board = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if board.version != 1 {
        return Err(format!("Unsupported moodboard version {}", board.version));
    }
    Ok(board)
}
fn list_boards(dir: &Path) -> Vec<Board> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut boards: Vec<_> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            path.file_name()?
                .to_str()?
                .ends_with(".moodboard.json")
                .then(|| read_board(&path).ok())
                .flatten()
        })
        .collect();
    boards.sort_by(|a, b| b.saved_at.cmp(&a.saved_at));
    boards
}
fn documents(place: &Place) -> Vec<Document> {
    list_boards(&place.projects_dir)
        .into_iter()
        .map(|b| Document {
            id: b.id,
            name: b.name,
            saved_at: b.saved_at,
            detail: format!("{} items", b.cards.len()),
        })
        .collect()
}
fn create(place: Place, _: &egui::Context) -> Box<dyn CanopyApp> {
    Box::new(Moodboard::new(place))
}

struct Moodboard {
    place: Place,
    board: Board,
    boards: Vec<Board>,
    selected: Option<u64>,
    pan: Vec2,
    zoom: f32,
    canvas: Rect,
    textures: HashMap<String, Option<TextureHandle>>,
    pending: Vec<(u64, Receiver<Result<Card, String>>)>,
    playing: Option<PlayingVideo>,
    dirty: bool,
    last_edit: f64,
    error: Option<String>,
    /// The cards as they were before the edit in progress, and the states Ctrl+Z and Ctrl+Y step through.
    before: Vec<Card>,
    undo: Vec<Vec<Card>>,
    redo: Vec<Vec<Card>>,
    /// When the cards last changed; `stale` says `before` is behind them.
    edited: Option<Instant>,
    stale: bool,
}

struct PlayingVideo {
    id: u64,
    stream: Option<FrameStream>,
    ahead: Option<Frame>,
    texture: Option<TextureHandle>,
    elapsed: f64,
    started: Option<Instant>,
    duration: f64,
}

impl Moodboard {
    fn new(place: Place) -> Self {
        let boards = list_boards(&place.projects_dir);
        let board = boards
            .first()
            .cloned()
            .unwrap_or_else(|| Board::new("Untitled board".into()));
        let (pan, zoom) = view_of(&board);
        Self {
            before: board.cards.clone(),
            undo: Vec::new(),
            redo: Vec::new(),
            edited: None,
            stale: false,
            place,
            board,
            boards,
            selected: None,
            pan,
            zoom,
            canvas: Rect::NOTHING,
            textures: HashMap::new(),
            pending: Vec::new(),
            playing: None,
            dirty: false,
            last_edit: 0.0,
            error: None,
        }
    }
    fn world(&self, screen: Pos2) -> Pos2 {
        pos2(
            (screen.x - self.canvas.center().x - self.pan.x) / self.zoom,
            (screen.y - self.canvas.center().y - self.pan.y) / self.zoom,
        )
    }
    fn screen(&self, x: f32, y: f32) -> Pos2 {
        self.canvas.center() + self.pan + vec2(x, y) * self.zoom
    }
    fn mark(&mut self, ui: &Ui) {
        self.record();
        self.dirty = true;
        self.last_edit = ui.input(|i| i.time);
    }
    /// Notes that the cards just changed. Changes that follow each other closely (a drag, typing) undo as one.
    fn record(&mut self) {
        if self.edited.is_none() {
            self.undo.push(std::mem::take(&mut self.before));
            if self.undo.len() > 100 {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
        self.edited = Some(Instant::now());
        self.stale = true;
    }
    /// Once an edit has rested, the next change starts a new undo step.
    fn settle(&mut self) {
        if self.edited.is_some_and(|at| at.elapsed().as_millis() > 600) {
            self.edited = None;
        }
        if self.edited.is_none() && self.stale {
            self.before = self.board.cards.clone();
            self.stale = false;
        }
    }
    /// Ctrl+Z (`back`) or Ctrl+Y. False when there's nothing to step to.
    fn step(&mut self, back: bool) -> bool {
        self.edited = None;
        let (from, to) = if back { (&mut self.undo, &mut self.redo) } else { (&mut self.redo, &mut self.undo) };
        let Some(cards) = from.pop() else { return false };
        to.push(std::mem::replace(&mut self.board.cards, cards));
        self.before = self.board.cards.clone();
        self.stale = false;
        if !self.board.cards.iter().any(|c| Some(c.id) == self.selected) {
            self.selected = None;
        }
        self.playing = None;
        self.dirty = true;
        self.last_edit = 0.0;
        true
    }
    fn fresh_history(&mut self) {
        self.before = self.board.cards.clone();
        self.undo.clear();
        self.redo.clear();
        self.edited = None;
        self.stale = false;
    }
    fn remove_selected(&mut self) -> bool {
        let Some(id) = self.selected.take() else { return false };
        if self.playing.as_ref().is_some_and(|p| p.id == id) {
            self.playing = None;
        }
        self.board.cards.retain(|c| c.id != id);
        true
    }
    fn duplicate_selected(&mut self) -> bool {
        let Some(mut card) = self.board.cards.iter().find(|c| Some(c.id) == self.selected).cloned() else { return false };
        card.id = self.next_id();
        card.x += 24.0;
        card.y += 24.0;
        self.selected = Some(card.id);
        self.board.cards.push(card);
        true
    }
    /// The middle of what's on screen, in the board's own coordinates.
    fn view_center(&self) -> Pos2 {
        if self.canvas.is_positive() { self.world(self.canvas.center()) } else { Pos2::ZERO }
    }
    fn fit_all(&mut self) {
        if self.board.cards.is_empty() {
            self.pan = Vec2::ZERO;
            self.zoom = 1.0;
            return;
        }
        let cards = &self.board.cards;
        let min_x = cards.iter().map(|c| c.x).fold(f32::INFINITY, f32::min);
        let min_y = cards.iter().map(|c| c.y).fold(f32::INFINITY, f32::min);
        let max_x = cards.iter().map(|c| c.x + c.w).fold(f32::NEG_INFINITY, f32::max);
        let max_y = cards.iter().map(|c| c.y + c.h).fold(f32::NEG_INFINITY, f32::max);
        self.zoom = ((self.canvas.width() - 80.0) / (max_x - min_x).max(1.0)).min((self.canvas.height() - 80.0) / (max_y - min_y).max(1.0)).clamp(0.1, 2.0);
        self.pan = -vec2((min_x + max_x) * 0.5, (min_y + max_y) * 0.5) * self.zoom;
    }
    fn save_board(&mut self) -> Result<(), String> {
        if !self.dirty {
            return Ok(());
        }
        fs::create_dir_all(&self.place.projects_dir).map_err(|e| e.to_string())?;
        self.board.saved_at = now();
        self.board.view = Some([self.pan.x, self.pan.y, self.zoom]);
        let target = board_path(&self.place.projects_dir, &self.board.id);
        let temp = target.with_extension("json.tmp");
        fs::write(
            &temp,
            serde_json::to_vec_pretty(&self.board).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        fs::rename(&temp, &target).map_err(|e| e.to_string())?;
        self.boards = list_boards(&self.place.projects_dir);
        self.dirty = false;
        Ok(())
    }
    fn switch(&mut self, id: &str) -> Result<(), String> {
        self.save()?;
        self.board = read_board(&board_path(&self.place.projects_dir, id))?;
        self.selected = None;
        (self.pan, self.zoom) = view_of(&self.board);
        self.textures.clear();
        self.playing = None;
        self.fresh_history();
        Ok(())
    }
    fn add_note(&mut self, at: Pos2, ui: &Ui) {
        let id = self.next_id();
        self.board.cards.push(Card {
            id,
            x: at.x,
            y: at.y,
            w: 240.0,
            h: 190.0,
            kind: CardKind::Note {
                text: String::new(),
            },
        });
        self.selected = Some(id);
        self.mark(ui);
    }
    fn next_id(&self) -> u64 {
        self.board
            .cards
            .iter()
            .map(|c| c.id)
            .chain(self.pending.iter().map(|(id, _)| *id))
            .max()
            .unwrap_or(0)
            + 1
    }
    fn add_images(&mut self, files: Vec<SharedFile>, at: Pos2) -> Receipt {
        let mut receipt = Receipt::default();
        let asset_dir = self.place.projects_dir.join("assets").join(&self.board.id);
        if let Err(e) = fs::create_dir_all(&asset_dir) {
            receipt.note = Some(e.to_string());
            return receipt;
        }
        for file in files {
            if !is_image(&file) {
                receipt.skipped.push(file.name);
                continue;
            }
            let Ok((width, height)) = image::image_dimensions(&file.path) else {
                receipt.skipped.push(file.name);
                continue;
            };
            let ext = file
                .path
                .extension()
                .and_then(|x| x.to_str())
                .unwrap_or("png")
                .to_ascii_lowercase();
            let id = self.next_id();
            let name = format!("{id}.{ext}");
            let target = asset_dir.join(&name);
            if fs::copy(&file.path, &target).is_err() {
                receipt.skipped.push(file.name);
                continue;
            }
            let w = 280.0;
            let h = IMAGE_HEADER + (w * height as f32 / width.max(1) as f32).clamp(80.0, 440.0);
            self.board.cards.push(Card {
                id,
                x: at.x + receipt.accepted as f32 * 28.0,
                y: at.y + receipt.accepted as f32 * 28.0,
                w,
                h,
                kind: CardKind::Image {
                    file: format!("assets/{}/{name}", self.board.id),
                    name: file.name,
                },
            });
            receipt.accepted += 1;
        }
        if receipt.accepted > 0 {
            self.record();
            self.dirty = true;
            self.last_edit = 0.0;
        }
        receipt
    }
    fn add_videos(&mut self, files: Vec<SharedFile>, at: Pos2) -> Receipt {
        let mut receipt = Receipt::default();
        let asset_dir = self.place.projects_dir.join("assets").join(&self.board.id);
        if let Err(e) = fs::create_dir_all(&asset_dir) {
            receipt.note = Some(e.to_string());
            return receipt;
        }
        for file in files {
            if !is_video(&file) || !file.path.is_file() {
                receipt.skipped.push(file.name);
                continue;
            }
            let id = self.next_id();
            let ext = file
                .path
                .extension()
                .and_then(|x| x.to_str())
                .unwrap_or("mp4")
                .to_ascii_lowercase();
            let relative = format!("assets/{}/{}.{}", self.board.id, id, ext);
            let thumb_relative = format!("assets/{}/{}-poster.png", self.board.id, id);
            let target = self.place.projects_dir.join(&relative);
            let thumb_target = self.place.projects_dir.join(&thumb_relative);
            let source = file.path;
            let name = file.name;
            let place = pos2(
                at.x + receipt.accepted as f32 * 28.0,
                at.y + receipt.accepted as f32 * 28.0,
            );
            let (sender, receiver) = mpsc::channel();
            std::thread::spawn(move || {
                let result = (|| -> Result<Card, String> {
                    let media = ffmpeg::probe(&source)?;
                    if media.kind != MediaKind::Video || media.width == 0 || media.height == 0 {
                        return Err(format!("{name} has no readable video picture"));
                    }
                    fs::copy(&source, &target).map_err(|e| format!("Couldn't copy {name}: {e}"))?;
                    let decode_w = media.width.min(640).max(2);
                    let decode_h = ((media.height as f64 * decode_w as f64 / media.width as f64)
                        .round() as u32)
                        .clamp(2, 720);
                    let thumb = ffmpeg::still(&target, 0.0, decode_w, decode_h, Picture::default())
                        .ok()
                        .and_then(|frame| {
                            image::RgbaImage::from_raw(
                                frame.size[0] as u32,
                                frame.size[1] as u32,
                                frame.rgba,
                            )
                        })
                        .and_then(|image| image.save(&thumb_target).ok().map(|_| thumb_relative));
                    let w = 300.0;
                    let h = IMAGE_HEADER
                        + (w * media.height as f32 / media.width as f32).clamp(100.0, 450.0);
                    Ok(Card {
                        id,
                        x: place.x,
                        y: place.y,
                        w,
                        h,
                        kind: CardKind::Video {
                            file: relative,
                            name,
                            thumb,
                            duration: media.duration,
                            width: media.width,
                            height: media.height,
                        },
                    })
                })();
                let _ = sender.send(result);
            });
            self.pending.push((id, receiver));
            receipt.accepted += 1;
        }
        if receipt.accepted > 0 {
            receipt.note = Some(format!("Importing {} video(s)…", receipt.accepted));
        }
        receipt
    }
    fn finish_imports(&mut self) -> Result<(), String> {
        let mut first_error = None;
        for (_, receiver) in std::mem::take(&mut self.pending) {
            match receiver.recv() {
                Ok(Ok(card)) => {
                    self.record();
                    self.board.cards.push(card);
                    self.dirty = true;
                    self.last_edit = 0.0;
                }
                Ok(Err(error)) => {
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
                Err(error) => {
                    if first_error.is_none() {
                        first_error = Some(error.to_string());
                    }
                }
            }
        }
        first_error.map_or(Ok(()), Err)
    }
    fn poll_imports(&mut self, ui: &Ui, host: &mut dyn Host) {
        let mut waiting = Vec::new();
        for (id, receiver) in std::mem::take(&mut self.pending) {
            match receiver.try_recv() {
                Ok(Ok(card)) => {
                    host.toast(format!(
                        "Added {} to Moodie",
                        match &card.kind {
                            CardKind::Video { name, .. } => name.as_str(),
                            _ => "video",
                        }
                    ));
                    self.board.cards.push(card);
                    self.mark(ui);
                }
                Ok(Err(error)) => host.toast(error),
                Err(TryRecvError::Empty) => waiting.push((id, receiver)),
                Err(TryRecvError::Disconnected) => {
                    host.toast("Video import stopped unexpectedly".into())
                }
            }
        }
        self.pending = waiting;
        if !self.pending.is_empty() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(200));
        }
    }
    fn toggle_video(&mut self, id: u64) {
        let Some(card) = self.board.cards.iter().find(|c| c.id == id) else {
            return;
        };
        let CardKind::Video {
            file,
            duration,
            width,
            height,
            ..
        } = &card.kind
        else {
            return;
        };
        let path = self.place.projects_dir.join(file);
        let decode_w = width.min(&640).max(&2).div_ceil(2) * 2;
        let decode_h = ((*height as f64 * decode_w as f64 / (*width).max(1) as f64).round() as u32)
            .clamp(2, 720)
            .div_ceil(2)
            * 2;
        if let Some(playing) = &mut self.playing {
            if playing.id == id {
                if let Some(started) = playing.started.take() {
                    playing.elapsed += started.elapsed().as_secs_f64();
                    playing.stream = None;
                    playing.ahead = None;
                } else {
                    playing.stream = Some(FrameStream::start(
                        &path,
                        playing.elapsed,
                        24,
                        decode_w,
                        decode_h,
                        Picture::default(),
                        1.0,
                        None,
                    ));
                    playing.started = Some(Instant::now());
                }
                return;
            }
        }
        self.playing = Some(PlayingVideo {
            id,
            stream: Some(FrameStream::start(
                &path,
                0.0,
                24,
                decode_w,
                decode_h,
                Picture::default(),
                1.0,
                None,
            )),
            ahead: None,
            texture: None,
            elapsed: 0.0,
            started: Some(Instant::now()),
            duration: *duration,
        });
    }
    fn advance_video(&mut self, ui: &Ui) {
        let Some(playing) = &mut self.playing else {
            return;
        };
        let Some(started) = playing.started else {
            return;
        };
        let position = playing.elapsed + started.elapsed().as_secs_f64();
        let mut latest = None;
        let mut ended = false;
        loop {
            let frame = if let Some(ahead) = playing.ahead.take() {
                Ok(ahead)
            } else {
                playing.stream.as_ref().unwrap().frames.try_recv()
            };
            match frame {
                Ok(frame) if frame.time <= position + 0.025 => latest = Some(frame),
                Ok(frame) => {
                    playing.ahead = Some(frame);
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    ended = true;
                    break;
                }
            }
        }
        if let Some(frame) = latest {
            let image = egui::ColorImage::from_rgba_unmultiplied(frame.size, &frame.rgba);
            match &mut playing.texture {
                Some(texture) => texture.set(image, TextureOptions::LINEAR),
                None => {
                    playing.texture = Some(ui.ctx().load_texture(
                        format!("moodie-video-{}", playing.id),
                        image,
                        TextureOptions::LINEAR,
                    ))
                }
            }
        }
        if ended || (playing.duration > 0.0 && position >= playing.duration) {
            playing.stream = None;
            playing.started = None;
            playing.elapsed = 0.0;
            playing.ahead = None;
        } else {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(33));
        }
    }
    /// Moves the open board's file to the Recycle Bin and shows the next board (or a new one).
    fn delete_board(&mut self, host: &mut dyn Host) {
        let file = board_path(&self.place.projects_dir, &self.board.id);
        if file.is_file() {
            if let Err(error) = canopy_ui::canopy_core::files::recycle(&file) {
                host.toast(error.0);
                return;
            }
        }
        host.toast(format!("Moved {} to the Recycle Bin", self.board.name));
        self.boards = list_boards(&self.place.projects_dir);
        self.board = self.boards.first().cloned().unwrap_or_else(|| Board::new("Untitled board".into()));
        (self.pan, self.zoom) = view_of(&self.board);
        self.selected = None;
        self.playing = None;
        self.textures.clear();
        self.dirty = false;
        self.fresh_history();
    }
    fn texture(&mut self, ui: &Ui, file: &str) -> Option<&TextureHandle> {
        self.textures
            .entry(file.to_owned())
            .or_insert_with(|| {
                let path = self.place.projects_dir.join(file);
                let picture = image::open(path).ok()?.thumbnail(1600, 1600).to_rgba8();
                let image = egui::ColorImage::from_rgba_unmultiplied(
                    [picture.width() as usize, picture.height() as usize],
                    picture.as_raw(),
                );
                Some(ui.ctx().load_texture(
                    format!("moodboard-{}-{file}", self.board.id),
                    image,
                    TextureOptions::LINEAR,
                ))
            })
            .as_ref()
    }
}

impl CanopyApp for Moodboard {
    fn ui(&mut self, ui: &mut Ui, host: &mut dyn Host) {
        self.poll_imports(ui, host);
        self.advance_video(ui);
        let whole = ui.max_rect();
        let toolbar = Rect::from_min_size(whole.min, vec2(whole.width(), 76.0));
        let inspector_width = if self.selected.is_some() { 252.0 } else { 0.0 };
        self.canvas = Rect::from_min_max(
            pos2(whole.left(), toolbar.bottom()),
            pos2(whole.right() - inspector_width, whole.bottom()),
        );
        ui.painter()
            .rect_filled(toolbar, 0.0, Color32::from_rgb(29, 26, 18));
        let mut delete_board = false;
        let mut history = None;
        ui.scope_builder(
            UiBuilder::new().max_rect(toolbar.shrink2(vec2(10.0, 4.0))),
            |ui| {
                ui.horizontal_centered(|ui| {
                    let mut switch = None;
                    egui::ComboBox::from_id_salt("moodboard-list")
                        .selected_text(&self.board.name)
                        .width(140.0)
                        .show_ui(ui, |ui| {
                            for board in &self.boards {
                                if ui
                                    .selectable_label(board.id == self.board.id, &board.name)
                                    .clicked()
                                {
                                    switch = Some(board.id.clone());
                                }
                            }
                        });
                    if let Some(id) = switch {
                        if let Err(e) = self.switch(&id) {
                            self.error = Some(e);
                        }
                    }
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut self.board.name)
                                .desired_width(140.0)
                                .hint_text("Board name"),
                        )
                        .changed()
                    {
                        self.mark(ui);
                    }
                    if ui.button("New board").clicked() {
                        match self.save() {
                            Ok(()) => {
                                self.board = Board::new("Untitled board".into());
                                self.selected = None;
                                self.pan = Vec2::ZERO;
                                self.zoom = 1.0;
                                self.textures.clear();
                                self.playing = None;
                                self.fresh_history();
                                self.dirty = true;
                                self.last_edit = 0.0;
                            }
                            Err(e) => self.error = Some(e),
                        }
                    }
                    if ui.button("Delete board").on_hover_text("Moves this board to the Recycle Bin; its pictures and videos stay in the space").clicked() {
                        delete_board = true;
                    }
                    if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("Undo")).on_hover_text("Ctrl+Z").clicked() {
                        history = Some(true);
                    }
                    if ui.add_enabled(!self.redo.is_empty(), egui::Button::new("Redo")).on_hover_text("Ctrl+Y").clicked() {
                        history = Some(false);
                    }
                });
                ui.horizontal_centered(|ui| {
                    if ui.button("+ Note").on_hover_text("Or double-click the board").clicked() {
                        let at = self.view_center() - vec2(120.0, 95.0);
                        self.add_note(at, ui);
                    }
                    if ui.button("+ Images").clicked() {
                        if let Some(paths) = rfd::FileDialog::new()
                            .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
                            .pick_files()
                        {
                            let files = paths.into_iter().map(SharedFile::new).collect();
                            let at = self.view_center() - vec2(140.0, 100.0);
                            let receipt = self.add_images(files, at);
                            if receipt.accepted > 0 {
                                host.toast(format!("Added {} image(s)", receipt.accepted));
                            }
                        }
                    }
                    if ui.button("+ Videos").clicked() {
                        if let Some(paths) = rfd::FileDialog::new()
                            .add_filter("Videos", &["mp4", "mov", "webm", "mkv", "m4v", "avi"])
                            .pick_files()
                        {
                            let at = self.view_center() - vec2(150.0, 100.0);
                            let receipt = self.add_videos(paths.into_iter().map(SharedFile::new).collect(), at);
                            if let Some(note) = receipt.note {
                                host.toast(note);
                            }
                        }
                    }
                    if ui.button("Fit all").on_hover_text("F").clicked() {
                        self.fit_all();
                    }
                    if ui.button("100%").on_hover_text("Back to actual size").clicked() {
                        let middle = self.view_center();
                        self.zoom = 1.0;
                        self.pan = -middle.to_vec2();
                    }
                    ui.label(format!("{}%", (self.zoom * 100.0).round()));
                    ui.label("Wheel zooms · middle drag or Space+drag pans · double-click adds a note");
                });
            },
        );
        let painter = ui.painter().with_clip_rect(self.canvas);
        painter.rect_filled(self.canvas, 0.0, Color32::from_rgb(20, 23, 19));
        let step = (80.0 * self.zoom).max(12.0);
        let origin = self.canvas.center() + self.pan;
        let mut x = self.canvas.left() + (origin.x - self.canvas.left()).rem_euclid(step);
        while x < self.canvas.right() {
            let mut y = self.canvas.top() + (origin.y - self.canvas.top()).rem_euclid(step);
            while y < self.canvas.bottom() {
                painter.circle_filled(pos2(x, y), 1.0, Color32::from_rgb(57, 66, 56));
                y += step;
            }
            x += step;
        }
        let canvas_response = ui.interact(
            self.canvas,
            ui.scope_id().with("moodboard-canvas"),
            Sense::click_and_drag(),
        );
        let space_down = ui.input(|i| i.key_down(egui::Key::Space));
        if canvas_response.dragged_by(egui::PointerButton::Middle)
            || canvas_response.dragged_by(egui::PointerButton::Secondary)
            || (space_down && canvas_response.dragged())
        {
            self.pan += canvas_response.drag_delta();
        }
        if canvas_response.clicked() {
            self.selected = None;
        }
        if canvas_response.double_clicked() {
            if let Some(at) = canvas_response.interact_pointer_pos() {
                let at = self.world(at) - vec2(120.0, 20.0);
                self.add_note(at, ui);
            }
        }
        if canvas_response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                let pointer = ui
                    .input(|i| i.pointer.hover_pos())
                    .unwrap_or(self.canvas.center());
                let before = self.world(pointer);
                self.zoom = (self.zoom * (scroll * 0.0015).exp()).clamp(0.1, 4.0);
                self.pan = pointer - self.canvas.center() - before.to_vec2() * self.zoom;
            }
        }
        if self.board.cards.is_empty() {
            painter.text(
                self.canvas.center(),
                Align2::CENTER_CENTER,
                "Add images, videos and notes to start your board",
                font::display(18.0),
                color::INK_MUTED,
            );
        }
        let mut changed = false;
        let mut pick = None;
        let mut play = None;
        for index in 0..self.board.cards.len() {
            let card = self.board.cards[index].clone();
            let top_left = self.screen(card.x, card.y);
            let rect = Rect::from_min_size(top_left, vec2(card.w, card.h) * self.zoom);
            if !rect.intersects(self.canvas) {
                continue;
            }
            let header =
                Rect::from_min_size(rect.min, vec2(rect.width(), IMAGE_HEADER * self.zoom));
            let selected = self.selected == Some(card.id);
            let fill = match card.kind {
                CardKind::Note { .. } => Color32::from_rgb(60, 56, 38),
                CardKind::Image { .. } => Color32::from_rgb(35, 38, 34),
                CardKind::Video { .. } => Color32::from_rgb(27, 36, 35),
            };
            painter.rect_filled(rect, 7.0, fill);
            painter.rect_stroke(
                rect,
                7.0,
                Stroke::new(
                    if selected { 2.0 } else { 1.0 },
                    if selected {
                        color::LEAF
                    } else {
                        color::HAIRLINE_STRONG
                    },
                ),
                StrokeKind::Inside,
            );
            match &card.kind {
                CardKind::Note { text } => {
                    let body = Rect::from_min_max(
                        pos2(
                            rect.left() + 12.0 * self.zoom,
                            header.bottom() + 6.0 * self.zoom,
                        ),
                        rect.max - vec2(10.0, 8.0) * self.zoom,
                    );
                    let galley = painter.layout(
                        if text.is_empty() {
                            "Write a note…".into()
                        } else {
                            text.clone()
                        },
                        font::ui(15.0 * self.zoom),
                        color::INK,
                        body.width(),
                    );
                    painter
                        .with_clip_rect(body)
                        .galley(body.min, galley, color::INK);
                }
                CardKind::Image { file, .. } => {
                    if let Some(texture) = self.texture(ui, file) {
                        let image_rect = Rect::from_min_max(
                            pos2(rect.left() + 5.0, header.bottom() + 4.0),
                            rect.max - vec2(5.0, 5.0),
                        );
                        painter.image(
                            texture.id(),
                            image_rect,
                            Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }
                }
                CardKind::Video { thumb, .. } => {
                    let texture = self
                        .playing
                        .as_ref()
                        .filter(|p| p.id == card.id)
                        .and_then(|p| p.texture.clone())
                        .or_else(|| {
                            thumb
                                .as_ref()
                                .and_then(|path| self.texture(ui, path).cloned())
                        });
                    if let Some(texture) = texture {
                        let image_rect = Rect::from_min_max(
                            pos2(rect.left() + 5.0, header.bottom() + 4.0),
                            rect.max - vec2(5.0, 5.0),
                        );
                        painter.image(
                            texture.id(),
                            image_rect,
                            Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }
                    let control = Rect::from_center_size(
                        pos2(rect.center().x, (header.bottom() + rect.bottom()) * 0.5),
                        vec2(42.0, 42.0),
                    );
                    painter.circle_filled(control.center(), 21.0, Color32::from_black_alpha(190));
                    let playing = self
                        .playing
                        .as_ref()
                        .is_some_and(|p| p.id == card.id && p.started.is_some());
                    painter.text(
                        control.center(),
                        Align2::CENTER_CENTER,
                        if playing { "Ⅱ" } else { "▶" },
                        font::display(20.0),
                        color::INK,
                    );
                }
            }
            let label = match &card.kind {
                CardKind::Note { .. } => "Note",
                CardKind::Image { name, .. } => name.as_str(),
                CardKind::Video { name, .. } => name.as_str(),
            };
            painter.with_clip_rect(header.shrink(5.0)).text(
                header.left_center() + vec2(8.0, 0.0),
                Align2::LEFT_CENTER,
                label,
                font::ui((12.0 * self.zoom).max(8.0)),
                color::INK_SOFT,
            );
            let body_response = ui.interact(
                rect,
                ui.scope_id().with(("moodboard-card-body", card.id)),
                Sense::click_and_drag(),
            );
            if body_response.clicked() {
                pick = Some(card.id);
            }
            if body_response.dragged_by(egui::PointerButton::Primary) && !space_down {
                let delta = body_response.drag_delta() / self.zoom;
                self.board.cards[index].x += delta.x;
                self.board.cards[index].y += delta.y;
                pick = Some(card.id);
                changed = true;
            }
            let response = ui.interact(
                header,
                ui.scope_id().with(("moodboard-card", card.id)),
                Sense::click_and_drag(),
            );
            if response.clicked() {
                pick = Some(card.id);
            }
            if response.dragged() {
                let delta = response.drag_delta() / self.zoom;
                self.board.cards[index].x += delta.x;
                self.board.cards[index].y += delta.y;
                pick = Some(card.id);
                changed = true;
            }
            {
                let grip =
                    Rect::from_center_size(rect.right_bottom() - vec2(9.0, 9.0), vec2(18.0, 18.0));
                painter.rect_filled(grip, 4.0, color::LEAF);
                painter.line_segment(
                    [
                        grip.left_bottom() + vec2(5.0, -3.0),
                        grip.right_top() + vec2(-3.0, 5.0),
                    ],
                    Stroke::new(1.5, color::CTA),
                );
                let resize = ui
                    .interact(
                        grip,
                        ui.scope_id().with(("moodboard-resize", card.id)),
                        Sense::click_and_drag(),
                    )
                    .on_hover_cursor(egui::CursorIcon::ResizeNwSe)
                    .on_hover_text("Drag to resize");
                if resize.clicked() || resize.dragged() {
                    pick = Some(card.id);
                }
                if resize.dragged() {
                    let (w, h) = if matches!(&card.kind, CardKind::Note { .. }) {
                        let delta = resize.drag_delta() / self.zoom.max(0.1);
                        ((card.w + delta.x).clamp(120.0, 800.0), (card.h + delta.y).clamp(90.0, 800.0))
                    } else {
                        resized_image(card.w, card.h, resize.drag_delta(), self.zoom)
                    };
                    self.board.cards[index].w = w;
                    self.board.cards[index].h = h;
                    changed = true;
                }
            }
            if matches!(&card.kind, CardKind::Video { .. }) {
                let control = Rect::from_center_size(
                    pos2(rect.center().x, (header.bottom() + rect.bottom()) * 0.5),
                    vec2(42.0, 42.0),
                );
                if ui
                    .interact(
                        control,
                        ui.scope_id().with(("moodie-play", card.id)),
                        Sense::click(),
                    )
                    .on_hover_text("Play or pause video")
                    .clicked()
                {
                    play = Some(card.id);
                    pick = Some(card.id);
                }
            }
        }
        if let Some(id) = pick {
            self.selected = Some(id);
            // What you pick up comes to the front.
            if let Some(at) = self.board.cards.iter().position(|c| c.id == id).filter(|at| at + 1 != self.board.cards.len()) {
                let card = self.board.cards.remove(at);
                self.board.cards.push(card);
                // Saved, but not an undo step of its own.
                self.dirty = true;
                self.last_edit = ui.input(|i| i.time);
            }
        }
        // Keys, while this pane has the keyboard and no text field does.
        if host.focused() && !ui.ctx().memory(|m| m.focused().is_some()) {
            use egui::{Key, Modifiers};
            let (undo, redo, duplicate, delete, escape, fit, nudge, pasted) = ui.input_mut(|i| {
                let redo = i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z) || i.consume_key(Modifiers::COMMAND, Key::Y);
                let undo = !redo && i.consume_key(Modifiers::COMMAND, Key::Z);
                let step = if i.modifiers.shift { 10.0 } else { 1.0 };
                let mut nudge = Vec2::ZERO;
                for (key, by) in [(Key::ArrowLeft, vec2(-1.0, 0.0)), (Key::ArrowRight, vec2(1.0, 0.0)), (Key::ArrowUp, vec2(0.0, -1.0)), (Key::ArrowDown, vec2(0.0, 1.0))] {
                    if i.key_pressed(key) {
                        nudge += by * step;
                    }
                }
                let pasted = i.events.iter().find_map(|event| match event {
                    egui::Event::Paste(text) if !text.trim().is_empty() => Some(text.clone()),
                    _ => None,
                });
                (undo, redo, i.consume_key(Modifiers::COMMAND, Key::D), i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace), i.key_pressed(Key::Escape), i.consume_key(Modifiers::NONE, Key::F), nudge, pasted)
            });
            if undo {
                history = Some(true);
            }
            if redo {
                history = Some(false);
            }
            if duplicate && self.duplicate_selected() {
                changed = true;
            }
            if delete && self.remove_selected() {
                changed = true;
            }
            if escape {
                self.selected = None;
            }
            if fit {
                self.fit_all();
            }
            if nudge != Vec2::ZERO {
                if let Some(card) = self.board.cards.iter_mut().find(|c| Some(c.id) == self.selected) {
                    card.x += nudge.x;
                    card.y += nudge.y;
                    changed = true;
                }
            }
            // Text on the clipboard lands as a note in the middle of the view.
            if let Some(text) = pasted {
                let at = self.view_center() - vec2(120.0, 95.0);
                self.add_note(at, ui);
                if let Some(CardKind::Note { text: note }) = self.board.cards.last_mut().map(|c| &mut c.kind) {
                    *note = text;
                }
            }
        }
        if let Some(back) = history {
            if !self.step(back) {
                host.toast(if back { "Nothing to undo".into() } else { "Nothing to redo".into() });
            }
        }
        if delete_board {
            self.delete_board(host);
        }
        if let Some(id) = play {
            self.toggle_video(id);
        }
        if changed {
            self.mark(ui);
        }
        if self.selected.is_some() {
            let side = Rect::from_min_max(pos2(self.canvas.right(), toolbar.bottom()), whole.max);
            ui.painter()
                .rect_filled(side, 0.0, Color32::from_rgb(29, 30, 24));
            ui.scope_builder(UiBuilder::new().max_rect(side.shrink(12.0)), |ui| {
                let mut changed = false;
                let mut remove = false;
                let mut duplicate = false;
                if let Some(card) = self
                    .board
                    .cards
                    .iter_mut()
                    .find(|c| Some(c.id) == self.selected)
                {
                    ui.heading(match card.kind {
                        CardKind::Note { .. } => "Note",
                        CardKind::Image { .. } => "Image",
                        CardKind::Video { .. } => "Video",
                    });
                    ui.add_space(8.0);
                    if let CardKind::Note { text } = &mut card.kind {
                        ui.label("Text");
                        changed |= ui
                            .add(
                                egui::TextEdit::multiline(text)
                                    .desired_width(225.0)
                                    .desired_rows(10),
                            )
                            .changed();
                    }
                    if let CardKind::Video { file, duration, .. } = &card.kind {
                        ui.label(format!(
                            "Length: {}:{:02}",
                            *duration as u64 / 60,
                            *duration as u64 % 60
                        ));
                        ui.label("Play on the canvas, or open with sound.");
                        if ui.button("Open with sound").clicked() {
                            if let Err(error) =
                                open_with_default_player(&self.place.projects_dir.join(file))
                            {
                                host.toast(error);
                            }
                        }
                    }
                    ui.add_space(8.0);
                    ui.label("Size");
                    if matches!(&card.kind, CardKind::Image { .. } | CardKind::Video { .. }) {
                        ui.label("Drag the green corner to resize.");
                        let ratio = (card.h - IMAGE_HEADER).max(1.0) / card.w.max(1.0);
                        if ui
                            .add(egui::Slider::new(&mut card.w, 120.0..=1600.0).text("Width"))
                            .changed()
                        {
                            card.h = IMAGE_HEADER + card.w * ratio;
                            changed = true;
                        }
                    } else {
                        changed |= ui
                            .add(egui::Slider::new(&mut card.w, 120.0..=800.0).text("Width"))
                            .changed();
                        changed |= ui
                            .add(egui::Slider::new(&mut card.h, 90.0..=800.0).text("Height"))
                            .changed();
                    }
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        duplicate = ui.button("Duplicate").on_hover_text("Ctrl+D").clicked();
                        remove = ui.button("Delete item").on_hover_text("Delete").clicked();
                    });
                    ui.add_space(8.0);
                    ui.label(canopy_ui::muted("Arrows nudge (Shift: 10). Ctrl+Z undoes."));
                }
                if duplicate {
                    changed |= self.duplicate_selected();
                }
                if remove {
                    changed |= self.remove_selected();
                }
                if changed {
                    self.mark(ui);
                }
            });
        }
        self.settle();
        let time = ui.input(|i| i.time);
        if self.dirty && (self.last_edit == 0.0 || time - self.last_edit > 0.8) {
            if let Err(e) = self.save_board() {
                self.error = Some(e);
            }
        }
        if let Some(error) = self.error.take() {
            host.toast(format!("Moodie couldn't save: {error}"));
        }
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(400));
    }
    fn receive(&mut self, files: Vec<SharedFile>, host: &mut dyn Host) -> Receipt {
        self.receive_at(files, self.canvas.center(), host)
    }
    fn receive_at(
        &mut self,
        files: Vec<SharedFile>,
        position: Pos2,
        _host: &mut dyn Host,
    ) -> Receipt {
        let at = if self.canvas.is_positive() {
            self.world(position)
        } else {
            Pos2::ZERO
        };
        let mut images = Vec::new();
        let mut videos = Vec::new();
        let mut skipped = Vec::new();
        for file in files {
            if is_image(&file) {
                images.push(file);
            } else if is_video(&file) {
                videos.push(file);
            } else {
                skipped.push(file.name);
            }
        }
        let mut receipt = self.add_images(images, at);
        let video = self.add_videos(videos, at);
        receipt.accepted += video.accepted;
        receipt.skipped.extend(video.skipped);
        receipt.skipped.extend(skipped);
        receipt.note = video.note.or(receipt.note);
        receipt
    }
    fn open_document(&mut self, id: &str, host: &mut dyn Host) {
        if let Err(e) = self.switch(id) {
            host.toast(format!("Couldn't open Moodie board: {e}"));
        }
    }
    fn save(&mut self) -> Result<(), String> {
        let imported = self.finish_imports();
        let saved = self.save_board();
        imported.and(saved)
    }
    fn title(&self) -> String {
        self.board.name.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mouse_resize_keeps_photo_proportions_at_any_zoom() {
        let original = (280.0, IMAGE_HEADER + 140.0);
        let normal = resized_image(original.0, original.1, vec2(50.0, 25.0), 1.0);
        let zoomed = resized_image(original.0, original.1, vec2(100.0, 50.0), 2.0);
        assert_eq!(normal, zoomed);
        assert_eq!(normal, (330.0, IMAGE_HEADER + 165.0));
        let small = resized_image(original.0, original.1, vec2(-1000.0, -500.0), 1.0);
        assert_eq!(small.0, 120.0);
        assert_eq!((small.1 - IMAGE_HEADER) / small.0, 0.5);
    }
    #[test]
    fn undo_and_redo_step_through_edits_and_a_board_reopens_where_it_was_left() {
        let dir = std::env::temp_dir().join(format!("canopy-moodie-undo-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let place = Place { space: None, projects_dir: dir.join("boards"), data_dir: dir.join("data"), exports_dir: dir.join("exports") };
        let mut app = Moodboard::new(place.clone());
        let note = |id| Card { id, x: 0.0, y: 0.0, w: 240.0, h: 190.0, kind: CardKind::Note { text: String::new() } };
        // Two edits apart in time are two steps; a duplicate right after the second joins it.
        app.record();
        app.board.cards.push(note(1));
        app.edited = None;
        app.settle();
        app.record();
        app.board.cards.push(note(2));
        app.selected = Some(2);
        app.record();
        assert!(app.duplicate_selected());
        assert_eq!(app.board.cards.len(), 3);
        assert!(app.step(true));
        assert_eq!(app.board.cards.len(), 1);
        assert!(app.step(true));
        assert!(app.board.cards.is_empty() && app.selected.is_none());
        assert!(!app.step(true));
        assert!(app.step(false) && app.step(false));
        assert_eq!(app.board.cards.iter().map(|c| c.id).collect::<Vec<_>>(), [1, 2, 3]);
        assert!(app.remove_selected() || app.selected.is_none());

        app.pan = vec2(120.0, -40.0);
        app.zoom = 1.5;
        app.dirty = true;
        app.save_board().unwrap();
        let again = Moodboard::new(place);
        assert_eq!((again.pan, again.zoom), (vec2(120.0, -40.0), 1.5));
        let _ = fs::remove_dir_all(dir);
    }
    #[test]
    fn board_roundtrip_preserves_world_positions_and_notes() {
        let board = Board {
            version: 1,
            id: "test".into(),
            name: "Ideas".into(),
            saved_at: 1,
            view: None,
            cards: vec![Card {
                id: 1,
                x: -4200.0,
                y: 9000.0,
                w: 240.0,
                h: 190.0,
                kind: CardKind::Note {
                    text: "Try warm colors".into(),
                },
            }],
        };
        let copy: Board = serde_json::from_slice(&serde_json::to_vec(&board).unwrap()).unwrap();
        assert_eq!(copy.cards[0].x, -4200.0);
        assert!(
            matches!(&copy.cards[0].kind, CardKind::Note { text } if text == "Try warm colors")
        );
    }
    #[test]
    fn repeated_saves_replace_the_same_board_file() {
        let dir = std::env::temp_dir().join(format!(
            "canopy-moodboard-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let place = Place {
            space: None,
            projects_dir: dir.join("boards"),
            data_dir: dir.join("data"),
            exports_dir: dir.join("exports"),
        };
        let mut app = Moodboard::new(place);
        app.board.name = "First".into();
        app.dirty = true;
        app.save_board().unwrap();
        app.board.name = "Second".into();
        app.dirty = true;
        app.save_board().unwrap();
        let loaded = read_board(&board_path(&app.place.projects_dir, &app.board.id)).unwrap();
        assert_eq!(loaded.name, "Second");
        let _ = fs::remove_dir_all(dir);
    }
    #[test]
    fn importing_an_image_copies_it_into_the_space() {
        let dir = std::env::temp_dir().join(format!(
            "canopy-moodboard-image-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let source = dir.join("sample.png");
        image::RgbaImage::from_pixel(12, 6, image::Rgba([200, 80, 30, 255]))
            .save(&source)
            .unwrap();
        let place = Place {
            space: None,
            projects_dir: dir.join("boards"),
            data_dir: dir.join("data"),
            exports_dir: dir.join("exports"),
        };
        let mut app = Moodboard::new(place);
        let receipt = app.add_images(vec![SharedFile::new(&source)], pos2(-100.0, 250.0));
        assert_eq!(receipt.accepted, 1);
        let CardKind::Image { file, .. } = &app.board.cards[0].kind else {
            panic!("expected image card")
        };
        assert!(app.place.projects_dir.join(file).is_file());
        assert_eq!(app.board.cards[0].y, 250.0);
        app.save_board().unwrap();
        assert_eq!(
            read_board(&board_path(&app.place.projects_dir, &app.board.id))
                .unwrap()
                .cards
                .len(),
            1
        );
        let _ = fs::remove_dir_all(dir);
    }
    #[test]
    fn importing_a_video_makes_a_playable_card_in_the_space() {
        let dir = std::env::temp_dir().join(format!(
            "canopy-moodie-video-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let source = dir.join("sample.mp4");
        let status = ffmpeg::command(&ffmpeg::ffmpeg())
            .args([
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=red:s=96x54:r=5:d=0.4",
                "-an",
                "-c:v",
                "mpeg4",
            ])
            .arg(&source)
            .status()
            .unwrap();
        assert!(status.success());
        let place = Place {
            space: None,
            projects_dir: dir.join("boards"),
            data_dir: dir.join("data"),
            exports_dir: dir.join("exports"),
        };
        let mut app = Moodboard::new(place);
        assert!(accepts(&SharedFile::new(&source)));
        let receipt = app.add_videos(vec![SharedFile::new(&source)], pos2(50.0, -30.0));
        assert_eq!(receipt.accepted, 1);
        app.save().unwrap();
        let card = &app.board.cards[0];
        assert_eq!((card.x, card.y), (50.0, -30.0));
        let CardKind::Video {
            file,
            thumb,
            duration,
            ..
        } = &card.kind
        else {
            panic!("expected video card")
        };
        assert!(app.place.projects_dir.join(file).is_file());
        assert!(
            app.place
                .projects_dir
                .join(thumb.as_ref().expect("poster"))
                .is_file()
        );
        assert!(*duration > 0.0);
        let stream = FrameStream::start(
            &app.place.projects_dir.join(file),
            0.0,
            5,
            96,
            54,
            Picture::default(),
            1.0,
            None,
        );
        assert_eq!(
            stream
                .frames
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap()
                .size,
            [96, 54]
        );
        assert_eq!(
            read_board(&board_path(&app.place.projects_dir, &app.board.id))
                .unwrap()
                .cards
                .len(),
            1
        );
        let _ = fs::remove_dir_all(dir);
    }
}
