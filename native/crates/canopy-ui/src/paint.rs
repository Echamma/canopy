//! Canopy Night's shapes: the glowing, dotted ground; leaf identities; rounded line icons on a 24-unit grid.

use crate::theme::{Accent, Mood, color, font};
use egui::epaint::{CornerRadius, Mesh, Shape, Stroke};
use egui::{Align2, Color32, Painter, Pos2, Rect, TextureHandle, TextureOptions, Vec2, pos2, vec2};

fn glow(painter: &Painter, center: Pos2, radius: Vec2, color: Color32) {
    // A radial gradient from `color` to clear at 70% of the radius, like the CSS ground.
    let reach = radius * 0.7;
    let mut mesh = Mesh::default();
    mesh.colored_vertex(center, color);
    const SEGMENTS: u32 = 48;
    for i in 0..=SEGMENTS {
        let angle = i as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
        mesh.colored_vertex(center + vec2(angle.cos() * reach.x, angle.sin() * reach.y), Color32::TRANSPARENT);
    }
    for i in 0..SEGMENTS {
        mesh.add_triangle(0, 1 + i, 2 + i);
    }
    painter.add(Shape::mesh(mesh));
}

fn dot_texture(ctx: &egui::Context) -> TextureHandle {
    let id = egui::Id::unique("canopy-ground-dots");
    if let Some(texture) = ctx.data(|data| data.get_temp::<TextureHandle>(id)) {
        return texture;
    }
    const PITCH: usize = 18;
    let mut pixels = vec![Color32::TRANSPARENT; PITCH * PITCH];
    for y in 0..PITCH {
        for x in 0..PITCH {
            let distance = ((x as f32 + 0.5 - 1.0).powi(2) + (y as f32 + 0.5 - 1.0).powi(2)).sqrt();
            let coverage = (1.3 - distance).clamp(0.0, 1.0);
            pixels[y * PITCH + x] = Color32::from_rgba_unmultiplied(238, 242, 234, (coverage * 0.05 * 255.0) as u8);
        }
    }
    let image = egui::ColorImage::new([PITCH, PITCH], pixels);
    let options = TextureOptions { wrap_mode: egui::TextureWrapMode::Repeat, ..TextureOptions::NEAREST };
    let texture = ctx.load_texture("canopy-ground-dots", image, options);
    ctx.data_mut(|data| data.insert_temp(id, texture.clone()));
    texture
}

/// The backdrop behind everything: the mood's ground, one or two soft glows, and the dotted grid.
pub fn paint_ground(painter: &Painter, rect: Rect, mood: Mood) {
    painter.rect_filled(rect, 0, mood.ground());
    let at = |x: f32, y: f32| pos2(rect.left() + rect.width() * x, rect.top() + rect.height() * y);
    let glows: [(Pos2, Vec2, Color32); 3] = match mood {
        Mood::Ember => [
            (at(0.55, 0.55), vec2(900.0, 700.0), Color32::from_rgba_unmultiplied(80, 60, 20, 89)),
            (at(0.30, 0.30), vec2(600.0, 400.0), Color32::from_rgba_unmultiplied(184, 90, 46, 36)),
            (at(0.65, 0.75), vec2(260.0, 300.0), Color32::from_rgba_unmultiplied(240, 200, 80, 71)),
        ],
        Mood::Tide => [
            (at(0.80, 0.10), vec2(900.0, 700.0), Color32::from_rgba_unmultiplied(31, 61, 43, 115)),
            (at(0.92, 0.98), vec2(500.0, 300.0), Color32::from_rgba_unmultiplied(217, 164, 65, 77)),
            (at(0.50, 1.05), vec2(1100.0, 500.0), Color32::from_rgba_unmultiplied(42, 117, 150, 89)),
        ],
    };
    for (center, radius, color) in glows {
        glow(painter, center, radius, color);
    }
    let texture = dot_texture(painter.ctx());
    let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(rect.width() / 18.0, rect.height() / 18.0));
    painter.image(texture.id(), rect, uv, Color32::WHITE);
}

/// The leaf shape that marks app and space identities: half-size radius on the top-left and bottom-right corners.
pub fn leaf(painter: &Painter, rect: Rect, fill: Color32) {
    let big = (rect.width() / 2.0).round() as u8;
    let small = (rect.width() / 6.0).round() as u8;
    painter.rect_filled(rect, CornerRadius { nw: big, ne: small, sw: small, se: big }, fill);
}

/// A leaf with the app's initials on it, like the launcher tiles.
pub fn leaf_with_initial(painter: &Painter, rect: Rect, accent: Accent, initial: &str) {
    leaf(painter, rect, accent.color());
    painter.text(rect.center(), Align2::CENTER_CENTER, initial, font::display(rect.height() * 0.42), accent.ink());
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Plus,
    Close,
    Chevron,
    Folder,
    FolderPlus,
    Search,
    Split,
    Image,
    Video,
    Audio,
    Archive,
    Project,
    File,
    Play,
    Pause,
    Scissors,
    Undo,
    Redo,
    Export,
    Trash,
    ZoomIn,
    ZoomOut,
    Grid,
    SkipBack,
    SkipForward,
    Magnet,
    Captions,
    Text,
    Lock,
    Sidebar,
    Swap,
}

/// Draws a rounded line icon (1.75 stroke on a 24 grid) filling `rect`.
pub fn icon(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let scale = rect.width().min(rect.height()) / 24.0;
    let origin = rect.center() - Vec2::splat(12.0 * scale);
    let p = |x: f32, y: f32| origin + vec2(x, y) * scale;
    let stroke = Stroke::new(1.75 * scale.max(0.6), color);
    let line = |points: &[(f32, f32)]| painter.add(Shape::line(points.iter().map(|&(x, y)| p(x, y)).collect(), stroke));
    let closed = |points: &[(f32, f32)]| painter.add(Shape::closed_line(points.iter().map(|&(x, y)| p(x, y)).collect(), stroke));
    let circle = |x: f32, y: f32, r: f32| painter.circle_stroke(p(x, y), r * scale, stroke);
    let filled = |points: &[(f32, f32)]| painter.add(Shape::convex_polygon(points.iter().map(|&(x, y)| p(x, y)).collect(), color, Stroke::NONE));
    let file = || {
        closed(&[(6.0, 3.0), (14.0, 3.0), (18.0, 7.0), (18.0, 21.0), (6.0, 21.0)]);
        line(&[(14.0, 3.0), (14.0, 7.0), (18.0, 7.0)]);
    };
    match icon {
        Icon::Plus => {
            line(&[(12.0, 5.0), (12.0, 19.0)]);
            line(&[(5.0, 12.0), (19.0, 12.0)]);
        }
        Icon::Close => {
            line(&[(6.0, 6.0), (18.0, 18.0)]);
            line(&[(18.0, 6.0), (6.0, 18.0)]);
        }
        Icon::Chevron => {
            line(&[(6.0, 9.0), (12.0, 15.0), (18.0, 9.0)]);
        }
        Icon::Folder => {
            closed(&[(3.0, 6.0), (9.0, 6.0), (11.0, 8.0), (21.0, 8.0), (21.0, 19.0), (3.0, 19.0)]);
        }
        Icon::FolderPlus => {
            closed(&[(3.0, 6.0), (9.0, 6.0), (11.0, 8.0), (21.0, 8.0), (21.0, 19.0), (3.0, 19.0)]);
            line(&[(12.0, 11.0), (12.0, 16.0)]);
            line(&[(9.5, 13.5), (14.5, 13.5)]);
        }
        Icon::Search => {
            circle(11.0, 11.0, 7.0);
            line(&[(20.0, 20.0), (16.5, 16.5)]);
        }
        Icon::Split => {
            painter.rect_stroke(Rect::from_min_max(p(3.0, 4.0), p(21.0, 20.0)), (2.0 * scale) as u8, stroke, egui::StrokeKind::Middle);
            line(&[(12.0, 4.0), (12.0, 20.0)]);
        }
        Icon::Image => {
            painter.rect_stroke(Rect::from_min_max(p(3.0, 4.0), p(21.0, 20.0)), (2.0 * scale) as u8, stroke, egui::StrokeKind::Middle);
            circle(9.0, 10.0, 2.0);
            line(&[(21.0, 17.0), (16.0, 12.0), (7.0, 20.0)]);
        }
        Icon::Video => {
            painter.rect_stroke(Rect::from_min_max(p(3.0, 5.0), p(16.0, 19.0)), (2.0 * scale) as u8, stroke, egui::StrokeKind::Middle);
            closed(&[(16.0, 10.0), (21.0, 7.0), (21.0, 17.0), (16.0, 14.0)]);
        }
        Icon::Audio => {
            line(&[(9.0, 18.0), (9.0, 6.0), (19.0, 4.0), (19.0, 16.0)]);
            circle(6.0, 18.0, 3.0);
            circle(16.0, 16.0, 3.0);
        }
        Icon::Archive => {
            closed(&[(4.0, 7.0), (20.0, 7.0), (20.0, 20.0), (4.0, 20.0)]);
            closed(&[(3.0, 4.0), (21.0, 4.0), (21.0, 7.0), (3.0, 7.0)]);
            line(&[(10.0, 11.0), (14.0, 11.0)]);
        }
        Icon::Project => {
            file();
            line(&[(9.0, 12.0), (15.0, 12.0)]);
            line(&[(9.0, 16.0), (15.0, 16.0)]);
        }
        Icon::File => file(),
        Icon::Play => {
            filled(&[(8.0, 5.0), (19.0, 12.0), (8.0, 19.0)]);
        }
        Icon::Pause => {
            painter.rect_filled(Rect::from_min_max(p(7.0, 5.0), p(10.0, 19.0)), (scale) as u8, color);
            painter.rect_filled(Rect::from_min_max(p(14.0, 5.0), p(17.0, 19.0)), (scale) as u8, color);
        }
        Icon::Scissors => {
            circle(6.0, 6.0, 3.0);
            circle(6.0, 18.0, 3.0);
            line(&[(8.5, 7.5), (20.0, 18.0)]);
            line(&[(8.5, 16.5), (20.0, 6.0)]);
        }
        Icon::Undo => {
            line(&[(9.0, 14.0), (4.0, 9.0), (9.0, 4.0)]);
            line(&[(4.0, 9.0), (15.0, 9.0), (18.5, 10.5), (20.0, 14.0), (18.5, 17.5), (15.0, 19.0), (11.0, 19.0)]);
        }
        Icon::Redo => {
            line(&[(15.0, 14.0), (20.0, 9.0), (15.0, 4.0)]);
            line(&[(20.0, 9.0), (9.0, 9.0), (5.5, 10.5), (4.0, 14.0), (5.5, 17.5), (9.0, 19.0), (13.0, 19.0)]);
        }
        Icon::Export => {
            line(&[(12.0, 15.0), (12.0, 3.0)]);
            line(&[(7.0, 8.0), (12.0, 3.0), (17.0, 8.0)]);
            line(&[(5.0, 15.0), (5.0, 21.0), (19.0, 21.0), (19.0, 15.0)]);
        }
        Icon::Trash => {
            line(&[(4.0, 6.0), (20.0, 6.0)]);
            line(&[(6.0, 6.0), (7.0, 21.0), (17.0, 21.0), (18.0, 6.0)]);
            line(&[(9.0, 6.0), (9.0, 3.0), (15.0, 3.0), (15.0, 6.0)]);
        }
        Icon::ZoomIn | Icon::ZoomOut => {
            circle(11.0, 11.0, 7.0);
            line(&[(20.0, 20.0), (16.5, 16.5)]);
            line(&[(8.0, 11.0), (14.0, 11.0)]);
            if icon == Icon::ZoomIn {
                line(&[(11.0, 8.0), (11.0, 14.0)]);
            }
        }
        Icon::Grid => {
            for (x, y) in [(4.0, 4.0), (13.0, 4.0), (4.0, 13.0), (13.0, 13.0)] {
                painter.rect_filled(Rect::from_min_max(p(x, y), p(x + 7.0, y + 7.0)), (1.5 * scale) as u8, color);
            }
        }
        Icon::Captions => {
            painter.rect_stroke(Rect::from_min_max(p(3.0, 5.0), p(21.0, 19.0)), (2.0 * scale) as u8, stroke, egui::StrokeKind::Middle);
            line(&[(7.0, 11.0), (11.0, 11.0)]);
            line(&[(13.0, 11.0), (17.0, 11.0)]);
            line(&[(7.0, 15.0), (15.0, 15.0)]);
        }
        Icon::Lock => {
            painter.rect_stroke(Rect::from_min_max(p(5.0, 11.0), p(19.0, 21.0)), (2.0 * scale) as u8, stroke, egui::StrokeKind::Middle);
            line(&[(8.0, 11.0), (8.0, 7.5), (9.2, 4.8), (12.0, 3.6), (14.8, 4.8), (16.0, 7.5), (16.0, 11.0)]);
        }
        Icon::Sidebar => {
            painter.rect_stroke(Rect::from_min_max(p(3.0, 4.0), p(21.0, 20.0)), (2.0 * scale) as u8, stroke, egui::StrokeKind::Middle);
            line(&[(9.0, 4.0), (9.0, 20.0)]);
        }
        Icon::Swap => {
            line(&[(4.0, 8.0), (20.0, 8.0)]);
            line(&[(16.0, 4.0), (20.0, 8.0), (16.0, 12.0)]);
            line(&[(20.0, 16.0), (4.0, 16.0)]);
            line(&[(8.0, 12.0), (4.0, 16.0), (8.0, 20.0)]);
        }
        Icon::Text => {
            line(&[(6.0, 6.0), (18.0, 6.0)]);
            line(&[(12.0, 6.0), (12.0, 19.0)]);
            line(&[(9.0, 19.0), (15.0, 19.0)]);
        }
        Icon::SkipBack => {
            line(&[(6.0, 5.0), (6.0, 19.0)]);
            filled(&[(19.0, 5.0), (19.0, 19.0), (8.0, 12.0)]);
        }
        Icon::SkipForward => {
            line(&[(18.0, 5.0), (18.0, 19.0)]);
            filled(&[(5.0, 5.0), (16.0, 12.0), (5.0, 19.0)]);
        }
        Icon::Magnet => {
            line(&[(6.0, 4.0), (6.0, 13.0), (8.0, 17.5), (12.0, 19.0), (16.0, 17.5), (18.0, 13.0), (18.0, 4.0)]);
            line(&[(6.0, 8.0), (10.0, 8.0)]);
            line(&[(14.0, 8.0), (18.0, 8.0)]);
            line(&[(10.0, 4.0), (10.0, 13.0), (12.0, 15.0), (14.0, 13.0), (14.0, 4.0)]);
        }
    }
}

/// A hairline-bordered translucent surface (panes, the sidebar, toolbars).
pub fn pane_frame(mood: Mood) -> egui::Frame {
    egui::Frame::new().fill(mood.pane()).stroke(Stroke::new(1.0, color::HAIRLINE)).corner_radius(10)
}
