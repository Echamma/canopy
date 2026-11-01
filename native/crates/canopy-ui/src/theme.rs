//! Canopy Night's tokens (see the Canopy Night design system) as egui colors, fonts and style.

use egui::{Color32, FontData, FontDefinitions, FontFamily, FontId, TextStyle};
use std::sync::Arc;

const fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied_const(r, g, b, (a * 255.0) as u8)
}

/// Colors that are the same in both moods.
pub mod color {
    use super::rgba;
    use egui::Color32;
    pub const VEIL: Color32 = rgba(238, 242, 234, 0.06);
    pub const VEIL_HOVER: Color32 = rgba(238, 242, 234, 0.11);
    pub const VEIL_STRONG: Color32 = rgba(238, 242, 234, 0.16);
    pub const HAIRLINE: Color32 = rgba(238, 242, 234, 0.10);
    pub const HAIRLINE_STRONG: Color32 = rgba(238, 242, 234, 0.14);
    pub const INK: Color32 = Color32::from_rgb(0xEE, 0xF2, 0xEA);
    pub const INK_SOFT: Color32 = Color32::from_rgb(0xC9, 0xD1, 0xCB);
    pub const INK_MUTED: Color32 = Color32::from_rgb(0xA7, 0xB3, 0xAA);
    pub const INK_FAINT: Color32 = Color32::from_rgb(0x7E, 0x8A, 0x81);
    pub const SELECT_FILL: Color32 = rgba(140, 198, 63, 0.16);
    pub const SELECT_LINE: Color32 = rgba(140, 198, 63, 0.60);
    pub const SELECT_INK: Color32 = Color32::from_rgb(0xBF, 0xE3, 0xA0);
    pub const WARM_FILL: Color32 = rgba(217, 164, 65, 0.22);
    pub const WARM_FILL_HOVER: Color32 = rgba(217, 164, 65, 0.32);
    pub const CTA: Color32 = Color32::from_rgb(0x2B, 0x5A, 0x3C);
    pub const CTA_HOVER: Color32 = Color32::from_rgb(0x2F, 0x6B, 0x45);
    pub const SUN: Color32 = Color32::from_rgb(0xD9, 0xA4, 0x41);
    pub const LEAF: Color32 = Color32::from_rgb(0x8C, 0xC6, 0x3F);
    pub const DANGER: Color32 = Color32::from_rgb(0xF0, 0x8A, 0x74);
    pub const SHADOW: Color32 = rgba(0, 0, 0, 0.55);
}

/// Ember is warm (a space, the work itself); tide is cool (choosing a space, setting up).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mood {
    Ember,
    Tide,
}

impl Mood {
    pub fn ground(self) -> Color32 {
        match self {
            Mood::Ember => Color32::from_rgb(0x0C, 0x0A, 0x05),
            Mood::Tide => Color32::from_rgb(0x04, 0x09, 0x0B),
        }
    }
    /// Translucent surface for panes, the sidebar and toolbars.
    pub fn pane(self) -> Color32 {
        match self {
            Mood::Ember => rgba(24, 20, 10, 0.60),
            Mood::Tide => rgba(18, 26, 21, 0.72),
        }
    }
    /// Near-opaque surface for popovers and dialogs.
    pub fn pane_solid(self) -> Color32 {
        match self {
            Mood::Ember => rgba(24, 20, 12, 0.95),
            Mood::Tide => rgba(18, 26, 21, 0.92),
        }
    }
}

/// Each app keeps one accent: its icon, its tab, a few highlights.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accent {
    Leaf,
    Sky,
    Clay,
    Berry,
    Moss,
}

impl Accent {
    pub fn color(self) -> Color32 {
        match self {
            Accent::Leaf => Color32::from_rgb(0x8C, 0xC6, 0x3F),
            Accent::Sky => Color32::from_rgb(0x2A, 0x75, 0x96),
            Accent::Clay => Color32::from_rgb(0xB8, 0x5A, 0x2E),
            Accent::Berry => Color32::from_rgb(0x8E, 0x46, 0x77),
            Accent::Moss => Color32::from_rgb(0x2F, 0x6B, 0x45),
        }
    }
    /// Text and icons drawn on the accent.
    pub fn ink(self) -> Color32 {
        match self {
            Accent::Leaf => Color32::from_rgb(0x24, 0x40, 0x0C),
            _ => Color32::WHITE,
        }
    }
    /// The space colors Canopy stores by name (`"leaf"`, `"sky"`, …).
    pub fn from_name(name: &str) -> Accent {
        match name {
            "sky" => Accent::Sky,
            "clay" => Accent::Clay,
            "berry" => Accent::Berry,
            "moss" => Accent::Moss,
            _ => Accent::Leaf,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Accent::Leaf => "leaf",
            Accent::Sky => "sky",
            Accent::Clay => "clay",
            Accent::Berry => "berry",
            Accent::Moss => "moss",
        }
    }
    pub const ALL: [Accent; 5] = [Accent::Leaf, Accent::Sky, Accent::Clay, Accent::Berry, Accent::Moss];
}

/// Nunito ExtraBold for headings, Nunito Sans for everything you read and click.
pub mod font {
    use egui::{FontFamily, FontId};
    pub fn display(size: f32) -> FontId {
        FontId::new(size, FontFamily::Name("display".into()))
    }
    /// Reading text in panes (Nunito Sans Regular).
    pub fn body(size: f32) -> FontId {
        FontId::new(size, FontFamily::Name("body".into()))
    }
    /// Chips, pills, rows and small buttons (Nunito Sans SemiBold, egui's default).
    pub fn ui(size: f32) -> FontId {
        FontId::new(size, FontFamily::Proportional)
    }
    /// Primary buttons and labels (Nunito Sans Bold).
    pub fn bold(size: f32) -> FontId {
        FontId::new(size, FontFamily::Name("bold".into()))
    }
}

/// The face captions use: Nunito ExtraBold (`font::display` on screen), also handed to FFmpeg when an export burns
/// captions in. Its name table says "Nunito ExtraLight", left over from the variable font it was cut from, and that is
/// the name FFmpeg finds it by.
pub const CAPTION_FONT: (&str, &[u8]) = ("Nunito ExtraLight", include_bytes!("../fonts/Nunito-ExtraBold.ttf"));

macro_rules! font_files {
    ($($name:literal),* $(,)?) => {
        [$(($name, include_bytes!(concat!("../fonts/", $name, ".ttf")).as_slice())),*]
    };
}

fn fonts() -> FontDefinitions {
    let files = font_files!(
        "Nunito-ExtraBold", "Nunito-ExtraBold-LatinExt",
        "NunitoSans-Regular", "NunitoSans-Regular-LatinExt",
        "NunitoSans-SemiBold", "NunitoSans-SemiBold-LatinExt",
        "NunitoSans-Bold", "NunitoSans-Bold-LatinExt",
    );
    let mut definitions = FontDefinitions::default();
    for (name, bytes) in files {
        definitions.font_data.insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    }
    // egui's own fonts stay last in each list for symbols Nunito doesn't have.
    let fallback = definitions.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let family = |main: &str| {
        let mut list = vec![main.to_owned(), format!("{main}-LatinExt")];
        list.extend(fallback.iter().cloned());
        list
    };
    definitions.families.insert(FontFamily::Proportional, family("NunitoSans-SemiBold"));
    definitions.families.insert(FontFamily::Name("display".into()), family("Nunito-ExtraBold"));
    definitions.families.insert(FontFamily::Name("body".into()), family("NunitoSans-Regular"));
    definitions.families.insert(FontFamily::Name("bold".into()), family("NunitoSans-Bold"));
    definitions
}

/// Applies Canopy Night to a context: fonts, text sizes, dark widgets with veils and hairlines.
pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(fonts());
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Heading, font::display(20.0)),
            (TextStyle::Body, font::ui(14.0)),
            (TextStyle::Button, font::ui(14.0)),
            (TextStyle::Small, font::ui(12.0)),
            (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        style.spacing.interact_size.y = 30.0;
        let visuals = &mut style.visuals;
        visuals.dark_mode = true;
        visuals.override_text_color = None;
        visuals.panel_fill = Color32::TRANSPARENT;
        visuals.window_fill = Mood::Ember.pane_solid();
        visuals.window_stroke = egui::Stroke::new(1.0, color::HAIRLINE_STRONG);
        visuals.window_corner_radius = 12.into();
        visuals.menu_corner_radius = 12.into();
        visuals.extreme_bg_color = color::VEIL;
        visuals.faint_bg_color = color::VEIL;
        visuals.code_bg_color = color::VEIL;
        visuals.hyperlink_color = color::SELECT_INK;
        visuals.selection.bg_fill = color::SELECT_FILL;
        visuals.selection.stroke = egui::Stroke::new(1.0, color::SELECT_INK);
        visuals.text_cursor.stroke = egui::Stroke::new(2.0, color::LEAF);
        let widgets = &mut visuals.widgets;
        for (state, fill, text) in [
            (&mut widgets.noninteractive, Color32::TRANSPARENT, color::INK),
            (&mut widgets.inactive, color::VEIL, color::INK),
            (&mut widgets.hovered, color::VEIL_HOVER, color::INK),
            (&mut widgets.active, color::VEIL_STRONG, color::INK),
            (&mut widgets.open, color::VEIL_STRONG, color::INK),
        ] {
            state.bg_fill = fill;
            state.weak_bg_fill = fill;
            state.bg_stroke = egui::Stroke::new(1.0, color::HAIRLINE);
            state.fg_stroke = egui::Stroke::new(1.5, text);
            state.corner_radius = 8.into();
            state.expansion = 0.0;
        }
        widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, color::HAIRLINE);
        widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, color::INK_SOFT);
        widgets.hovered.bg_stroke = egui::Stroke::new(1.0, color::HAIRLINE_STRONG);
    });
}
