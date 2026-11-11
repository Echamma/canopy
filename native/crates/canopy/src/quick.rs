//! Quick open (Ctrl+K): type a few letters to reach an app, a project, a file, another space or a command.

use canopy_ui::{Accent, Icon, color, font, icon, leaf};
use egui::{Align2, Key, Modifiers, Rect, Sense, pos2, vec2};

#[derive(Default)]
pub struct State {
    pub query: String,
    pub selected: usize,
}

pub enum Mark {
    Leaf(Accent),
    Icon(Icon),
}

pub struct Entry<A> {
    pub label: String,
    pub detail: String,
    /// The keys that do the same ("Ctrl+B"), shown at the row's end.
    pub keys: &'static str,
    pub mark: Mark,
    pub act: A,
}

pub enum Outcome<A> {
    Stay,
    Close,
    Pick(A),
}

/// How well a row answers what was typed: lower is better, `None` leaves it out. Every typed word has to be
/// somewhere in the row; a label that starts with the words comes first, then one with a word that does.
fn score(label: &str, detail: &str, query: &str) -> Option<u8> {
    let (label, detail) = (label.to_lowercase(), detail.to_lowercase());
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Some(0);
    }
    if label.starts_with(&query) {
        return Some(0);
    }
    if label.split(|c: char| !c.is_alphanumeric()).any(|word| word.starts_with(&query)) {
        return Some(1);
    }
    if label.contains(&query) {
        return Some(2);
    }
    query.split_whitespace().all(|word| label.contains(word) || detail.contains(word)).then_some(3)
}

/// The rows to show for a query, best first; rows that match equally keep their order.
pub fn rank<A>(entries: &[Entry<A>], query: &str) -> Vec<usize> {
    let mut order: Vec<(u8, usize)> = entries.iter().enumerate().filter_map(|(index, entry)| score(&entry.label, &entry.detail, query).map(|s| (s, index))).collect();
    order.sort_by_key(|(score, _)| *score);
    order.into_iter().map(|(_, index)| index).collect()
}

pub fn show<A>(ctx: &egui::Context, state: &mut State, mut entries: Vec<Entry<A>>) -> Outcome<A> {
    // Taken before the text field sees them: the arrows move through the rows, Enter picks one.
    let (up, down, enter, escape) = ctx.input_mut(|i| {
        (i.consume_key(Modifiers::NONE, Key::ArrowUp), i.consume_key(Modifiers::NONE, Key::ArrowDown), i.consume_key(Modifiers::NONE, Key::Enter), i.consume_key(Modifiers::NONE, Key::Escape))
    });
    let order = rank(&entries, &state.query);
    if down && !order.is_empty() {
        state.selected = (state.selected + 1) % order.len();
    }
    if up && !order.is_empty() {
        state.selected = (state.selected + order.len() - 1) % order.len();
    }
    state.selected = state.selected.min(order.len().saturating_sub(1));
    let mut picked = enter.then(|| order.get(state.selected).copied()).flatten();
    let modal = egui::Modal::new(egui::Id::NULL.with("canopy-quick-open")).show(ctx, |ui| {
        ui.set_width(560.0);
        let before = state.query.clone();
        let field = ui.add(egui::TextEdit::singleline(&mut state.query).hint_text("Find an app, a project, a file or a command").desired_width(f32::INFINITY).margin(vec2(10.0, 8.0)));
        field.request_focus();
        if state.query != before {
            state.selected = 0;
        }
        ui.add_space(6.0);
        egui::ScrollArea::vertical().max_height(380.0).min_scrolled_height(380.0).auto_shrink(false).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            if order.is_empty() {
                ui.add_space(8.0);
                ui.label(canopy_ui::muted("Nothing here matches that."));
            }
            for (row, &index) in order.iter().enumerate() {
                let entry = &entries[index];
                let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
                let selected = row == state.selected;
                let painter = ui.painter();
                if selected {
                    painter.rect_filled(rect, 8, color::SELECT_FILL);
                } else if response.hovered() {
                    painter.rect_filled(rect, 8, color::VEIL_HOVER);
                }
                let mark = Rect::from_center_size(pos2(rect.left() + 18.0, rect.center().y), vec2(16.0, 16.0));
                match entry.mark {
                    Mark::Leaf(accent) => leaf(painter, mark, accent.color()),
                    Mark::Icon(which) => icon(painter, mark, which, color::INK_SOFT),
                }
                let keys = painter.text(pos2(rect.right() - 10.0, rect.center().y), Align2::RIGHT_CENTER, entry.keys, font::ui(12.0), color::INK_FAINT);
                let painter = painter.with_clip_rect(Rect::from_min_max(rect.min, pos2(keys.left() - 10.0, rect.bottom())));
                let label = painter.text(pos2(rect.left() + 36.0, rect.center().y), Align2::LEFT_CENTER, &entry.label, font::ui(14.0), color::INK);
                painter.text(pos2(label.right() + 10.0, rect.center().y), Align2::LEFT_CENTER, &entry.detail, font::ui(12.0), color::INK_MUTED);
                if selected && (up || down) {
                    response.scroll_to_me(None);
                }
                if response.clicked() {
                    picked = Some(index);
                }
            }
        });
        ui.add_space(4.0);
        ui.label(egui::RichText::new("↑ ↓ to move  ·  Enter to open  ·  Esc to close").font(font::ui(12.0)).color(color::INK_FAINT));
    });
    match picked {
        Some(index) => Outcome::Pick(entries.swap_remove(index).act),
        None if escape || modal.should_close() => Outcome::Close,
        None => Outcome::Stay,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries() -> Vec<Entry<u8>> {
        [("Forma", "Animate shapes and pictures"), ("Folio", "Make carousels"), ("Launch folio", "Folio · 6 slides"), ("clip.mp4", "45.8 MB"), ("Show the space folder", "")]
            .into_iter()
            .enumerate()
            .map(|(index, (label, detail))| Entry { label: label.into(), detail: detail.into(), keys: "", mark: Mark::Icon(Icon::File), act: index as u8 })
            .collect()
    }

    fn found(query: &str) -> Vec<u8> {
        let entries = entries();
        rank(&entries, query).into_iter().map(|index| entries[index].act).collect()
    }

    #[test]
    fn nothing_typed_lists_everything_in_order() {
        assert_eq!(found("  "), [0, 1, 2, 3, 4]);
    }

    #[test]
    fn labels_that_start_with_the_words_come_first() {
        assert_eq!(found("fo"), [0, 1, 2, 4]);
        assert_eq!(found("FOLIO"), [1, 2]);
        assert_eq!(found("lip"), [3]);
    }

    #[test]
    fn every_word_has_to_be_in_the_row_and_details_count() {
        assert_eq!(found("space show"), [4]);
        assert_eq!(found("slides"), [2]);
        assert_eq!(found("carousels forma"), [] as [u8; 0]);
    }
}
