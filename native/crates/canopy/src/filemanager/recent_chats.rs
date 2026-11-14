//! The local Codex chat index contains names and IDs. Reading it doesn't open a chat.
use super::{codex_launch::valid_thread_id, selected_thread_id};
use egui::{Button, ComboBox, TextEdit, Ui};
use serde::Deserialize;
use std::collections::HashSet;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

#[derive(Deserialize)]
struct Chat {
    id: String,
    thread_name: String,
    #[serde(default)]
    updated_at: String,
}

fn read_chats(input: impl BufRead) -> io::Result<Vec<Chat>> {
    let mut rows = Vec::new();
    for line in input.lines() {
        let line = line?;
        if let Ok(chat) = serde_json::from_str::<Chat>(&line) {
            if valid_thread_id(&chat.id) && !chat.thread_name.trim().is_empty() {
                rows.push(chat);
            }
        }
    }
    // Later records supersede older names for the same chat, including on a tied timestamp.
    rows.reverse();
    rows.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let mut seen = HashSet::new();
    rows.retain(|chat| seen.insert(chat.id.to_ascii_lowercase()));
    Ok(rows)
}

fn load() -> Option<Vec<Chat>> {
    let home = std::env::var_os("CODEX_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|p| p.join(".codex")))?;
    let file = File::open(home.join("session_index.jsonl")).ok()?;
    read_chats(BufReader::new(file)).ok()
}

#[derive(Default)]
enum List {
    #[default]
    Unopened,
    Loading(Receiver<Option<Vec<Chat>>>),
    Ready(Vec<Chat>),
    Unavailable,
}

#[derive(Default)]
pub(super) struct Picker {
    list: List,
    manual: bool,
    search: String,
}

impl Picker {
    pub(super) fn show(&mut self, ui: &mut Ui, value: &mut String) {
        if matches!(self.list, List::Unopened) {
            let ctx = ui.ctx().clone();
            let (tx, rx) = channel();
            self.list = match std::thread::Builder::new()
                .name("canopy-chat-names".into())
                .spawn(move || {
                    let _ = tx.send(load());
                    ctx.request_repaint();
                }) {
                Ok(_) => List::Loading(rx),
                Err(_) => List::Unavailable,
            };
        }
        if let List::Loading(rx) = &self.list {
            match rx.try_recv() {
                Ok(Some(chats)) => self.list = List::Ready(chats),
                Ok(None) | Err(TryRecvError::Disconnected) => self.list = List::Unavailable,
                Err(TryRecvError::Empty) => {}
            }
        }
        let chats = match &self.list {
            List::Ready(chats) => chats.as_slice(),
            _ => &[],
        };
        let current = selected_thread_id(value);
        let named = current.and_then(|id| chats.iter().find(|c| c.id.eq_ignore_ascii_case(id)));
        let manual = self.manual || (chats.is_empty() && !matches!(self.list, List::Loading(_)));
        if manual {
            ui.label("Chat link or ID");
            ui.add(
                TextEdit::singleline(value)
                    .desired_width(f32::INFINITY)
                    .hint_text("Paste a link or ID from Codex"),
            );
            if !value.trim().is_empty() && selected_thread_id(value).is_none() {
                ui.label("Paste the full chat link or ID.");
            }
            if !chats.is_empty() && ui.link("Choose from recent chats").clicked() {
                self.manual = false;
            }
            return;
        }
        let label = match named {
            Some(chat) => chat.thread_name.as_str(),
            None if current.is_some() => value.as_str(),
            None if matches!(self.list, List::Loading(_)) => "Loading chats…",
            None => "Choose a chat…",
        };
        let mut picked = None;
        ComboBox::from_id_salt("recent-codex-chats")
            .selected_text(label)
            .width(ui.available_width())
            .height(220.0)
            .truncate()
            .show_ui(ui, |ui| {
                ui.add(
                    TextEdit::singleline(&mut self.search)
                        .hint_text("Find a chat…")
                        .desired_width(f32::INFINITY),
                );
                let query = self.search.trim().to_lowercase();
                for chat in chats
                    .iter()
                    .filter(|chat| chat.thread_name.to_lowercase().contains(&query))
                {
                    let selected = current.is_some_and(|id| id.eq_ignore_ascii_case(&chat.id));
                    if ui
                        .add(Button::selectable(selected, &chat.thread_name).truncate())
                        .on_hover_text(format!("{}\n{}", chat.thread_name, chat.id))
                        .clicked()
                    {
                        picked = Some(chat.id.clone());
                    }
                }
            });
        if let Some(id) = picked {
            *value = id;
        }
        if ui.link("Paste a chat link instead").clicked() {
            self.manual = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_chats_keep_newest_names_and_skip_partial_writes() {
        let a = "00000000-0000-0000-0000-000000000001";
        let b = "00000000-0000-0000-0000-000000000002";
        let row = |id: &str, name: &str, day: u32| {
            serde_json::json!({
                "id": id, "thread_name": name, "updated_at": format!("2026-11-{day:02}T10:00:00Z")
            })
            .to_string()
        };
        let index = [
            row(a, "Old name", 12),
            row(a, "ليلة", 14),
            row(b, "Second", 13),
            row(a, "Stale update", 11),
            row("broken", "Skip", 15),
            r#"{"id":"#.into(),
        ]
        .join("\n");
        let chats = read_chats(index.as_bytes()).unwrap();
        assert_eq!(
            chats
                .iter()
                .map(|c| c.thread_name.as_str())
                .collect::<Vec<_>>(),
            ["ليلة", "Second"]
        );
    }
}
