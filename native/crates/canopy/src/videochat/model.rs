//! What a Video Chat project holds, as the web page saves it in the engine (`state.json`): the chat, the clip plan, the
//! rendered files and the studio settings. Fields this pane doesn't know are kept as they were, so the web page and
//! this pane can take turns on one project.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub role: String,
    pub content: String,
}

/// A spoken word, in seconds from the start of its Short.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Word {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

/// What shows on screen at once: a few words.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Line {
    pub words: Vec<Word>,
}

impl Line {
    pub fn text(&self) -> String {
        self.words.iter().map(|w| w.text.trim()).collect::<Vec<_>>().join(" ")
    }

    /// The same span with new words, its time shared out by their length (as the web page rewords a line).
    pub fn reworded(&self, text: &str) -> Line {
        let tokens: Vec<&str> = text.split_whitespace().collect();
        let (Some(first), Some(last)) = (self.words.first(), self.words.last()) else {
            return Line::default();
        };
        if tokens.is_empty() {
            return Line::default();
        }
        let (start, end) = (first.start, last.end);
        let total: usize = tokens.iter().map(|t| t.chars().count()).sum();
        let mut cursor = start;
        Line {
            words: tokens
                .iter()
                .map(|token| {
                    let span = (end - start) * token.chars().count() as f64 / total.max(1) as f64;
                    let word = Word { text: (*token).to_owned(), start: cursor, end: cursor + span };
                    cursor += span;
                    word
                })
                .collect(),
        }
    }
}

/// The line showing `seconds` into a Short, and which of its words is being said.
pub fn caption_at(lines: &[Line], seconds: f64) -> Option<(&Line, Option<usize>)> {
    lines.iter().find_map(|line| {
        let (first, last) = (line.words.first()?, line.words.last()?);
        if seconds < first.start || seconds > last.end + 0.15 {
            return None;
        }
        let active = (0..line.words.len()).find(|&i| {
            let until = line.words.get(i + 1).map_or(last.end + 0.15, |next| next.start);
            seconds >= line.words[i].start && seconds < until
        });
        Some((line, active))
    })
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scores {
    #[serde(default)]
    pub hook: f64,
    #[serde(default)]
    pub context: f64,
    #[serde(default)]
    pub impact: f64,
    #[serde(default)]
    pub completeness: f64,
    #[serde(default)]
    pub confidence: f64,
}

/// A proposed clip: a vertical Short, a topic chapter, or an archived Short (one that failed a check).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Clip {
    pub start_seconds: f64,
    pub end_seconds: f64,
    #[serde(default)]
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scores: Option<Scores>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_flags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hook_pattern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub captions: Option<Vec<Line>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive_reason: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Clip {
    pub fn length(&self) -> f64 {
        (self.end_seconds - self.start_seconds).max(0.0)
    }
    pub fn lines(&self) -> &[Line] {
        self.captions.as_deref().unwrap_or_default()
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    #[serde(default)]
    pub longer_segments: Vec<Clip>,
    #[serde(default)]
    pub shorts: Vec<Clip>,
    #[serde(default)]
    pub archived: Vec<Clip>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rendered {
    pub download_url: String,
    #[serde(default)]
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_seconds: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rendered_seconds: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Renders {
    #[serde(default)]
    pub longer_videos: Vec<Rendered>,
    #[serde(default)]
    pub shorts: Vec<Rendered>,
}

/// A rectangle on the source picture, as fractions of its width and height.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Region {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

pub const FULL_FRAME: Region = Region { x: 0.0, y: 0.0, w: 1.0, h: 1.0 };

impl Region {
    pub fn is_full(&self) -> bool {
        *self == FULL_FRAME
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Panel {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// "top" or "bottom": above or below the gameplay.
    pub position: Side,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Top,
    Bottom,
}

impl Panel {
    pub fn region(&self) -> Region {
        Region { x: self.x, y: self.y, w: self.w, h: self.h }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    /// "crop" (the middle of the picture) or "stacked" (regions above and below the gameplay).
    pub mode: String,
    #[serde(default = "full_frame")]
    pub gameplay: Region,
    #[serde(default)]
    pub panels: Vec<Panel>,
}

fn full_frame() -> Region {
    FULL_FRAME
}

impl Default for Layout {
    fn default() -> Self {
        Layout { mode: "crop".into(), gameplay: FULL_FRAME, panels: Vec::new() }
    }
}

impl Layout {
    /// Whether Shorts stack regions (a layout with no regions renders as the middle crop).
    pub fn stacked(&self) -> bool {
        self.mode == "stacked" && !self.panels.is_empty()
    }
}

/// Output pixels of a Short.
pub const SHORT_W: f32 = 1080.0;
pub const SHORT_H: f32 = 1920.0;

/// Where each piece of a stacked Short goes, top to bottom: (the source region, its top and height in Short pixels).
/// The same sums as the engine's `stackedHeights`, so the preview and StreamCuts match the render.
pub fn stacked_pieces(layout: &Layout, source_w: f32, source_h: f32) -> Vec<(Region, f32, f32)> {
    let even = |n: f32| ((n / 2.0).round() * 2.0).max(2.0);
    let count = layout.panels.len().max(1) as f32;
    let game = layout.gameplay;
    let natural = SHORT_W * game.h * source_h / (game.w * source_w).max(1e-6);
    let game_h = even(natural.min(SHORT_H - 160.0 * count));
    let panel_h = even((SHORT_H - game_h) / count);
    let mut heights = vec![panel_h; layout.panels.len()];
    if let Some(last) = heights.last_mut() {
        *last += SHORT_H - game_h - panel_h * count;
    }
    let mut pieces = Vec::new();
    for (panel, h) in layout.panels.iter().zip(&heights).filter(|(p, _)| p.position == Side::Top) {
        pieces.push((panel.region(), *h));
    }
    pieces.push((game, game_h));
    for (panel, h) in layout.panels.iter().zip(&heights).filter(|(p, _)| p.position == Side::Bottom) {
        pieces.push((panel.region(), *h));
    }
    let mut top = 0.0;
    pieces
        .into_iter()
        .map(|(region, h)| {
            let piece = (region, top, h);
            top += h;
            piece
        })
        .collect()
}

/// The part of `region` that covers a `slot_w`x`slot_h` slot: its middle, cut to the slot's shape (as FFmpeg's
/// cover-and-crop does), in fractions of the source.
pub fn cover(region: Region, source_w: f32, source_h: f32, slot_w: f32, slot_h: f32) -> Region {
    let (w, h) = (region.w * source_w, region.h * source_h);
    let slot = slot_w / slot_h;
    if w / h > slot {
        let kept = h * slot / source_w;
        Region { x: region.x + (region.w - kept) / 2.0, w: kept, ..region }
    } else {
        let kept = w / slot / source_h;
        Region { y: region.y + (region.h - kept) / 2.0, h: kept, ..region }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptionStyle {
    pub font: String,
    pub font_size: f64,
    pub text_color: String,
    pub highlight_color: String,
    pub outline_color: String,
    pub position_y: f32,
    pub uppercase: bool,
    pub words_per_line: u32,
}

impl Default for CaptionStyle {
    fn default() -> Self {
        CaptionStyle {
            font: "Arial Black".into(),
            font_size: 84.0,
            text_color: "#FFFFFF".into(),
            highlight_color: "#FFD400".into(),
            outline_color: "#000000".into(),
            position_y: 0.72,
            uppercase: true,
            words_per_line: 3,
        }
    }
}

pub const CAPTION_FONTS: [&str; 4] = ["Arial Black", "Impact", "Segoe UI Black", "Bahnschrift"];

/// `#RRGGBB` as a color (white when it isn't one).
pub fn hex_color(hex: &str) -> egui::Color32 {
    let digits = hex.trim_start_matches('#');
    let channel = |i: usize| digits.get(i..i + 2).and_then(|d| u8::from_str_radix(d, 16).ok()).unwrap_or(255);
    egui::Color32::from_rgb(channel(0), channel(2), channel(4))
}

pub fn to_hex(color: egui::Color32) -> String {
    format!("#{:02X}{:02X}{:02X}", color.r(), color.g(), color.b())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub min_chapters: u32,
    pub max_chapters: u32,
    pub min_chapter_duration_seconds: f64,
    pub min_shorts: u32,
    pub max_shorts: u32,
    pub min_score: u32,
    pub min_short_duration_seconds: f64,
    pub max_short_duration_seconds: f64,
    pub include_longer_videos: bool,
    pub include_shorts: bool,
    pub transcription_device: String,
    pub transcription_model: String,
    pub ai_provider: String,
    pub gemini_model: String,
    pub codex_model: String,
    pub codex_effort: String,
    pub pause_removal: String,
    pub min_pause_seconds: f64,
    pub short_layout: Layout,
    pub caption_style: CaptionStyle,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            min_chapters: 0,
            max_chapters: 10,
            min_chapter_duration_seconds: 30.0,
            min_shorts: 5,
            max_shorts: 10,
            min_score: 4,
            min_short_duration_seconds: 15.0,
            max_short_duration_seconds: 90.0,
            include_longer_videos: true,
            include_shorts: true,
            transcription_device: "auto".into(),
            transcription_model: "medium".into(),
            ai_provider: "gemini".into(),
            gemini_model: String::new(),
            codex_model: "gpt-5.6-luna".into(),
            codex_effort: "low".into(),
            pause_removal: "off".into(),
            min_pause_seconds: 2.0,
            short_layout: Layout::default(),
            caption_style: CaptionStyle::default(),
            extra: Map::new(),
        }
    }
}

impl Settings {
    /// Settings saved by the web page, over the defaults (a project saved before a setting existed lacks it).
    pub fn from_saved(saved: Option<&Value>) -> Settings {
        let mut merged = serde_json::to_value(Settings::default()).unwrap_or_default();
        if let (Some(target), Some(Value::Object(saved))) = (merged.as_object_mut(), saved) {
            for (key, value) in saved {
                target.insert(key.clone(), value.clone());
            }
        }
        serde_json::from_value(merged).unwrap_or_default()
    }

    /// What `plan-cuts` takes as `settings`.
    pub fn planning(&self) -> Value {
        serde_json::json!({
            "includeChapters": self.include_longer_videos,
            "includeShorts": self.include_shorts,
            "minChapters": self.min_chapters.min(self.max_chapters),
            "maxChapters": self.max_chapters.max(self.min_chapters).max(1),
            "minChapterDurationSeconds": self.min_chapter_duration_seconds,
            "minShorts": self.min_shorts.min(self.max_shorts),
            "maxShorts": self.max_shorts.max(self.min_shorts).max(1),
            "minScore": self.min_score,
            "minShortDurationSeconds": self.min_short_duration_seconds,
            "maxShortDurationSeconds": self.max_short_duration_seconds.max(self.min_short_duration_seconds),
        })
    }

    /// Which AI answers: the provider, model and effort fields chat and planning requests take.
    pub fn ai(&self) -> Map<String, Value> {
        let mut ai = Map::new();
        if self.ai_provider == "codex" {
            ai.insert("provider".into(), "codex".into());
            ai.insert("model".into(), self.codex_model.clone().into());
            ai.insert("effort".into(), self.codex_effort.clone().into());
        } else {
            ai.insert("provider".into(), "gemini".into());
            if !self.gemini_model.is_empty() {
                ai.insert("model".into(), self.gemini_model.clone().into());
            }
        }
        ai
    }

    pub fn ai_name(&self) -> &'static str {
        if self.ai_provider == "codex" { "Codex" } else { "Gemini" }
    }
}

/// A project in the engine's list.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Summary {
    pub job_id: String,
    pub name: String,
    pub source_size: u64,
    pub duration_seconds: Option<f64>,
    pub created_at: String,
    pub updated_at: String,
    pub transcription: Progress,
    pub message_count: usize,
    pub short_count: usize,
    pub rendered_count: usize,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Progress {
    pub state: String,
    pub chunks_done: u32,
    pub chunk_count: u32,
}

impl Summary {
    /// "10:32:49 · transcribed · 12 Shorts"
    pub fn detail(&self) -> String {
        let mut parts = Vec::new();
        if let Some(d) = self.duration_seconds {
            parts.push(clock(d));
        }
        let t = &self.transcription;
        if t.chunk_count > 0 && t.chunks_done < t.chunk_count {
            parts.push(format!("transcribed {}%", t.chunks_done * 100 / t.chunk_count));
        } else if t.chunk_count > 0 {
            parts.push("transcribed".into());
        }
        if self.short_count > 0 {
            parts.push(format!("{} Shorts", self.short_count));
        }
        if self.rendered_count > 0 {
            parts.push(format!("{} rendered", self.rendered_count));
        }
        if parts.is_empty() {
            self.created_at.get(..10).unwrap_or_default().to_owned()
        } else {
            parts.join(" · ")
        }
    }
}

/// The name without its extension ("VOD.mp4" → "VOD").
pub fn without_extension(name: &str) -> &str {
    match name.rfind('.') {
        Some(dot) if dot > 0 && name.len() - dot <= 5 => &name[..dot],
        _ => name,
    }
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Transcription {
    pub state: String,
    pub chunk_count: u32,
    pub chunks_done: u32,
    pub covered_seconds: f64,
    pub duration_seconds: f64,
    pub segments: Vec<Segment>,
    pub total_segments: usize,
    pub error: Option<String>,
}

impl Transcription {
    pub fn percent(&self) -> u32 {
        if self.duration_seconds > 0.0 {
            (self.covered_seconds / self.duration_seconds * 100.0).floor().clamp(0.0, 100.0) as u32
        } else {
            0
        }
    }
    pub fn active(&self) -> bool {
        matches!(self.state.as_str(), "running" | "queued")
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Download {
    pub key: String,
    pub url: String,
    pub quality: String,
    pub browser: String,
    pub status: String,
    pub scheduled_for: Option<f64>,
    pub after_key: Option<String>,
    pub percent: f64,
    pub speed: Option<String>,
    pub eta: Option<String>,
    pub file_name: Option<String>,
    pub size_bytes: Option<u64>,
    pub error: Option<String>,
    pub resumed: bool,
    pub project_job_id: Option<String>,
}

impl Download {
    pub fn name(&self) -> String {
        match &self.file_name {
            // Downloads are saved as "<key>__<name>.mp4".
            Some(file) => without_extension(file.split_once("__").map_or(file.as_str(), |(_, rest)| rest)).to_owned(),
            None => self.url.trim_start_matches("https://").trim_start_matches("http://").trim_start_matches("www.").to_owned(),
        }
    }

    pub fn unfinished(&self) -> bool {
        self.status != "done"
    }

    pub fn quality_label(&self) -> &str {
        match self.quality.as_str() {
            "best" => "Source",
            "720" => "720p",
            "480" => "480p",
            "audio" => "Audio",
            other => other,
        }
    }

    pub fn detail(&self) -> String {
        let quality = self.quality_label();
        match self.status.as_str() {
            "scheduled" => match self.scheduled_for {
                Some(at) => format!("{quality} · starts {}", local_time(at / 1000.0)),
                None => format!("{quality} · scheduled"),
            },
            "queued" => format!("{quality} · queued"),
            "paused" => format!("{quality} · paused at {:.1}%", self.percent),
            "downloading" => {
                let mut parts = vec![quality.to_owned(), format!("{:.1}%", self.percent)];
                parts.extend(self.speed.clone());
                parts.extend(self.eta.as_ref().map(|eta| format!("ETA {eta}")));
                if self.resumed {
                    parts.push("resumed".into());
                }
                parts.join(" · ")
            }
            "done" => {
                let mut parts = vec![quality.to_owned()];
                parts.extend(self.size_bytes.map(|b| canopy_ui::format_size(b)));
                parts.push("ready".into());
                parts.join(" · ")
            }
            _ => format!("{quality} · stopped at {:.1}%", self.percent),
        }
    }
}

/// "1:02:05" or "4:07".
pub fn clock(seconds: f64) -> String {
    let s = seconds.max(0.0).round() as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// "1:02:05.4": to a tenth, for clip edges.
pub fn clock_tenths(seconds: f64) -> String {
    let tenths = (seconds.max(0.0) * 10.0).round() as u64;
    let s = tenths / 10;
    if s >= 3600 {
        format!("{}:{:02}:{:02}.{}", s / 3600, s / 60 % 60, s % 60, tenths % 10)
    } else {
        format!("{}:{:02}.{}", s / 60, s % 60, tenths % 10)
    }
}

/// Reads "1:02:05", "62:05", "3725" or "3725.5" as seconds.
pub fn parse_clock(text: &str) -> Option<f64> {
    let mut seconds = 0.0;
    for part in text.trim().split(':') {
        let value: f64 = part.trim().parse().ok()?;
        if value < 0.0 {
            return None;
        }
        seconds = seconds * 60.0 + value;
    }
    Some(seconds)
}

/// Seconds since 1970 (UTC) as a local "Sat 7 Nov 01:00".
pub fn local_time(seconds: f64) -> String {
    let local = seconds as i64 + crate::videochat::local_offset_seconds();
    let days = local.div_euclid(86_400);
    let (_, month, day) = civil(days);
    let weekday = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"][days.rem_euclid(7) as usize];
    let months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let minutes = local.rem_euclid(86_400) / 60;
    format!("{weekday} {day} {} {:02}:{:02}", months[month as usize - 1], minutes / 60, minutes % 60)
}

/// The date of a day number (days since 1970-01-01), by Howard Hinnant's civil-from-days.
pub fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

/// The day number of a date.
pub fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let m = month as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Reads a local "2026-11-07 01:00" as milliseconds since 1970 (UTC).
pub fn parse_local(text: &str) -> Option<f64> {
    let (date, time) = text.trim().split_once([' ', 'T'])?;
    let mut d = date.split('-').map(|p| p.trim().parse::<i64>());
    let (year, month, day) = (d.next()?.ok()?, d.next()?.ok()?, d.next()?.ok()?);
    let (hour, minute) = time.trim().split_once(':')?;
    let (hour, minute): (i64, i64) = (hour.trim().parse().ok()?, minute.trim().parse().ok()?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 {
        return None;
    }
    let local = days_from_civil(year, month as u32, day as u32) * 86_400 + hour * 3600 + minute * 60;
    Some((local - crate::videochat::local_offset_seconds()) as f64 * 1000.0)
}

/// Tomorrow at 1 AM, local, as "2026-11-07 01:00": a fair default for a download overnight.
pub fn tomorrow_at_one() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let today = (now + crate::videochat::local_offset_seconds()).div_euclid(86_400);
    let (year, month, day) = civil(today + 1);
    format!("{year}-{month:02}-{day:02} 01:00")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_project_keeps_what_this_pane_doesnt_know() {
        let saved = serde_json::json!({
            "shorts": [{"startSeconds": 5.0, "endSeconds": 9.5, "title": "A", "description": "d", "newThing": 3,
                        "captions": [{"words": [{"text": "hi", "start": 0.0, "end": 0.4}]}]}],
            "longerSegments": [], "archived": [], "futureList": [1]
        });
        let plan: Plan = serde_json::from_value(saved.clone()).unwrap();
        assert_eq!(plan.shorts[0].length(), 4.5);
        assert_eq!(plan.shorts[0].lines()[0].text(), "hi");
        let back = serde_json::to_value(&plan).unwrap();
        assert_eq!(back["shorts"][0]["newThing"], 3);
        assert_eq!(back["futureList"], serde_json::json!([1]));
        let settings = Settings::from_saved(Some(&serde_json::json!({"maxShorts": 100, "aiProvider": "codex", "later": true})));
        assert_eq!((settings.max_shorts, settings.min_shorts, settings.ai_name()), (100, 5, "Codex"));
        assert_eq!(serde_json::to_value(&settings).unwrap()["later"], true);
    }

    #[test]
    fn stacked_pieces_match_the_engine() {
        // The owner's layout: a facecam on top of the gameplay, on a 1920x1080 source.
        let layout = Layout {
            mode: "stacked".into(),
            gameplay: Region { x: 0.1968, y: 0.0, w: 0.5951, h: 0.952 },
            panels: vec![Panel { x: 0.766, y: 0.676, w: 0.2339, h: 0.282, position: Side::Top }],
        };
        let pieces = stacked_pieces(&layout, 1920.0, 1080.0);
        // natural = 1080*0.952*1080/(0.5951*1920) = 971.8 -> 972; the panel gets the other 948.
        assert_eq!(pieces.len(), 2);
        assert_eq!((pieces[0].1, pieces[0].2), (0.0, 948.0));
        assert_eq!((pieces[1].1, pieces[1].2), (948.0, 972.0));
        let kept = cover(pieces[0].0, 1920.0, 1080.0, 1080.0, 948.0);
        let aspect = kept.w * 1920.0 / (kept.h * 1080.0);
        assert!((aspect - 1080.0 / 948.0).abs() < 1e-3, "cut to the slot's shape");
        assert!(kept.x >= 0.766 - 1e-6 && kept.x + kept.w <= 0.766 + 0.2339 + 1e-6);
    }

    #[test]
    fn lines_reword_and_light_up_in_turn() {
        let line = Line { words: vec![Word { text: "one".into(), start: 1.0, end: 1.5 }, Word { text: "two".into(), start: 1.5, end: 2.0 }] };
        let reworded = line.reworded("a abc");
        assert_eq!(reworded.words.len(), 2);
        assert!((reworded.words[0].end - 1.25).abs() < 1e-9 && reworded.words[1].end == 2.0);
        let lines = vec![line];
        assert_eq!(caption_at(&lines, 1.6).map(|(_, w)| w), Some(Some(1)));
        assert!(caption_at(&lines, 3.0).is_none());
    }

    #[test]
    fn clocks_and_dates_read_back() {
        assert_eq!(clock(3725.0), "1:02:05");
        assert_eq!(clock_tenths(65.44), "1:05.4");
        assert_eq!(parse_clock("1:02:05"), Some(3725.0));
        assert_eq!(parse_clock("90.5"), Some(90.5));
        assert_eq!(parse_clock("x"), None);
        assert_eq!(civil(days_from_civil(2026, 11, 7)), (2026, 11, 7));
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert!(parse_local("2026-11-07 01:00").is_some());
        assert!(parse_local("2026-13-07 01:00").is_none());
        assert_eq!(without_extension("VOD 11.mp4"), "VOD 11");
        let download = Download { file_name: Some("abc__Stream night.mp4".into()), ..Default::default() };
        assert_eq!(download.name(), "Stream night");
    }
}
