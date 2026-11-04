//! A space-local, native moodboard. Positions are in an unbounded world plane.
use canopy_ui::{
    Accent, AppDescriptor, CanopyApp, Document, Host, Place, Receipt, SharedFile, color, font,
};
use egui::{
    Align2, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, TextureHandle, TextureOptions, Ui,
    UiBuilder, Vec2, pos2, vec2,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

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

fn accepts(file: &SharedFile) -> bool {
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
    Note { text: String },
    Image { file: String, name: String },
}

#[derive(Clone, Serialize, Deserialize)]
struct Board {
    version: u32,
    id: String,
    name: String,
    saved_at: u64,
    cards: Vec<Card>,
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
        }
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
    dirty: bool,
    last_edit: f64,
    error: Option<String>,
}

impl Moodboard {
    fn new(place: Place) -> Self {
        let boards = list_boards(&place.projects_dir);
        let board = boards
            .first()
            .cloned()
            .unwrap_or_else(|| Board::new("Untitled board".into()));
        Self {
            place,
            board,
            boards,
            selected: None,
            pan: Vec2::ZERO,
            zoom: 1.0,
            canvas: Rect::NOTHING,
            textures: HashMap::new(),
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
        self.dirty = true;
        self.last_edit = ui.input(|i| i.time);
    }
    fn save_board(&mut self) -> Result<(), String> {
        if !self.dirty {
            return Ok(());
        }
        fs::create_dir_all(&self.place.projects_dir).map_err(|e| e.to_string())?;
        self.board.saved_at = now();
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
        self.save_board()?;
        self.board = read_board(&board_path(&self.place.projects_dir, id))?;
        self.selected = None;
        self.pan = Vec2::ZERO;
        self.zoom = 1.0;
        self.textures.clear();
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
        self.board.cards.iter().map(|c| c.id).max().unwrap_or(0) + 1
    }
    fn add_images(&mut self, files: Vec<SharedFile>, at: Pos2) -> Receipt {
        let mut receipt = Receipt::default();
        let asset_dir = self.place.projects_dir.join("assets").join(&self.board.id);
        if let Err(e) = fs::create_dir_all(&asset_dir) {
            receipt.note = Some(e.to_string());
            return receipt;
        }
        for file in files {
            if !accepts(&file) {
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
            let h = (w * height as f32 / width.max(1) as f32).clamp(80.0, 440.0);
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
            self.dirty = true;
            self.last_edit = 0.0;
        }
        receipt
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
        let whole = ui.max_rect();
        let toolbar = Rect::from_min_size(whole.min, vec2(whole.width(), 76.0));
        let inspector_width = if self.selected.is_some() { 252.0 } else { 0.0 };
        self.canvas = Rect::from_min_max(
            pos2(whole.left(), toolbar.bottom()),
            pos2(whole.right() - inspector_width, whole.bottom()),
        );
        ui.painter()
            .rect_filled(toolbar, 0.0, Color32::from_rgb(29, 26, 18));
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
                        match self.save_board() {
                            Ok(()) => {
                                self.board = Board::new("Untitled board".into());
                                self.selected = None;
                                self.pan = Vec2::ZERO;
                                self.zoom = 1.0;
                                self.textures.clear();
                                self.mark(ui);
                            }
                            Err(e) => self.error = Some(e),
                        }
                    }
                });
                ui.horizontal_centered(|ui| {
                    if ui.button("+ Note").clicked() {
                        self.add_note(pos2(-100.0, -80.0), ui);
                    }
                    if ui.button("+ Images").clicked() {
                        if let Some(paths) = rfd::FileDialog::new()
                            .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
                            .pick_files()
                        {
                            let files = paths.into_iter().map(SharedFile::new).collect();
                            let receipt = self.add_images(files, pos2(-140.0, -100.0));
                            if receipt.accepted > 0 {
                                host.toast(format!("Added {} image(s)", receipt.accepted));
                            }
                        }
                    }
                    if ui.button("Fit all").clicked() {
                        if !self.board.cards.is_empty() {
                            let min_x = self
                                .board
                                .cards
                                .iter()
                                .map(|c| c.x)
                                .fold(f32::INFINITY, f32::min);
                            let min_y = self
                                .board
                                .cards
                                .iter()
                                .map(|c| c.y)
                                .fold(f32::INFINITY, f32::min);
                            let max_x = self
                                .board
                                .cards
                                .iter()
                                .map(|c| c.x + c.w)
                                .fold(f32::NEG_INFINITY, f32::max);
                            let max_y = self
                                .board
                                .cards
                                .iter()
                                .map(|c| c.y + c.h)
                                .fold(f32::NEG_INFINITY, f32::max);
                            self.zoom = ((self.canvas.width() - 80.0) / (max_x - min_x).max(1.0))
                                .min((self.canvas.height() - 80.0) / (max_y - min_y).max(1.0))
                                .clamp(0.1, 2.0);
                            self.pan =
                                -vec2((min_x + max_x) * 0.5, (min_y + max_y) * 0.5) * self.zoom;
                        } else {
                            self.pan = Vec2::ZERO;
                            self.zoom = 1.0;
                        }
                    }
                    ui.label(format!("{}%", (self.zoom * 100.0).round()));
                    ui.label("Wheel: zoom · middle drag or Space+drag: pan");
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
                "Add images and notes to start your board",
                font::display(18.0),
                color::INK_MUTED,
            );
        }
        let mut changed = false;
        let mut pick = None;
        for index in 0..self.board.cards.len() {
            let card = self.board.cards[index].clone();
            let top_left = self.screen(card.x, card.y);
            let rect = Rect::from_min_size(top_left, vec2(card.w, card.h) * self.zoom);
            if !rect.intersects(self.canvas) {
                continue;
            }
            let header = Rect::from_min_size(rect.min, vec2(rect.width(), 28.0 * self.zoom));
            let selected = self.selected == Some(card.id);
            let fill = match card.kind {
                CardKind::Note { .. } => Color32::from_rgb(60, 56, 38),
                CardKind::Image { .. } => Color32::from_rgb(35, 38, 34),
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
                        if text.is_empty() { "Write a note…".into() } else { text.clone() },
                        font::ui(15.0 * self.zoom), color::INK, body.width(),
                    );
                    painter.with_clip_rect(body).galley(body.min, galley, color::INK);
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
            }
            let label = match &card.kind {
                CardKind::Note { .. } => "Note",
                CardKind::Image { name, .. } => name.as_str(),
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
                Sense::click(),
            );
            if body_response.clicked() {
                pick = Some(card.id);
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
        }
        if let Some(id) = pick {
            self.selected = Some(id);
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
                if let Some(card) = self
                    .board
                    .cards
                    .iter_mut()
                    .find(|c| Some(c.id) == self.selected)
                {
                    ui.heading(match card.kind {
                        CardKind::Note { .. } => "Note",
                        CardKind::Image { .. } => "Image",
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
                    ui.add_space(8.0);
                    ui.label("Size");
                    changed |= ui
                        .add(egui::Slider::new(&mut card.w, 120.0..=800.0).text("Width"))
                        .changed();
                    changed |= ui
                        .add(egui::Slider::new(&mut card.h, 90.0..=800.0).text("Height"))
                        .changed();
                    ui.add_space(12.0);
                    remove = ui.button("Delete item").clicked();
                }
                if remove {
                    self.board.cards.retain(|c| Some(c.id) != self.selected);
                    self.selected = None;
                    changed = true;
                }
                if changed {
                    self.mark(ui);
                }
            });
        }
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
        self.add_images(files, at)
    }
    fn open_document(&mut self, id: &str, host: &mut dyn Host) {
        if let Err(e) = self.switch(id) {
            host.toast(format!("Couldn't open Moodie board: {e}"));
        }
    }
    fn save(&mut self) -> Result<(), String> {
        self.save_board()
    }
    fn title(&self) -> String {
        self.board.name.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn board_roundtrip_preserves_world_positions_and_notes() {
        let board = Board {
            version: 1,
            id: "test".into(),
            name: "Ideas".into(),
            saved_at: 1,
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
}
