//! Drawing Video Chat's pane: the library (import, downloads, projects, transcription) on the left, the video with its
//! clip map and transcript in the middle, and the producer chat, clips and settings on the right.

use super::model::{self, Clip, Line, Panel, Region, Side, clock, clock_tenths};
use super::*;
use canopy_ui::{Icon, Mood, Row, RowMark, color, font, icon_button, muted, pill, primary_button, quiet_button, section_label};
use egui::{Align, Align2, Color32, CursorIcon, Key, Layout, Modifiers, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui, UiBuilder, pos2, vec2};

const RAIL: f32 = 250.0;
const SIDE: f32 = 350.0;
const PROMPTS: [&str; 3] = [
    "Find the three strongest hooks and make vertical Shorts.",
    "Split this stream into useful topic chapters.",
    "Pull the funniest moments as Shorts.",
];

pub(super) fn draw(app: &mut VideoChat, ui: &mut Ui, host: &mut dyn Host) {
    let rect = ui.max_rect();
    match app.engine {
        EngineState::Running => {}
        state => {
            engine_screen(app, ui, rect, state);
            return;
        }
    }
    if host.focused() {
        shortcuts(app, ui, host);
    }
    if let Some(player) = app.open.as_mut().and_then(|o| o.player.as_mut()) {
        player.update(ui.ctx());
    }
    let bar = Rect::from_min_size(rect.min, vec2(rect.width(), 40.0));
    toolbar(app, ui, bar);
    let body = Rect::from_min_max(pos2(rect.left(), bar.bottom() + 4.0), rect.max);
    let narrow = body.width() < 1000.0;
    let rail_w = if app.show_library { RAIL } else { 0.0 };
    let side_w = if narrow { 300.0 } else { SIDE };
    let rail = Rect::from_min_size(body.min, vec2(rail_w, body.height()));
    let side = Rect::from_min_max(pos2(body.right() - side_w, body.top()), body.max);
    let middle = Rect::from_min_max(pos2(rail.right() + if rail_w > 0.0 { 6.0 } else { 0.0 }, body.top()), pos2(side.left() - 6.0, body.bottom()));
    if rail_w > 0.0 {
        panel(ui, rail, |ui| library(app, ui));
    }
    panel(ui, side, |ui| side_panel(app, ui, host));
    stage_column(app, ui, middle, host);
    dialogs(app, ui);
}

/// A pane-colored card filling `rect`, with `contents` inside its padding.
fn panel(ui: &mut Ui, rect: Rect, contents: impl FnOnce(&mut Ui)) {
    ui.painter().rect_filled(rect, 10, color::VEIL);
    ui.painter().rect_stroke(rect, 10, Stroke::new(1.0, color::HAIRLINE), StrokeKind::Inside);
    ui.scope_builder(UiBuilder::new().max_rect(rect.shrink(10.0)), |ui| {
        ui.set_clip_rect(rect.shrink(4.0));
        contents(ui)
    });
}

fn engine_screen(app: &mut VideoChat, ui: &mut Ui, rect: Rect, state: EngineState) {
    ui.scope_builder(UiBuilder::new().max_rect(rect), |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(rect.height() * 0.32);
            match state {
                EngineState::Checking | EngineState::Starting => {
                    ui.add(egui::Spinner::new().size(22.0));
                    ui.add_space(8.0);
                    ui.label(RichText::new("Starting Video Chat").font(font::display(20.0)));
                    ui.label(muted("Its engine (downloads, transcription and the AI producer) takes a few seconds to wake up."));
                }
                EngineState::Missing => {
                    ui.label(RichText::new("Where is Video Chat?").font(font::display(20.0)));
                    ui.label(muted("Canopy runs Video Chat's engine from its folder (the one with backend and frontend in it)."));
                    ui.add_space(10.0);
                    if primary_button(ui, "Find the Video Chat folder", Mood::Ember).clicked() {
                        if let Some(dir) = rfd::FileDialog::new().set_title("Find the Video Chat folder").pick_folder() {
                            match engine::remember_folder(&dir) {
                                Ok(()) => {
                                    app.folder = Some(dir);
                                    app.start_engine_then_wait();
                                }
                                Err(error) => app.engine_error = error,
                            }
                        }
                    }
                    if !app.engine_error.is_empty() {
                        ui.colored_label(color::DANGER, &app.engine_error);
                    }
                }
                _ => {
                    ui.label(RichText::new("Video Chat's engine didn't start").font(font::display(20.0)));
                    ui.add(egui::Label::new(RichText::new(&app.engine_error).color(color::DANGER)).wrap());
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.add_space((ui.available_width() - 260.0).max(0.0) / 2.0);
                        if quiet_button(ui, "Try again", None).clicked() {
                            engine::stop();
                            app.check_engine();
                        }
                        if quiet_button(ui, "Show its log", Some(Icon::File)).clicked() {
                            let _ = std::process::Command::new("notepad").arg(app.place.data_dir.join("engine.log")).spawn();
                        }
                    });
                }
            }
        });
    });
}

fn toolbar(app: &mut VideoChat, ui: &mut Ui, rect: Rect) {
    ui.scope_builder(UiBuilder::new().max_rect(rect.shrink2(vec2(4.0, 5.0))), |ui| {
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if icon_button(ui, Icon::Folder, if app.show_library { "Hide the library" } else { "Show the library" }, app.show_library).clicked() {
                app.show_library = !app.show_library;
            }
            match &app.open {
                Some(open) => {
                    ui.label(RichText::new(&open.name).font(font::display(17.0)).color(color::INK));
                    let mut detail = clock(open.duration);
                    if open.size > 0 {
                        detail.push_str(&format!(" · {}", canopy_ui::format_size(open.size)));
                    }
                    if let Some(source) = &open.source {
                        detail.push_str(&format!(" · {}×{}", source.width, source.height));
                    }
                    ui.label(muted(detail));
                }
                None => {
                    ui.label(RichText::new("Video Chat").font(font::display(17.0)).color(color::INK));
                    ui.label(muted("AI clip studio"));
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let here = engine::started_here();
                let engine = pill(ui, "Engine on", Some(Accent::Moss), false).on_hover_text(if here {
                    "Canopy started Video Chat's engine. It stops when Canopy closes, unless it's still downloading or transcribing."
                } else {
                    "Video Chat's engine was already running (the web page may have started it)."
                });
                engine.context_menu(|ui| {
                    if ui.button("Show the engine's log").clicked() {
                        let _ = std::process::Command::new("notepad").arg(app.place.data_dir.join("engine.log")).spawn();
                        ui.close();
                    }
                    if let Some(folder) = &app.folder {
                        if ui.button("Show Video Chat's folder").clicked() {
                            let _ = std::process::Command::new("explorer").arg(folder).spawn();
                            ui.close();
                        }
                    }
                    if here && ui.button("Restart the engine").on_hover_text("Downloads pause and pick up again; transcription carries on from its last finished part").clicked() {
                        engine::stop();
                        app.start_engine_then_wait();
                        ui.close();
                    }
                });
                if let Some(error) = app.error.clone() {
                    if ui.small_button("×").on_hover_text("Dismiss").clicked() {
                        app.error = None;
                    }
                    let shown = if error.len() > 140 { format!("{}…", &error[..error.floor_char_boundary(140)]) } else { error.clone() };
                    ui.label(RichText::new(shown).font(font::ui(13.0)).color(color::DANGER)).on_hover_text(error);
                } else if let Some(busy) = &app.busy {
                    let detail = app.progress.lock().unwrap_or_else(|p| p.into_inner()).clone();
                    ui.label(RichText::new(if detail.is_empty() { busy.clone() } else { detail }).font(font::ui(13.0)).color(color::SELECT_INK));
                    ui.add(egui::Spinner::new().size(14.0));
                } else if let Some(step) = app.open.as_ref().and_then(|o| app.planning.lock().unwrap_or_else(|p| p.into_inner()).get(&o.job).cloned()) {
                    ui.label(RichText::new(step).font(font::ui(13.0)).color(color::SELECT_INK)).on_hover_text("You can open other projects meanwhile; the plan is saved into this one.");
                    ui.add(egui::Spinner::new().size(14.0));
                }
            });
        });
    });
}

// ---- The library ----

fn library(app: &mut VideoChat, ui: &mut Ui) {
    egui::ScrollArea::vertical().id_salt("vc-library").auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        ui.horizontal(|ui| {
            if quiet_button(ui, "Import video", Some(Icon::Plus)).on_hover_text("A video on this PC becomes a project. On the same drive it's linked, not copied, so even a 40 GB VOD is instant.").clicked() {
                if let Some(path) = rfd::FileDialog::new().set_title("Import a video").add_filter("Video", &["mp4", "mov", "mkv", "webm", "m4v", "avi", "ts", "flv"]).pick_file() {
                    app.import(path);
                }
            }
        });
        ui.label(muted("Or drop a video on Video Chat.").font(font::ui(12.0)));
        ui.add_space(6.0);
        if app.open.is_some() {
            analysis(app, ui);
            ui.add_space(8.0);
        }
        projects(app, ui);
        ui.add_space(8.0);
        downloads(app, ui);
    });
}

fn downloads(app: &mut VideoChat, ui: &mut Ui) {
    section_label(ui, "Download a video");
    let width = ui.available_width();
    ui.add(egui::TextEdit::singleline(&mut app.form.url).hint_text("Twitch or Instagram link").desired_width(width).margin(vec2(8.0, 6.0)));
    let instagram = app.form.url.to_lowercase().contains("instagram.com");
    ui.horizontal(|ui| {
        if !instagram {
            egui::ComboBox::from_id_salt("vc-quality").width(80.0).selected_text(match app.form.quality.as_str() {
                "best" => "Source",
                "480" => "480p",
                "audio" => "Audio only",
                _ => "720p",
            }).show_ui(ui, |ui| {
                for (value, label) in [("best", "Source"), ("720", "720p"), ("480", "480p"), ("audio", "Audio only")] {
                    ui.selectable_value(&mut app.form.quality, value.to_owned(), label);
                }
            });
        }
        egui::ComboBox::from_id_salt("vc-login").width(110.0).selected_text(match app.form.browser.as_str() {
            "" => "No login".to_owned(),
            b => format!("{} login", capitalized(b)),
        }).show_ui(ui, |ui| {
            ui.selectable_value(&mut app.form.browser, String::new(), "No login");
            for browser in ["zen", "chrome", "edge", "firefox", "brave", "opera"] {
                ui.selectable_value(&mut app.form.browser, browser.to_owned(), format!("{} login", capitalized(browser)));
            }
        }).response.on_hover_text("Uses the Twitch or Instagram login saved in this browser (needed for subscriber-only VODs and most Instagram videos)");
    });
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("vc-when").width(110.0).selected_text(match app.form.when {
            When::Now => "Start now",
            When::Later => "Schedule…",
            When::Queue => "After the queue",
        }).show_ui(ui, |ui| {
            ui.selectable_value(&mut app.form.when, When::Now, "Start now");
            ui.selectable_value(&mut app.form.when, When::Later, "Schedule…");
            ui.selectable_value(&mut app.form.when, When::Queue, "After the queue");
        });
        if app.form.when == When::Later {
            let valid = model::parse_local(&app.form.at).is_some();
            ui.add(egui::TextEdit::singleline(&mut app.form.at).desired_width(ui.available_width()).hint_text("2026-11-07 01:00").text_color(if valid { color::INK } else { color::DANGER }))
                .on_hover_text("Local time, as year-month-day hour:minute");
        }
    });
    if let Some((folder, default, same_drive)) = app.download_folder.clone() {
        ui.horizontal(|ui| {
            let shown = if folder == default { "Video Chat's downloads folder".to_owned() } else { folder.clone() };
            ui.add(egui::Label::new(muted(format!("Save to: {shown}")).font(font::ui(12.0))).truncate()).on_hover_text(&folder);
        });
        ui.horizontal(|ui| {
            if ui.small_button("Change…").clicked() {
                if let Some(dir) = rfd::FileDialog::new().set_title("Save downloads in").pick_folder() {
                    let body = json!({"folder": dir});
                    app.spawn(move || Reply::Folder(engine::request("PUT", "/api/twitch/folder", Some(&body), Some(Duration::from_secs(10)))));
                }
            }
            if folder != default && ui.small_button("Use the default").clicked() {
                app.spawn(|| Reply::Folder(engine::request("PUT", "/api/twitch/folder", Some(&json!({"folder": ""})), Some(Duration::from_secs(10)))));
            }
            if ui.small_button("Show").clicked() {
                let _ = std::process::Command::new("explorer").arg(&folder).spawn();
            }
        });
        if !same_drive {
            ui.label(RichText::new("This folder is on another drive than Video Chat's projects, so opening a download copies it (slow for big VODs).").font(font::ui(12.0)).color(color::SUN));
        }
    }
    let url = app.form.url.trim().to_owned();
    let ready = !url.is_empty() && (app.form.when != When::Later || model::parse_local(&app.form.at).is_some());
    let label = match app.form.when {
        When::Now => "Download",
        When::Later => "Schedule download",
        When::Queue => "Add to the queue",
    };
    ui.add_enabled_ui(ready, |ui| {
        if quiet_button(ui, label, Some(Icon::Export)).clicked() {
            let mut body = json!({"url": url, "quality": app.form.quality, "browser": app.form.browser});
            let path = match app.form.when {
                When::Now => "/api/twitch/downloads",
                When::Queue => "/api/twitch/downloads/queue",
                When::Later => {
                    body["at"] = json!(model::parse_local(&app.form.at).unwrap_or_default());
                    "/api/twitch/downloads/schedule"
                }
            };
            app.change("POST", path.into(), Some(body), match app.form.when {
                When::Now => "Download started",
                When::Later => "Download scheduled",
                When::Queue => "Added to the queue",
            });
            app.form.url.clear();
            app.downloads_at = Instant::now() - Duration::from_secs(60);
        }
    });

    // Unfinished downloads always show, first; then the newest finished ones.
    let unfinished: Vec<Download> = app.downloads.iter().filter(|d| d.unfinished()).cloned().collect();
    let finished: Vec<Download> = app.downloads.iter().filter(|d| !d.unfinished()).cloned().collect();
    let more = finished.len().saturating_sub(6);
    let shown: Vec<Download> = unfinished.into_iter().chain(finished.into_iter().take(if app.all_downloads { usize::MAX } else { 6 })).collect();
    for download in shown {
        download_row(app, ui, &download);
    }
    if more > 0 && ui.small_button(if app.all_downloads { "Show fewer".to_owned() } else { format!("Show all {} downloads", app.downloads.len()) }).clicked() {
        app.all_downloads = !app.all_downloads;
    }
}

fn download_row(app: &mut VideoChat, ui: &mut Ui, download: &Download) {
    let key = engine::encode(&download.key);
    egui::Frame::new().fill(color::VEIL).corner_radius(8).inner_margin(8.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.add(egui::Label::new(RichText::new(download.name()).font(font::ui(13.0)).color(color::INK)).truncate()).on_hover_text(&download.url);
        let detail = if download.status == "queued" { format!("{} · {}", download.detail(), queue_place(download, &app.downloads)) } else { download.detail() };
        ui.label(muted(detail).font(font::ui(12.0)));
        if download.status == "downloading" {
            ui.add(egui::ProgressBar::new((download.percent / 100.0) as f32).desired_height(4.0));
        }
        if let (Some(error), "error") = (&download.error, download.status.as_str()) {
            ui.add(egui::Label::new(RichText::new(error).font(font::ui(12.0)).color(color::DANGER)).wrap());
        }
        ui.horizontal_wrapped(|ui| {
            let again = json!({"url": download.url, "quality": download.quality, "browser": download.browser});
            match download.status.as_str() {
                "downloading" => {
                    if ui.small_button("Pause").clicked() {
                        app.change("POST", format!("/api/twitch/downloads/{key}/pause"), None, "Download paused");
                    }
                }
                "error" | "paused" => {
                    if ui.small_button("Resume").clicked() {
                        app.change("POST", "/api/twitch/downloads".into(), Some(again), "Download resumed");
                    }
                }
                "scheduled" | "queued" => {
                    if ui.small_button("Start now").clicked() {
                        app.change("POST", "/api/twitch/downloads".into(), Some(again), "Download started");
                    }
                    if ui.small_button("Cancel").clicked() {
                        app.change("DELETE", format!("/api/twitch/downloads/{key}/schedule"), None, "Cancelled");
                    }
                }
                "done" => {
                    let opened = download.project_job_id.is_some() && download.project_job_id.as_deref() == app.open.as_ref().map(|o| o.job.as_str());
                    let label = if download.project_job_id.is_some() { "Open project" } else { "Open in Video Chat" };
                    if ui.add_enabled(!opened, egui::Button::new(RichText::new(label).color(color::SELECT_INK))).clicked() {
                        match &download.project_job_id {
                            Some(job) => {
                                let job = job.clone();
                                app.open_project(&job);
                            }
                            None => {
                                let path = format!("/api/twitch/downloads/{key}/project");
                                app.spawn(move || Reply::Changed { done: "Made a project from the download".into(), result: engine::request("POST", &path, None, Some(Duration::from_secs(600))), open: true });
                                app.busy = Some("Making a project from the download".into());
                            }
                        }
                    }
                }
                _ => {}
            }
            if !matches!(download.status.as_str(), "downloading" | "scheduled" | "queued") && ui.small_button("Delete").clicked() {
                app.confirm = Some(Confirm::DeleteDownload(download.clone()));
            }
        });
    });
}

/// "#2 in the queue, after Stream night".
fn queue_place(download: &Download, all: &[Download]) -> String {
    let find = |key: &Option<String>| key.as_ref().and_then(|k| all.iter().find(|d| &d.key == k));
    let first = find(&download.after_key);
    let (mut place, mut ahead) = (1, first);
    while let Some(d) = ahead.filter(|d| d.status == "queued" && d.after_key.is_some() && place < 100) {
        place += 1;
        ahead = find(&d.after_key);
    }
    format!("#{place} in the queue, after {}", first.map_or("the one before".to_owned(), Download::name))
}

fn projects(app: &mut VideoChat, ui: &mut Ui) {
    ui.horizontal(|ui| {
        section_label(ui, "Projects");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| ui.label(muted(app.projects.len().to_string()).font(font::ui(12.0))));
    });
    if app.projects.len() > 5 {
        ui.add(egui::TextEdit::singleline(&mut app.project_search).hint_text("Search projects").desired_width(ui.available_width()).margin(vec2(8.0, 5.0)));
    }
    if app.projects.is_empty() {
        ui.label(muted("No projects yet. Import a video or download one.").font(font::ui(12.0)));
    }
    let query = app.project_search.trim().to_lowercase();
    let planning = app.planning.lock().unwrap_or_else(|p| p.into_inner()).clone();
    let open_job = app.open.as_ref().map(|o| o.job.clone());
    for project in app.projects.clone() {
        if !query.is_empty() && !project.name.to_lowercase().contains(&query) {
            continue;
        }
        let detail = planning.get(&project.job_id).cloned().unwrap_or_else(|| project.detail());
        let open = open_job.as_deref() == Some(project.job_id.as_str());
        let row = (Row { title: model::without_extension(&project.name), detail: &detail, mark: RowMark::Leaf(Accent::Moss), highlighted: open }).show(ui);
        let row = row.on_hover_text(&project.name);
        if row.clicked() && !open && app.busy.is_none() {
            app.open_project(&project.job_id);
        }
        row.context_menu(|ui| {
            if ui.button("Rename…").clicked() {
                app.rename = Some((project.job_id.clone(), model::without_extension(&project.name).to_owned()));
                ui.close();
            }
            if let Some(folder) = &app.folder {
                if ui.button("Show in folder").clicked() {
                    let _ = std::process::Command::new("explorer").arg(engine::jobs_dir(folder).join(&project.job_id)).spawn();
                    ui.close();
                }
            }
            ui.separator();
            if ui.button(RichText::new("Delete…").color(color::DANGER)).clicked() {
                app.confirm = Some(Confirm::DeleteProject(project.clone()));
                ui.close();
            }
        });
    }
}

fn analysis(app: &mut VideoChat, ui: &mut Ui) {
    let Some(open) = &app.open else { return };
    section_label(ui, "Transcript");
    let mut start = false;
    egui::Frame::new().fill(color::VEIL).corner_radius(8).inner_margin(8.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let (label, detail) = match &open.transcription {
            None => ("Checking…".to_owned(), String::new()),
            Some(t) => match t.state.as_str() {
                "done" => ("Transcript ready".to_owned(), format!("{} timed lines", open.transcript.len())),
                "running" => (format!("Transcribing {}%", t.percent()), format!("{} of {} done; chat works on what's there", clock(t.covered_seconds), clock(t.duration_seconds))),
                "queued" => ("Waiting to transcribe".to_owned(), "Another project is transcribing; this one starts after it.".to_owned()),
                "error" => ("Transcription stopped".to_owned(), t.error.clone().unwrap_or_default()),
                _ => ("Not transcribed yet".to_owned(), "It starts by itself when you first ask the producer, or now:".to_owned()),
            },
        };
        ui.label(RichText::new(label).font(font::ui(14.0)).color(color::INK));
        if !detail.is_empty() {
            ui.add(egui::Label::new(muted(detail).font(font::ui(12.0))).wrap());
        }
        if let Some(t) = &open.transcription {
            if t.chunk_count > 0 && t.state != "done" {
                ui.add(egui::ProgressBar::new(t.percent() as f32 / 100.0).desired_height(4.0));
            }
            if matches!(t.state.as_str(), "idle" | "error" | "") {
                start = quiet_button(ui, if t.state == "error" { "Resume transcription" } else { "Start transcription" }, None).clicked();
            }
        }
        ui.label(muted(format!("Faster-Whisper {} · {}", open.settings.transcription_model, open.settings.transcription_device)).font(font::ui(11.0)));
    });
    if start {
        app.start_transcription();
    }
}

// ---- The video, the clip map and the transcript ----

fn stage_column(app: &mut VideoChat, ui: &mut Ui, rect: Rect, host: &mut dyn Host) {
    if app.open.is_none() {
        welcome(app, ui, rect);
        return;
    }
    let transport_h = 36.0;
    let map_h = 120.0;
    let transcript_h = (rect.height() * 0.24).clamp(110.0, 240.0);
    let stage = Rect::from_min_max(rect.min, pos2(rect.right(), rect.bottom() - transcript_h - map_h - transport_h - 18.0));
    let transport = Rect::from_min_size(pos2(rect.left(), stage.bottom() + 6.0), vec2(rect.width(), transport_h));
    let map = Rect::from_min_size(pos2(rect.left(), transport.bottom() + 6.0), vec2(rect.width(), map_h));
    let transcript = Rect::from_min_max(pos2(rect.left(), map.bottom() + 6.0), rect.max);
    video(app, ui, stage);
    transport_bar(app, ui, transport, host);
    clip_map(app, ui, map, host);
    panel(ui, transcript, |ui| transcript_panel(app, ui));
}

fn welcome(app: &mut VideoChat, ui: &mut Ui, rect: Rect) {
    ui.painter().rect_filled(rect, 10, color::VEIL);
    ui.painter().rect_stroke(rect, 10, Stroke::new(1.0, color::HAIRLINE), StrokeKind::Inside);
    ui.scope_builder(UiBuilder::new().max_rect(rect), |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(rect.height() * 0.3);
            ui.label(RichText::new("Start with a long video").font(font::display(22.0)));
            ui.label(muted("A stream, podcast or interview. Ask the AI producer about it, and it plans Shorts and chapters you can review, caption and render, or open in StreamCuts."));
            ui.add_space(12.0);
            if primary_button(ui, "Import video", Mood::Ember).clicked() {
                if let Some(path) = rfd::FileDialog::new().set_title("Import a video").add_filter("Video", &["mp4", "mov", "mkv", "webm", "m4v", "avi", "ts", "flv"]).pick_file() {
                    app.import(path);
                }
            }
            ui.add_space(8.0);
            ui.label(muted("Drop a video here, paste a Twitch link under Download, or pick a project on the left."));
        });
    });
}

/// Where the picture goes in `rect`, keeping the source's shape.
fn picture_rect(rect: Rect, width: u32, height: u32) -> Rect {
    let aspect = width.max(1) as f32 / height.max(1) as f32;
    let size = if rect.width() / rect.height() > aspect { vec2(rect.height() * aspect, rect.height()) } else { vec2(rect.width(), rect.width() / aspect) };
    Rect::from_center_size(rect.center(), size)
}

fn video(app: &mut VideoChat, ui: &mut Ui, rect: Rect) {
    ui.painter().rect_filled(rect, 10, Color32::from_rgb(6, 6, 4));
    let selected = app.selected_clip().cloned();
    let selected_kind = app.open.as_ref().and_then(|o| o.selected).map(|s| s.kind);
    let Some(open) = &mut app.open else { return };
    let Some(source) = open.source.clone() else {
        let text = open.source_error.clone().unwrap_or_else(|| "Reading the video…".into());
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, font::ui(14.0), color::INK_MUTED);
        return;
    };
    let picture = picture_rect(rect.shrink(4.0), source.width, source.height);
    let Some(player) = &open.player else { return };
    let playhead = player.playhead;
    match player.texture() {
        Some(texture) => {
            ui.painter().image(texture.id(), picture, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            if player.waiting() {
                ui.painter().rect_filled(picture, 0, Color32::from_black_alpha(60));
            }
        }
        None => {
            ui.painter().text(picture.center(), Align2::CENTER_CENTER, "Loading the picture…", font::ui(14.0), color::INK_MUTED);
        }
    }
    // Clicking the picture plays or pauses it, as a video player does.
    let editing = open.layout_edit.is_some();
    let click = ui.interact(picture, ui.scope_id().with("vc-picture"), if editing { Sense::drag() } else { Sense::click() });
    if !editing && click.clicked() {
        if let Some(player) = &mut open.player {
            player.toggle();
        }
    }
    // The Short's captions while it plays.
    if let (Some(clip), Some(ClipKind::Short), false) = (&selected, selected_kind, editing) {
        if let Some((line, word)) = model::caption_at(clip.lines(), playhead - clip.start_seconds) {
            draw_caption(ui, picture, line, word, &open.settings.caption_style);
        }
    }
    if let Some(clip) = &selected {
        let tag = match selected_kind {
            Some(ClipKind::Short) => "SHORT",
            _ => "CHAPTER",
        };
        let label = format!("{tag} · {}", clip.title);
        let galley = ui.painter().layout_no_wrap(label, font::ui(12.0), color::INK);
        let chip = Rect::from_min_size(picture.min + vec2(10.0, 10.0), galley.size() + vec2(16.0, 8.0));
        ui.painter().rect_filled(chip, 6, Color32::from_black_alpha(160));
        ui.painter().galley(chip.min + vec2(8.0, 4.0), galley, color::INK);
    }
    if editing {
        layout_editor(app, ui, picture, &click);
    } else if let Some(open) = &app.open {
        if open.settings.short_layout.stacked() && selected_kind == Some(ClipKind::Short) {
            // A small preview of how the Short stacks, from the current picture.
            if let Some(texture) = open.player.as_ref().and_then(|p| p.texture()) {
                let h = (rect.height() * 0.42).clamp(90.0, 240.0);
                let preview = Rect::from_min_size(pos2(rect.right() - h * 9.0 / 16.0 - 10.0, rect.bottom() - h - 10.0), vec2(h * 9.0 / 16.0, h));
                stacked_preview(ui, preview, texture, &open.settings.short_layout, source.width as f32, source.height as f32);
            }
        }
    }
}

fn draw_caption(ui: &Ui, picture: Rect, line: &Line, word: Option<usize>, style: &model::CaptionStyle) {
    let size = (picture.height() * style.font_size as f32 / 1920.0 * 1.6).clamp(12.0, 64.0);
    let (text_color, highlight, outline) = (model::hex_color(&style.text_color), model::hex_color(&style.highlight_color), model::hex_color(&style.outline_color));
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = picture.width() * 0.86;
    job.halign = Align::Center;
    for (i, w) in line.words.iter().enumerate() {
        let text = if style.uppercase { w.text.trim().to_uppercase() } else { w.text.trim().to_owned() };
        let color = if Some(i) == word { highlight } else { text_color };
        job.append(&text, if i == 0 { 0.0 } else { size * 0.28 }, egui::TextFormat::simple(font::display(size), color));
    }
    let galley = ui.painter().layout_job(job);
    let at = pos2(picture.center().x, picture.top() + picture.height() * style.position_y - galley.size().y / 2.0);
    let outline_w = (size * 0.08).max(1.0);
    for (dx, dy) in [(-1.0, -1.0), (0.0, -1.0), (1.0, -1.0), (-1.0, 0.0), (1.0, 0.0), (-1.0, 1.0), (0.0, 1.0), (1.0, 1.0)] {
        ui.painter().galley_with_override_text_color(at + vec2(dx, dy) * outline_w, galley.clone(), outline);
    }
    ui.painter().galley(at, galley, text_color);
}

/// Draws a stacked Short from the source picture into `rect` (9:16), as the render stacks it.
fn stacked_preview(ui: &Ui, rect: Rect, texture: &egui::TextureHandle, layout: &model::Layout, sw: f32, sh: f32) {
    ui.painter().rect_filled(rect.expand(2.0), 6, Color32::BLACK);
    let scale = rect.height() / model::SHORT_H;
    for (region, top, height) in model::stacked_pieces(layout, sw, sh) {
        let kept = model::cover(region, sw, sh, model::SHORT_W, height);
        let slot = Rect::from_min_size(pos2(rect.left(), rect.top() + top * scale), vec2(rect.width(), height * scale));
        let uv = Rect::from_min_size(pos2(kept.x, kept.y), vec2(kept.w, kept.h));
        ui.painter().image(texture.id(), slot, uv, Color32::WHITE);
    }
    ui.painter().rect_stroke(rect.expand(2.0), 6, Stroke::new(1.0, color::HAIRLINE_STRONG), StrokeKind::Outside);
}

/// Drawing the stacked layout's regions on the picture: drag a box, then say what it is.
fn layout_editor(app: &mut VideoChat, ui: &mut Ui, picture: Rect, response: &egui::Response) {
    let Some(open) = &mut app.open else { return };
    let to_frac = |p: Pos2| pos2(((p.x - picture.left()) / picture.width()).clamp(0.0, 1.0), ((p.y - picture.top()) / picture.height()).clamp(0.0, 1.0));
    let to_rect = |r: Region| Rect::from_min_size(picture.min + vec2(r.x * picture.width(), r.y * picture.height()), vec2(r.w * picture.width(), r.h * picture.height()));
    let between = |a: Pos2, b: Pos2| Region { x: a.x.min(b.x), y: a.y.min(b.y), w: (a.x - b.x).abs(), h: (a.y - b.y).abs() };
    let edit = open.layout_edit.get_or_insert_with(LayoutEdit::default);
    if response.drag_started() && edit.draft.is_none() {
        edit.from = response.interact_pointer_pos().map(to_frac);
    }
    if let (Some(from), Some(now)) = (edit.from, response.interact_pointer_pos()) {
        edit.draft = Some(between(from, to_frac(now)));
    }
    if response.drag_stopped() {
        edit.from = None;
        edit.draft = edit.draft.filter(|r| r.w > 0.02 && r.h > 0.02);
    }
    ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
    ui.painter().rect_filled(picture, 0, Color32::from_black_alpha(70));
    let layout = &open.settings.short_layout;
    let mut regions: Vec<(Region, String, Color32)> = Vec::new();
    if layout.mode == "stacked" && !layout.gameplay.is_full() {
        regions.push((layout.gameplay, "Gameplay".into(), color::LEAF));
    }
    for (i, panel) in layout.panels.iter().enumerate() {
        regions.push((panel.region(), format!("{} {}", if panel.position == Side::Top { "On top" } else { "Below" }, i + 1), color::SUN));
    }
    for (region, label, tint) in regions {
        let r = to_rect(region);
        ui.painter().rect_stroke(r, 4, Stroke::new(2.0, tint), StrokeKind::Inside);
        ui.painter().text(r.min + vec2(6.0, 6.0), Align2::LEFT_TOP, label, font::ui(12.0), tint);
    }
    let mut assign = None;
    let mut cancel = false;
    if let Some(draft) = edit.draft {
        let r = to_rect(draft);
        ui.painter().rect_stroke(r, 4, Stroke::new(2.0, color::INK), StrokeKind::Inside);
        if edit.from.is_none() {
            let full = layout.panels.len() >= 4;
            egui::Area::new(ui.scope_id().with("vc-region-menu")).order(egui::Order::Foreground).fixed_pos(pos2(r.left(), (r.bottom() + 6.0).min(picture.bottom() - 34.0))).show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui.add_enabled(!full, egui::Button::new("Put on top")).clicked() {
                            assign = Some(Some(Side::Top));
                        }
                        if ui.add_enabled(!full, egui::Button::new("Put below")).clicked() {
                            assign = Some(Some(Side::Bottom));
                        }
                        if ui.button("Gameplay area").clicked() {
                            assign = Some(None);
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                });
            });
        }
    } else {
        let hint = "Drag a box around your facecam, chat or anything else to stack it on the Short";
        let galley = ui.painter().layout_no_wrap(hint.into(), font::ui(13.0), color::INK);
        let chip = Rect::from_center_size(pos2(picture.center().x, picture.top() + 22.0), galley.size() + vec2(16.0, 8.0));
        ui.painter().rect_filled(chip, 6, Color32::from_black_alpha(170));
        ui.painter().galley(chip.min + vec2(8.0, 4.0), galley, color::INK);
    }
    if let (Some(side), Some(draft)) = (assign, edit.draft) {
        let layout = &mut open.settings.short_layout;
        layout.mode = "stacked".into();
        match side {
            Some(position) => layout.panels.push(Panel { x: draft.x, y: draft.y, w: draft.w, h: draft.h, position }),
            None => layout.gameplay = draft,
        }
        edit.draft = None;
        open.dirty_at = Some(Instant::now());
    }
    if cancel {
        edit.draft = None;
    }
}

fn transport_bar(app: &mut VideoChat, ui: &mut Ui, rect: Rect, host: &mut dyn Host) {
    let selected = app.selected_clip().cloned();
    let selected_ref = app.open.as_ref().and_then(|o| o.selected);
    let Some(open) = &mut app.open else { return };
    let mut edit = None;
    ui.scope_builder(UiBuilder::new().max_rect(rect), |ui| {
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let Some(player) = &mut open.player else {
                ui.label(muted("The player starts once the video is read."));
                return;
            };
            if icon_button(ui, Icon::SkipBack, "Back 10 seconds (Shift+Left: 1 s)", false).clicked() {
                player.seek(player.playhead - 10.0);
            }
            let playing = player.playing();
            if icon_button(ui, if playing { Icon::Pause } else { Icon::Play }, if playing { "Pause (Space)" } else { "Play (Space)" }, playing).clicked() {
                player.toggle();
            }
            if icon_button(ui, Icon::SkipForward, "On 10 seconds (Shift+Right: 1 s)", false).clicked() {
                player.seek(player.playhead + 10.0);
            }
            ui.add_space(6.0);
            ui.label(RichText::new(format!("{} / {}", clock_tenths(player.playhead), clock(player.duration()))).font(font::ui(13.0)).color(color::INK));
            if let Some(clip) = &selected {
                ui.add_space(8.0);
                if quiet_button(ui, "Play this clip", Some(Icon::Play)).on_hover_text("From its start to its end").clicked() {
                    player.play_span(clip.start_seconds, clip.end_seconds);
                }
                if selected_ref.is_some_and(|s| s.kind == ClipKind::Short) && quiet_button(ui, "Edit in StreamCuts", Some(Icon::Scissors)).on_hover_text("Opens this Short in StreamCuts as a cut of the whole video: drag its edges to grow it (E)").clicked() {
                    edit = selected_ref.map(|s| s.index);
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if open.settings.short_layout.mode == "stacked" || open.layout_edit.is_some() {
                    let editing = open.layout_edit.is_some();
                    let tip = if editing { "Done with the Short layout" } else { "Edit the Short layout: draw the regions stacked above and below the gameplay" };
                    if icon_button(ui, Icon::Grid, tip, editing).clicked() {
                        open.layout_edit = if editing { None } else { Some(LayoutEdit::default()) };
                        if let Some(player) = &mut open.player {
                            player.pause();
                        }
                    }
                }
            });
        });
    });
    if let Some(index) = edit {
        app.edit_in_streamcuts(index, false, host);
    }
}

/// The clip map: the plan's chapters and Shorts along the video, zoomable (wheel), with the playhead.
fn clip_map(app: &mut VideoChat, ui: &mut Ui, rect: Rect, host: &mut dyn Host) {
    ui.painter().rect_filled(rect, 10, color::VEIL);
    ui.painter().rect_stroke(rect, 10, Stroke::new(1.0, color::HAIRLINE), StrokeKind::Inside);
    let Some(open) = &mut app.open else { return };
    let duration = open.player.as_ref().map_or(open.duration, |p| p.duration()).max(1.0);
    let label_w = 70.0;
    let inner = rect.shrink2(vec2(10.0, 8.0));
    let lanes = Rect::from_min_max(pos2(inner.left() + label_w, inner.top() + 22.0), inner.max);
    let (mut from, mut to) = open.view;
    to = to.min(duration).max(from + 1.0);
    from = from.clamp(0.0, (duration - (to - from)).max(0.0));
    let painter = ui.painter().with_clip_rect(rect);

    // Zoom with the wheel around the pointer; Shift+wheel (or a sideways wheel) pans.
    let area = ui.interact(Rect::from_min_max(pos2(lanes.left(), inner.top()), lanes.max), ui.scope_id().with("vc-map"), Sense::click_and_drag());
    if area.hovered() {
        let (scroll, zoom) = ui.input(|i| (i.smooth_scroll_delta, i.modifiers.shift));
        if let Some(p) = area.hover_pos() {
            let pan = if zoom { scroll.y } else { scroll.x };
            if !zoom && scroll.y != 0.0 {
                let factor = (-scroll.y as f64 / 300.0).exp();
                let at = from + ((p.x - lanes.left()) / lanes.width()) as f64 * (to - from);
                let width = ((to - from) * factor).clamp(5.0, duration);
                from = (at - (at - from) * width / (to - from)).clamp(0.0, (duration - width).max(0.0));
                to = from + width;
            }
            if pan != 0.0 {
                let by = -pan as f64 / lanes.width() as f64 * (to - from);
                let width = to - from;
                from = (from + by).clamp(0.0, (duration - width).max(0.0));
                to = from + width;
            }
        }
    }
    open.view = (from, to);
    let x_of = |t: f64| lanes.left() + ((t - from) / (to - from)) as f32 * lanes.width();
    let t_of = |x: f32| from + ((x - lanes.left()) / lanes.width()) as f64 * (to - from);

    // Ruler: ticks at a round step for the zoom.
    let span = to - from;
    let step = [1.0, 5.0, 10.0, 30.0, 60.0, 300.0, 600.0, 1800.0, 3600.0, 7200.0].into_iter().find(|s| span / s <= 8.0).unwrap_or(14_400.0);
    let mut tick = (from / step).ceil() * step;
    while tick <= to {
        let x = x_of(tick);
        painter.line_segment([pos2(x, inner.top() + 14.0), pos2(x, lanes.bottom())], Stroke::new(1.0, color::HAIRLINE));
        painter.text(pos2(x + 3.0, inner.top()), Align2::LEFT_TOP, clock(tick), font::ui(11.0), color::INK_FAINT);
        tick += step;
    }
    // How far the transcript reaches.
    if let Some(t) = &open.transcription {
        let reach = if t.state == "done" { duration } else { t.covered_seconds };
        if reach > from {
            let band = Rect::from_min_max(pos2(lanes.left(), inner.top() + 16.0), pos2(x_of(reach.min(to)), inner.top() + 19.0));
            painter.rect_filled(band, 1, color::LEAF.gamma_multiply(0.5));
        }
    }
    let lane_h = (lanes.height() - 6.0) / 2.0;
    let lane_rects = [
        (ClipKind::Chapter, "Chapters", Rect::from_min_size(lanes.min, vec2(lanes.width(), lane_h))),
        (ClipKind::Short, "Shorts", Rect::from_min_size(pos2(lanes.left(), lanes.top() + lane_h + 6.0), vec2(lanes.width(), lane_h))),
    ];
    let selected = open.selected;
    let mut clicked = None;
    let mut menu_action: Option<(ClipKind, usize, &'static str)> = None;
    let mut played = None;
    for (kind, label, lane) in lane_rects {
        painter.text(pos2(inner.left(), lane.center().y), Align2::LEFT_CENTER, label, font::ui(12.0), color::INK_MUTED);
        painter.rect_filled(lane, 6, color::VEIL);
        let clips: &[Clip] = match (&open.plan, kind) {
            (Some(plan), ClipKind::Short) => &plan.shorts,
            (Some(plan), ClipKind::Chapter) => &plan.longer_segments,
            (None, _) => &[],
        };
        if clips.is_empty() {
            painter.text(lane.left_center() + vec2(8.0, 0.0), Align2::LEFT_CENTER, "No proposals yet", font::ui(11.0), color::INK_FAINT);
        }
        for (i, clip) in clips.iter().enumerate() {
            if clip.end_seconds < from || clip.start_seconds > to {
                continue;
            }
            let (x0, x1) = (x_of(clip.start_seconds).max(lane.left()), x_of(clip.end_seconds).min(lane.right()));
            let r = Rect::from_min_max(pos2(x0, lane.top() + 3.0), pos2((x1).max(x0 + 4.0), lane.bottom() - 3.0));
            let is_selected = selected == Some(Selected { kind, index: i });
            let tint = if kind == ClipKind::Short { color::SUN } else { Accent::Sky.color() };
            let response = ui.interact(r, ui.scope_id().with(("vc-clip", kind == ClipKind::Short, i)), Sense::click());
            painter.rect_filled(r, 4, tint.gamma_multiply(if is_selected { 0.85 } else if response.hovered() { 0.6 } else { 0.4 }));
            if is_selected {
                painter.rect_stroke(r, 4, Stroke::new(2.0, color::INK), StrokeKind::Inside);
            }
            if r.width() > 40.0 {
                painter.with_clip_rect(r.shrink(2.0)).text(r.left_center() + vec2(5.0, 0.0), Align2::LEFT_CENTER, &clip.title, font::ui(11.0), color::INK);
            }
            let response = response.on_hover_text(format!("{}\n{} – {} · {}", clip.title, clock_tenths(clip.start_seconds), clock_tenths(clip.end_seconds), clock(clip.length())));
            if response.clicked() {
                clicked = Some((kind, i));
            }
            if response.double_clicked() {
                played = Some((clip.start_seconds, clip.end_seconds));
            }
            response.context_menu(|ui| {
                clip_menu(ui, kind, i, &mut menu_action);
            });
        }
    }
    // The playhead.
    if let Some(player) = &open.player {
        let t = player.playhead;
        if t >= from && t <= to {
            let x = x_of(t);
            painter.line_segment([pos2(x, inner.top() + 12.0), pos2(x, lanes.bottom())], Stroke::new(2.0, color::INK));
        }
    }
    // Clicking or dragging on the map moves the playhead.
    if clicked.is_none() && (area.clicked() || area.dragged()) {
        if let (Some(p), Some(player)) = (area.interact_pointer_pos(), open.player.as_mut()) {
            player.seek(t_of(p.x));
        }
    }
    let whole = (from, to) != (0.0, duration);
    // Zoom buttons, top right.
    let buttons = Rect::from_min_size(pos2(inner.right() - 150.0, inner.top() - 4.0), vec2(150.0, 18.0));
    ui.scope_builder(UiBuilder::new().max_rect(buttons), |ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if whole && ui.small_button("Whole video").clicked() {
                open.view = (0.0, duration);
            }
            if let Some(sel) = selected {
                let clip = open.plan.as_ref().and_then(|p| match sel.kind {
                    ClipKind::Short => p.shorts.get(sel.index),
                    ClipKind::Chapter => p.longer_segments.get(sel.index),
                });
                if let Some(clip) = clip {
                    if ui.small_button("Zoom to clip").clicked() {
                        let pad = clip.length().max(10.0) * 0.5;
                        open.view = ((clip.start_seconds - pad).max(0.0), (clip.end_seconds + pad).min(duration));
                    }
                }
            }
        });
    });
    if !whole {
        // Where the window is on the whole video.
        let bar = Rect::from_min_size(pos2(lanes.left(), rect.bottom() - 4.0), vec2(lanes.width(), 2.0));
        let a = lanes.left() + (from / duration) as f32 * lanes.width();
        let b = lanes.left() + (to / duration) as f32 * lanes.width();
        painter.rect_filled(Rect::from_min_max(pos2(a, bar.top()), pos2(b.max(a + 3.0), bar.bottom())), 1, color::INK_FAINT);
    }
    if let Some((kind, index)) = clicked {
        app.select(kind, index);
    }
    if let Some((start, end)) = played {
        if let Some(player) = app.open.as_mut().and_then(|o| o.player.as_mut()) {
            player.play_span(start, end);
        }
    }
    if let Some((kind, index, action)) = menu_action {
        clip_action(app, kind, index, action, host);
    }
}

/// The right-click menu of a proposed clip (on the map or in the list).
fn clip_menu(ui: &mut Ui, kind: ClipKind, index: usize, action: &mut Option<(ClipKind, usize, &'static str)>) {
    let mut item = |ui: &mut Ui, label: &str, name: &'static str, hint: &str| {
        if ui.button(label).on_hover_text(hint).clicked() {
            *action = Some((kind, index, name));
            ui.close();
        }
    };
    if kind == ClipKind::Short {
        item(ui, "Edit in StreamCuts", "streamcuts", "Open it in StreamCuts as a cut of the whole video: drag its edges to grow it");
        item(ui, "New StreamCuts edit", "streamcuts-fresh", "Start another StreamCuts edit of it, keeping the one you made before");
        ui.separator();
    }
    item(ui, "Play it", "play", "From its start to its end");
    item(ui, "Zoom to it", "zoom", "Show it large on the clip map");
    if kind == ClipKind::Short {
        item(ui, "Make captions", "captions", "From its audio; they're burned in when it renders");
    }
    ui.separator();
    item(ui, "Remove", "remove", "Take it out of the plan (Ctrl+Z brings it back)");
}

fn clip_action(app: &mut VideoChat, kind: ClipKind, index: usize, action: &str, host: &mut dyn Host) {
    match action {
        "streamcuts" | "streamcuts-fresh" => {
            app.select(kind, index);
            app.edit_in_streamcuts(index, action == "streamcuts-fresh", host);
        }
        "play" => {
            app.select(kind, index);
            if let Some((a, b)) = app.selected_clip().map(|c| (c.start_seconds, c.end_seconds)) {
                if let Some(player) = app.open.as_mut().and_then(|o| o.player.as_mut()) {
                    player.play_span(a, b);
                }
            }
        }
        "zoom" => {
            app.select(kind, index);
            if let Some((a, b)) = app.selected_clip().map(|c| (c.start_seconds, c.end_seconds)) {
                if let Some(open) = &mut app.open {
                    let pad = (b - a).max(10.0) * 0.5;
                    open.view = ((a - pad).max(0.0), (b + pad).min(open.duration.max(b + pad)));
                }
            }
        }
        "captions" => app.make_captions(vec![index]),
        "remove" => remove_clip(app, kind, index),
        _ => {}
    }
}

fn remove_clip(app: &mut VideoChat, kind: ClipKind, index: usize) {
    app.remember();
    let Some(open) = &mut app.open else { return };
    let Some(plan) = &mut open.plan else { return };
    let list = match kind {
        ClipKind::Short => &mut plan.shorts,
        ClipKind::Chapter => &mut plan.longer_segments,
    };
    if index < list.len() {
        list.remove(index);
        open.selected = None;
        open.caption_drafts.clear();
        open.dirty_at = Some(Instant::now());
        app.toasts.push("Removed from the plan. Ctrl+Z brings it back.".into());
    }
}

fn transcript_panel(app: &mut VideoChat, ui: &mut Ui) {
    let selected = app.selected_clip().cloned();
    let Some(open) = &app.open else { return };
    let playhead = open.player.as_ref().map_or(0.0, |p| p.playhead);
    ui.horizontal(|ui| {
        section_label(ui, "Transcript");
        ui.add(egui::TextEdit::singleline(&mut app.transcript_query).hint_text("Search what was said").desired_width(220.0).margin(vec2(8.0, 4.0)));
        if !app.transcript_query.is_empty() && ui.small_button("×").clicked() {
            app.transcript_query.clear();
        }
    });
    let query = app.transcript_query.trim().to_lowercase();
    let Some(open) = &app.open else { return };
    if open.transcript.is_empty() {
        ui.label(muted("The transcript shows here as it's made. Ask the producer something, or start transcription on the left."));
        return;
    }
    let (from, to) = match &selected {
        Some(clip) => (clip.start_seconds - 8.0, clip.end_seconds + 8.0),
        None => (playhead - 20.0, playhead + 40.0),
    };
    let lines: Vec<&Segment> = if query.is_empty() {
        open.transcript.iter().filter(|s| s.end >= from && s.start <= to).collect()
    } else {
        open.transcript.iter().filter(|s| s.text.to_lowercase().contains(&query)).take(300).collect()
    };
    if !query.is_empty() {
        ui.label(muted(format!("{} found{}", lines.len(), if lines.len() == 300 { " (the first 300)" } else { "" })).font(font::ui(12.0)));
    }
    let mut seek = None;
    egui::ScrollArea::vertical().id_salt("vc-transcript").auto_shrink(false).show(ui, |ui| {
        for segment in lines {
            let inside = selected.as_ref().is_some_and(|c| segment.end >= c.start_seconds && segment.start <= c.end_seconds);
            let now = playhead >= segment.start && playhead < segment.end;
            ui.horizontal_top(|ui| {
                if ui.add(egui::Button::new(RichText::new(clock(segment.start)).font(font::ui(12.0)).color(color::SELECT_INK)).frame(false)).on_hover_text("Go there").clicked() {
                    seek = Some(segment.start);
                }
                let ink = if now { color::INK } else if inside { color::INK_SOFT } else { color::INK_MUTED };
                ui.add(egui::Label::new(RichText::new(segment.text.trim()).font(font::body(13.0)).color(ink)).wrap());
            });
        }
    });
    if let Some(t) = seek {
        app.seek(t);
    }
}

// ---- The right side: the producer, the clips and the settings ----

fn side_panel(app: &mut VideoChat, ui: &mut Ui, host: &mut dyn Host) {
    let count = app.open.as_ref().and_then(|o| o.plan.as_ref()).map_or(0, |p| p.shorts.len() + p.longer_segments.len());
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (tab, label) in [(Tab::Producer, "Producer".to_owned()), (Tab::Clips, if count > 0 { format!("Clips {count}") } else { "Clips".into() }), (Tab::Settings, "Settings".to_owned())] {
            if pill(ui, &label, None, app.tab == tab).clicked() {
                app.tab = tab;
            }
        }
    });
    ui.add_space(6.0);
    if app.open.is_none() {
        ui.label(muted("Open a project to talk about it."));
        if app.tab != Tab::Settings {
            return;
        }
    }
    match app.tab {
        Tab::Producer => producer(app, ui),
        Tab::Clips => clips(app, ui, host),
        Tab::Settings => settings(app, ui),
    }
}

fn producer(app: &mut VideoChat, ui: &mut Ui) {
    let Some(open) = &app.open else { return };
    let ai = open.settings.ai_name();
    let composer_h = 150.0;
    let list_h = (ui.available_height() - composer_h).max(80.0);
    let mut suggestion = None;
    egui::ScrollArea::vertical().id_salt("vc-chat").max_height(list_h).auto_shrink(false).stick_to_bottom(true).show(ui, |ui| {
        if open.messages.is_empty() {
            bubble(ui, false, "Ask me anything about this video. I transcribe long videos in parts, so you can ask about the beginning while the rest keeps going; then I turn your direction into a clip plan you can edit.");
            for prompt in PROMPTS {
                if ui.add(egui::Button::new(RichText::new(prompt).font(font::ui(12.0))).wrap()).clicked() {
                    suggestion = Some(prompt.to_owned());
                }
            }
        }
        for message in &open.messages {
            bubble(ui, message.role == "user", &message.content);
        }
    });
    if let Some(text) = suggestion {
        app.prompt = text;
    }
    let busy = app.busy.is_some();
    let planning = app.planning.lock().unwrap_or_else(|p| p.into_inner()).contains_key(&open.job);
    let has_plan = open.plan.is_some();
    let has_direction = !app.prompt.trim().is_empty() || open.messages.iter().any(|m| m.role == "user");
    ui.add_space(6.0);
    let edit = ui.add(egui::TextEdit::multiline(&mut app.prompt).desired_rows(3).desired_width(f32::INFINITY).hint_text(format!("Ask {ai} a question, or describe the clips you want. Enter sends; Shift+Enter starts a new line.")));
    let send_key = edit.has_focus() && ui.input(|i| i.key_pressed(Key::Enter) && !i.modifiers.shift);
    if send_key {
        // The Enter typed a line break; it was meant as Send.
        let trimmed = app.prompt.trim_end_matches(['\n', '\r']).to_owned();
        app.prompt = trimmed;
    }
    let mut send = send_key && !busy && !app.prompt.trim().is_empty();
    let mut plan = false;
    ui.horizontal(|ui| {
        ui.add_enabled_ui(!busy && !app.prompt.trim().is_empty(), |ui| {
            send |= primary_button(ui, &format!("Send to {ai}"), Mood::Ember).clicked();
        });
        ui.add_enabled_ui(!busy && !planning && has_direction, |ui| {
            plan = quiet_button(ui, if has_plan { "Rebuild clip plan" } else { "Create clip plan" }, None)
                .on_hover_text("From your last few messages. It waits for the whole transcript and runs in the background: you can open other projects meanwhile.")
                .clicked();
        });
    });
    if send {
        app.send_chat();
    }
    if plan {
        app.make_plan();
    }
}

fn bubble(ui: &mut Ui, user: bool, text: &str) {
    let fill = if user { color::SELECT_FILL } else { color::VEIL };
    ui.with_layout(Layout::top_down(if user { Align::Max } else { Align::Min }), |ui| {
        egui::Frame::new().fill(fill).corner_radius(10).inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
            ui.set_max_width(ui.available_width() * 0.92);
            ui.add(egui::Label::new(RichText::new(text).font(font::body(13.5)).color(color::INK)).wrap().selectable(true));
        });
    });
    ui.add_space(4.0);
}

fn clips(app: &mut VideoChat, ui: &mut Ui, host: &mut dyn Host) {
    let Some(open) = &app.open else { return };
    let Some(plan) = open.plan.clone() else {
        ui.add(egui::Label::new(muted("No clip plan yet. Tell the producer what you want, then Create clip plan.")).wrap());
        return;
    };
    let settings = open.settings.clone();
    let selected = open.selected;
    let rendered = open.rendered.clone();
    let mut action = None;
    let mut select = None;
    let mut play = None;
    egui::ScrollArea::vertical().id_salt("vc-clips").auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        let shorts = if settings.include_shorts { plan.shorts.len() } else { 0 };
        let chapters = if settings.include_longer_videos { plan.longer_segments.len() } else { 0 };
        let plural = |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
        ui.label(muted(format!("{} · {}", plural(shorts, "Short", "Shorts"), plural(chapters, "chapter", "chapters"))).font(font::ui(12.0)));
        if settings.include_shorts && (plan.shorts.len() as u32) < settings.min_shorts {
            ui.add(egui::Label::new(RichText::new(format!(
                "Only {} Shorts scored {}+, under your minimum of {}.{}",
                plan.shorts.len(), settings.min_score, settings.min_shorts,
                if plan.archived.is_empty() { "" } else { " The archive below has near-misses you can restore." }
            )).font(font::ui(12.0)).color(color::SUN)).wrap());
        }
        let mut row = |ui: &mut Ui, kind: ClipKind, i: usize, clip: &Clip| {
            let active = selected == Some(Selected { kind, index: i });
            let mut detail = format!("{} – {} · {}", clock(clip.start_seconds), clock(clip.end_seconds), clock(clip.length()));
            if let Some(total) = clip.total {
                detail.push_str(&format!(" · score {total}/8"));
            }
            if !clip.lines().is_empty() {
                detail.push_str(" · captions");
            }
            let mark = if kind == ClipKind::Short { RowMark::Icon(Icon::Video) } else { RowMark::Icon(Icon::Text) };
            let response = (Row { title: &clip.title, detail: &detail, mark, highlighted: active }).show(ui);
            let mut hover = clip.reason.clone().unwrap_or_default();
            if let Some(flags) = clip.review_flags.as_ref().filter(|f| !f.is_empty()) {
                hover.push_str(&format!("\nTo check: {}", flags.join(", ")));
            }
            let response = if hover.trim().is_empty() { response } else { response.on_hover_text(hover.trim()) };
            if response.clicked() {
                select = Some((kind, i));
            }
            if response.double_clicked() {
                play = Some((clip.start_seconds, clip.end_seconds));
            }
            response.context_menu(|ui| clip_menu(ui, kind, i, &mut action));
        };
        if settings.include_shorts {
            for (i, clip) in plan.shorts.iter().enumerate() {
                row(ui, ClipKind::Short, i, clip);
            }
        }
        if settings.include_longer_videos {
            for (i, clip) in plan.longer_segments.iter().enumerate() {
                row(ui, ClipKind::Chapter, i, clip);
            }
        }
        ui.add_space(6.0);
        let missing: Vec<usize> = plan.shorts.iter().enumerate().filter(|(_, s)| s.lines().is_empty()).map(|(i, _)| i).collect();
        let busy = app.busy.is_some();
        ui.vertical(|ui| {
            if settings.include_shorts && !plan.shorts.is_empty() {
                ui.add_enabled_ui(!busy && !missing.is_empty(), |ui| {
                    let label = match missing.len() {
                        0 => "All Shorts have captions".to_owned(),
                        1 => "Make captions for 1 Short".to_owned(),
                        n => format!("Make captions for {n} Shorts"),
                    };
                    if quiet_button(ui, &label, Some(Icon::Captions)).clicked() {
                        action = Some((ClipKind::Short, usize::MAX, "captions-all"));
                    }
                });
            }
            ui.add_enabled_ui(!busy && shorts + chapters > 0, |ui| {
                if primary_button(ui, &format!("Render {} clips", shorts + chapters), Mood::Ember).clicked() {
                    action = Some((ClipKind::Short, usize::MAX, "render"));
                }
            });
        });
        if !plan.archived.is_empty() {
            ui.add_space(6.0);
            egui::CollapsingHeader::new(RichText::new(format!("Archive ({})", plan.archived.len())).font(font::ui(13.0)).color(color::INK_SOFT)).id_salt("vc-archive").show(ui, |ui| {
                ui.label(muted("The AI proposed these, but they broke a rule. Ask about one, or restore it.").font(font::ui(12.0)));
                for (i, clip) in plan.archived.iter().enumerate() {
                    egui::Frame::new().fill(color::VEIL).corner_radius(8).inner_margin(8.0).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        if ui.add(egui::Label::new(RichText::new(&clip.title).color(color::INK)).sense(Sense::click())).on_hover_text("Go there").clicked() {
                            play = Some((clip.start_seconds, f64::NAN));
                        }
                        ui.label(muted(format!("{} – {}{}", clock(clip.start_seconds), clock(clip.end_seconds), clip.total.map(|t| format!(" · score {t}/8")).unwrap_or_default())).font(font::ui(12.0)));
                        if let Some(reason) = &clip.archive_reason {
                            ui.add(egui::Label::new(RichText::new(reason).font(font::ui(12.0)).color(color::SUN)).wrap());
                        }
                        ui.horizontal(|ui| {
                            if ui.small_button("Ask about this").clicked() {
                                action = Some((ClipKind::Short, i, "ask-archived"));
                            }
                            if ui.small_button("Restore").clicked() {
                                action = Some((ClipKind::Short, i, "restore"));
                            }
                        });
                    });
                }
            });
        }
        if let Some(rendered) = &rendered {
            ui.add_space(8.0);
            section_label(ui, "Rendered");
            for (video, kind) in rendered.shorts.iter().map(|v| (v, "Short")).chain(rendered.longer_videos.iter().map(|v| (v, "Chapter"))) {
                rendered_row(app, ui, video, kind, host);
            }
        }
        if selected.is_some() {
            ui.add_space(8.0);
            inspector(app, ui, &mut action);
        }
    });
    if let Some((kind, index)) = select {
        app.select(kind, index);
    }
    if let Some((start, end)) = play {
        if end.is_nan() {
            app.seek(start);
        } else if let Some(player) = app.open.as_mut().and_then(|o| o.player.as_mut()) {
            player.play_span(start, end);
        }
    }
    if let Some((kind, index, name)) = action {
        match name {
            "captions-all" => {
                let missing = plan.shorts.iter().enumerate().filter(|(_, s)| s.lines().is_empty()).map(|(i, _)| i).collect();
                app.make_captions(missing);
            }
            "render" => app.render(),
            "ask-archived" => {
                if let Some(clip) = plan.archived.get(index) {
                    app.prompt = format!(
                        "About the archived clip \"{}\" ({}–{}): it was set aside because {} Is it worth keeping? If so, suggest corrected start and end times from the transcript.",
                        clip.title, clock(clip.start_seconds), clock(clip.end_seconds), clip.archive_reason.clone().unwrap_or_default()
                    );
                    app.seek(clip.start_seconds);
                    app.tab = Tab::Producer;
                }
            }
            "restore" => {
                app.remember();
                if let Some(plan) = app.open.as_mut().and_then(|o| o.plan.as_mut()) {
                    if index < plan.archived.len() {
                        let mut clip = plan.archived.remove(index);
                        clip.archive_reason = None;
                        plan.shorts.push(clip);
                    }
                }
                app.mark_dirty();
            }
            "set-in" | "set-out" => {
                let t = app.playhead();
                app.remember();
                if let Some(clip) = app.selected_clip_mut() {
                    if name == "set-in" {
                        clip.start_seconds = t.min(clip.end_seconds - 0.5);
                    } else {
                        clip.end_seconds = t.max(clip.start_seconds + 0.5);
                    }
                }
                app.mark_dirty();
            }
            other => clip_action(app, kind, index, other, host),
        }
    }
}

fn rendered_row(app: &mut VideoChat, ui: &mut Ui, video: &model::Rendered, kind: &str, host: &mut dyn Host) {
    let path = app.folder.as_deref().and_then(|f| engine::rendered_path(f, &video.download_url));
    egui::Frame::new().fill(color::VEIL).corner_radius(8).inner_margin(8.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.add(egui::Label::new(RichText::new(&video.title).color(color::INK)).truncate());
        let mut detail = kind.to_owned();
        if let (Some(before), Some(after)) = (video.original_seconds, video.rendered_seconds) {
            if after < before - 0.5 {
                detail.push_str(&format!(" · pauses cut: {} → {}", clock(before), clock(after)));
            }
        }
        ui.label(muted(detail).font(font::ui(12.0)));
        let Some(path) = path else {
            ui.label(muted("The file isn't in Video Chat's folder anymore.").font(font::ui(12.0)));
            return;
        };
        ui.horizontal(|ui| {
            if ui.small_button("Play").clicked() {
                let _ = std::process::Command::new("explorer").arg(&path).spawn();
            }
            if ui.small_button("Show in folder").clicked() {
                let _ = std::process::Command::new("explorer").arg(format!("/select,{}", path.display())).spawn();
            }
            if let Some(space) = &app.place.space {
                if ui.small_button("Add to Files").on_hover_text("Copy it into this space's Files, to send on to StreamCuts or Folio").clicked() {
                    let files = canopy_ui::canopy_core::spaces::SpacePaths::new(&space.dir).files();
                    match canopy_ui::canopy_core::files::copy_into(&files, &path) {
                        Ok(name) => host.exported(files.join(name)),
                        Err(error) => host.toast(format!("Couldn't copy it: {error}")),
                    }
                }
            }
        });
    });
}

/// The selected clip: its title, edges, description and captions.
fn inspector(app: &mut VideoChat, ui: &mut Ui, action: &mut Option<(ClipKind, usize, &'static str)>) {
    let Some(open) = &app.open else { return };
    let Some(selected) = open.selected else { return };
    let Some(mut clip) = app.selected_clip().cloned() else { return };
    let before = clip.clone();
    let duration = open.duration.max(clip.end_seconds);
    let mut drafts = open.caption_drafts.clone();
    let mut caption_edit: Option<Box<dyn FnOnce(&mut Vec<Line>)>> = None;
    egui::Frame::new().fill(color::VEIL).corner_radius(10).inner_margin(10.0).stroke(Stroke::new(1.0, color::HAIRLINE_STRONG)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            section_label(ui, if selected.kind == ClipKind::Short { "Selected Short" } else { "Selected chapter" });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.small_button("Remove").on_hover_text("Delete key; Ctrl+Z brings it back").clicked() {
                    *action = Some((selected.kind, selected.index, "remove"));
                }
            });
        });
        ui.add(egui::TextEdit::singleline(&mut clip.title).desired_width(f32::INFINITY).font(font::bold(14.0)));
        egui::Grid::new("vc-edges").num_columns(4).spacing(vec2(6.0, 4.0)).show(ui, |ui| {
            for (label, value, set, hint) in [("Start", &mut clip.start_seconds, "set-in", "I"), ("End", &mut clip.end_seconds, "set-out", "O")] {
                ui.label(label);
                ui.add(egui::DragValue::new(value).range(0.0..=duration).speed(0.1).custom_formatter(|v, _| clock_tenths(v)).custom_parser(model::parse_clock));
                ui.horizontal(|ui| {
                    if ui.small_button("−1 s").clicked() {
                        *value = (*value - 1.0).max(0.0);
                    }
                    if ui.small_button("+1 s").clicked() {
                        *value = (*value + 1.0).min(duration);
                    }
                });
                if ui.small_button("Playhead").on_hover_text(format!("Set it to where the video is ({hint})")).clicked() {
                    *action = Some((selected.kind, selected.index, set));
                }
                ui.end_row();
            }
        });
        ui.label(muted(format!("Length {}", clock_tenths(clip.length()))).font(font::ui(12.0)));
        if selected.kind == ClipKind::Short {
            let mut description = clip.description.clone().unwrap_or_default();
            if ui.add(egui::TextEdit::multiline(&mut description).desired_rows(2).desired_width(f32::INFINITY).hint_text("Description for posting")).changed() {
                clip.description = Some(description);
            }
            ui.horizontal(|ui| {
                if primary_button(ui, "Edit in StreamCuts", Mood::Ember).on_hover_text("As a cut of the whole video: drag its edges in StreamCuts to grow it (E)").clicked() {
                    *action = Some((ClipKind::Short, selected.index, "streamcuts"));
                }
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                section_label(ui, "Captions");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add_enabled(app.busy.is_none(), egui::Button::new(if clip.lines().is_empty() { "Make captions" } else { "Remake" }).small()).clicked() {
                        *action = Some((ClipKind::Short, selected.index, "captions"));
                    }
                });
            });
            let lines = clip.lines().to_vec();
            if lines.is_empty() {
                ui.add(egui::Label::new(muted("None yet. Make them from the clip's audio, then fix any words here. They're burned in when it renders.").font(font::ui(12.0))).wrap());
            }
            for (i, line) in lines.iter().enumerate() {
                let Some(first) = line.words.first() else { continue };
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(RichText::new(clock_tenths(first.start)).font(font::ui(11.0)).color(color::SELECT_INK)).frame(false)).clicked() {
                        *action = Some((ClipKind::Short, i, "seek-line"));
                    }
                    let draft = drafts.entry(i).or_insert_with(|| line.text());
                    let field = ui.add(egui::TextEdit::singleline(draft).desired_width(ui.available_width() - 70.0));
                    if field.lost_focus() && draft.trim() != line.text() {
                        let text = draft.trim().to_owned();
                        caption_edit = Some(Box::new(move |all: &mut Vec<Line>| {
                            all[i] = all[i].reworded(&text);
                            all.retain(|l| !l.words.is_empty());
                        }));
                    }
                    ui.menu_button("…", |ui| {
                        if line.words.len() > 1 && ui.button("Split in two").clicked() {
                            caption_edit = Some(Box::new(move |all: &mut Vec<Line>| {
                                let middle = all[i].words.len().div_ceil(2);
                                let rest = all[i].words.split_off(middle);
                                all.insert(i + 1, Line { words: rest });
                            }));
                            ui.close();
                        }
                        if i + 1 < lines.len() && ui.button("Merge with the next").clicked() {
                            caption_edit = Some(Box::new(move |all: &mut Vec<Line>| {
                                let next = all.remove(i + 1);
                                all[i].words.extend(next.words);
                            }));
                            ui.close();
                        }
                        if ui.button("Delete").clicked() {
                            caption_edit = Some(Box::new(move |all: &mut Vec<Line>| {
                                all.remove(i);
                            }));
                            ui.close();
                        }
                    });
                });
            }
        }
    });
    if let Some(open) = &mut app.open {
        open.caption_drafts = drafts;
    }
    if let Some(edit) = caption_edit {
        let mut lines = clip.lines().to_vec();
        edit(&mut lines);
        clip.captions = Some(lines);
        if let Some(open) = &mut app.open {
            open.caption_drafts.clear();
        }
    }
    if clip.start_seconds > clip.end_seconds - 0.5 {
        clip.end_seconds = (clip.start_seconds + 0.5).min(duration);
    }
    if clip != before {
        // Typing in the title doesn't record a step per letter: only edges and captions do.
        let edge = clip.start_seconds != before.start_seconds || clip.end_seconds != before.end_seconds || clip.captions != before.captions;
        if edge {
            app.remember();
        }
        let moved_start = clip.start_seconds != before.start_seconds;
        if let Some(target) = app.selected_clip_mut() {
            *target = clip.clone();
        }
        app.mark_dirty();
        if moved_start {
            app.seek(clip.start_seconds);
        }
    }
    if let Some((_, line, "seek-line")) = *action {
        if let Some(word) = app.selected_clip().and_then(|c| c.lines().get(line)).and_then(|l| l.words.first()) {
            let t = app.selected_clip().map_or(0.0, |c| c.start_seconds) + word.start;
            app.seek(t);
        }
        *action = None;
    }
}

fn settings(app: &mut VideoChat, ui: &mut Ui) {
    let Some(open) = &mut app.open else {
        ui.label(muted("Settings belong to a project. Open one first."));
        return;
    };
    let before = open.settings.clone();
    let s = &mut open.settings;
    let mut refresh_codex = false;
    egui::ScrollArea::vertical().id_salt("vc-settings").auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        section_label(ui, "What to make");
        ui.checkbox(&mut s.include_shorts, "Vertical Shorts");
        ui.checkbox(&mut s.include_longer_videos, "Topic chapters");
        egui::Grid::new("vc-plan-settings").num_columns(2).spacing(vec2(10.0, 6.0)).show(ui, |ui| {
            let whole = |ui: &mut Ui, label: &str, value: &mut u32, range: std::ops::RangeInclusive<u32>| {
                ui.label(label);
                ui.add(egui::DragValue::new(value).range(range));
                ui.end_row();
            };
            whole(ui, "Fewest Shorts", &mut s.min_shorts, 0..=999);
            whole(ui, "Most Shorts", &mut s.max_shorts, 1..=999);
            whole(ui, "Lowest score (0-8)", &mut s.min_score, 0..=8);
            whole(ui, "Fewest chapters", &mut s.min_chapters, 0..=20);
            whole(ui, "Most chapters", &mut s.max_chapters, 1..=20);
            let seconds = |ui: &mut Ui, label: &str, value: &mut f64, range: std::ops::RangeInclusive<f64>| {
                ui.label(label);
                ui.add(egui::DragValue::new(value).range(range).suffix(" s").speed(0.5));
                ui.end_row();
            };
            seconds(ui, "Shortest Short", &mut s.min_short_duration_seconds, 5.0..=180.0);
            seconds(ui, "Longest Short", &mut s.max_short_duration_seconds, 5.0..=180.0);
            seconds(ui, "Shortest chapter", &mut s.min_chapter_duration_seconds, 5.0..=3600.0);
        });
        ui.label(muted("With no fewest chapters, the AI marks chapters only where the video has them.").font(font::ui(12.0)));
        ui.add_space(6.0);
        section_label(ui, "The AI");
        egui::Grid::new("vc-ai-settings").num_columns(2).spacing(vec2(10.0, 6.0)).show(ui, |ui| {
            ui.label("Provider");
            egui::ComboBox::from_id_salt("vc-provider").selected_text(if s.ai_provider == "codex" { "Codex CLI (this PC)" } else { "Gemini API" }).show_ui(ui, |ui| {
                ui.selectable_value(&mut s.ai_provider, "gemini".to_owned(), "Gemini API");
                ui.selectable_value(&mut s.ai_provider, "codex".to_owned(), "Codex CLI (this PC)");
            });
            ui.end_row();
            if s.ai_provider == "codex" {
                let models: Vec<(String, String, Vec<String>, Option<String>)> = app.codex.as_ref().and_then(|c| c.get("models")).and_then(Value::as_array).map(|models| {
                    models.iter().filter_map(|m| Some((
                        m.get("id")?.as_str()?.to_owned(),
                        m.get("displayName").and_then(Value::as_str).unwrap_or_default().to_owned(),
                        m.get("efforts").and_then(Value::as_array).map(|e| e.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect()).unwrap_or_default(),
                        m.get("defaultEffort").and_then(Value::as_str).map(str::to_owned),
                    ))).collect()
                }).unwrap_or_default();
                ui.label("Model");
                let current = models.iter().find(|m| m.0 == s.codex_model).map_or(s.codex_model.clone(), |m| m.1.clone());
                egui::ComboBox::from_id_salt("vc-codex-model").selected_text(current).show_ui(ui, |ui| {
                    for (id, name, efforts, default) in &models {
                        if ui.selectable_label(&s.codex_model == id, name).clicked() {
                            s.codex_model = id.clone();
                            if !efforts.contains(&s.codex_effort) {
                                s.codex_effort = default.clone().or_else(|| efforts.first().cloned()).unwrap_or_else(|| "low".into());
                            }
                        }
                    }
                });
                ui.end_row();
                ui.label("Effort");
                let efforts = models.iter().find(|m| m.0 == s.codex_model).map(|m| m.2.clone()).filter(|e| !e.is_empty()).unwrap_or_else(|| vec!["low".into(), "medium".into(), "high".into()]);
                egui::ComboBox::from_id_salt("vc-codex-effort").selected_text(&s.codex_effort).show_ui(ui, |ui| {
                    for effort in efforts {
                        ui.selectable_value(&mut s.codex_effort, effort.clone(), effort);
                    }
                });
                ui.end_row();
            } else {
                ui.label("Model");
                let default = app.gemini_models.as_ref().map(|m| m.0.clone()).unwrap_or_default();
                let shown = if s.gemini_model.is_empty() { format!("Engine's default {default}") } else { s.gemini_model.clone() };
                egui::ComboBox::from_id_salt("vc-gemini").selected_text(shown).show_ui(ui, |ui| {
                    ui.selectable_value(&mut s.gemini_model, String::new(), format!("Engine's default {default}"));
                    for (id, name) in app.gemini_models.as_ref().map(|m| m.1.clone()).unwrap_or_default() {
                        ui.selectable_value(&mut s.gemini_model, id.clone(), format!("{name} ({id})"));
                    }
                });
                ui.end_row();
            }
        });
        if s.ai_provider == "codex" {
            codex_quota(ui, app.codex.as_ref(), &mut refresh_codex);
        }
        ui.add_space(6.0);
        section_label(ui, "Shorts");
        egui::Grid::new("vc-short-settings").num_columns(2).spacing(vec2(10.0, 6.0)).show(ui, |ui| {
            ui.label("Layout");
            egui::ComboBox::from_id_salt("vc-layout").selected_text(if s.short_layout.mode == "stacked" { "Stacked regions" } else { "Middle crop" }).show_ui(ui, |ui| {
                ui.selectable_value(&mut s.short_layout.mode, "crop".to_owned(), "Middle crop");
                ui.selectable_value(&mut s.short_layout.mode, "stacked".to_owned(), "Stacked regions");
            });
            ui.end_row();
        });
        if s.short_layout.mode == "stacked" {
            if s.short_layout.panels.is_empty() {
                ui.add(egui::Label::new(muted("No regions yet: Edit the Short layout (under the video) and drag a box around your facecam or chat. Until then Shorts use the middle crop.").font(font::ui(12.0))).wrap());
            }
            let mut remove = None;
            for (i, panel) in s.short_layout.panels.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("{} {}", if panel.position == Side::Top { "On top" } else { "Below" }, i + 1));
                    if ui.small_button("Remove").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                s.short_layout.panels.remove(i);
            }
            ui.horizontal(|ui| {
                ui.label(format!("Gameplay: {}", if s.short_layout.gameplay.is_full() { "the whole picture" } else { "a drawn area" }));
                if !s.short_layout.gameplay.is_full() && ui.small_button("Reset").clicked() {
                    s.short_layout.gameplay = model::FULL_FRAME;
                }
            });
        }
        let style = &mut s.caption_style;
        egui::Grid::new("vc-caption-settings").num_columns(2).spacing(vec2(10.0, 6.0)).show(ui, |ui| {
            ui.label("Caption font");
            egui::ComboBox::from_id_salt("vc-font").selected_text(&style.font).show_ui(ui, |ui| {
                for font in model::CAPTION_FONTS {
                    ui.selectable_value(&mut style.font, font.to_owned(), font);
                }
            });
            ui.end_row();
            ui.label("Size");
            ui.add(egui::DragValue::new(&mut style.font_size).range(36.0..=160.0).speed(1.0));
            ui.end_row();
            ui.label("Words per line");
            ui.add(egui::DragValue::new(&mut style.words_per_line).range(1..=6));
            ui.end_row();
            for (label, value) in [("Text", &mut style.text_color), ("Spoken word", &mut style.highlight_color), ("Outline", &mut style.outline_color)] {
                ui.label(label);
                let mut picked = model::hex_color(value);
                if egui::color_picker::color_edit_button_srgba(ui, &mut picked, egui::color_picker::Alpha::Opaque).changed() {
                    *value = model::to_hex(picked);
                }
                ui.end_row();
            }
            ui.label("Height");
            let mut percent = (style.position_y * 100.0).round();
            if ui.add(egui::Slider::new(&mut percent, 5.0..=95.0).suffix("% from the top")).changed() {
                style.position_y = percent / 100.0;
            }
            ui.end_row();
        });
        ui.checkbox(&mut style.uppercase, "ALL CAPS");
        ui.add_space(6.0);
        section_label(ui, "Rendering");
        egui::Grid::new("vc-render-settings").num_columns(2).spacing(vec2(10.0, 6.0)).show(ui, |ui| {
            ui.label("Pauses");
            egui::ComboBox::from_id_salt("vc-pauses").selected_text(match s.pause_removal.as_str() {
                "speech" => "Cut pauses in speech",
                "silence" => "Cut silence only",
                _ => "Keep them",
            }).show_ui(ui, |ui| {
                ui.selectable_value(&mut s.pause_removal, "off".to_owned(), "Keep them");
                ui.selectable_value(&mut s.pause_removal, "speech".to_owned(), "Cut pauses in speech");
                ui.selectable_value(&mut s.pause_removal, "silence".to_owned(), "Cut silence only");
            });
            ui.end_row();
            if s.pause_removal != "off" {
                ui.label("Longest pause kept");
                ui.add(egui::DragValue::new(&mut s.min_pause_seconds).range(0.3..=30.0).speed(0.1).suffix(" s"));
                ui.end_row();
            }
        });
        if s.pause_removal != "off" {
            ui.add(egui::Label::new(muted(if s.pause_removal == "speech" {
                "Cuts gaps between your words. Gameplay with no talking counts as a pause too."
            } else {
                "Cuts only where all the sound goes quiet, so gameplay with sound stays in."
            }).font(font::ui(12.0))).wrap());
        }
        ui.add_space(6.0);
        section_label(ui, "Transcription");
        egui::Grid::new("vc-whisper-settings").num_columns(2).spacing(vec2(10.0, 6.0)).show(ui, |ui| {
            ui.label("Whisper model");
            egui::ComboBox::from_id_salt("vc-whisper").selected_text(&s.transcription_model).show_ui(ui, |ui| {
                for model in ["tiny", "small", "medium", "large-v3-turbo"] {
                    ui.selectable_value(&mut s.transcription_model, model.to_owned(), model);
                }
            });
            ui.end_row();
            ui.label("Device");
            egui::ComboBox::from_id_salt("vc-device").selected_text(&s.transcription_device).show_ui(ui, |ui| {
                for device in ["auto", "cuda", "cpu"] {
                    ui.selectable_value(&mut s.transcription_device, device.to_owned(), device);
                }
            });
            ui.end_row();
        });
        ui.label(muted("A transcript already started keeps the model it began with.").font(font::ui(12.0)));
    });
    if open.settings != before {
        open.dirty_at = Some(Instant::now());
    }
    if refresh_codex {
        app.spawn(|| Reply::Codex(engine::request("GET", "/api/boss/codex", None, Some(Duration::from_secs(20))).unwrap_or_default()));
    }
}

fn codex_quota(ui: &mut Ui, codex: Option<&Value>, refresh: &mut bool) {
    egui::Frame::new().fill(color::VEIL).corner_radius(8).inner_margin(8.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new("Codex usage").font(font::ui(13.0)));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                *refresh = ui.small_button("Refresh").clicked();
            });
        });
        let quota = codex.and_then(|c| c.get("quota")).filter(|q| !q.is_null());
        let Some(quota) = quota else {
            ui.label(muted("No usage yet: it shows after the first Codex run.").font(font::ui(12.0)));
            return;
        };
        for (label, key) in [("5-hour", "fiveHour"), ("Weekly", "weekly")] {
            let Some(window) = quota.get(key).filter(|w| !w.is_null()) else { continue };
            let used = window.get("usedPercent").and_then(Value::as_f64).unwrap_or(0.0);
            ui.label(muted(format!("{label}: {}% used", used.round())).font(font::ui(12.0)));
            ui.add(egui::ProgressBar::new((used / 100.0) as f32).desired_height(4.0).fill(if used >= 90.0 { color::DANGER } else { color::LEAF }));
        }
    });
}

// ---- Dialogs and keys ----

fn dialogs(app: &mut VideoChat, ui: &mut Ui) {
    if let Some((job, mut name)) = app.rename.clone() {
        let mut done = None;
        let modal = egui::Modal::new(ui.scope_id().with("vc-rename")).show(ui.ctx(), |ui| {
            ui.set_width(380.0);
            ui.label(RichText::new("Rename the project").font(font::display(20.0)));
            ui.label(muted("A download it came from is renamed too."));
            let field = ui.add(egui::TextEdit::singleline(&mut name).desired_width(f32::INFINITY).margin(vec2(10.0, 8.0)));
            field.request_focus();
            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            ui.add_space(8.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if primary_button(ui, "Rename", Mood::Ember).clicked() || enter {
                    done = Some(true);
                }
                if quiet_button(ui, "Back", None).clicked() {
                    done = Some(false);
                }
            });
        });
        app.rename = Some((job.clone(), name.clone()));
        if modal.should_close() || done == Some(false) {
            app.rename = None;
        } else if done == Some(true) && !name.trim().is_empty() {
            app.rename = None;
            app.change("PUT", format!("/api/boss/jobs/{}/name", engine::encode(&job)), Some(json!({"name": name.trim()})), "Renamed");
            if let Some(open) = app.open.as_mut().filter(|o| o.job == job) {
                open.name = name.trim().to_owned();
            }
        }
    }
    if let Some(confirm) = &app.confirm {
        let (title, text) = match confirm {
            Confirm::DeleteProject(p) => (
                format!("Delete {}?", model::without_extension(&p.name)),
                "Its transcript, chat, clip plan and rendered clips go. If it came from a download, the downloaded video stays. StreamCuts edits made from it stay too.".to_owned(),
            ),
            Confirm::DeleteDownload(d) => (
                format!("Delete the downloaded video {}?", d.name()),
                if d.project_job_id.is_some() { "The project made from it keeps its own copy and still works.".to_owned() } else { "It's deleted from the downloads folder.".to_owned() },
            ),
        };
        let mut answer = None;
        let modal = egui::Modal::new(ui.scope_id().with("vc-confirm")).show(ui.ctx(), |ui| {
            ui.set_width(400.0);
            ui.label(RichText::new(title).font(font::display(20.0)));
            ui.add(egui::Label::new(muted(text)).wrap());
            ui.add_space(8.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add(egui::Button::new(RichText::new("Delete").color(color::DANGER)).min_size(vec2(80.0, 30.0))).clicked() {
                    answer = Some(true);
                }
                if quiet_button(ui, "Keep it", None).clicked() {
                    answer = Some(false);
                }
            });
        });
        if modal.should_close() || answer == Some(false) {
            app.confirm = None;
        } else if answer == Some(true) {
            match app.confirm.take() {
                Some(Confirm::DeleteProject(p)) => {
                    if app.open.as_ref().is_some_and(|o| o.job == p.job_id) {
                        app.open = None;
                    }
                    app.change("DELETE", format!("/api/boss/jobs/{}", engine::encode(&p.job_id)), None, "Project deleted");
                }
                Some(Confirm::DeleteDownload(d)) => {
                    app.change("DELETE", format!("/api/twitch/downloads/{}", engine::encode(&d.key)), None, "Download deleted");
                }
                None => {}
            }
        }
    }
}

/// Space plays, arrows move, I and O set the selected clip's edges, E opens it in StreamCuts, Delete removes it,
/// Ctrl+Z and Ctrl+Y undo and redo plan edits. Not while typing.
fn shortcuts(app: &mut VideoChat, ui: &mut Ui, host: &mut dyn Host) {
    if ui.ctx().memory(|m| m.focused().is_some()) || app.open.is_none() || app.rename.is_some() || app.confirm.is_some() {
        return;
    }
    let pressed = |ui: &mut Ui, modifiers: Modifiers, key: Key| ui.input_mut(|i| i.consume_key(modifiers, key));
    if pressed(ui, Modifiers::NONE, Key::Space) {
        if let Some(player) = app.open.as_mut().and_then(|o| o.player.as_mut()) {
            player.toggle();
        }
    }
    for (key, step) in [(Key::ArrowLeft, -5.0), (Key::ArrowRight, 5.0)] {
        if pressed(ui, Modifiers::NONE, key) {
            app.seek(app.playhead() + step);
        }
        if pressed(ui, Modifiers::SHIFT, key) {
            app.seek(app.playhead() + step / 5.0);
        }
    }
    if pressed(ui, Modifiers::COMMAND, Key::Z) {
        app.undo(true);
    }
    if pressed(ui, Modifiers::COMMAND, Key::Y) || pressed(ui, Modifiers::COMMAND | Modifiers::SHIFT, Key::Z) {
        app.undo(false);
    }
    let Some(selected) = app.open.as_ref().and_then(|o| o.selected) else { return };
    for (key, start) in [(Key::I, true), (Key::O, false)] {
        if pressed(ui, Modifiers::NONE, key) {
            let t = app.playhead();
            app.remember();
            if let Some(clip) = app.selected_clip_mut() {
                if start {
                    clip.start_seconds = t.min(clip.end_seconds - 0.5).max(0.0);
                } else {
                    clip.end_seconds = t.max(clip.start_seconds + 0.5);
                }
            }
            app.mark_dirty();
        }
    }
    if pressed(ui, Modifiers::NONE, Key::E) && selected.kind == ClipKind::Short {
        app.edit_in_streamcuts(selected.index, false, host);
    }
    if pressed(ui, Modifiers::NONE, Key::Delete) {
        remove_clip(app, selected.kind, selected.index);
    }
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}
