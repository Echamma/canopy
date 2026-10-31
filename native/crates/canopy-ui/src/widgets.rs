//! Canopy Night's controls: pills, icon buttons, the one filled action per screen, quiet buttons and list rows.

use crate::paint::{Icon, icon, leaf};
use crate::theme::{Accent, Mood, color, font};
use egui::{Align2, Color32, CursorIcon, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2, vec2};

fn galley_width(ui: &Ui, text: &str, font: egui::FontId) -> f32 {
    ui.painter().layout_no_wrap(text.to_owned(), font, color::INK).size().x
}

fn focus_ring(ui: &Ui, response: &Response, rect: Rect, radius: u8) {
    if response.has_focus() {
        ui.painter().rect_stroke(rect.expand(2.0), radius + 2, Stroke::new(2.0, color::LEAF), StrokeKind::Outside);
    }
}

/// A 30 px pill: veil fill, hairline edge, an optional leaf for an app or space, and a selected state.
pub fn pill(ui: &mut Ui, text: &str, accent: Option<Accent>, selected: bool) -> Response {
    let font = if selected { font::bold(14.0) } else { font::ui(14.0) };
    let width = galley_width(ui, text, font.clone()) + 20.0 + if accent.is_some() { 22.0 } else { 0.0 };
    let (rect, response) = ui.allocate_exact_size(vec2(width, 30.0), Sense::click());
    let fill = if selected { color::VEIL_STRONG } else if response.hovered() { color::VEIL_HOVER } else { color::VEIL };
    let painter = ui.painter();
    painter.rect_filled(rect, 8, fill);
    painter.rect_stroke(rect, 8, Stroke::new(1.0, color::HAIRLINE), StrokeKind::Inside);
    let mut x = rect.left() + 10.0;
    if let Some(accent) = accent {
        leaf(painter, Rect::from_center_size(egui::pos2(x + 7.0, rect.center().y), Vec2::splat(14.0)), accent.color());
        x += 22.0;
    }
    let ink = if selected || response.hovered() { color::INK } else { color::INK_SOFT };
    painter.text(egui::pos2(x, rect.center().y), Align2::LEFT_CENTER, text, font, ink);
    focus_ring(ui, &response, rect, 8);
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// A 30 px square icon button. `pressed` shows a toggle that is on.
pub fn icon_button(ui: &mut Ui, which: Icon, label: &str, pressed: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::click());
    let painter = ui.painter();
    if pressed {
        painter.rect_filled(rect, 8, color::SELECT_FILL);
        painter.rect_stroke(rect, 8, Stroke::new(1.0, color::SELECT_LINE), StrokeKind::Inside);
    } else if response.hovered() {
        painter.rect_filled(rect, 8, color::VEIL_HOVER);
    }
    let ink = if pressed { color::SELECT_INK } else if response.hovered() { color::INK } else { color::INK_SOFT };
    icon(painter, Rect::from_center_size(rect.center(), Vec2::splat(16.0)), which, ink);
    focus_ring(ui, &response, rect, 8);
    let label = label.to_owned();
    response.on_hover_text(label).on_hover_cursor(CursorIcon::PointingHand)
}

/// The one filled button on a screen: amber in ember, green in tide.
pub fn primary_button(ui: &mut Ui, text: &str, mood: Mood) -> Response {
    let width = galley_width(ui, text, font::bold(15.0)) + 36.0;
    let (rect, response) = ui.allocate_exact_size(vec2(width, 40.0), Sense::click());
    let fill = match (mood, response.hovered()) {
        (Mood::Ember, false) => color::WARM_FILL,
        (Mood::Ember, true) => color::WARM_FILL_HOVER,
        (Mood::Tide, false) => color::CTA,
        (Mood::Tide, true) => color::CTA_HOVER,
    };
    ui.painter().rect_filled(rect, 10, fill);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, font::bold(15.0), color::INK);
    focus_ring(ui, &response, rect, 10);
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// A small outlined button for secondary actions ("Add files", "Try again").
pub fn quiet_button(ui: &mut Ui, text: &str, which: Option<Icon>) -> Response {
    let width = galley_width(ui, text, font::ui(14.0)) + 24.0 + if which.is_some() { 20.0 } else { 0.0 };
    let (rect, response) = ui.allocate_exact_size(vec2(width, 30.0), Sense::click());
    let painter = ui.painter();
    if response.hovered() {
        painter.rect_filled(rect, 8, color::VEIL_HOVER);
    }
    painter.rect_stroke(rect, 8, Stroke::new(1.0, color::HAIRLINE_STRONG), StrokeKind::Inside);
    let mut x = rect.left() + 12.0;
    if let Some(which) = which {
        icon(painter, Rect::from_center_size(egui::pos2(x + 7.0, rect.center().y), Vec2::splat(14.0)), which, color::INK);
        x += 20.0;
    }
    painter.text(egui::pos2(x, rect.center().y), Align2::LEFT_CENTER, text, font::ui(14.0), color::INK);
    focus_ring(ui, &response, rect, 8);
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// A caption-sized label over a group ("Projects", "Files").
pub fn section_label(ui: &mut Ui, text: &str) {
    ui.label(egui::RichText::new(text).font(font::ui(12.0)).color(color::INK_MUTED));
}

/// A two-line list row with a leading mark: the sidebar's projects and files.
pub struct Row<'a> {
    pub title: &'a str,
    pub detail: &'a str,
    pub mark: RowMark,
    pub highlighted: bool,
}

pub enum RowMark {
    Leaf(Accent),
    Icon(Icon),
}

impl Row<'_> {
    pub fn show(self, ui: &mut Ui) -> Response {
        let width = ui.available_width();
        let (rect, response) = ui.allocate_exact_size(vec2(width, 40.0), Sense::click_and_drag());
        let painter = ui.painter();
        let fill = if self.highlighted { color::SELECT_FILL } else if response.hovered() { color::VEIL_HOVER } else { color::VEIL };
        painter.rect_filled(rect, 8, fill);
        if self.highlighted {
            painter.rect_stroke(rect, 8, Stroke::new(1.0, color::SELECT_LINE), StrokeKind::Inside);
        }
        let mark = Rect::from_center_size(egui::pos2(rect.left() + 18.0, rect.center().y), Vec2::splat(16.0));
        match self.mark {
            RowMark::Leaf(accent) => leaf(painter, mark, accent.color()),
            RowMark::Icon(which) => icon(painter, mark, which, color::INK_SOFT),
        }
        let text_left = rect.left() + 36.0;
        let clip = Rect::from_min_max(egui::pos2(text_left, rect.top()), egui::pos2(rect.right() - 8.0, rect.bottom()));
        let painter = painter.with_clip_rect(clip);
        painter.text(egui::pos2(text_left, rect.top() + 6.0), Align2::LEFT_TOP, self.title, font::ui(14.0), color::INK);
        painter.text(egui::pos2(text_left, rect.bottom() - 6.0), Align2::LEFT_BOTTOM, self.detail, font::ui(12.0), color::INK_MUTED);
        focus_ring(ui, &response, rect, 8);
        response
    }
}

/// Human-sized bytes: 820 KB, 45.8 MB.
pub fn format_size(bytes: u64) -> String {
    let b = bytes as f64;
    if b < 1024.0 {
        format!("{bytes} B")
    } else if b < 1024.0 * 1024.0 {
        format!("{:.0} KB", b / 1024.0)
    } else if b < 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} MB", b / 1024.0 / 1024.0)
    } else {
        format!("{:.2} GB", b / 1024.0 / 1024.0 / 1024.0)
    }
}

/// Text in one of Canopy's colors, for labels that aren't controls.
pub fn muted(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text.into()).color(color::INK_MUTED)
}

pub const TRANSPARENT: Color32 = Color32::TRANSPARENT;
