//! The shell: "Where to?" (your spaces) and a space (app tabs, the sidebar of projects and files, one or two panes).
//! Every app draws natively inside its pane.

use canopy_core::files::{FileEntry, copy_into, list_files};
use canopy_core::spaces::{Space, SpacePaths, Spaces};
use canopy_ui::{
    Accent, AppDescriptor, CanopyApp, Document, DraggedFiles, Host, Icon, Mood, Row, RowMark, SharedFile, SpaceInfo, color, font, format_size, icon_button,
    leaf, leaf_with_initial, paint_ground, pill, primary_button, quiet_button, section_label,
};
use egui::{Align2, DragAndDrop, Key, Modifiers, Rect, Sense, Ui, UiBuilder, pos2, vec2};
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
}

struct NewSpace {
    name: String,
    color: Accent,
    parent: Option<PathBuf>,
    error: String,
}

pub struct Shell {
    spaces: Spaces,
    slots: Vec<Slot>,
    open: Option<Open>,
    toasts: Vec<Toast>,
    opens: Vec<(usize, String)>,
    new_space: Option<NewSpace>,
    home_note: String,
}

impl Shell {
    pub fn new() -> Self {
        let profile = std::env::var_os("CANOPY_PROFILE").map(PathBuf::from).or_else(|| dirs::data_dir().map(|d| d.join("Canopy"))).unwrap_or_else(std::env::temp_dir);
        let root = std::env::var_os("CANOPY_SPACES_DIR").map(PathBuf::from).or_else(|| dirs::document_dir().map(|d| d.join("Canopy"))).unwrap_or_else(std::env::temp_dir);
        Self { spaces: Spaces::new(profile.join("spaces.json"), root), slots: catalog(), open: None, toasts: Vec::new(), opens: Vec::new(), new_space: None, home_note: String::new() }
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
                let info = SpaceInfo { id: space.id.clone(), name: space.name.clone(), dir: space.dir.clone() };
                self.open = Some(Open { space, info, apps: HashMap::new(), visible: Vec::new(), focused: None, split: false, files: Vec::new(), documents: Vec::new(), listed_at: Instant::now() - Duration::from_secs(60), search: String::new() });
            }
            Err(error) => self.home_note = error.0,
        }
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
                if let Some(dir) = rfd::FileDialog::new().set_title("Open a Canopy space").pick_folder() {
                    match self.spaces.add(&dir) {
                        Ok(space) => self.open_space(&space.id),
                        Err(error) => self.home_note = error.0,
                    }
                }
            }
        });
        let spaces = self.spaces.list();
        let tile = 96.0;
        let count = spaces.len() + 1;
        let row_width = count as f32 * tile + (count - 1) as f32 * 24.0;
        let top = rect.center().y - 40.0;
        ui.painter().text(pos2(rect.center().x, top - 30.0), Align2::CENTER_CENTER, "Where to?", font::display(26.0), color::INK);
        let mut x = rect.center().x - row_width / 2.0;
        let mut open = None;
        let mut forget = None;
        for (space, available) in &spaces {
            let area = Rect::from_min_size(pos2(x, top), vec2(tile, 96.0));
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
                if ui.button("Show in Explorer").clicked() {
                    let _ = std::process::Command::new("explorer").arg(&dir).spawn();
                }
                if ui.button("Remove from this list").clicked() {
                    forget = Some(id.clone());
                }
            });
            x += tile + 24.0;
        }
        let area = Rect::from_min_size(pos2(x, top), vec2(tile, 96.0));
        let response = ui.interact(area, ui.scope_id().with("new-space"), Sense::click());
        let mark = Rect::from_center_size(pos2(area.center().x, area.top() + 34.0), vec2(68.0, 68.0));
        let edge = if response.hovered() { color::SELECT_LINE } else { egui::Color32::from_rgba_unmultiplied(238, 242, 234, 72) };
        ui.painter().rect_stroke(mark, 12, egui::Stroke::new(1.5, edge), egui::StrokeKind::Inside);
        canopy_ui::icon(ui.painter(), Rect::from_center_size(mark.center(), vec2(20.0, 20.0)), Icon::Plus, color::INK_MUTED);
        ui.painter().text(pos2(area.center().x, area.top() + 84.0), Align2::CENTER_CENTER, "New space", font::ui(13.0), color::INK_MUTED);
        if response.clicked() {
            self.new_space = Some(NewSpace { name: String::new(), color: Accent::Leaf, parent: None, error: String::new() });
        }
        let note = if self.home_note.is_empty() && spaces.is_empty() { "A space keeps your apps' work together, with the files they share." } else { &self.home_note };
        ui.painter().text(pos2(rect.center().x, top + 130.0), Align2::CENTER_CENTER, note, font::ui(13.0), color::INK_MUTED);
        if let Some(id) = forget {
            let _ = self.spaces.forget(&id);
        }
        if let Some(id) = open {
            self.open_space(&id);
        }
    }

    fn new_space_dialog(&mut self, ui: &mut Ui) {
        let Some(form) = &mut self.new_space else { return };
        let mut create = false;
        let modal = egui::Modal::new(ui.scope_id().with("new-space-dialog")).show(ui.ctx(), |ui| {
            ui.set_width(400.0);
            ui.label(egui::RichText::new("Name your space").font(font::display(22.0)));
            ui.label(canopy_ui::muted("A space keeps your Forma, Folio and StreamCuts work together, with the files they share."));
            ui.add_space(10.0);
            let name = ui.add(egui::TextEdit::singleline(&mut form.name).hint_text("Client work").desired_width(f32::INFINITY).margin(vec2(10.0, 8.0)));
            name.request_focus();
            if name.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                create = true;
            }
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
            ui.horizontal(|ui| {
                let place = form.parent.clone().map(|p| p.display().to_string()).unwrap_or_else(|| self.spaces.default_root().display().to_string());
                ui.label(canopy_ui::muted(format!("Kept in {place}")));
                if quiet_button(ui, "Change", None).clicked() {
                    form.parent = rfd::FileDialog::new().set_title("Choose where to keep this space").pick_folder().or(form.parent.clone());
                }
            });
            if !form.error.is_empty() {
                ui.colored_label(color::DANGER, &form.error);
            }
            ui.add_space(8.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if primary_button(ui, "Create space", Mood::Tide).clicked() {
                    create = true;
                }
                if quiet_button(ui, "Back", None).clicked() {
                    form.error = "\u{0}".into();
                }
            });
        });
        if modal.should_close() || form.error == "\u{0}" {
            self.new_space = None;
            return;
        }
        if create {
            let dir = form.parent.as_ref().map(|p| p.join(canopy_core::files::safe_file_name(form.name.trim())));
            match self.spaces.create(&form.name, form.color.name(), dir) {
                Ok(space) => {
                    self.new_space = None;
                    self.open_space(&space.id);
                }
                Err(error) => form.error = error.0,
            }
        }
    }

    // ---- A space ----
    fn space(&mut self, ui: &mut Ui) {
        self.refresh();
        let ctx = ui.ctx().clone();
        let rect = ui.max_rect();
        paint_ground(ui.painter(), rect, Mood::Ember);

        // Ctrl+Tab cycles through the apps; Ctrl+1-4 jump to one.
        let (cycle, jump) = ctx.input_mut(|i| {
            let back = i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::Tab);
            let forward = !back && i.consume_key(Modifiers::CTRL, Key::Tab);
            let jump = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6].iter().position(|k| i.consume_key(Modifiers::CTRL, *k));
            (if back { -1 } else if forward { 1 } else { 0 }, jump)
        });
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
        let sidebar = Rect::from_min_size(body.min, vec2(250.0, body.height()));
        let main = Rect::from_min_max(pos2(sidebar.right() + 6.0, body.top()), body.max);
        self.sidebar(ui, sidebar);
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

        // Files dropped from Explorer: onto the sidebar they join the space's Files; onto a pane, the app gets them.
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
        if !dropped.is_empty() {
            let at = canopy_ui::os_drop_position(&ctx).unwrap_or(sidebar.center());
            if sidebar.contains(at) || self.open.as_ref().is_some_and(|o| o.visible.is_empty()) {
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
                let Some(open) = &self.open else { return };
                let space_name = open.space.name.clone();
                let accent = Accent::from_name(&open.space.color);
                let pill_response = pill(ui, &space_name, Some(accent), false);
                let mut switch_to = None;
                let spaces = self.spaces.list();
                pill_response.context_menu(|ui| {
                    for (space, available) in &spaces {
                        if *available && ui.button(&space.name).clicked() {
                            switch_to = Some(space.id.clone());
                        }
                    }
                });
                if pill_response.clicked() {
                    self.close_space();
                    return;
                }
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
                    if icon_button(ui, Icon::Split, "Show two apps side by side", split).clicked() {
                        self.set_split(&ctx, !split);
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
                    let mark = match file.kind {
                        canopy_core::files::Kind::Image => Icon::Image,
                        canopy_core::files::Kind::Video => Icon::Video,
                        canopy_core::files::Kind::Audio => Icon::Audio,
                        canopy_core::files::Kind::Project => Icon::Project,
                        canopy_core::files::Kind::Archive => Icon::Archive,
                        canopy_core::files::Kind::File => Icon::File,
                    };
                    let size = format_size(file.size);
                    // Dragged into an app it's handed over; double-click or right-click sends it too.
                    let row = (Row { title: &file.name, detail: &size, mark: RowMark::Icon(mark), highlighted: false }).show(ui);
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
                        if ui.button("Show in folder").clicked() {
                            let _ = std::process::Command::new("explorer").arg(format!("/select,{}", shared.path.display())).spawn();
                        }
                    });
                }
            });
        });
        ui.scope_builder(UiBuilder::new().max_rect(hint), |ui| {
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                ui.label(egui::RichText::new("Drag a file into an app, or right-click it to send it.").font(font::ui(12.0)).color(color::INK_MUTED));
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
        if add {
            if let Some(paths) = rfd::FileDialog::new().set_title("Add files to this space").pick_files() {
                let count = paths.iter().filter(|p| copy_into(&files_dir, p).is_ok()).count();
                open.listed_at = Instant::now() - Duration::from_secs(60);
                self.toasts.push(Toast { text: format!("Added {count} file{} to Files", if count == 1 { "" } else { "s" }), file: None, targets: Vec::new(), at: Instant::now() });
            }
        }
        if let Some(index) = show {
            self.show_app(&ctx, index);
        }
        if let Some((index, id)) = open_doc {
            self.show_app(&ctx, index);
            if let Some(open) = &mut self.open {
                if let Some(app) = open.apps.get_mut(&index) {
                    let mut host = PaneHost { toasts: &mut self.toasts, slots: &self.slots, from: index, focused: true, opens: &mut self.opens };
                    app.open_document(&id, &mut host);
                }
            }
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
                ui.painter().text(pos2(tile.center().x, tile.bottom() + 34.0), Align2::CENTER_CENTER, slot.description, font::ui(12.0), color::INK_MUTED);
                if response.clicked() {
                    show = Some(index);
                }
            }
            ui.painter().text(pos2(center.x, center.y + 110.0), Align2::CENTER_CENTER, "Ctrl+Tab moves between apps. Ctrl+1 through Ctrl+6 jump to one.", font::ui(13.0), color::INK_MUTED);
            if let Some(index) = show {
                self.show_app(&ctx, index);
            }
            return;
        }
        let visible = open.visible.clone();
        let width = (rect.width() - 6.0 * (visible.len() as f32 - 1.0)) / visible.len() as f32;
        let mut close = None;
        let mut focus = None;
        let mut toggle_split = false;
        let mut drops = Vec::new();
        for (n, index) in visible.iter().copied().enumerate() {
            let pane = Rect::from_min_size(pos2(rect.left() + n as f32 * (width + 6.0), rect.top()), vec2(width, rect.height()));
            let focused = self.open.as_ref().and_then(|o| o.focused) == Some(index);
            ui.painter().rect_filled(pane, 10, Mood::Ember.pane());
            ui.painter().rect_stroke(pane, 10, egui::Stroke::new(1.0, if focused && visible.len() > 1 { color::HAIRLINE_STRONG } else { color::HAIRLINE }), egui::StrokeKind::Inside);
            let head = Rect::from_min_size(pane.min, vec2(pane.width(), 42.0));
            let slot = &self.slots[index];
            ui.scope_builder(UiBuilder::new().max_rect(head.shrink2(vec2(8.0, 6.0))), |ui| {
                ui.horizontal_centered(|ui| {
                    pill(ui, slot.name, Some(slot.accent), false);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icon_button(ui, Icon::Close, &format!("Close {}", slot.name), false).clicked() {
                            close = Some(index);
                        }
                        let split = self.open.as_ref().is_some_and(|o| o.split);
                        if icon_button(ui, Icon::Split, if split { "Show one app" } else { "Show two apps side by side" }, false).clicked() {
                            toggle_split = true;
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
                        let mut host = PaneHost { toasts: &mut self.toasts, slots: &self.slots, from: index, focused, opens: &mut self.opens };
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
        if let Some(index) = close {
            if let Some(open) = &mut self.open {
                if let Some(mut app) = open.apps.remove(&index) {
                    let _ = app.save();
                }
                open.visible.retain(|v| *v != index);
                open.focused = open.visible.first().copied();
            }
        }
        for (index, files, position) in drops {
            self.send_at(&ctx, files, index, position);
        }
    }
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
        if self.open.is_some() {
            self.space(ui);
        } else {
            self.home(ui);
        }
        self.new_space_dialog(ui);
        ui.ctx().request_repaint_after(Duration::from_secs(15));
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
        let info = SpaceInfo { id: space.id.clone(), name: space.name.clone(), dir: dir.clone() };
        let slots = ["a", "b", "c"]
            .map(|name| Slot { name, description: "", accent: Accent::Leaf, initial: "A", kind: Kind::External { exe: PathBuf::new(), accepts: |_| true } })
            .into();
        let open = Open { space, info, apps: HashMap::new(), visible: Vec::new(), focused: None, split: false, files: Vec::new(), documents: Vec::new(), listed_at: Instant::now(), search: String::new() };
        Shell { spaces: Spaces::new(dir.join("spaces.json"), &dir), slots, open: Some(open), toasts: Vec::new(), opens: Vec::new(), new_space: None, home_note: String::new() }
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
