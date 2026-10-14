// Copyright: klepci contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Minimalist safe facade over an Anki collection for klepci frontends.
//!
//! Deliberately small: open/close, deck list, review loop (next/show/answer),
//! timing stats as JSON. Everything else lives in `anki`, `klepci-stats`
//! and `klepci-sync`. All frontends (desktop, mobile, TUI) talk to this.

use std::fmt::Display;
use std::path::Path;

use anki::prelude::*;
use anki::scheduler::answering::CardAnswer;
use anki::scheduler::answering::Rating;
use anki::scheduler::states::SchedulingStates;

/// Canonical card-HTML to plain text, shared by the Rust frontends.
pub use anki::klepci::html_to_text;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreError(pub String);

impl Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CoreError {}

fn core_err<E: Display>(err: E) -> CoreError {
    CoreError(err.to_string())
}

#[derive(Debug, Clone, Serialize)]
pub struct DeckInfo {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CardView {
    pub id: i64,
    pub question: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnswerView {
    pub id: i64,
    pub question: String,
    pub answer: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrowserRow {
    pub id: i64,
    pub deck: String,
    pub preview: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EditorData {
    pub note_id: i64,
    pub notetype_name: String,
    pub field_names: Vec<String>,
    pub fields: Vec<String>,
    pub tags: String,
}

pub struct Core {
    col: anki::collection::Collection,
    shown: Option<(i64, SchedulingStates)>,
}

impl Core {
    /// Open (or create) the collection at `path`.
    pub fn open(path: &Path) -> Result<Self, CoreError> {
        Ok(Core {
            col: anki::collection::CollectionBuilder::new(path)
                .build()
                .map_err(core_err)?,
            shown: None,
        })
    }

    pub fn close(self) -> Result<(), CoreError> {
        self.col.close(None).map_err(core_err)
    }

    /// All decks, including the default one.
    pub fn decks(&self) -> Result<Vec<DeckInfo>, CoreError> {
        Ok(self
            .col
            .get_all_deck_names(false)
            .map_err(core_err)?
            .into_iter()
            .map(|(id, name)| DeckInfo { id: id.0, name })
            .collect())
    }

    /// Next due card across the whole collection, with rendered question.
    /// Remembers the scheduling states so [`Core::answer`] is safe.
    pub fn next_card(&mut self) -> Result<Option<CardView>, CoreError> {
        match self.col.get_next_card().map_err(core_err)? {
            None => Ok(None),
            Some(queued) => {
                let id = queued.card.id().0;
                let rendered = self
                    .col
                    .render_existing_card(queued.card.id(), false, false)
                    .map_err(core_err)?;
                self.shown = Some((id, queued.states));
                Ok(Some(CardView {
                    id,
                    question: rendered.question().into_owned(),
                }))
            }
        }
    }

    /// Render question + answer for a card (usually the one just shown).
    pub fn show_answer(&mut self, card_id: i64) -> Result<AnswerView, CoreError> {
        let rendered = self
            .col
            .render_existing_card(CardId(card_id), false, false)
            .map_err(core_err)?;
        Ok(AnswerView {
            id: card_id,
            question: rendered.question().into_owned(),
            answer: rendered.answer().into_owned(),
        })
    }

    /// Answer the last shown card. `rating`: 1=again 2=hard 3=good 4=easy.
    /// Refuses cards that were not shown (stale scheduling states are a
    /// data-corruption vector).
    pub fn answer(&mut self, card_id: i64, rating: u8, secs_taken: u32) -> Result<(), CoreError> {
        let (shown_id, states) = self.shown.clone().ok_or_else(|| {
            CoreError("no card shown; call next_card() before answer()".to_string())
        })?;
        if shown_id != card_id {
            return Err(CoreError(format!(
                "stale card: shown {shown_id}, answered {card_id}"
            )));
        }
        let (rating, new_state) = match rating {
            1 => (Rating::Again, states.again),
            2 => (Rating::Hard, states.hard),
            3 => (Rating::Good, states.good),
            4 => (Rating::Easy, states.easy),
            other => return Err(CoreError(format!("rating must be 1-4, got {other}"))),
        };
        let mut answer = CardAnswer {
            card_id: CardId(card_id),
            current_state: states.current,
            new_state,
            rating,
            answered_at: TimestampMillis::now(),
            milliseconds_taken: secs_taken.saturating_mul(1000),
            custom_data: None,
            from_queue: true,
        };
        self.col.answer_card(&mut answer).map_err(core_err)?;
        self.shown = None;
        Ok(())
    }

    /// Timing stats for `search` as compact JSON (heatmap, slowest cards,
    /// deck-relative outliers). For agents and stats screens.
    pub fn timing_json(&mut self, search: &str, top_n: u32) -> Result<String, CoreError> {
        let resp = self
            .col
            .timing_stats_for_search(search, top_n)
            .map_err(core_err)?;
        let slowest: Vec<_> = resp
            .slowest
            .iter()
            .map(|card| {
                serde_json::json!({
                    "card_id": card.card_id,
                    "reviews": card.reviews,
                    "mean_secs": card.mean_secs,
                    "max_secs": card.max_secs,
                })
            })
            .collect();
        let outliers: Vec<_> = resp
            .outliers
            .iter()
            .map(|card| {
                serde_json::json!({
                    "card_id": card.card_id,
                    "mean_secs": card.mean_secs,
                    "z_vs_deck": card.z_vs_deck,
                })
            })
            .collect();
        let days: Vec<_> = resp
            .days
            .iter()
            .map(|day| {
                serde_json::json!({
                    "day": day.day,
                    "reviews": day.reviews,
                    "total_secs": day.total_secs,
                })
            })
            .collect();
        Ok(serde_json::json!({
            "reviews": resp.reviews,
            "total_secs": resp.total_secs,
            "mean_secs": resp.mean_secs,
            "std_secs": resp.std_secs,
            "slowest": slowest,
            "outliers": outliers,
            "days": days,
        })
        .to_string())
    }

    /// Browser rows for `search`, capped at `limit`.
    pub fn search(&mut self, query: &str, limit: usize) -> Result<Vec<BrowserRow>, CoreError> {
        Ok(self
            .col
            .browser_cards_for_search(query, limit)
            .map_err(core_err)?
            .into_iter()
            .map(|row| BrowserRow {
                id: row.id,
                deck: row.deck,
                preview: row.preview,
            })
            .collect())
    }

    /// Suspend (`suspend=true`) or unsuspend cards by id.
    pub fn suspend(&mut self, ids: &[i64], suspend: bool) -> Result<(), CoreError> {
        self.col.suspend_cards(ids, suspend).map_err(core_err)
    }

    /// Bury cards until tomorrow.
    pub fn bury(&mut self, ids: &[i64]) -> Result<(), CoreError> {
        self.col.bury_cards(ids).map_err(core_err)
    }

    /// Delete the notes behind the given cards. Returns notes removed.
    pub fn delete_cards(&mut self, ids: &[i64]) -> Result<u32, CoreError> {
        self.col.delete_cards(ids).map_err(core_err)
    }

    /// Move cards to another deck.
    pub fn set_deck(&mut self, ids: &[i64], deck_id: i64) -> Result<(), CoreError> {
        self.col.set_deck_for_cards(ids, deck_id).map_err(core_err)
    }

    /// Add (`add=true`) or remove a tag on the notes behind the cards.
    pub fn tag(&mut self, ids: &[i64], tag: &str, add: bool) -> Result<(), CoreError> {
        self.col.tag_cards(ids, tag, add).map_err(core_err)
    }

    /// All notetype names, sorted.
    pub fn notetype_names(&mut self) -> Result<Vec<String>, CoreError> {
        self.col.notetype_names().map_err(core_err)
    }

    /// Field names of a notetype, in order.
    pub fn field_names(&mut self, notetype: &str) -> Result<Vec<String>, CoreError> {
        self.col
            .field_names_for_notetype(notetype)
            .map_err(core_err)
    }

    /// Load a card's note for editing.
    pub fn edit_data(&mut self, card_id: i64) -> Result<EditorData, CoreError> {
        self.col
            .note_editor_data(card_id)
            .map_err(core_err)
            .map(|data| EditorData {
                note_id: data.note_id,
                notetype_name: data.notetype_name,
                field_names: data.field_names,
                fields: data.fields,
                tags: data.tags,
            })
    }

    /// Save edited fields + tags (space-separated).
    pub fn save_note(
        &mut self,
        note_id: i64,
        fields: Vec<String>,
        tags: &str,
    ) -> Result<(), CoreError> {
        self.col
            .update_note_fields(note_id, fields, tags)
            .map_err(core_err)
    }

    /// Add a new note. Returns its id.
    pub fn add_note(
        &mut self,
        notetype: &str,
        deck_id: i64,
        fields: Vec<String>,
        tags: &str,
    ) -> Result<i64, CoreError> {
        self.col
            .add_note_to_deck(notetype, deck_id, fields, tags)
            .map_err(core_err)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn test_core(name: &str) -> (Core, tempfile_guard::Guard) {
        let dir = std::env::temp_dir().join(format!(
            "klep-core-{name}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let core = Core::open(&dir.join("test.anki2")).unwrap();
        (core, tempfile_guard::Guard(dir))
    }

    mod tempfile_guard {
        use std::path::PathBuf;
        pub struct Guard(pub PathBuf);
        impl Drop for Guard {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    fn add_basic_note(core: &mut Core) {
        let nt = core
            .col
            .get_notetype_by_name("Basic")
            .unwrap()
            .expect("stock Basic notetype");
        let mut note = nt.new_note();
        core.col.add_note(&mut note, DeckId(1)).unwrap();
    }

    #[test]
    fn decks_lists_default() {
        let (core, _guard) = test_core("decks");
        let decks = core.decks().unwrap();
        assert!(decks.iter().any(|deck| deck.id == 1));
    }

    #[test]
    fn review_loop_next_show_answer() {
        let (mut core, _guard) = test_core("review");
        add_basic_note(&mut core);
        let card = core.next_card().unwrap().expect("one due card");
        assert!(!card.question.is_empty());
        let both = core.show_answer(card.id).unwrap();
        assert!(!both.answer.is_empty());
        core.answer(card.id, 3, 5).unwrap();
        // answering twice without re-showing is refused
        assert!(core.answer(card.id, 3, 5).is_err());
    }

    #[test]
    fn answer_rejects_unknown_card() {
        let (mut core, _guard) = test_core("reject");
        assert!(core.answer(123, 3, 1).is_err());
        assert!(core.answer(123, 9, 1).is_err());
    }

    #[test]
    fn timing_json_empty_collection() {
        let (mut core, _guard) = test_core("timing");
        let json = core.timing_json("", 5).unwrap();
        assert!(json.contains("\"reviews\":0"));
    }

    #[test]
    fn browser_search_and_bulk() {
        let (mut core, _guard) = test_core("browser");
        add_basic_note(&mut core);
        add_basic_note(&mut core);
        let rows = core.search("", 100).unwrap();
        assert_eq!(rows.len(), 2);
        let ids: Vec<i64> = rows.iter().map(|row| row.id).collect();
        core.suspend(&ids[0..1], true).unwrap();
        core.tag(&ids, "klepci", true).unwrap();
        core.suspend(&ids[0..1], false).unwrap();
        core.bury(&ids[1..2]).unwrap();
        assert_eq!(core.delete_cards(&ids[0..1]).unwrap(), 1);
        assert_eq!(core.search("", 100).unwrap().len(), 1);
    }

    #[test]
    fn editor_and_add() {
        let (mut core, _guard) = test_core("editor");
        add_basic_note(&mut core);
        let rows = core.search("", 100).unwrap();
        let data = core.edit_data(rows[0].id).unwrap();
        assert_eq!(data.notetype_name, "Basic");
        assert_eq!(data.field_names.len(), 2);
        core.save_note(
            data.note_id,
            vec!["Q".to_string(), "A".to_string()],
            "t1 t2",
        )
        .unwrap();
        let data = core.edit_data(rows[0].id).unwrap();
        assert_eq!(data.fields[0], "Q");
        assert_eq!(data.tags, "t1 t2");
        let nid = core
            .add_note("Basic", 1, vec!["nq".to_string(), "na".to_string()], "")
            .unwrap();
        assert!(nid > 0);
        assert!(core
            .notetype_names()
            .unwrap()
            .contains(&"Basic".to_string()));
    }
}
