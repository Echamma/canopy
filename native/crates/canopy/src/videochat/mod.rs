//! Video Chat in Canopy: talk to an AI producer about a long video (a 12-hour Twitch VOD), get a plan of Shorts and
//! chapters, review them on the video, caption and render them, or hand any Short to StreamCuts as an editable cut of
//! the whole video (right-click it). The pane is native; the work (downloads, transcription, AI plans, rendering) runs
//! in Video Chat's Node engine, which Canopy starts and stops (see `engine`). Projects are the engine's, the same ones
//! the web page shows.

pub mod engine;
pub mod handoff;
pub mod model;
mod player;
mod view;

use canopy_ui::{Accent, AppDescriptor, CanopyApp, Document, Host, Place, Receipt, SharedFile};
use canopy_ui::canopy_core::files::Kind;
use model::{Download, Line, Message, Plan, Renders, Segment, Settings, Summary, Transcription};
use player::Player;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use streamcuts::project::Media;

pub const DESCRIPTOR: AppDescriptor = AppDescriptor {
    id: "videochat",
    name: "Video Chat",
    description: "Find Shorts in long videos with an AI producer",
    accent: Accent::Moss,
    initial: "VC",
    folder: "Video Chat",
    icon_png: include_bytes!("../../../../../assets/canopy.png"),
    accepts,
    documents,
    create,
};

fn accepts(file: &SharedFile) -> bool {
    file.kind == Kind::Video
}

/// Video Chat's projects live in its engine, the same in every space, so the sidebar doesn't list them.
fn documents(_: &Place) -> Vec<Document> {
    Vec::new()
}

fn create(place: Place, ctx: &egui::Context) -> Box<dyn CanopyApp> {
    Box::new(VideoChat::new(place, ctx))
}

/// Stops the engine Canopy started, unless it's busy (call on the way out).
pub fn shutdown() {
    engine::shutdown();
}

/// Minutes east of UTC, in seconds: local times for scheduled downloads.
#[cfg(windows)]
pub fn local_offset_seconds() -> i64 {
    #[repr(C)]
    struct TimeZoneInformation {
        bias: i32,
        _rest: [u8; 168],
    }
    unsafe extern "system" {
        fn GetTimeZoneInformation(info: *mut TimeZoneInformation) -> u32;
    }
    let mut info = TimeZoneInformation { bias: 0, _rest: [0; 168] };
    // SAFETY: GetTimeZoneInformation fills a TIME_ZONE_INFORMATION (172 bytes) we own.
    let kind = unsafe { GetTimeZoneInformation(&mut info) };
    let daylight = i32::from_le_bytes([info._rest[164], info._rest[165], info._rest[166], info._rest[167]]);
    -((info.bias + if kind == 2 { daylight } else { 0 }) as i64) * 60
}

#[cfg(not(windows))]
pub fn local_offset_seconds() -> i64 {
    0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EngineState {
    Checking,
    Starting,
    Running,
    /// No Video Chat folder found: the pane asks where it is.
    Missing,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Producer,
    Clips,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipKind {
    Short,
    Chapter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Selected {
    kind: ClipKind,
    index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum When {
    Now,
    Later,
    Queue,
}

struct DownloadForm {
    url: String,
    quality: String,
    browser: String,
    when: When,
    at: String,
}

/// What a background call brought back.
enum Reply {
    Health(bool),
    Projects(Result<Vec<Summary>, String>),
    Downloads(Result<Vec<Download>, String>),
    Folder(Result<Value, String>),
    Models(Value),
    Codex(Value),
    Opened { job: String, result: Result<(Summary, Value), String> },
    Probed { job: String, result: Result<Media, String> },
    Transcript { job: String, since: usize, result: Result<Transcription, String> },
    Chat { job: String, result: Result<String, String> },
    Planned { job: String, settings: Settings, result: Result<Plan, String> },
    Captions { job: String, span: (f64, f64), result: Result<Vec<Line>, String> },
    Rendered { job: String, result: Result<Renders, String> },
    Imported(Result<String, String>),
    /// A download or project change finished: refresh lists, say `done` (or the error).
    Changed { done: String, result: Result<Value, String>, open: bool },
    Saved(Result<(), String>),
}

/// The project open in the pane.
struct Open {
    job: String,
    name: String,
    size: u64,
    duration: f64,
    /// The whole saved state, so fields this pane doesn't know survive a save.
    state: Value,
    messages: Vec<Message>,
    plan: Option<Plan>,
    rendered: Option<Renders>,
    settings: Settings,
    transcript: Vec<Segment>,
    transcription: Option<Transcription>,
    polled_at: Instant,
    source: Option<Media>,
    source_error: Option<String>,
    player: Option<Player>,
    selected: Option<Selected>,
    undo: Vec<Plan>,
    redo: Vec<Plan>,
    dirty_at: Option<Instant>,
    /// The clip map's window on the video, in seconds.
    view: (f64, f64),
    /// Drawing layout regions on the picture: where a drag began, and the box drawn.
    layout_edit: Option<LayoutEdit>,
    caption_drafts: HashMap<usize, String>,
}

#[derive(Default)]
struct LayoutEdit {
    from: Option<egui::Pos2>,
    draft: Option<model::Region>,
}

pub struct VideoChat {
    place: Place,
    ctx: egui::Context,
    folder: Option<PathBuf>,
    engine: EngineState,
    engine_error: String,
    started_at: Instant,
    replies: Receiver<Reply>,
    sender: Sender<Reply>,
    /// Calls in flight that shouldn't be doubled (polls).
    pending: HashSet<&'static str>,
    projects: Vec<Summary>,
    projects_at: Instant,
    project_search: String,
    downloads: Vec<Download>,
    downloads_at: Instant,
    all_downloads: bool,
    form: DownloadForm,
    download_folder: Option<(String, String, bool)>,
    gemini_models: Option<(String, Vec<(String, String)>)>,
    codex: Option<Value>,
    open: Option<Open>,
    /// Clip plans running in the background, by project: what each is doing now.
    planning: Arc<Mutex<HashMap<String, String>>>,
    /// What a background chat or render is doing, for the status line.
    busy: Option<String>,
    progress: Arc<Mutex<String>>,
    error: Option<String>,
    tab: Tab,
    prompt: String,
    rename: Option<(String, String)>,
    confirm: Option<Confirm>,
    transcript_query: String,
    show_library: bool,
    toasts: Vec<String>,
    /// Files sent before the engine was up.
    waiting_imports: Vec<PathBuf>,
}

enum Confirm {
    DeleteProject(Summary),
    DeleteDownload(Download),
}

impl VideoChat {
    pub fn new(place: Place, ctx: &egui::Context) -> Self {
        let (sender, replies) = channel();
        let long_ago = Instant::now() - Duration::from_secs(600);
        let mut app = Self {
            place,
            ctx: ctx.clone(),
            folder: engine::folder(),
            engine: EngineState::Checking,
            engine_error: String::new(),
            started_at: Instant::now(),
            replies,
            sender,
            pending: HashSet::new(),
            projects: Vec::new(),
            projects_at: long_ago,
            project_search: String::new(),
            downloads: Vec::new(),
            downloads_at: long_ago,
            all_downloads: false,
            form: DownloadForm { url: String::new(), quality: "720".into(), browser: "zen".into(), when: When::Now, at: model::tomorrow_at_one() },
            download_folder: None,
            gemini_models: None,
            codex: None,
            open: None,
            planning: Arc::new(Mutex::new(HashMap::new())),
            busy: None,
            progress: Arc::new(Mutex::new(String::new())),
            error: None,
            tab: Tab::Producer,
            prompt: String::new(),
            rename: None,
            confirm: None,
            transcript_query: String::new(),
            show_library: true,
            toasts: Vec::new(),
            waiting_imports: Vec::new(),
        };
        app.check_engine();
        app
    }

    /// Runs `call` on a thread of its own and hands its reply to the next frame.
    fn spawn(&self, call: impl FnOnce() -> Reply + Send + 'static) {
        let (sender, ctx) = (self.sender.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let _ = sender.send(call());
            ctx.request_repaint();
        });
    }

    fn check_engine(&mut self) {
        self.engine = EngineState::Checking;
        self.spawn(|| Reply::Health(engine::healthy()));
    }

    fn start_engine(&mut self) {
        let Some(folder) = self.folder.clone() else {
            self.engine = EngineState::Missing;
            return;
        };
        match engine::start(&folder, &self.place.data_dir.join("engine.log")) {
            Ok(()) => {
                self.engine = EngineState::Starting;
                self.started_at = Instant::now();
            }
            Err(error) => {
                self.engine = EngineState::Failed;
                self.engine_error = error;
            }
        }
    }

    fn engine_ready(&mut self) {
        self.engine = EngineState::Running;
        self.refresh_projects();
        self.refresh_downloads();
        self.spawn(|| Reply::Folder(engine::request("GET", "/api/twitch/folder", None, Some(Duration::from_secs(10)))));
        self.spawn(|| Reply::Models(engine::request("GET", "/api/boss/models", None, Some(Duration::from_secs(20))).unwrap_or_default()));
        self.spawn(|| Reply::Codex(engine::request("GET", "/api/boss/codex", None, Some(Duration::from_secs(20))).unwrap_or_default()));
        for path in std::mem::take(&mut self.waiting_imports) {
            self.import(path);
        }
    }

    fn refresh_projects(&mut self) {
        if !self.pending.insert("projects") {
            return;
        }
        self.projects_at = Instant::now();
        self.spawn(|| {
            Reply::Projects(
                engine::request("GET", "/api/boss/projects", None, Some(Duration::from_secs(30)))
                    .and_then(|v| serde_json::from_value(v).map_err(|e| e.to_string())),
            )
        });
    }

    fn refresh_downloads(&mut self) {
        if !self.pending.insert("downloads") {
            return;
        }
        self.downloads_at = Instant::now();
        self.spawn(|| {
            Reply::Downloads(
                engine::request("GET", "/api/twitch/downloads", None, Some(Duration::from_secs(15)))
                    .and_then(|v| serde_json::from_value(v).map_err(|e| e.to_string())),
            )
        });
    }

    /// Opens a project: its saved state, its transcript so far, and its source in the player.
    fn open_project(&mut self, job: &str) {
        if let Err(error) = self.save_now() {
            self.error = Some(format!("Couldn't save the project you had open: {error}"));
        }
        let job = job.to_owned();
        self.busy = Some("Opening the project".into());
        self.spawn(move || {
            let result = engine::request("GET", &format!("/api/boss/jobs/{}/state", engine::encode(&job)), None, Some(Duration::from_secs(30)))
                .and_then(|v| {
                    let meta: Summary = serde_json::from_value(v.get("meta").cloned().unwrap_or_default()).map_err(|e| e.to_string())?;
                    Ok((meta, v.get("state").cloned().unwrap_or(Value::Null)))
                });
            Reply::Opened { job, result }
        });
    }

    fn poll_transcript(&mut self) {
        let Some(open) = &mut self.open else { return };
        if !self.pending.insert("transcript") {
            return;
        }
        open.polled_at = Instant::now();
        let (job, since) = (open.job.clone(), open.transcript.len());
        self.spawn(move || {
            let result = engine::request("GET", &format!("/api/boss/jobs/{}/transcription?since={since}", engine::encode(&job)), None, Some(Duration::from_secs(30)))
                .and_then(|v| serde_json::from_value(v).map_err(|e| e.to_string()));
            Reply::Transcript { job, since, result }
        });
    }

    /// Asks the engine to transcribe the open project (or carry on where it stopped).
    fn start_transcription(&mut self) {
        let Some(open) = &self.open else { return };
        let body = json!({"jobId": open.job, "device": open.settings.transcription_device, "model": open.settings.transcription_model});
        self.spawn(move || Reply::Changed {
            done: "Transcription started".into(),
            result: engine::request("POST", "/api/boss/transcribe", Some(&body), Some(Duration::from_secs(120))),
            open: false,
        });
        if let Some(open) = &mut self.open {
            open.polled_at = Instant::now() - Duration::from_secs(10);
        }
    }

    /// Imports a video from this PC as a new project (linked, not copied, on the same drive).
    fn import(&mut self, path: PathBuf) {
        if self.engine != EngineState::Running {
            self.waiting_imports.push(path);
            self.toasts.push("Video Chat starts its engine first, then imports it".into());
            return;
        }
        self.busy = Some("Importing the video".into());
        self.spawn(move || {
            let result = engine::request("POST", "/api/boss/import-file", Some(&json!({"path": path})), Some(Duration::from_secs(600)))
                .and_then(|v| v.get("jobId").and_then(Value::as_str).map(str::to_owned).ok_or_else(|| "The engine made no project.".to_string()));
            Reply::Imported(result)
        });
    }

    /// A download, project or other change: runs `method path body`, then says `done` and refreshes the lists.
    fn change(&mut self, method: &'static str, path: String, body: Option<Value>, done: impl Into<String>) {
        let done = done.into();
        self.spawn(move || Reply::Changed {
            result: engine::request(method, &path, body.as_ref(), Some(Duration::from_secs(60))),
            done,
            open: false,
        });
    }

    // ---- The open project ----

    fn mark_dirty(&mut self) {
        if let Some(open) = &mut self.open {
            open.dirty_at = Some(Instant::now());
        }
    }

    /// Records the plan before an edit, for Ctrl+Z.
    fn remember(&mut self) {
        if let Some(open) = &mut self.open {
            if let Some(plan) = &open.plan {
                open.undo.push(plan.clone());
                if open.undo.len() > 100 {
                    open.undo.remove(0);
                }
                open.redo.clear();
            }
        }
    }

    fn undo(&mut self, back: bool) {
        let Some(open) = &mut self.open else { return };
        let (from, to) = if back { (&mut open.undo, &mut open.redo) } else { (&mut open.redo, &mut open.undo) };
        if let Some(plan) = from.pop() {
            if let Some(current) = open.plan.replace(plan) {
                to.push(current);
            }
            open.selected = None;
            open.caption_drafts.clear();
            open.dirty_at = Some(Instant::now());
        }
    }

    /// The saved state with this pane's parts written over it.
    fn state_body(open: &Open) -> Value {
        let mut state = match &open.state {
            Value::Object(map) => map.clone(),
            _ => serde_json::Map::new(),
        };
        state.insert("messages".into(), serde_json::to_value(&open.messages).unwrap_or_default());
        state.insert("plan".into(), serde_json::to_value(&open.plan).unwrap_or_default());
        state.insert("rendered".into(), serde_json::to_value(&open.rendered).unwrap_or_default());
        state.insert("settings".into(), serde_json::to_value(&open.settings).unwrap_or_default());
        state.remove("savedAt");
        Value::Object(state)
    }

    fn save_soon(&mut self) {
        let Some(open) = &mut self.open else { return };
        if !open.dirty_at.is_some_and(|at| at.elapsed() > Duration::from_millis(900)) {
            return;
        }
        open.dirty_at = None;
        let body = Self::state_body(open);
        open.state = body.clone();
        let job = open.job.clone();
        self.spawn(move || {
            Reply::Saved(engine::request("PUT", &format!("/api/boss/jobs/{}/state", engine::encode(&job)), Some(&body), Some(Duration::from_secs(30))).map(|_| ()))
        });
    }

    /// Saves now, waiting for it (switching projects, closing).
    fn save_now(&mut self) -> Result<(), String> {
        let Some(open) = &mut self.open else { return Ok(()) };
        if open.dirty_at.take().is_none() {
            return Ok(());
        }
        let body = Self::state_body(open);
        open.state = body.clone();
        engine::request("PUT", &format!("/api/boss/jobs/{}/state", engine::encode(&open.job)), Some(&body), Some(Duration::from_secs(10))).map(|_| ())
    }

    fn selected_clip(&self) -> Option<&model::Clip> {
        let open = self.open.as_ref()?;
        let (plan, selected) = (open.plan.as_ref()?, open.selected?);
        match selected.kind {
            ClipKind::Short => plan.shorts.get(selected.index),
            ClipKind::Chapter => plan.longer_segments.get(selected.index),
        }
    }

    fn selected_clip_mut(&mut self) -> Option<&mut model::Clip> {
        let open = self.open.as_mut()?;
        let selected = open.selected?;
        let plan = open.plan.as_mut()?;
        match selected.kind {
            ClipKind::Short => plan.shorts.get_mut(selected.index),
            ClipKind::Chapter => plan.longer_segments.get_mut(selected.index),
        }
    }

    fn select(&mut self, kind: ClipKind, index: usize) {
        let Some(open) = &mut self.open else { return };
        open.selected = Some(Selected { kind, index });
        open.caption_drafts.clear();
        let start = self.selected_clip().map(|c| c.start_seconds);
        if let (Some(start), Some(open)) = (start, &mut self.open) {
            if let Some(player) = &mut open.player {
                player.seek(start);
            }
            // Keep it in view on the clip map.
            let (from, to) = open.view;
            if start < from || start > to {
                let width = to - from;
                open.view = ((start - width * 0.2).max(0.0), (start - width * 0.2).max(0.0) + width);
            }
        }
    }

    fn seek(&mut self, t: f64) {
        if let Some(player) = self.open.as_mut().and_then(|o| o.player.as_mut()) {
            player.seek(t);
        }
    }

    fn playhead(&self) -> f64 {
        self.open.as_ref().and_then(|o| o.player.as_ref()).map_or(0.0, |p| p.playhead)
    }

    /// Chat with the AI about the video: starts transcription if needed, waits for the first part, then asks.
    fn send_chat(&mut self) {
        let text = self.prompt.trim().to_owned();
        let Some(open) = &mut self.open else { return };
        if text.is_empty() || self.busy.is_some() {
            return;
        }
        open.messages.push(Message { role: "user".into(), content: text });
        open.dirty_at = Some(Instant::now());
        self.prompt.clear();
        self.error = None;
        let (job, duration, settings, messages) = (open.job.clone(), open.duration, open.settings.clone(), open.messages.clone());
        let transcribed = open.transcription.as_ref().is_some_and(|t| t.active() || t.state == "done");
        self.busy = Some(format!("Asking {} about your video", settings.ai_name()));
        let progress = self.progress.clone();
        self.spawn(move || {
            let result = (|| {
                let segments = wait_for_transcript(&job, &settings, transcribed, false, &progress, |_| ())?;
                *progress.lock().unwrap_or_else(|p| p.into_inner()) = format!("Asking {} about your video", settings.ai_name());
                let mut body = json!({"jobId": job, "durationSeconds": duration, "segments": segments.0, "coveredSeconds": segments.1.max(1.0), "messages": messages});
                body.as_object_mut().unwrap().extend(settings.ai());
                let reply = engine::request("POST", "/api/boss/chat", Some(&body), None)?;
                reply.get("message").and_then(Value::as_str).map(str::to_owned).ok_or_else(|| "The AI gave no answer.".to_string())
            })();
            Reply::Chat { job, result }
        });
    }

    /// Builds a clip plan in the background from the chat's directions. It waits for the whole transcript, and saves
    /// into its project even if another one is open by then.
    fn make_plan(&mut self) {
        let Some(open) = &self.open else { return };
        let mut directions: Vec<String> = open.messages.iter().filter(|m| m.role == "user").map(|m| m.content.clone()).collect();
        directions = directions.split_off(directions.len().saturating_sub(4));
        if !self.prompt.trim().is_empty() {
            directions.push(self.prompt.trim().to_owned());
        }
        if directions.is_empty() {
            self.error = Some(format!("Tell {} what you want to make first.", open.settings.ai_name()));
            return;
        }
        if !open.settings.include_longer_videos && !open.settings.include_shorts {
            self.error = Some("Choose Shorts, chapters or both in Settings.".into());
            return;
        }
        let job = open.job.clone();
        if self.planning.lock().unwrap_or_else(|p| p.into_inner()).contains_key(&job) {
            return;
        }
        let (duration, settings) = (open.duration, open.settings.clone());
        let transcribed = open.transcription.as_ref().is_some_and(|t| t.active() || t.state == "done");
        self.prompt.clear();
        self.error = None;
        let planning = self.planning.clone();
        let step = move |text: &str| {
            planning.lock().unwrap_or_else(|p| p.into_inner()).insert(job.clone(), text.to_owned());
        };
        step("Getting ready to plan");
        let (job, planning) = (open.job.clone(), self.planning.clone());
        let progress = self.progress.clone();
        let prompt = directions.join("\n");
        self.spawn(move || {
            let set = |text: &str| {
                planning.lock().unwrap_or_else(|p| p.into_inner()).insert(job.clone(), text.to_owned());
            };
            let result = (|| {
                let (segments, _) = wait_for_transcript(&job, &settings, transcribed, true, &progress, |text| set(text))?;
                set("Building the clip plan");
                let mut body = json!({
                    "jobId": job, "prompt": prompt, "segments": segments, "durationSeconds": duration,
                    "coveredSeconds": duration, "settings": settings.planning(),
                });
                body.as_object_mut().unwrap().extend(settings.ai());
                let plan = engine::request("POST", "/api/boss/plan-cuts", Some(&body), None)?;
                serde_json::from_value::<Plan>(plan).map_err(|e| e.to_string())
            })();
            planning.lock().unwrap_or_else(|p| p.into_inner()).remove(&job);
            Reply::Planned { job, settings, result }
        });
    }

    /// Captions for Shorts (by index) from their audio, one after another.
    fn make_captions(&mut self, indexes: Vec<usize>) {
        let Some(open) = &self.open else { return };
        let Some(plan) = &open.plan else { return };
        let spans: Vec<(f64, f64)> = indexes.iter().filter_map(|&i| plan.shorts.get(i)).map(|s| (s.start_seconds, s.end_seconds)).collect();
        if spans.is_empty() {
            return;
        }
        let (job, words) = (open.job.clone(), open.settings.caption_style.words_per_line);
        let count = spans.len();
        self.busy = Some(if count > 1 { format!("Making captions for {count} Shorts") } else { "Making captions".into() });
        let (sender, ctx, progress) = (self.sender.clone(), self.ctx.clone(), self.progress.clone());
        std::thread::spawn(move || {
            for (n, span) in spans.into_iter().enumerate() {
                if count > 1 {
                    *progress.lock().unwrap_or_else(|p| p.into_inner()) = format!("Making captions {}/{count}", n + 1);
                    ctx.request_repaint();
                }
                let body = json!({"startSeconds": span.0, "endSeconds": span.1, "wordsPerLine": words});
                let result = engine::request("POST", &format!("/api/boss/jobs/{}/captions", engine::encode(&job)), Some(&body), None)
                    .and_then(|v| serde_json::from_value::<Vec<Line>>(v.get("lines").cloned().unwrap_or_default()).map_err(|e| e.to_string()));
                let failed = result.is_err();
                let _ = sender.send(Reply::Captions { job: job.clone(), span, result });
                ctx.request_repaint();
                if failed {
                    break;
                }
            }
            *progress.lock().unwrap_or_else(|p| p.into_inner()) = String::new();
            let _ = sender.send(Reply::Changed { done: String::new(), result: Ok(Value::Null), open: false });
            ctx.request_repaint();
        });
    }

    fn render(&mut self) {
        let Some(open) = &self.open else { return };
        let Some(plan) = &open.plan else { return };
        let settings = &open.settings;
        let longer: &[model::Clip] = if settings.include_longer_videos { &plan.longer_segments } else { &[] };
        let shorts: &[model::Clip] = if settings.include_shorts { &plan.shorts } else { &[] };
        if longer.is_empty() && shorts.is_empty() {
            self.error = Some("Keep at least one clip before rendering.".into());
            return;
        }
        let body = json!({
            "jobId": open.job,
            "longerSegments": longer,
            "shorts": shorts.iter().map(|s| {
                let mut s = serde_json::to_value(s).unwrap_or_default();
                if s.get("description").is_none_or(Value::is_null) { s["description"] = "".into(); }
                s
            }).collect::<Vec<_>>(),
            "pauseRemoval": {"mode": settings.pause_removal, "minPauseSeconds": settings.min_pause_seconds},
            "shortLayout": settings.short_layout,
            "captionStyle": settings.caption_style,
        });
        let job = open.job.clone();
        self.busy = Some(format!("Rendering {} clips", longer.len() + shorts.len()));
        self.error = None;
        self.spawn(move || Reply::Rendered {
            result: engine::request("POST", "/api/boss/render", Some(&body), None).and_then(|v| serde_json::from_value(v).map_err(|e| e.to_string())),
            job,
        });
    }

    /// Hands Short `index` to StreamCuts: the edit made from it before, or a new one (`fresh` always makes one).
    fn edit_in_streamcuts(&mut self, index: usize, fresh: bool, host: &mut dyn Host) {
        let Some(open) = &self.open else { return };
        let Some(short) = open.plan.as_ref().and_then(|p| p.shorts.get(index)).cloned() else { return };
        let Some(source) = open.source.clone() else {
            host.toast("Still reading the video; try again in a moment".into());
            return;
        };
        let streamcuts = &streamcuts::DESCRIPTOR;
        let place = match &self.place.space {
            Some(space) => streamcuts.place_in_space(space),
            None => streamcuts.standalone_place(),
        };
        let origin = handoff::origin(&open.job, &short);
        let existing = (!fresh).then(|| handoff::existing(&place.projects_dir, &origin)).flatten();
        let reused = existing.is_some();
        let document = match existing {
            Some(id) => id,
            None => {
                let style = &open.settings.caption_style;
                let size = (style.font_size as f32 / model::SHORT_H / 1.385).max(0.015);
                let mut project = handoff::short_project(&open.job, &source, &short, &open.settings.short_layout, style.uppercase, size, style.position_y);
                match handoff::save(&place.projects_dir, &mut project) {
                    Ok(id) => id,
                    Err(error) => {
                        host.toast(format!("Couldn't make the StreamCuts project: {error}"));
                        return;
                    }
                }
            }
        };
        let title = if short.title.is_empty() { "the Short".to_owned() } else { format!("\u{201c}{}\u{201d}", short.title) };
        if host.open_in(streamcuts.id, &document) {
            host.toast(if reused {
                format!("Opened your StreamCuts edit of {title}")
            } else {
                format!("{title} is in StreamCuts: drag its edges, or Clip > Show more of the video, to grow it")
            });
        } else {
            // On its own: StreamCuts opens the project from its own exe, next to this one.
            let file = place.projects_dir.join(&document);
            let exe = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("StreamCuts.exe")));
            match exe.filter(|e| e.is_file()).map(|e| std::process::Command::new(e).arg(&file).spawn()) {
                Some(Ok(_)) => host.toast(format!("Opening {title} in StreamCuts")),
                _ => host.toast(format!("Saved {title} for StreamCuts: {}", file.display())),
            }
        }
    }

    // ---- Replies ----

    fn take_replies(&mut self) {
        while let Ok(reply) = self.replies.try_recv() {
            self.apply(reply);
        }
        if let Some(note) = self.open.as_mut().and_then(|o| o.player.as_mut()).and_then(|p| p.note.take()) {
            self.toasts.push(note);
        }
    }

    fn apply(&mut self, reply: Reply) {
        match reply {
            Reply::Health(true) => self.engine_ready(),
            Reply::Health(false) => match self.engine {
                EngineState::Starting if self.started_at.elapsed() < Duration::from_secs(90) => {
                    let ctx = self.ctx.clone();
                    let sender = self.sender.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_millis(800));
                        let _ = sender.send(Reply::Health(engine::healthy()));
                        ctx.request_repaint();
                    });
                }
                EngineState::Starting => {
                    self.engine = EngineState::Failed;
                    self.engine_error = format!("It didn't answer within 90 seconds. Its log: {}", self.place.data_dir.join("engine.log").display());
                }
                _ => self.start_engine_then_wait(),
            },
            Reply::Projects(result) => {
                self.pending.remove("projects");
                if let Ok(projects) = result {
                    self.projects = projects;
                }
            }
            Reply::Downloads(result) => {
                self.pending.remove("downloads");
                if let Ok(downloads) = result {
                    self.downloads = downloads;
                }
            }
            Reply::Folder(Ok(value)) => {
                let text = |key: &str| value.get(key).and_then(Value::as_str).unwrap_or_default().to_owned();
                self.download_folder = Some((text("folder"), text("defaultFolder"), value.get("sameDriveAsProjects").and_then(Value::as_bool).unwrap_or(true)));
            }
            Reply::Folder(Err(error)) => self.error = Some(error),
            Reply::Models(value) => {
                let models = value.get("models").and_then(Value::as_array).map(|models| {
                    models
                        .iter()
                        .filter_map(|m| Some((m.get("id")?.as_str()?.to_owned(), m.get("displayName").and_then(Value::as_str).unwrap_or_default().to_owned())))
                        .collect()
                });
                if let Some(models) = models {
                    self.gemini_models = Some((value.get("default").and_then(Value::as_str).unwrap_or_default().to_owned(), models));
                }
            }
            Reply::Codex(value) => self.codex = (!value.is_null()).then_some(value),
            Reply::Opened { job, result } => {
                self.busy = None;
                match result {
                    Ok((meta, state)) => self.opened(job, meta, state),
                    Err(error) => self.error = Some(format!("That project couldn't be opened: {error}")),
                }
            }
            Reply::Probed { job, result } => {
                let ctx = self.ctx.clone();
                if let Some(open) = self.open.as_mut().filter(|o| o.job == job) {
                    match result {
                        Ok(media) => {
                            open.player = Some(Player::new(&ctx, &media));
                            if open.duration <= 0.0 {
                                open.duration = media.duration;
                            }
                            open.source = Some(media);
                        }
                        Err(error) => open.source_error = Some(error),
                    }
                }
            }
            Reply::Transcript { job, since, result } => {
                self.pending.remove("transcript");
                if let (Some(open), Ok(mut status)) = (self.open.as_mut().filter(|o| o.job == job), result) {
                    if open.transcript.len() == since {
                        open.transcript.append(&mut status.segments);
                    }
                    open.transcription = Some(status);
                }
            }
            Reply::Chat { job, result } => {
                self.busy = None;
                match result {
                    Ok(answer) => {
                        if let Some(open) = self.open.as_mut().filter(|o| o.job == job) {
                            open.messages.push(Message { role: "assistant".into(), content: answer });
                            open.dirty_at = Some(Instant::now());
                        }
                        self.refresh_codex_if_used();
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            Reply::Planned { job, settings, result } => match result {
                Ok(plan) => {
                    self.refresh_codex_if_used();
                    if let Some(open) = self.open.as_mut().filter(|o| o.job == job) {
                        if let Some(old) = open.plan.take() {
                            open.undo.push(old);
                        }
                        let first = if !plan.shorts.is_empty() { Some(Selected { kind: ClipKind::Short, index: 0 }) } else if !plan.longer_segments.is_empty() { Some(Selected { kind: ClipKind::Chapter, index: 0 }) } else { None };
                        open.plan = Some(plan);
                        open.rendered = None;
                        open.dirty_at = Some(Instant::now());
                        self.tab = Tab::Clips;
                        if let Some(first) = first {
                            self.select(first.kind, first.index);
                        }
                        self.toasts.push("Your clip plan is ready".into());
                    } else {
                        // Another project is open: the plan goes into its own project.
                        let name = self.projects.iter().find(|p| p.job_id == job).map(|p| model::without_extension(&p.name).to_owned()).unwrap_or_default();
                        self.spawn(move || {
                            let path = format!("/api/boss/jobs/{}/state", engine::encode(&job));
                            let result = engine::request("GET", &path, None, Some(Duration::from_secs(30))).and_then(|v| {
                                let mut state = v.get("state").cloned().filter(Value::is_object).unwrap_or_else(|| json!({"messages": [], "settings": settings}));
                                state["plan"] = serde_json::to_value(&plan).unwrap_or_default();
                                state["rendered"] = Value::Null;
                                engine::request("PUT", &path, Some(&state), Some(Duration::from_secs(30)))
                            });
                            Reply::Changed { done: format!("The clip plan for {name} is ready"), result, open: false }
                        });
                    }
                }
                Err(error) => {
                    let name = self.projects.iter().find(|p| p.job_id == job).map(|p| model::without_extension(&p.name).to_owned()).unwrap_or_default();
                    self.error = Some(if self.open.as_ref().is_some_and(|o| o.job == job) { error } else { format!("Clip plan for {name}: {error}") });
                }
            },
            Reply::Captions { job, span, result } => match result {
                Ok(lines) => {
                    if let Some(open) = self.open.as_mut().filter(|o| o.job == job) {
                        if let Some(short) = open.plan.as_mut().and_then(|p| p.shorts.iter_mut().find(|s| (s.start_seconds, s.end_seconds) == span)) {
                            short.captions = Some(lines);
                            open.dirty_at = Some(Instant::now());
                            open.caption_drafts.clear();
                        }
                    }
                }
                Err(error) => self.error = Some(format!("Captions couldn't be made: {error}")),
            },
            Reply::Rendered { job, result } => {
                self.busy = None;
                match result {
                    Ok(renders) => {
                        if let Some(open) = self.open.as_mut().filter(|o| o.job == job) {
                            let count = renders.shorts.len() + renders.longer_videos.len();
                            open.rendered = Some(renders);
                            open.dirty_at = Some(Instant::now());
                            self.toasts.push(format!("Rendered {count} clips"));
                            self.tab = Tab::Clips;
                        }
                    }
                    Err(error) => self.error = Some(format!("The clips couldn't be rendered: {error}")),
                }
            }
            Reply::Imported(result) => {
                self.busy = None;
                match result {
                    Ok(job) => {
                        self.refresh_projects();
                        self.open_project(&job);
                    }
                    Err(error) => self.error = Some(format!("The video couldn't be imported: {error}")),
                }
            }
            Reply::Changed { done, result, open } => {
                if done.is_empty() {
                    // Captions finished (or stopped).
                    self.busy = None;
                    return;
                }
                match result {
                    Ok(value) => {
                        self.toasts.push(done);
                        if open {
                            if let Some(job) = value.get("jobId").and_then(Value::as_str) {
                                let job = job.to_owned();
                                self.open_project(&job);
                            }
                        }
                    }
                    Err(error) => self.error = Some(error),
                }
                self.pending.remove("projects");
                self.pending.remove("downloads");
                self.refresh_projects();
                self.refresh_downloads();
            }
            Reply::Saved(result) => {
                if let Err(error) = result {
                    self.error = Some(format!("Couldn't save: {error}"));
                }
            }
        }
    }

    fn start_engine_then_wait(&mut self) {
        self.start_engine();
        if self.engine == EngineState::Starting {
            let (sender, ctx) = (self.sender.clone(), self.ctx.clone());
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(1500));
                let _ = sender.send(Reply::Health(engine::healthy()));
                ctx.request_repaint();
            });
        }
    }

    fn refresh_codex_if_used(&mut self) {
        if self.open.as_ref().is_some_and(|o| o.settings.ai_provider == "codex") {
            self.spawn(|| Reply::Codex(engine::request("GET", "/api/boss/codex", None, Some(Duration::from_secs(20))).unwrap_or_default()));
        }
    }

    fn opened(&mut self, job: String, meta: Summary, state: Value) {
        let plan: Option<Plan> = state.get("plan").filter(|p| !p.is_null()).and_then(|p| serde_json::from_value(p.clone()).ok());
        let duration = meta.duration_seconds.unwrap_or(0.0);
        let mut open = Open {
            job: job.clone(),
            name: model::without_extension(&meta.name).to_owned(),
            size: meta.source_size,
            duration,
            messages: state.get("messages").and_then(|m| serde_json::from_value(m.clone()).ok()).unwrap_or_default(),
            plan,
            rendered: state.get("rendered").filter(|r| !r.is_null()).and_then(|r| serde_json::from_value(r.clone()).ok()),
            settings: Settings::from_saved(state.get("settings")),
            state,
            transcript: Vec::new(),
            transcription: None,
            polled_at: Instant::now() - Duration::from_secs(60),
            source: None,
            source_error: None,
            player: None,
            selected: None,
            undo: Vec::new(),
            redo: Vec::new(),
            dirty_at: None,
            view: (0.0, duration.max(1.0)),
            layout_edit: None,
            caption_drafts: HashMap::new(),
        };
        if open.plan.as_ref().is_some_and(|p| !p.shorts.is_empty() || !p.longer_segments.is_empty()) {
            self.tab = Tab::Clips;
        } else {
            self.tab = Tab::Producer;
        }
        // The source on disk, read for the player off the UI thread.
        match self.folder.as_deref().and_then(|f| engine::source_path(f, &job)) {
            Some(path) => {
                let job = job.clone();
                self.spawn(move || Reply::Probed { result: streamcuts::ffmpeg::probe(&path), job });
            }
            None => open.source_error = Some("The project's video isn't in Video Chat's folder.".into()),
        }
        self.open = Some(open);
        self.prompt.clear();
        self.error = None;
        self.transcript_query.clear();
        self.pending.remove("transcript");
        self.poll_transcript();
    }

    /// Polls what changes on its own: the project list, downloads, and the open project's transcript.
    fn poll(&mut self) {
        if self.engine != EngineState::Running {
            return;
        }
        if self.projects_at.elapsed() > Duration::from_secs(5) {
            self.refresh_projects();
        }
        let active = self.downloads.iter().any(|d| matches!(d.status.as_str(), "downloading" | "queued" | "scheduled"));
        if self.downloads_at.elapsed() > Duration::from_secs(if active { 2 } else { 15 }) {
            self.refresh_downloads();
        }
        let transcribing = self.open.as_ref().is_some_and(|o| o.transcription.as_ref().is_none_or(Transcription::active));
        if transcribing && self.open.as_ref().is_some_and(|o| o.polled_at.elapsed() > Duration::from_secs(3)) {
            self.poll_transcript();
        }
        self.ctx.request_repaint_after(Duration::from_secs(1));
    }
}

/// Waits (on a background thread) until the project has a transcript to work from: some of it for chat, all of it
/// for a clip plan (`whole`). Starts transcription when it isn't going. Returns the segments and how far they reach.
fn wait_for_transcript(
    job: &str,
    settings: &Settings,
    going: bool,
    whole: bool,
    progress: &Mutex<String>,
    step: impl Fn(&str),
) -> Result<(Vec<Segment>, f64), String> {
    let path = format!("/api/boss/jobs/{}/transcription", engine::encode(job));
    let status = |since: usize| -> Result<Transcription, String> {
        engine::request("GET", &format!("{path}?since={since}"), None, Some(Duration::from_secs(30))).and_then(|v| serde_json::from_value(v).map_err(|e| e.to_string()))
    };
    let mut current = status(usize::MAX >> 12)?;
    if !going && matches!(current.state.as_str(), "idle" | "error" | "") {
        let body = json!({"jobId": job, "device": settings.transcription_device, "model": settings.transcription_model});
        engine::request("POST", "/api/boss/transcribe", Some(&body), Some(Duration::from_secs(120)))?;
        current = status(usize::MAX >> 12)?;
    }
    loop {
        let ready = if whole { current.state == "done" } else { current.chunks_done > 0 };
        if ready {
            break;
        }
        if !current.active() {
            return Err(current.error.clone().unwrap_or_else(|| "Transcription stopped before it was done. Resume it, then try again.".into()));
        }
        let text = match (current.state.as_str(), whole) {
            ("queued", _) => "Waiting for another project's transcription to finish first".to_owned(),
            (_, true) => format!("Transcribing {}%; the clip plan starts when it's done", current.percent()),
            (_, false) => "Transcribing the first part; chat starts as soon as it's ready".to_owned(),
        };
        step(&text);
        *progress.lock().unwrap_or_else(|p| p.into_inner()) = text;
        std::thread::sleep(Duration::from_secs(3));
        current = status(usize::MAX >> 12)?;
    }
    let all = status(0)?;
    Ok((all.segments, all.covered_seconds))
}

impl CanopyApp for VideoChat {
    fn ui(&mut self, ui: &mut egui::Ui, host: &mut dyn Host) {
        self.take_replies();
        for toast in self.toasts.drain(..) {
            host.toast(toast);
        }
        self.poll();
        self.save_soon();
        view::draw(self, ui, host);
    }

    fn receive(&mut self, files: Vec<SharedFile>, _host: &mut dyn Host) -> Receipt {
        let mut receipt = Receipt::default();
        for file in files {
            if accepts(&file) {
                self.import(file.path);
                receipt.accepted += 1;
            } else {
                receipt.skipped.push(file.name);
            }
        }
        if receipt.accepted > 0 {
            receipt.note = Some(if receipt.accepted == 1 { "Importing the video into Video Chat".into() } else { format!("Importing {} videos into Video Chat", receipt.accepted) });
        }
        receipt
    }

    fn open_document(&mut self, id: &str, _host: &mut dyn Host) {
        self.open_project(id);
    }

    fn save(&mut self) -> Result<(), String> {
        self.save_now()
    }

    fn title(&self) -> String {
        self.open.as_ref().map(|o| o.name.clone()).unwrap_or_default()
    }
}
