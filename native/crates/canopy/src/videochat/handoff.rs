//! A Short handed to StreamCuts: a vertical project whose clip is a window on the whole source video, starting where
//! the Short starts. Trimming its edges outwards (or the Clip menu's "Show more of the video") shows what came before
//! and after. A stacked layout becomes one grouped clip per region, each cropped and placed as the render stacks them,
//! so the layers can be moved, resized and recropped on StreamCuts' preview. Captions carry over word by word.

use super::model::{Clip as Short, Layout, Region, SHORT_H, SHORT_W, cover, stacked_pieces};
use std::path::{Path, PathBuf};
use streamcuts::project::{Caption, Framing, Media, Placement, Project, Word, new_id};
use streamcuts::storage;

/// How a handed-over Short is known again: the Video Chat project and the Short's span.
pub fn origin(job: &str, short: &Short) -> String {
    format!("videochat:{job}:{:.1}-{:.1}", short.start_seconds, short.end_seconds)
}

/// The StreamCuts project already made from this Short in `dir`, as its document id (file name).
pub fn existing(dir: &Path, origin: &str) -> Option<String> {
    storage::list(dir).into_iter().find(|doc| {
        storage::load(&dir.join(&doc.id))
            .ok()
            .is_some_and(|p| p.origin.as_deref() == Some(origin))
    }).map(|doc| doc.id)
}

/// The project for `short` of `source` (probed), laid out as `layout`, with `caption_size` its captions' size as a
/// fraction of the frame and `caption_middle` where their middle sits from the top.
pub fn short_project(
    job: &str,
    source: &Media,
    short: &Short,
    layout: &Layout,
    uppercase: bool,
    caption_size: f32,
    caption_middle: f32,
) -> Project {
    let title = if short.title.trim().is_empty() { "Short" } else { short.title.trim() };
    let mut project = Project::new(title);
    project.width = SHORT_W as u32;
    project.height = SHORT_H as u32;
    if source.frame_rate > 0.0 {
        project.fps = source.frame_rate.round().clamp(24.0, 60.0) as u32;
    }
    project.origin = Some(origin(job, short));
    let media = project.add_media(Media { id: new_id(), ..source.clone() });
    let length = short.length().max(0.5);
    let (sw, sh) = (source.width.max(1) as f32, source.height.max(1) as f32);
    let base = project.place(&media, Some(0), 0.0).expect("a video track");
    let mut layers = vec![base.clone()];
    if layout.stacked() {
        // Bottom of the stack first: the gameplay stays on Video 1, regions go on tracks above it.
        let pieces = stacked_pieces(layout, sw, sh);
        let game = pieces.iter().position(|(r, _, _)| *r == layout.gameplay).unwrap_or(0);
        let mut order: Vec<usize> = vec![game];
        order.extend((0..pieces.len()).filter(|&i| i != game));
        for (n, &i) in order.iter().enumerate() {
            let (region, top, height) = pieces[i];
            let id = if n == 0 {
                base.clone()
            } else {
                let track = project.add_track(streamcuts::project::TrackKind::Video);
                project.place(&media, Some(track), 0.0).expect("a new track has room")
            };
            let clip = project.clip_mut(&id).unwrap();
            clip.framing = Framing::Fit;
            clip.place = placement(cover(region, sw, sh, SHORT_W, height), sw, sh, top, height);
            if n > 0 {
                clip.volume = 0.0; // One sound: the gameplay's.
                layers.push(id);
            }
        }
    } else {
        project.clip_mut(&base).unwrap().framing = Framing::Fill { x: 0.5, y: 0.5 };
    }
    let group = (layers.len() > 1).then(new_id);
    for id in &layers {
        let clip = project.clip_mut(id).unwrap();
        (clip.source_in, clip.duration, clip.group) = (short.start_seconds.max(0.0), length, group.clone());
    }
    // Captions: Video Chat's lines are in seconds from the Short's start, as the timeline is.
    for line in short.lines() {
        let (Some(first), Some(last)) = (line.words.first(), line.words.last()) else { continue };
        let words: Vec<Word> = line
            .words
            .iter()
            .map(|w| Word {
                text: if uppercase { w.text.trim().to_uppercase() } else { w.text.trim().to_owned() },
                start: w.start,
                end: w.end,
            })
            .collect();
        project.captions.push(Caption {
            id: new_id(),
            start: first.start.max(0.0),
            end: last.end.min(length),
            text: words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" "),
            words,
            clip: Some(base.clone()),
        });
    }
    if !project.captions.is_empty() {
        project.caption_words = true;
        project.caption_size = caption_size.clamp(0.015, 0.12);
        project.caption_bottom = (1.0 - caption_middle - project.caption_size / 2.0).clamp(0.0, 0.9);
    }
    project
}

/// Where a clip's picture goes so that `kept` (a part of the source) fills the strip `top`..`top + height` of the
/// Short. The clip is framed to fit (the source's full width, with bars above and below on a vertical frame), then
/// cropped down to `kept`, scaled and moved into the strip.
fn placement(kept: Region, sw: f32, sh: f32, top: f32, height: f32) -> Placement {
    // Where the source sits in the framed picture, as fractions of the frame.
    let fit = (SHORT_W / sw).min(SHORT_H / sh);
    let (pw, ph) = (sw * fit / SHORT_W, sh * fit / SHORT_H);
    let (px, py) = ((1.0 - pw) / 2.0, (1.0 - ph) / 2.0);
    let crop = [
        px + kept.x * pw,
        py + kept.y * ph,
        1.0 - (px + (kept.x + kept.w) * pw),
        1.0 - (py + (kept.y + kept.h) * ph),
    ];
    // What's left spans (1 - l - r) of the picture's width: scaled up to the Short's whole width.
    let scale = 1.0 / (1.0 - crop[0] - crop[2]).max(1e-3);
    let x = scale / 2.0 - scale * crop[0];
    let y = top / SHORT_H + scale / 2.0 - scale * crop[1];
    let _ = height;
    Placement { x, y, scale, crop }
}

/// Saves the project in StreamCuts' projects folder and returns its document id.
pub fn save(dir: &Path, project: &mut Project) -> Result<String, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let path: PathBuf = storage::save(dir, project)?;
    Ok(path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::videochat::model::{Line, Panel, Side, Word as SpokenWord};
    use streamcuts::project::{Attributes, MediaKind};

    fn source() -> Media {
        Media {
            id: new_id(),
            path: PathBuf::from("D:/vods/source.mp4"),
            name: "source.mp4".into(),
            kind: MediaKind::Video,
            duration: 36_000.0,
            width: 1920,
            height: 1080,
            has_audio: true,
            frame_rate: 59.94,
            attributes: Attributes::default(),
        }
    }

    fn short() -> Short {
        Short {
            start_seconds: 5544.3,
            end_seconds: 5617.9,
            title: "Three Nights Cost 1300".into(),
            captions: Some(vec![Line {
                words: vec![
                    SpokenWord { text: "three".into(), start: 0.5, end: 0.9 },
                    SpokenWord { text: "nights".into(), start: 0.9, end: 1.4 },
                ],
            }]),
            ..Default::default()
        }
    }

    #[test]
    fn a_cropped_short_is_one_clip_on_the_whole_video() {
        let project = short_project("job", &source(), &short(), &Layout::default(), true, 0.03, 0.72);
        assert_eq!((project.width, project.height, project.fps), (1080, 1920, 60));
        let clips: Vec<_> = project.tracks.iter().flat_map(|t| &t.clips).collect();
        assert_eq!(clips.len(), 1);
        let clip = clips[0];
        assert_eq!((clip.start, clip.source_in), (0.0, 5544.3));
        assert!((clip.duration - 73.6).abs() < 1e-9);
        assert_eq!(clip.framing, Framing::Fill { x: 0.5, y: 0.5 });
        assert_eq!(project.media[0].duration, 36_000.0, "the whole video, so the clip can grow");
        assert_eq!(project.captions[0].text, "THREE NIGHTS");
        assert!(project.caption_words);
        assert!((project.caption_bottom - (1.0 - 0.72 - 0.015)).abs() < 1e-6);
        assert_eq!(project.origin.as_deref(), Some("videochat:job:5544.3-5617.9"));
    }

    #[test]
    fn a_stacked_short_is_grouped_layers_placed_as_the_render_stacks_them() {
        let layout = Layout {
            mode: "stacked".into(),
            gameplay: Region { x: 0.2, y: 0.0, w: 0.6, h: 0.95 },
            panels: vec![Panel { x: 0.77, y: 0.68, w: 0.23, h: 0.28, position: Side::Top }],
        };
        let mut project = short_project("job", &source(), &short(), &layout, false, 0.03, 0.72);
        let pieces = stacked_pieces(&layout, 1920.0, 1080.0);
        let clips: Vec<_> = project.tracks.iter().flat_map(|t| &t.clips).cloned().collect();
        assert_eq!(clips.len(), 2);
        assert!(clips.iter().all(|c| c.group.is_some() && c.group == clips[0].group));
        // Each layer's visible part lands exactly on its strip of the Short.
        for (clip, (_, top, height)) in [(&clips[1], pieces[1]), (&clips[0], pieces[0])] {
            let [l, t, r, b] = clip.place.shown();
            assert!(l.abs() < 1e-4 && (r - 1.0).abs() < 1e-4, "{l} {r}");
            assert!((t - top / SHORT_H).abs() < 1e-4 && (b - (top + height) / SHORT_H).abs() < 1e-4, "{t} {b}");
        }
        let (game, facecam) = (&clips[1], &clips[0]);
        assert_eq!((game.volume, facecam.volume), (1.0, 0.0), "one sound");
        // Showing more of the video grows every layer.
        let id = game.id.clone();
        assert_eq!(project.extend_start(&id, 10.0), 10.0);
        assert!(project.tracks.iter().flat_map(|t| &t.clips).all(|c| (c.source_in - 5534.3).abs() < 1e-9));
    }

    #[test]
    fn a_handed_over_short_is_found_again() {
        let dir = std::env::temp_dir().join(format!("videochat-handoff-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut project = short_project("job", &source(), &short(), &Layout::default(), true, 0.03, 0.72);
        let id = save(&dir, &mut project).unwrap();
        assert_eq!(existing(&dir, &origin("job", &short())), Some(id));
        assert_eq!(existing(&dir, "videochat:other:1.0-2.0"), None);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
