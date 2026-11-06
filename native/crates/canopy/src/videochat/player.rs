//! The source video in Video Chat's pane, played with StreamCuts' own playback: a one-clip StreamCuts project of the
//! whole source, so stills while paused, a frame stream with sound while playing, frame-exact seeking, and no limit on
//! length (a 12-hour VOD seeks as fast as a short one).

use egui::TextureHandle;
use streamcuts::audio::Audio;
use streamcuts::preview::{Clock, Preview};
use streamcuts::project::{Framing, Media, Project};
use streamcuts::worker::{Done, Worker};

pub struct Player {
    pub project: Project,
    preview: Preview,
    worker: Worker,
    audio: Audio,
    clock: Clock,
    pub playhead: f64,
    /// Playing a Short stops at its end.
    stop_at: Option<f64>,
    /// Why there's no sound, to say once.
    pub note: Option<String>,
}

impl Player {
    pub fn new(ctx: &egui::Context, source: &Media) -> Self {
        let mut project = Project::new("Source");
        // The source's own shape, so the preview shows all of it (decoded at up to 960 wide).
        (project.width, project.height) = (source.width.max(2), source.height.max(2));
        if source.frame_rate > 0.0 {
            project.fps = source.frame_rate.round().clamp(1.0, 120.0) as u32;
        }
        let media = project.add_media(source.clone());
        if let Some(id) = project.place(&media, Some(0), 0.0) {
            project.clip_mut(&id).unwrap().framing = Framing::Fit;
        }
        Self {
            project,
            preview: Preview::default(),
            worker: Worker::new(ctx.clone()),
            audio: Audio::default(),
            clock: Clock::default(),
            playhead: 0.0,
            stop_at: None,
            note: None,
        }
    }

    pub fn duration(&self) -> f64 {
        self.project.duration()
    }

    pub fn playing(&self) -> bool {
        self.clock.playing
    }

    pub fn texture(&self) -> Option<&TextureHandle> {
        self.preview.texture.as_ref()
    }

    /// Whether the picture shown is from somewhere else while a still is on its way.
    pub fn waiting(&self) -> bool {
        self.preview.waiting()
    }

    pub fn seek(&mut self, t: f64) {
        self.playhead = t.clamp(0.0, self.duration());
        if self.clock.playing {
            self.play_from(self.playhead);
        }
    }

    fn play_from(&mut self, t: f64) {
        self.clock.play(t);
        let had_output = self.audio.failed.is_none();
        if !self.audio.play(&self.project, t) && had_output {
            self.note = self.audio.failed.clone().map(|e| format!("Playing without sound: {e}"));
        }
    }

    pub fn pause(&mut self) {
        self.clock.pause();
        self.audio.pause();
        self.stop_at = None;
    }

    pub fn toggle(&mut self) {
        if self.clock.playing {
            self.pause();
        } else {
            if self.playhead >= self.duration() - 0.05 {
                self.playhead = 0.0;
            }
            self.play_from(self.playhead);
        }
    }

    /// Plays `from`..`to` (a Short or chapter) and stops at its end.
    pub fn play_span(&mut self, from: f64, to: f64) {
        self.playhead = from.clamp(0.0, self.duration());
        self.play_from(self.playhead);
        self.stop_at = Some(to);
    }

    /// Call every frame the player is shown: keeps the clock, the sound and the picture going.
    pub fn update(&mut self, ctx: &egui::Context) {
        while let Ok(done) = self.worker.done.try_recv() {
            if let Done::Still(key, frame) = done {
                self.preview.still_done(ctx, key, frame);
            }
        }
        if self.clock.playing {
            if self.audio.update(&self.project, self.playhead) {
                self.playhead = self.audio.now().unwrap_or_else(|| self.clock.now());
            } else {
                // The sound stopped while the pane was out of sight: stop where it stopped.
                self.playhead = self.audio.now().unwrap_or(self.playhead);
                self.pause();
            }
            let end = self.stop_at.unwrap_or(f64::INFINITY).min(self.duration());
            if self.playhead >= end {
                self.playhead = end;
                self.pause();
            }
            ctx.request_repaint();
        }
        self.preview.update(ctx, &self.project, self.playhead, self.clock.playing, &self.worker);
    }
}
