//! The shell: "Where to?" (your spaces) and a space (app tabs, the sidebar of projects and files, one or two panes).
//! Every app draws natively inside its pane.

use canopy_core::files::{FileEntry, copy_into, list_files};
use canopy_core::spaces::{Space, SpacePaths, Spaces};
use canopy_ui::{
    Accent, AppDescriptor, CanopyApp, Document, DraggedFiles, Host, Icon, Mood, Row, RowMark, SharedFile, SpaceInfo, color, font, format_size, icon_button,
    leaf, leaf_with_initial, paint_ground, pill, primary_button, quiet_button, section_label,
};
use crate::layout::{self, Layout, Prefs};
use crate::quick::{self, Entry, Mark, Outcome};
use egui::{Align2, CursorIcon, DragAndDrop, Key, Modifiers, Rect, Sense, Ui, UiBuilder, pos2, vec2};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

enum Kind {
    Native(&'static AppDescriptor),
    /// Not rebuilt in Rust yet: the pane offers the app's current exe. Forma, Folio and StreamCuts are all native
    /// now; this stays for an app that joins Canopy before it is.
    #[allow(dead_code)]
    External { exe: PathBuf, accepts: fn(&SharedFile) -> bool },
}

struct Slot {
    name: &'static str,
    description: &'static str,
    accent: Accent,
    initial: &'static str,
    kind: Kind,
}

fn catalog() -> Vec<Slot> {
    vec![
        Slot { name: "Forma", description: "Animate shapes and pictures", accent: Accent::Clay, initial: "Fo",
            kind: Kind::Native(&forma_native::DESCRIPTOR) },
        Slot { name: "Folio", description: "Make carousels", accent: Accent::Sky, initial: "Fl",
            kind: Kind::Native(&folio_native::DESCRIPTOR) },
        Slot { name: "StreamCuts", description: "Edit videos and cut clips", accent: Accent::Berry, initial: "SC", kind: Kind::Native(&streamcuts::DESCRIPTOR) },
        Slot { name: "Moodie", description: "Collect images and notes on an endless canvas", accent: Accent::Leaf, initial: "Mo", kind: Kind::Native(&canopy::moodboard::DESCRIPTOR) },
        Slot { name: "Video Chat", description: "Find Shorts in long videos with an AI producer", accent: Accent::Moss, initial: "VC", kind: Kind::Native(&canopy::videochat::DESCRIPTOR) },
        Slot { name: "File Manager", description: "Your files with titles and descriptions; drag them out to post", accent: Accent::Sky, initial: "FM", kind: Kind::Native(&canopy::filemanager::DESCRIPTOR) },
    ]
}

impl Slot {
    /// The name layouts remember the app by.
    fn id(&self) -> &'static str {
        match &self.kind {
            Kind::Native(d) => d.id,
            Kind::External { .. } => self.name,
        }
    }

    fn accepts(&self, file: &SharedFile) -> bool {
        match &self.kind {
            Kind::Native(d) => (d.accepts)(file),
            Kind::External { accepts, .. } => accepts(file),
        }
    }
}

struct Toast {
    text: String,
    file: Option<PathBuf>,
    targets: Vec<usize>,
    at: Instant,
}

/// What an app inside a pane can ask of the shell.
struct PaneHost<'a> {
    toasts: &'a mut Vec<Toast>,
    slots: &'a [Slot],
    from: usize,
    focused: bool,
    /// Projects an app asked to open in another app (the app's slot, the document), opened once the panes are drawn.
    opens: &'a mut Vec<(usize, String)>,
}

impl Host for PaneHost<'_> {
    fn toast(&mut self, text: String) {
        self.toasts.push(Toast { text, file: None, targets: Vec::new(), at: Instant::now() });
    }
    fn exported(&mut self, path: PathBuf) {
        let file = SharedFile::new(&path);
        let targets = self.slots.iter().enumerate().filter(|(i, s)| *i != self.from && matches!(s.kind, Kind::Native(_)) && s.accepts(&file)).map(|(i, _)| i).collect();
        self.toasts.push(Toast { text: format!("Saved {} to Files", file.name), file: Some(path), targets, at: Instant::now() });
    }
    fn focused(&self) -> bool {
        self.focused
    }
    fn open_in(&mut self, app: &str, document: &str) -> bool {
        match self.slots.iter().position(|s| matches!(s.kind, Kind::Native(d) if d.id == app)) {
            Some(index) => {
                self.opens.push((index, document.to_owned()));
                true
            }
            None => false,
        }
    }
}

struct Open {
    space: Space,
    info: SpaceInfo,
    apps: HashMap<usize, Box<dyn CanopyApp>>,
    visible: Vec<usize>,
    focused: Option<usize>,
    split: bool,
    files: Vec<FileEntry>,
    documents: Vec<(usize, Document)>,
    listed_at: Instant,
    search: String,
    /// The left pane's share of the width when two apps show.
    ratio: f32,
    /// The layout this space had last time, put back on its first frame (apps need the egui context to start).
    pending: Option<Layout>,
    /// The layout as last written to the space's folder.
    saved: Layout,
}

impl Open {
    fn new(space: Space, layout: Layout) -> Self {
        let info = SpaceInfo { id: space.id.clone(), name: space.name.clone(), dir: space.dir.clone() };
        Self {
            space, info, apps: HashMap::new(), visible: Vec::new(), focused: None, split: false, files: Vec::new(), documents: Vec::new(),
            listed_at: Instant::now() - Duration::from_secs(60), search: String::new(), ratio: layout::clamp_ratio(layout.ratio), saved: layout.clone(), pending: Some(layout),
        }
    }

    fn layout_file(&self) -> PathBuf {
        SpacePaths::new(&self.space.dir).app_data("canopy").join("layout.json")
    }

    fn layout(&self, slots: &[Slot]) -> Layout {
        Layout {
            visible: self.visible.iter().map(|index| slots[*index].id().to_owned()).collect(),
            focused: self.focused.filter(|f| self.visible.contains(f)).map(|index| slots[index].id().to_owned()),
            split: self.split,
            ratio: self.ratio,
        }
    }

    /// Writes the layout to the space's folder when it has changed.
    fn remember(&mut self, slots: &[Slot]) {
        let layout = self.layout(slots);
        if self.pending.is_none() && layout != self.saved {
            layout::save(&self.layout_file(), &layout);
            self.saved = layout;
        }
    }
}

/// The dialog that names a new space, or renames and recolors one (`editing` is its id).
struct SpaceForm {
    editing: Option<String>,
    name: String,
    color: Accent,
    parent: Option<PathBuf>,
    error: String,
}

/// The dialog that renames one of the space's files.
struct RenameFile {
    from: String,
    name: String,
    error: String,
}

/// What quick open can do.
enum Act {
    App(usize),
    Document(usize, String),
    File(PathBuf),
    Space(String),
    Split,
    Sidebar,
    ClosePane,
    Swap,
    AddFiles,
    RevealFiles,
    EditSpace,
    AllSpaces,
    NewSpace,
    OpenFolder,
    FullScreen,
}

pub struct Shell {
    spaces: Spaces,
    slots: Vec<Slot>,
    open: Option<Open>,
    toasts: Vec<Toast>,
    opens: Vec<(usize, String)>,
    space_form: Option<SpaceForm>,
    rename_file: Option<RenameFile>,
    quick: Option<quick::State>,
    prefs: Prefs,
    prefs_file: PathBuf,
    window_title: String,
    home_note: String,
}

impl Shell {
    pub fn new() -> Self {
        let profile = std::env::var_os("CANOPY_PROFILE").map(PathBuf::from).or_else(|| dirs::data_dir().map(|d| d.join("Canopy"))).unwrap_or_else(std::env::temp_dir);
        let root = std::env::var_os("CANOPY_SPACES_DIR").map(PathBuf::from).or_else(|| dirs::document_dir().map(|d| d.join("Canopy"))).unwrap_or_else(std::env::temp_dir);
        let prefs_file = profile.join("shell.json");
        Self {
            spaces: Spaces::new(profile.join("spaces.json"), root), slots: catalog(), open: None, toasts: Vec::new(), opens: Vec::new(), space_form: None, rename_file: None, quick: None,
            prefs: layout::load(&prefs_file), prefs_file, window_title: "Canopy".into(), home_note: String::new(),
        }
    }

    /// A dialog is up: the apps leave the keyboard alone.
    fn busy(&self) -> bool {
        self.quick.is_some() || self.space_form.is_some() || self.rename_file.is_some()
    }

    fn set_sidebar_hidden(&mut self, hidden: bool) {
        self.prefs.sidebar_hidden = hidden;
        layout::save(&self.prefs_file, &self.prefs);
    }

    /// `canopy <space folder>` opens that space (from a shortcut, Explorer, or tests).
    pub fn open_from_args(&mut self) {
        if let Some(dir) = std::env::args().skip(1).map(PathBuf::from).find(|p| SpacePaths::new(p).manifest().is_file()) {
            match self.spaces.add(&dir) {
                Ok(space) => self.open_space(&space.id),
                Err(error) => self.home_note = error.0,
            }
        }
    }

    fn toast(&mut self, text: impl Into<String>) {
        self.toasts.push(Toast { text: text.into(), file: None, targets: Vec::new(), at: Instant::now() });
    }

    fn close_space(&mut self) {
        if let Some(mut open) = self.open.take() {
            open.remember(&self.slots);
            for (index, app) in open.apps.iter_mut() {
                if let Err(error) = app.save() {
                    self.toasts.push(Toast { text: format!("{} couldn't save: {error}", self.slots[*index].name), file: None, targets: Vec::new(), at: Instant::now() });
                }
            }
        }
    }

    fn open_space(&mut self, id: &str) {
        self.close_space();
        match self.spaces.touch(id) {
            Ok(space) => {
                let layout = layout::load(&SpacePaths::new(&space.dir).app_data("canopy").join("layout.json"));
                self.open = Some(Open::new(space, layout));
                self.home_note.clear();
            }
            Err(error) => self.home_note = error.0,
        }
    }

    /// Shows the apps a space had open last time, where they were.
    fn restore(&mut self, ctx: &egui::Context, layout: Layout) {
        let index_of = |id: &str| self.slots.iter().position(|slot| slot.id() == id);
        let mut visible: Vec<usize> = Vec::new();
        for index in layout.visible.iter().filter_map(|id| index_of(id)) {
            if !visible.contains(&index) {
                visible.push(index);
            }
        }
        visible.truncate(if layout.split { 2 } else { 1 });
        let focused = layout.focused.as_deref().and_then(index_of).filter(|f| visible.contains(f));
        if let Some(open) = &mut self.open {
            open.split = layout.split;
        }
        for index in &visible {
            self.show_app(ctx, *index);
        }
        if let Some(open) = &mut self.open {
            open.focused = focused.or(open.focused);
        }
    }

    fn close_pane(&mut self, index: usize) {
        let Some(open) = &mut self.open else { return };
        if let Some(mut app) = open.apps.remove(&index) {
            if let Err(error) = app.save() {
                self.toasts.push(Toast { text: format!("{} couldn't save: {error}", self.slots[index].name), file: None, targets: Vec::new(), at: Instant::now() });
            }
        }
        open.visible.retain(|v| *v != index);
        open.focused = open.visible.first().copied();
    }

    fn swap_panes(&mut self) {
        if let Some(open) = &mut self.open {
            if open.visible.len() == 2 {
                open.visible.swap(0, 1);
                open.ratio = 1.0 - open.ratio;
            }
        }
    }

    /// Opens a file in the app that takes it: the one in use if it does, else the first that does.
    fn send_to_best(&mut self, ctx: &egui::Context, file: SharedFile) {
        let focused = self.open.as_ref().and_then(|o| o.focused);
        match focused.filter(|f| self.slots[*f].accepts(&file)).or_else(|| self.slots.iter().position(|s| s.accepts(&file))) {
            Some(index) => self.send(ctx, vec![file], index),
            None => {
                let _ = std::process::Command::new("explorer").arg(format!("/select,{}", file.path.display())).spawn();
            }
        }
    }

    fn add_files(&mut self) {
        let Some(open) = &mut self.open else { return };
        let files_dir = SpacePaths::new(&open.space.dir).files();
        if let Some(paths) = rfd::FileDialog::new().set_title("Add files to this space").pick_files() {
            let count = paths.iter().filter(|p| copy_into(&files_dir, p).is_ok()).count();
            open.listed_at = Instant::now() - Duration::from_secs(60);
            self.toast(format!("Added {count} file{} to Files", if count == 1 { "" } else { "s" }));
        }
    }

    fn open_folder(&mut self) {
        if let Some(dir) = rfd::FileDialog::new().set_title("Open a Canopy space").pick_folder() {
            match self.spaces.add(&dir) {
                Ok(space) => self.open_space(&space.id),
                Err(error) => self.home_note = error.0,
            }
        }
    }

    fn edit_space(&mut self, space: &Space) {
        self.space_form = Some(SpaceForm { editing: Some(space.id.clone()), name: space.name.clone(), color: Accent::from_name(&space.color), parent: None, error: String::new() });
    }

    fn show_app(&mut self, ctx: &egui::Context, index: usize) {
        let Some(open) = &mut self.open else { return };
        if let Kind::Native(descriptor) = self.slots[index].kind {
            open.apps.entry(index).or_insert_with(|| (descriptor.create)(descriptor.place_in_space(&open.info), ctx));
        }
        if !open.visible.contains(&index) {
            if !open.split {
                open.visible = vec![index];
            } else if open.visible.len() < 2 {
                open.visible.push(index);
            } else {
                let replace = open.visible.iter().position(|v| Some(*v) != open.focused).unwrap_or(0);
                open.visible[replace] = index;
            }
        }
        open.focused = Some(index);
    }

    fn send(&mut self, ctx: &egui::Context, files: Vec<SharedFile>, index: usize) {
        self.send_at(ctx, files, index, None);
    }

    fn send_at(&mut self, ctx: &egui::Context, files: Vec<SharedFile>, index: usize, position: Option<egui::Pos2>) {
        self.show_app(ctx, index);
        let slot = &self.slots[index];
        let Some(open) = &mut self.open else { return };
        let Some(app) = open.apps.get_mut(&index) else {
            self.toasts.push(Toast { text: format!("{} isn't in Canopy yet; open it from its own window", slot.name), file: None, targets: Vec::new(), at: Instant::now() });
            return;
        };
        let mut host = PaneHost { toasts: &mut self.toasts, slots: &self.slots, from: index, focused: true, opens: &mut self.opens };
        let receipt = match position {
            Some(position) => app.receive_at(files.clone(), position, &mut host),
            None => app.receive(files.clone(), &mut host),
        };
        let text = if let Some(note) = receipt.note {
            note
        } else if !receipt.skipped.is_empty() {
            format!("{} can't open {}", slot.name, receipt.skipped.join(", "))
        } else if files.len() == 1 {
            format!("Sent {} to {}", files[0].name, slot.name)
        } else {
            format!("Sent {} files to {}", files.len(), slot.name)
        };
        self.toast(text);
    }

    fn refresh(&mut self) {
        let Some(open) = &mut self.open else { return };
        // A file an app just exported shows at once; otherwise the folder is read every second and a half.
        let exported = self.toasts.iter().any(|t| t.file.is_some() && t.at > open.listed_at);
        if !exported && open.listed_at.elapsed() < Duration::from_millis(1500) {
            return;
        }
        open.listed_at = Instant::now();
        open.files = list_files(&SpacePaths::new(&open.space.dir).files());
        open.documents = self.slots.iter().enumerate().filter_map(|(i, s)| match s.kind { Kind::Native(d) => Some((i, d)), _ => None })
            .flat_map(|(i, d)| (d.documents)(&d.place_in_space(&open.info)).into_iter().map(move |doc| (i, doc)))
            .collect();
        open.documents.sort_by(|a, b| b.1.saved_at.cmp(&a.1.saved_at));
    }

    // ---- Where to? ----
    fn home(&mut self, ui: &mut Ui) {
        let rect = ui.max_rect();
        paint_ground(ui.painter(), rect, Mood::Tide);
        ui.painter().text(pos2(rect.left() + 24.0, rect.top() + 24.0), Align2::CENTER_CENTER, "C", font::display(15.0), color::INK);
        let rail = Rect::from_min_size(pos2(rect.left() + 7.0, rect.bottom() - 44.0), vec2(34.0, 34.0));
        ui.scope_builder(UiBuilder::new().max_rect(rail), |ui| {
            if icon_button(ui, Icon::FolderPlus, "Open a space folder", false).clicked() {
                self.open_folder();
            }
        });
        let spaces = self.spaces.list();
        // Tiles wrap into rows that fit the window, each row centered, the whole block around the middle.
        let (tile, gap, row_height) = (96.0, 24.0, 120.0);
        let count = spaces.len() + 1;
        let per_row = (((rect.width() - 120.0 + gap) / (tile + gap)).floor() as usize).max(1);
        let rows = count.div_ceil(per_row);
        let top = (rect.center().y - 40.0 - (rows - 1) as f32 * row_height / 2.0).max(rect.top() + 90.0);
        let place = |n: usize| {
            let in_row = if n / per_row + 1 == rows { count - (rows - 1) * per_row } else { per_row };
            let row_width = in_row as f32 * tile + (in_row - 1) as f32 * gap;
            pos2(rect.center().x - row_width / 2.0 + (n % per_row) as f32 * (tile + gap), top + (n / per_row) as f32 * row_height)
        };
        ui.painter().text(pos2(rect.center().x, top - 30.0), Align2::CENTER_CENTER, "Where to?", font::display(26.0), color::INK);
        let mut open = None;
        let mut forget = None;
        let mut edit = None;
        for (n, (space, available)) in spaces.iter().enumerate() {
            let area = Rect::from_min_size(place(n), vec2(tile, 96.0));
            let response = ui.interact(area, ui.scope_id().with(("space", &space.id)), Sense::click());
            let mark = Rect::from_center_size(pos2(area.center().x, area.top() + 34.0), vec2(68.0, 68.0));
            let fill = Accent::from_name(&space.color).color();
            leaf(ui.painter(), mark, if *available { fill } else { fill.gamma_multiply(0.35) });
            let ink = if response.hovered() { color::INK } else { color::INK_MUTED };
            ui.painter().with_clip_rect(area).text(pos2(area.center().x, area.top() + 84.0), Align2::CENTER_CENTER, &space.name, font::ui(13.0), ink);
            if response.clicked() {
                open = Some(space.id.clone());
            }
            let dir = space.dir.clone();
            let id = space.id.clone();
            response.on_hover_text(dir.display().to_string()).context_menu(|ui| {
                if ui.button("Open").clicked() {
                    open = Some(id.clone());
                }
                if ui.button("Rename or recolor…").clicked() {
                    edit = Some(space.clone());
                }
                if ui.button("Show in Explorer").clicked() {
                    let _ = std::process::Command::new("explorer").arg(&dir).spawn();
                }
                if ui.button("Remove from this list").clicked() {
                    forget = Some(id.clone());
                }
            });
        }
        let area = Rect::from_min_size(place(count - 1), vec2(tile, 96.0));
        let response = ui.interact(area, ui.scope_id().with("new-space"), Sense::click());
        let mark = Rect::from_center_size(pos2(area.center().x, area.top() + 34.0), vec2(68.0, 68.0));
        let edge = if response.hovered() { color::SELECT_LINE } else { egui::Color32::from_rgba_unmultiplied(238, 242, 234, 72) };
        ui.painter().rect_stroke(mark, 12, egui::Stroke::new(1.5, edge), egui::StrokeKind::Inside);
        canopy_ui::icon(ui.painter(), Rect::from_center_size(mark.center(), vec2(20.0, 20.0)), Icon::Plus, color::INK_MUTED);
        ui.painter().text(pos2(area.center().x, area.top() + 84.0), Align2::CENTER_CENTER, "New space", font::ui(13.0), color::INK_MUTED);
        if response.clicked() {
            self.new_space();
        }
        let note = if !self.home_note.is_empty() {
            self.home_note.as_str()
        } else if spaces.is_empty() {
            "A space keeps your apps' work together, with the files they share."
        } else {
            "Ctrl+K finds a space by name. Right-click one to rename it."
        };
        ui.painter().text(pos2(rect.center().x, top + rows as f32 * row_height + 10.0), Align2::CENTER_CENTER, note, font::ui(13.0), color::INK_MUTED);
        if let Some(id) = forget {
            let _ = self.spaces.forget(&id);
        }
        if let Some(space) = edit {
            self.edit_space(&space);
        }
        if let Some(id) = open {
            self.open_space(&id);
        }
    }

    fn new_space(&mut self) {
        self.space_form = Some(SpaceForm { editing: None, name: String::new(), color: Accent::Leaf, parent: None, error: String::new() });
    }

    fn space_form_dialog(&mut self, ui: &mut Ui) {
        let Some(form) = &mut self.space_form else { return };
        let editing = form.editing.clone();
        let mut create = false;
        let modal = egui::Modal::new(ui.scope_id().with("new-space-dialog")).show(ui.ctx(), |ui| {
            ui.set_width(400.0);
            ui.label(egui::RichText::new(if editing.is_some() { "Rename or recolor" } else { "Name your space" }).font(font::display(22.0)));
            if editing.is_none() {
                ui.label(canopy_ui::muted("A space keeps your apps' work together, with the files they share."));
            }
            ui.add_space(10.0);
            let name = ui.add(egui::TextEdit::singleline(&mut form.name).hint_text("Client work").desired_width(f32::INFINITY).margin(vec2(10.0, 8.0)));
            name.request_focus();
            create = ui.input(|i| i.key_pressed(Key::Enter));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                for accent in Accent::ALL {
                    let (rect, response) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::click());
                    leaf(ui.painter(), rect.shrink(2.0), accent.color());
                    if form.color == accent {
                        ui.painter().rect_stroke(rect, 8, egui::Stroke::new(2.0, color::INK), egui::StrokeKind::Outside);
                    }
                    if response.on_hover_text(accent.name()).clicked() {
                        form.color = accent;
                    }
                }
            });
            ui.add_space(8.0);
            if editing.is_none() {
                ui.horizontal(|ui| {
                    let place = form.parent.clone().map(|p| p.display().to_string()).unwrap_or_else(|| self.spaces.default_root().display().to_string());
                    ui.label(canopy_ui::muted(format!("Kept in {place}")));
                    if quiet_button(ui, "Change", None).clicked() {
                        form.parent = rfd::FileDialog::new().set_title("Choose where to keep this space").pick_folder().or(form.parent.clone());
                    }
                });
            }
            if !form.error.is_empty() {
                ui.colored_label(color::DANGER, &form.error);
            }
            ui.add_space(8.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if primary_button(ui, if editing.is_some() { "Save" } else { "Create space" }, Mood::Tide).clicked() {
                    create = true;
                }
                if quiet_button(ui, "Back", None).clicked() {
                    form.error = "\u{0}".into();
                }
            });
        });
        if modal.should_close() || form.error == "\u{0}" {
            self.space_form = None;
            return;
        }
        if !create {
            return;
        }
        match editing {
            Some(id) => match self.spaces.rename(&id, &form.name).and_then(|_| self.spaces.recolor(&id, form.color.name())) {
                Ok(space) => {
                    self.space_form = None;
                    // The open space takes its new name and color at once.
                    if let Some(open) = self.open.as_mut().filter(|open| open.space.id == space.id) {
                        open.info.name = space.name.clone();
                        open.space.name = space.name;
                        open.space.color = space.color;
                    }
                }
                Err(error) => form.error = error.0,
            },
            None => {
                let dir = form.parent.as_ref().map(|p| p.join(canopy_core::files::safe_file_name(form.name.trim())));
                match self.spaces.create(&form.name, form.color.name(), dir) {
                    Ok(space) => {
                        self.space_form = None;
                        self.open_space(&space.id);
                    }
                    Err(error) => form.error = error.0,
                }
            }
        }
    }

    fn rename_file_dialog(&mut self, ui: &mut Ui) {
        let Some(form) = &mut self.rename_file else { return };
        let (mut save, mut back) = (false, false);
        let modal = egui::Modal::new(ui.scope_id().with("rename-file-dialog")).show(ui.ctx(), |ui| {
            ui.set_width(400.0);
            ui.label(egui::RichText::new("Rename").font(font::display(22.0)));
            ui.label(canopy_ui::muted(&form.from));
            ui.add_space(10.0);
            let name = ui.add(egui::TextEdit::singleline(&mut form.name).desired_width(f32::INFINITY).margin(vec2(10.0, 8.0)));
            // The field keeps the keyboard while the dialog is up, so Enter is read from the keys, not from it losing focus.
            name.request_focus();
            save = ui.input(|i| i.key_pressed(Key::Enter));
            if !form.error.is_empty() {
                ui.colored_label(color::DANGER, &form.error);
            }
            ui.add_space(8.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                save |= primary_button(ui, "Rename", Mood::Ember).clicked();
                back = quiet_button(ui, "Back", None).clicked();
            });
        });
        if modal.should_close() || back {
            self.rename_file = None;
            return;
        }
        if !save {
            return;
        }
        let Some(open) = &mut self.open else { return };
        let dir = SpacePaths::new(&open.space.dir).files();
        match canopy_core::files::rename(&dir, &form.from, &form.name) {
            Ok(name) => {
                // The File Manager's title and description for the file go with it.
                let _ = canopy::filemanager::notes::Notes::open().moved(&dir.join(&form.from), &dir.join(&name));
                open.listed_at = Instant::now() - Duration::from_secs(60);
                self.rename_file = None;
                self.toast(format!("Renamed to {name}"));
            }
            Err(error) => form.error = error.0,
        }
    }

    /// Everything quick open can reach from here, in the order it lists them when nothing is typed.
    fn quick_entries(&self) -> Vec<Entry<Act>> {
        const KEYS: [&str; 9] = ["Ctrl+1", "Ctrl+2", "Ctrl+3", "Ctrl+4", "Ctrl+5", "Ctrl+6", "Ctrl+7", "Ctrl+8", "Ctrl+9"];
        let command = |label: &str, keys: &'static str, which: Icon, act: Act| Entry { label: label.to_owned(), detail: String::new(), keys, mark: Mark::Icon(which), act };
        let mut entries = Vec::new();
        if let Some(open) = &self.open {
            for (index, slot) in self.slots.iter().enumerate() {
                entries.push(Entry { label: slot.name.to_owned(), detail: slot.description.to_owned(), keys: KEYS.get(index).copied().unwrap_or(""), mark: Mark::Leaf(slot.accent), act: Act::App(index) });
            }
            for (index, doc) in &open.documents {
                let slot = &self.slots[*index];
                entries.push(Entry { label: doc.name.clone(), detail: format!("{} · {}", slot.name, doc.detail), keys: "", mark: Mark::Leaf(slot.accent), act: Act::Document(*index, doc.id.clone()) });
            }
            let files_dir = SpacePaths::new(&open.space.dir).files();
            for file in &open.files {
                entries.push(Entry { label: file.name.clone(), detail: format_size(file.size), keys: "", mark: Mark::Icon(file_icon(file.kind)), act: Act::File(files_dir.join(&file.name)) });
            }
            entries.push(command(if open.split { "Show one app" } else { "Show two apps side by side" }, "Ctrl+J", Icon::Split, Act::Split));
            if open.visible.len() == 2 {
                entries.push(command("Swap the two apps", "", Icon::Swap, Act::Swap));
            }
            if let Some(focused) = open.focused.filter(|f| open.visible.contains(f)) {
                entries.push(command(&format!("Close {}", self.slots[focused].name), "Ctrl+W", Icon::Close, Act::ClosePane));
            }
            entries.push(command(if self.prefs.sidebar_hidden { "Show the sidebar" } else { "Hide the sidebar" }, "Ctrl+B", Icon::Sidebar, Act::Sidebar));
            entries.push(command("Add files to this space", "", Icon::Plus, Act::AddFiles));
            entries.push(command("Show the space folder", "", Icon::Folder, Act::RevealFiles));
            entries.push(command("Rename or recolor this space", "", Icon::Text, Act::EditSpace));
            entries.push(command("All spaces", "", Icon::Grid, Act::AllSpaces));
        }
        let here = self.open.as_ref().map(|open| open.space.id.as_str());
        for (space, available) in self.spaces.list() {
            if available && Some(space.id.as_str()) != here {
                entries.push(Entry { label: space.name.clone(), detail: "Space".into(), keys: "", mark: Mark::Leaf(Accent::from_name(&space.color)), act: Act::Space(space.id) });
            }
        }
        entries.push(command("New space", "", Icon::Plus, Act::NewSpace));
        entries.push(command("Open a space folder", "", Icon::FolderPlus, Act::OpenFolder));
        entries.push(command("Full screen", "F11", Icon::ZoomIn, Act::FullScreen));
        entries
    }

    fn quick_open(&mut self, ctx: &egui::Context) {
        let entries = self.quick_entries();
        let Some(state) = &mut self.quick else { return };
        let act = match quick::show(ctx, state, entries) {
            Outcome::Stay => return,
            Outcome::Close => {
                self.quick = None;
                return;
            }
            Outcome::Pick(act) => act,
        };
        self.quick = None;
        match act {
            Act::App(index) => self.show_app(ctx, index),
            Act::Document(index, id) => self.open_document(ctx, index, &id),
            Act::File(path) => self.send_to_best(ctx, SharedFile::new(path)),
            Act::Space(id) => self.open_space(&id),
            Act::Split => {
                let split = self.open.as_ref().is_some_and(|o| o.split);
                self.set_split(ctx, !split);
            }
            Act::Sidebar => self.set_sidebar_hidden(!self.prefs.sidebar_hidden),
            Act::ClosePane => {
                if let Some(index) = self.open.as_ref().and_then(|o| o.focused) {
                    self.close_pane(index);
                }
            }
            Act::Swap => self.swap_panes(),
            Act::AddFiles => self.add_files(),
            Act::RevealFiles => {
                if let Some(open) = &self.open {
                    let _ = std::process::Command::new("explorer").arg(SpacePaths::new(&open.space.dir).files()).spawn();
                }
            }
            Act::EditSpace => {
                if let Some(space) = self.open.as_ref().map(|open| open.space.clone()) {
                    self.edit_space(&space);
                }
            }
            Act::AllSpaces => self.close_space(),
            Act::NewSpace => {
                self.close_space();
                self.new_space();
            }
            Act::OpenFolder => self.open_folder(),
            Act::FullScreen => toggle_full_screen(ctx),
        }
    }

    fn open_document(&mut self, ctx: &egui::Context, index: usize, id: &str) {
        self.show_app(ctx, index);
        if let Some(app) = self.open.as_mut().and_then(|open| open.apps.get_mut(&index)) {
            let mut host = PaneHost { toasts: &mut self.toasts, slots: &self.slots, from: index, focused: true, opens: &mut self.opens };
            app.open_document(id, &mut host);
        }
    }

    // ---- A space ----
    fn space(&mut self, ui: &mut Ui) {
        self.refresh();
        let ctx = ui.ctx().clone();
        let rect = ui.max_rect();
        paint_ground(ui.painter(), rect, Mood::Ember);
        if let Some(layout) = self.open.as_mut().and_then(|open| open.pending.take()) {
            self.restore(&ctx, layout);
        }

        // Ctrl+Tab cycles through the apps; Ctrl+1-9 jump to one. Ctrl+J splits, Ctrl+W closes the app in use,
        // Ctrl+B hides the sidebar.
        let (cycle, jump, split, close, hide) = ctx.input_mut(|i| {
            let back = i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::Tab);
            let forward = !back && i.consume_key(Modifiers::CTRL, Key::Tab);
            let jump = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9].iter().position(|k| i.consume_key(Modifiers::CTRL, *k));
            (if back { -1 } else if forward { 1 } else { 0 }, jump, i.consume_key(Modifiers::CTRL, Key::J), i.consume_key(Modifiers::CTRL, Key::W), i.consume_key(Modifiers::CTRL, Key::B))
        });
        // With a dialog up the keys are its own.
        let (cycle, jump, split, close, hide) = if self.busy() { (0, None, false, false, false) } else { (cycle, jump, split, close, hide) };
        if split {
            let split = self.open.as_ref().is_some_and(|o| o.split);
            self.set_split(&ctx, !split);
        }
        if close {
            if let Some(index) = self.open.as_ref().and_then(|o| o.focused) {
                self.close_pane(index);
            }
        }
        if hide {
            self.set_sidebar_hidden(!self.prefs.sidebar_hidden);
        }
        if cycle != 0 {
            let current = self.open.as_ref().and_then(|o| o.focused).map(|f| f as i32).unwrap_or(-1);
            let next = (current + cycle).rem_euclid(self.slots.len() as i32) as usize;
            self.show_app(&ctx, next);
        }
        if let Some(index) = jump.filter(|i| *i < self.slots.len()) {
            self.show_app(&ctx, index);
        }

        let top = Rect::from_min_size(rect.min, vec2(rect.width(), 42.0));
        self.top_bar(ui, top);
        let body = Rect::from_min_max(pos2(rect.left() + 6.0, top.bottom()), rect.max - vec2(6.0, 6.0));
        // The sidebar is as wide as it was left (drag its edge; double-click the edge for the usual width), or hidden.
        let hidden = self.prefs.sidebar_hidden;
        let width = if hidden { 0.0 } else { layout::clamp_sidebar(self.prefs.sidebar_width).min((body.width() - 520.0).max(200.0)) };
        let sidebar = Rect::from_min_size(body.min, vec2(width, body.height()));
        let main = Rect::from_min_max(pos2(if hidden { body.left() } else { sidebar.right() + 6.0 }, body.top()), body.max);
        if !hidden {
            self.sidebar(ui, sidebar);
            let grip = Rect::from_min_max(pos2(sidebar.right(), body.top() + 8.0), pos2(main.left(), body.bottom() - 8.0));
            let response = ui.interact(grip, ui.scope_id().with("sidebar-edge"), Sense::click_and_drag()).on_hover_cursor(CursorIcon::ResizeHorizontal);
            if response.hovered() || response.dragged() {
                ui.painter().line_segment([grip.center_top(), grip.center_bottom()], egui::Stroke::new(2.0, color::SELECT_LINE));
            }
            if response.dragged() {
                ctx.set_cursor_icon(CursorIcon::ResizeHorizontal);
                self.prefs.sidebar_width = layout::clamp_sidebar(width + response.drag_delta().x);
            }
            if response.double_clicked() {
                self.prefs.sidebar_width = layout::SIDEBAR_WIDTH;
            }
            if response.drag_stopped() || response.double_clicked() {
                layout::save(&self.prefs_file, &self.prefs);
            }
        }
        self.main(ui, main);
        // An app asked to open a project in another one (Video Chat hands a Short to StreamCuts).
        for (index, document) in std::mem::take(&mut self.opens) {
            self.show_app(&ctx, index);
            if let Some(app) = self.open.as_mut().and_then(|open| open.apps.get_mut(&index)) {
                let mut opens = Vec::new();
                let mut host = PaneHost { toasts: &mut self.toasts, slots: &self.slots, from: index, focused: true, opens: &mut opens };
                app.open_document(&document, &mut host);
            }
        }
        // The layout is written once a drag is over, not on every frame of it.
        if !ctx.input(|i| i.pointer.any_down()) {
            if let Some(open) = &mut self.open {
                open.remember(&self.slots);
            }
        }

        // Files dropped from Explorer: onto the sidebar they join the space's Files; onto a pane, the app gets them.
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
        if !dropped.is_empty() {
            // A drop whose place isn't known joins Files, like one on the sidebar.
            let at = canopy_ui::os_drop_position(&ctx).filter(|at| hidden || !sidebar.contains(*at)).unwrap_or(pos2(f32::NEG_INFINITY, 0.0));
            if !at.x.is_finite() || self.open.as_ref().is_some_and(|o| o.visible.is_empty()) {
                if let Some(open) = &mut self.open {
                    let files = SpacePaths::new(&open.space.dir).files();
                    let added = dropped.iter().filter(|p| copy_into(&files, p).is_ok()).count();
                    open.listed_at = Instant::now() - Duration::from_secs(60);
                    self.toast(format!("Added {added} file{} to Files", if added == 1 { "" } else { "s" }));
                }
            } else if !main.contains(at) {
                if let Some(index) = self.open.as_ref().and_then(|o| o.focused) {
                    self.send(&ctx, dropped.into_iter().map(SharedFile::new).collect(), index);
                }
            }
        }
    }

    fn top_bar(&mut self, ui: &mut Ui, rect: Rect) {
        let ctx = ui.ctx().clone();
        ui.scope_builder(UiBuilder::new().max_rect(rect.shrink2(vec2(6.0, 6.0))), |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let (logo, response) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
                if response.hovered() {
                    ui.painter().rect_filled(logo, 8, color::VEIL_HOVER);
                }
                ui.painter().text(logo.center(), Align2::CENTER_CENTER, "C", font::display(15.0), color::INK);
                if response.on_hover_text("All spaces").clicked() {
                    self.close_space();
                    return;
                }
                let hidden = self.prefs.sidebar_hidden;
                if icon_button(ui, Icon::Sidebar, if hidden { "Show the sidebar (Ctrl+B)" } else { "Hide the sidebar (Ctrl+B)" }, !hidden).clicked() {
                    self.set_sidebar_hidden(!hidden);
                }
                let Some(open) = &self.open else { return };
                let space = open.space.clone();
                let pill_response = pill(ui, &space.name, Some(Accent::from_name(&space.color)), false);
                let mut switch_to = None;
                let mut edit = false;
                let spaces = self.spaces.list();
                pill_response.context_menu(|ui| {
                    edit = ui.button("Rename or recolor…").clicked();
                    if ui.button("Show in Explorer").clicked() {
                        let _ = std::process::Command::new("explorer").arg(&space.dir).spawn();
                    }
                    if spaces.iter().any(|(other, available)| *available && other.id != space.id) {
                        ui.separator();
                    }
                    for (other, available) in &spaces {
                        if *available && other.id != space.id && ui.button(format!("Go to {}", other.name)).clicked() {
                            switch_to = Some(other.id.clone());
                        }
                    }
                });
                if edit {
                    self.edit_space(&space);
                }
                if pill_response.on_hover_text("All spaces (right-click for more)").clicked() {
                    self.close_space();
                    return;
                }
                let Some(open) = &self.open else { return };
                let mut clicked = None;
                for (index, slot) in self.slots.iter().enumerate() {
                    let focused = open.focused == Some(index) && open.visible.contains(&index);
                    if pill(ui, slot.name, Some(slot.accent), focused).on_hover_text(slot.description).clicked() {
                        clicked = Some(index);
                    }
                }
                let split = open.split;
                if let Some(index) = clicked {
                    self.show_app(&ctx, index);
                }
                if let Some(id) = switch_to {
                    self.open_space(&id);
                }
                // Toast, then split and clock on the right.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let now = chrono_like_clock();
                    ui.label(egui::RichText::new(now).font(font::ui(14.0)).color(color::INK));
                    if icon_button(ui, Icon::Split, "Show two apps side by side (Ctrl+J)", split).clicked() {
                        self.set_split(&ctx, !split);
                    }
                    if icon_button(ui, Icon::Search, "Find anything (Ctrl+K)", false).clicked() {
                        self.quick = Some(quick::State::default());
                    }
                    self.toasts.retain(|t| t.at.elapsed() < Duration::from_secs(if t.targets.is_empty() { 5 } else { 10 }));
                    let mut send = None;
                    if let Some(toast) = self.toasts.last() {
                        for &target in toast.targets.iter().rev() {
                            if pill(ui, &format!("Send to {}", self.slots[target].name), Some(self.slots[target].accent), false).clicked() {
                                send = toast.file.clone().map(|f| (f, target));
                            }
                        }
                        ui.label(egui::RichText::new(&toast.text).font(font::ui(13.0)).color(color::SELECT_INK));
                        ctx.request_repaint_after(Duration::from_millis(500));
                    }
                    if let Some((file, target)) = send {
                        self.toasts.clear();
                        self.send(&ctx, vec![SharedFile::new(file)], target);
                    }
                });
            });
        });
    }

    fn set_split(&mut self, ctx: &egui::Context, split: bool) {
        let Some(open) = &mut self.open else { return };
        open.split = split;
        if !split {
            open.visible.retain(|v| Some(*v) == open.focused);
            open.visible.truncate(1);
        } else if open.visible.len() < 2 {
            let other = (0..self.slots.len()).find(|i| !open.visible.contains(i) && open.apps.contains_key(i)).or_else(|| (0..self.slots.len()).find(|i| !open.visible.contains(i)));
            if let Some(other) = other {
                let keep = open.focused;
                self.show_app(ctx, other);
                if let Some(open) = &mut self.open {
                    open.focused = keep.or(Some(other));
                }
            }
        }
    }

    fn sidebar(&mut self, ui: &mut Ui, rect: Rect) {
        let ctx = ui.ctx().clone();
        ui.painter().add(canopy_ui::pane_frame(Mood::Ember).paint(rect));
        let mut send: Option<(SharedFile, usize)> = None;
        let mut open_doc = None;
        let mut change: Option<(FileChange, String)> = None;
        let slots = &self.slots;
        let Some(open) = &mut self.open else { return };
        let files_dir = SpacePaths::new(&open.space.dir).files();
        let mut reveal = false;
        let mut add = false;
        // From the bottom: the dock, the hint above it, and the lists fill the rest.
        let dock = Rect::from_min_max(pos2(rect.left(), rect.bottom() - 46.0), rect.max);
        let hint = Rect::from_min_max(pos2(rect.left() + 8.0, dock.top() - 40.0), pos2(rect.right() - 8.0, dock.top() - 4.0));
        ui.scope_builder(UiBuilder::new().max_rect(rect.shrink(8.0)), |ui| {
            ui.add(egui::TextEdit::singleline(&mut open.search).hint_text("Search this space").desired_width(f32::INFINITY).margin(vec2(10.0, 7.0)));
            let query = open.search.trim().to_lowercase();
            ui.add_space(6.0);
            let list_height = (hint.top() - 6.0 - ui.cursor().top()).max(60.0);
            egui::ScrollArea::vertical().max_height(list_height).auto_shrink(false).show(ui, |ui| {
                section_label(ui, "Projects");
                let docs: Vec<_> = open.documents.iter().filter(|(_, d)| query.is_empty() || d.name.to_lowercase().contains(&query)).collect();
                if docs.is_empty() {
                    ui.label(canopy_ui::muted("No projects yet. Open StreamCuts to start one."));
                }
                for (index, doc) in docs {
                    let detail = format!("{} · {}", slots[*index].name, doc.detail);
                    if (Row { title: &doc.name, detail: &detail, mark: RowMark::Leaf(slots[*index].accent), highlighted: false }).show(ui).clicked() {
                        open_doc = Some((*index, doc.id.clone()));
                    }
                }
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    section_label(ui, "Files");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        reveal = icon_button(ui, Icon::Folder, "Show the space folder", false).clicked();
                        add = icon_button(ui, Icon::Plus, "Add files", false).clicked();
                    });
                });
                let files: Vec<_> = open.files.iter().filter(|f| query.is_empty() || f.name.to_lowercase().contains(&query)).cloned().collect();
                if files.is_empty() {
                    ui.label(canopy_ui::muted("No files yet. Exports from your apps land here."));
                }
                for file in files {
                    let shared = SharedFile::new(files_dir.join(&file.name));
                    let size = format_size(file.size);
                    // Dragged into an app it's handed over; double-click or right-click sends it too.
                    let row = (Row { title: &file.name, detail: &size, mark: RowMark::Icon(file_icon(file.kind)), highlighted: false }).show(ui);
                    canopy_ui::drag_source(ui, &row, || DraggedFiles(vec![shared.clone()]), &file.name);
                    if row.double_clicked() {
                        let target = open.focused.filter(|f| slots[*f].accepts(&shared)).or_else(|| slots.iter().position(|s| s.accepts(&shared)));
                        match target {
                            Some(t) => send = Some((shared.clone(), t)),
                            None => {}
                        }
                    }
                    row.context_menu(|ui| {
                        for (index, slot) in slots.iter().enumerate() {
                            if ui.add_enabled(slot.accepts(&shared), egui::Button::new(format!("Send to {}", slot.name))).clicked() {
                                send = Some((shared.clone(), index));
                            }
                        }
                        ui.separator();
                        if ui.button("Open").clicked() {
                            let _ = std::process::Command::new("explorer").arg(&shared.path).spawn();
                        }
                        if ui.button("Show in folder").clicked() {
                            let _ = std::process::Command::new("explorer").arg(format!("/select,{}", shared.path.display())).spawn();
                        }
                        if ui.button("Copy path").clicked() {
                            ui.ctx().copy_text(shared.path.display().to_string());
                        }
                        ui.separator();
                        if ui.button("Rename…").clicked() {
                            change = Some((FileChange::Rename, file.name.clone()));
                        }
                        if ui.button("Duplicate").clicked() {
                            change = Some((FileChange::Duplicate, file.name.clone()));
                        }
                        if ui.button("Move to the Recycle Bin").clicked() {
                            change = Some((FileChange::Recycle, file.name.clone()));
                        }
                    });
                }
            });
        });
        ui.scope_builder(UiBuilder::new().max_rect(hint), |ui| {
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                ui.label(egui::RichText::new("Drag a file into an app, or right-click it. Ctrl+K finds anything.").font(font::ui(12.0)).color(color::INK_MUTED));
            });
        });
        // Dock
        ui.painter().line_segment([dock.left_top(), dock.right_top()], egui::Stroke::new(1.0, color::HAIRLINE));
        let mut show = None;
        for (index, slot) in slots.iter().enumerate() {
            let x = dock.left() + dock.width() * (index as f32 + 0.5) / slots.len() as f32;
            let button = Rect::from_center_size(pos2(x, dock.center().y), vec2(30.0, 30.0));
            let response = ui.interact(button, ui.scope_id().with(("dock", index)), Sense::click());
            if response.hovered() {
                ui.painter().rect_filled(button, 8, color::VEIL_HOVER);
            }
            leaf(ui.painter(), Rect::from_center_size(button.center(), vec2(18.0, 18.0)), slot.accent.color());
            if response.on_hover_text(format!("Open {}", slot.name)).clicked() {
                show = Some(index);
            }
        }
        if reveal {
            let _ = std::process::Command::new("explorer").arg(&files_dir).spawn();
        }
        match change {
            Some((FileChange::Rename, name)) => self.rename_file = Some(RenameFile { from: name.clone(), name, error: String::new() }),
            Some((FileChange::Duplicate, name)) => {
                let text = match copy_into(&files_dir, &files_dir.join(&name)) {
                    Ok(copy) => format!("Made {copy}"),
                    Err(error) => error.0,
                };
                open.listed_at = Instant::now() - Duration::from_secs(60);
                self.toast(text);
            }
            Some((FileChange::Recycle, name)) => {
                let text = match canopy_core::files::recycle(&files_dir.join(&name)) {
                    Ok(()) => format!("Moved {name} to the Recycle Bin"),
                    Err(error) => error.0,
                };
                open.listed_at = Instant::now() - Duration::from_secs(60);
                self.toast(text);
            }
            None => {}
        }
        if add {
            self.add_files();
        }
        if let Some(index) = show {
            self.show_app(&ctx, index);
        }
        if let Some((index, id)) = open_doc {
            self.open_document(&ctx, index, &id);
        }
        if let Some((file, index)) = send {
            self.send(&ctx, vec![file], index);
        }
    }

    fn main(&mut self, ui: &mut Ui, rect: Rect) {
        let ctx = ui.ctx().clone();
        let Some(open) = &self.open else { return };
        if open.visible.is_empty() {
            let mut show = None;
            let painter = ui.painter();
            let center = rect.center();
            painter.text(pos2(center.x, center.y - 110.0), Align2::CENTER_CENTER, "You're in", font::ui(15.0), color::INK_SOFT);
            painter.text(pos2(center.x, center.y - 78.0), Align2::CENTER_CENTER, &open.space.name, font::display(20.0), color::INK);
            let spacing = 150.0;
            let left = center.x - spacing * (self.slots.len() as f32 - 1.0) / 2.0;
            for (index, slot) in self.slots.iter().enumerate() {
                let tile = Rect::from_center_size(pos2(left + spacing * index as f32, center.y), vec2(58.0, 58.0));
                let response = ui.interact(tile.expand2(vec2(30.0, 30.0)), ui.scope_id().with(("tile", index)), Sense::click());
                ui.painter().rect_filled(tile, 12, if response.hovered() { color::VEIL_STRONG } else { color::VEIL });
                ui.painter().rect_stroke(tile, 12, egui::Stroke::new(1.0, color::HAIRLINE), egui::StrokeKind::Inside);
                leaf_with_initial(ui.painter(), Rect::from_center_size(tile.center(), vec2(30.0, 30.0)), slot.accent, slot.initial);
                ui.painter().text(pos2(tile.center().x, tile.bottom() + 16.0), Align2::CENTER_CENTER, slot.name, font::ui(14.0), color::INK);
                // Descriptions wrap under their own tile instead of running into the next one.
                let mut job = egui::text::LayoutJob::simple(slot.description.to_owned(), font::ui(12.0), color::INK_MUTED, spacing - 14.0);
                job.halign = egui::Align::Center;
                let galley = ui.painter().layout_job(job);
                ui.painter().galley(pos2(tile.center().x, tile.bottom() + 27.0), galley, color::INK_MUTED);
                if response.clicked() {
                    show = Some(index);
                }
            }
            ui.painter().text(pos2(center.x, center.y + 140.0), Align2::CENTER_CENTER, "Ctrl+K finds anything. Ctrl+Tab moves between apps, Ctrl+1 through Ctrl+6 jump to one.", font::ui(13.0), color::INK_MUTED);
            if let Some(index) = show {
                self.show_app(&ctx, index);
            }
            return;
        }
        let visible = open.visible.clone();
        // Two panes share the width as the edge between them was left (drag it; double-click for half and half).
        let ratio = open.ratio;
        let first = if visible.len() == 2 { ((rect.width() - 6.0) * ratio).round() } else { rect.width() };
        let busy = self.busy();
        let mut close = None;
        let mut focus = None;
        let mut toggle_split = false;
        let mut swap = false;
        let mut drops = Vec::new();
        for (n, index) in visible.iter().copied().enumerate() {
            let pane = if n == 0 { Rect::from_min_size(rect.min, vec2(first, rect.height())) } else { Rect::from_min_max(pos2(rect.left() + first + 6.0, rect.top()), rect.max) };
            let focused = self.open.as_ref().and_then(|o| o.focused) == Some(index);
            let title = self.open.as_ref().and_then(|o| o.apps.get(&index)).map(|app| app.title()).unwrap_or_default();
            ui.painter().rect_filled(pane, 10, Mood::Ember.pane());
            ui.painter().rect_stroke(pane, 10, egui::Stroke::new(1.0, if focused && visible.len() > 1 { color::HAIRLINE_STRONG } else { color::HAIRLINE }), egui::StrokeKind::Inside);
            let head = Rect::from_min_size(pane.min, vec2(pane.width(), 42.0));
            let slot = &self.slots[index];
            ui.scope_builder(UiBuilder::new().max_rect(head.shrink2(vec2(8.0, 6.0))), |ui| {
                ui.horizontal_centered(|ui| {
                    pill(ui, slot.name, Some(slot.accent), false);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icon_button(ui, Icon::Close, &format!("Close {} (Ctrl+W)", slot.name), false).clicked() {
                            close = Some(index);
                        }
                        let split = self.open.as_ref().is_some_and(|o| o.split);
                        if icon_button(ui, Icon::Split, if split { "Show one app (Ctrl+J)" } else { "Show two apps side by side (Ctrl+J)" }, false).clicked() {
                            toggle_split = true;
                        }
                        if visible.len() == 2 && icon_button(ui, Icon::Swap, "Swap the two apps", false).clicked() {
                            swap = true;
                        }
                        // What's open in the app, in the room that's left.
                        if !title.is_empty() && ui.available_width() > 60.0 {
                            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                ui.add(egui::Label::new(egui::RichText::new(&title).font(font::ui(13.0)).color(color::INK_MUTED)).truncate());
                            });
                        }
                    });
                });
            });
            let body = Rect::from_min_max(pos2(pane.left() + 4.0, head.bottom()), pane.max - vec2(4.0, 4.0));
            if ui.input(|i| i.pointer.any_pressed()) && ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| pane.contains(p)) {
                focus = Some(index);
            }
            match &slot.kind {
                Kind::Native(_) => {
                    let Some(open) = &mut self.open else { return };
                    if let Some(app) = open.apps.get_mut(&index) {
                        let mut host = PaneHost { toasts: &mut self.toasts, slots: &self.slots, from: index, focused: focused && !busy, opens: &mut self.opens };
                        ui.scope_builder(UiBuilder::new().max_rect(body), |ui| {
                            ui.set_clip_rect(body);
                            app.ui(ui, &mut host);
                        });
                    }
                }
                Kind::External { exe, .. } => {
                    let exe = exe.clone();
                    ui.scope_builder(UiBuilder::new().max_rect(body), |ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(body.height() * 0.35);
                            ui.label(egui::RichText::new(format!("{} isn't in the native Canopy yet", slot.name)).font(font::display(18.0)));
                            ui.label(canopy_ui::muted("It's being rebuilt in Rust. Until then, it opens in its own window."));
                            ui.add_space(10.0);
                            if primary_button(ui, &format!("Open {}", slot.name), Mood::Ember).clicked() {
                                if let Err(error) = std::process::Command::new(&exe).spawn() {
                                    self.toasts.push(Toast { text: format!("{} couldn't open: {error}", exe.display()), file: None, targets: Vec::new(), at: Instant::now() });
                                }
                            }
                        });
                    });
                }
            }
            // Explorer drops are routed by their actual pane, even when another pane has keyboard focus.
            if ui.input(|i| !i.raw.dropped_files.is_empty()) {
                if let Some(at) = canopy_ui::os_drop_position(&ctx).filter(|p| pane.contains(*p)) {
                    let files: Vec<_> = ui.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf())
                        .filter(|p| !p.as_os_str().is_empty()).map(SharedFile::new).collect());
                    if !files.is_empty() { drops.push((index, files, Some(at))); }
                }
            }
            // A file dragged from the sidebar and let go over the pane, that the app didn't take itself.
            if ui.input(|i| i.pointer.any_released()) && ui.input(|i| i.pointer.hover_pos()).is_some_and(|p| body.contains(p)) {
                if let Some(files) = DragAndDrop::take_payload::<DraggedFiles>(&ctx) {
                    drops.push((index, files.0.clone(), ui.input(|i| i.pointer.hover_pos())));
                }
            }
        }
        if let Some(index) = focus {
            if let Some(open) = &mut self.open {
                open.focused = Some(index);
            }
        }
        if toggle_split {
            let split = self.open.as_ref().is_some_and(|o| o.split);
            self.set_split(&ctx, !split);
        }
        if visible.len() == 2 {
            let grip = Rect::from_min_max(pos2(rect.left() + first, rect.top() + 8.0), pos2(rect.left() + first + 6.0, rect.bottom() - 8.0));
            let response = ui.interact(grip, ui.scope_id().with("split-edge"), Sense::click_and_drag()).on_hover_cursor(CursorIcon::ResizeHorizontal);
            if response.hovered() || response.dragged() {
                ui.painter().line_segment([grip.center_top(), grip.center_bottom()], egui::Stroke::new(2.0, color::SELECT_LINE));
            }
            if let Some(open) = &mut self.open {
                if response.dragged() {
                    ctx.set_cursor_icon(CursorIcon::ResizeHorizontal);
                    open.ratio = layout::clamp_ratio((first + response.drag_delta().x) / (rect.width() - 6.0));
                }
                if response.double_clicked() {
                    open.ratio = 0.5;
                }
            }
        }
        if swap {
            self.swap_panes();
        }
        if let Some(index) = close {
            self.close_pane(index);
        }
        for (index, files, position) in drops {
            self.send_at(&ctx, files, index, position);
        }
    }
}

/// What a file's right-click menu can do to it.
enum FileChange {
    Rename,
    Duplicate,
    Recycle,
}

fn file_icon(kind: canopy_core::files::Kind) -> Icon {
    match kind {
        canopy_core::files::Kind::Image => Icon::Image,
        canopy_core::files::Kind::Video => Icon::Video,
        canopy_core::files::Kind::Audio => Icon::Audio,
        canopy_core::files::Kind::Project => Icon::Project,
        canopy_core::files::Kind::Archive => Icon::Archive,
        canopy_core::files::Kind::File => Icon::File,
    }
}

fn toggle_full_screen(ctx: &egui::Context) {
    let full = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!full));
}

/// The clock in the top bar ("9:57 PM"), in local time.
fn chrono_like_clock() -> String {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let offset = local_offset_seconds();
    let seconds = (now as i64 + offset).rem_euclid(86_400);
    let (hour, minute) = (seconds / 3600, seconds / 60 % 60);
    let suffix = if hour < 12 { "AM" } else { "PM" };
    format!("{}:{minute:02} {suffix}", if hour % 12 == 0 { 12 } else { hour % 12 })
}

#[cfg(windows)]
fn local_offset_seconds() -> i64 {
    #[repr(C)]
    struct TimeZoneInformation { bias: i32, _rest: [u8; 168] }
    unsafe extern "system" {
        fn GetTimeZoneInformation(info: *mut TimeZoneInformation) -> u32;
    }
    let mut info = TimeZoneInformation { bias: 0, _rest: [0; 168] };
    // SAFETY: GetTimeZoneInformation fills a TIME_ZONE_INFORMATION (172 bytes) we own.
    let kind = unsafe { GetTimeZoneInformation(&mut info) };
    let daylight_bias = i32::from_le_bytes([info._rest[164], info._rest[165], info._rest[166], info._rest[167]]);
    let bias = info.bias + if kind == 2 { daylight_bias } else { 0 };
    -(bias as i64) * 60
}

#[cfg(not(windows))]
fn local_offset_seconds() -> i64 {
    0
}

impl eframe::App for Shell {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // Ctrl+K opens quick open (and closes it); F11 fills the screen.
        let (find, full) = ctx.input_mut(|i| (i.consume_key(Modifiers::CTRL, Key::K), i.consume_key(Modifiers::NONE, Key::F11)));
        if find {
            self.quick = if self.quick.is_some() || self.space_form.is_some() || self.rename_file.is_some() { None } else { Some(quick::State::default()) };
        }
        if full {
            toggle_full_screen(&ctx);
        }
        if self.open.is_some() {
            self.space(ui);
        } else {
            self.home(ui);
        }
        self.space_form_dialog(ui);
        self.rename_file_dialog(ui);
        self.quick_open(&ctx);
        // The window is named after the space, so the taskbar says where you are.
        let title = self.open.as_ref().map(|open| format!("{} · Canopy", open.space.name)).unwrap_or_else(|| "Canopy".into());
        if title != self.window_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.window_title = title;
        }
        ctx.request_repaint_after(Duration::from_secs(15));
    }

    fn on_exit(&mut self) {
        self.close_space();
        canopy::videochat::shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A shell with a space open and three apps that aren't native, so no app starts.
    fn shell() -> Shell {
        let dir = std::env::temp_dir().join("canopy-shell-tests");
        let space = Space { id: "test".into(), name: "Test".into(), color: "leaf".into(), created_at: 0, dir: dir.clone(), last_opened_at: 0 };
        let slots = ["a", "b", "c"]
            .map(|name| Slot { name, description: "", accent: Accent::Leaf, initial: "A", kind: Kind::External { exe: PathBuf::new(), accepts: |_| true } })
            .into();
        let mut open = Open::new(space, Layout::default());
        open.pending = None;
        Shell {
            spaces: Spaces::new(dir.join("spaces.json"), &dir), slots, open: Some(open), toasts: Vec::new(), opens: Vec::new(), space_form: None, rename_file: None, quick: None,
            prefs: Prefs::default(), prefs_file: dir.join("shell.json"), window_title: String::new(), home_note: String::new(),
        }
    }

    #[test]
    fn a_space_comes_back_with_the_apps_it_showed() {
        let ctx = egui::Context::default();
        let mut shell = shell();
        shell.show_app(&ctx, 2);
        shell.set_split(&ctx, true);
        shell.swap_panes();
        let open = shell.open.as_mut().unwrap();
        open.ratio = 0.4;
        let layout = open.layout(&shell.slots);
        assert_eq!(layout, Layout { visible: vec!["a".into(), "c".into()], focused: Some("c".into()), split: true, ratio: 0.4 });

        let mut again = self::shell();
        again.restore(&ctx, layout);
        assert_eq!(panes(&again), (vec![0, 2], Some(2)));
        assert!(again.open.as_ref().unwrap().split);

        // An app that's gone is left out; without split only the first app shows.
        let mut other = self::shell();
        other.restore(&ctx, Layout { visible: vec!["gone".into(), "b".into(), "c".into()], focused: Some("c".into()), split: false, ratio: 0.5 });
        assert_eq!(panes(&other), (vec![1], Some(1)));
    }

    #[test]
    fn closing_an_app_leaves_the_other_in_use() {
        let ctx = egui::Context::default();
        let mut shell = shell();
        shell.show_app(&ctx, 0);
        shell.set_split(&ctx, true);
        shell.close_pane(0);
        assert_eq!(panes(&shell), (vec![1], Some(1)));
        shell.close_pane(1);
        assert_eq!(panes(&shell), (vec![], None));
    }

    fn panes(shell: &Shell) -> (Vec<usize>, Option<usize>) {
        let open = shell.open.as_ref().unwrap();
        (open.visible.clone(), open.focused)
    }

    #[test]
    fn one_pane_shows_the_app_you_pick() {
        let ctx = egui::Context::default();
        let mut shell = shell();
        shell.show_app(&ctx, 1);
        shell.show_app(&ctx, 2);
        assert_eq!(panes(&shell), (vec![2], Some(2)));
    }

    #[test]
    fn split_adds_an_app_and_picking_another_replaces_the_one_not_in_use() {
        let ctx = egui::Context::default();
        let mut shell = shell();
        shell.show_app(&ctx, 0);
        shell.set_split(&ctx, true);
        assert_eq!(panes(&shell), (vec![0, 1], Some(0)));
        shell.show_app(&ctx, 2);
        assert_eq!(panes(&shell), (vec![0, 2], Some(2)));
        shell.set_split(&ctx, false);
        assert_eq!(panes(&shell), (vec![2], Some(2)));
    }

    #[test]
    fn sending_to_an_app_that_isnt_native_yet_shows_it_and_says_so() {
        let ctx = egui::Context::default();
        let mut shell = shell();
        shell.send(&ctx, vec![SharedFile::new("C:/clips/clip.mp4")], 1);
        assert_eq!(panes(&shell), (vec![1], Some(1)));
        assert!(shell.toasts.last().is_some_and(|toast| toast.text.contains("isn't in Canopy yet")));
    }
}
