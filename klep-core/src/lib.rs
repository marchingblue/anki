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

#[derive(Debug, Clone, Default, Serialize)]
pub struct DayReviews {
    pub day: i64,
    pub learn: u32,
    pub relearn: u32,
    pub young: u32,
    pub mature: u32,
    pub filtered: u32,
}

impl DayReviews {
    pub fn total(&self) -> u32 {
        self.learn + self.relearn + self.young + self.mature + self.filtered
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DayCount {
    pub day: i64,
    pub count: u32,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct IntervalBucket {
    pub days: u32,
    pub count: u32,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct EaseBucket {
    pub ease: u32,
    pub count: u32,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct HourBucket {
    pub hour: u32,
    pub total: u32,
    pub correct: u32,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CardCountsBreakdown {
    pub new: u32,
    pub learn: u32,
    pub relearn: u32,
    pub young: u32,
    pub mature: u32,
    pub suspended: u32,
    pub buried: u32,
}

impl CardCountsBreakdown {
    pub fn total(&self) -> u32 {
        self.new
            + self.learn
            + self.relearn
            + self.young
            + self.mature
            + self.suspended
            + self.buried
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TodayStats {
    pub answer_count: u32,
    pub answer_millis: u32,
    pub correct_count: u32,
    pub learn_count: u32,
    pub review_count: u32,
    pub relearn_count: u32,
    pub mature_correct: u32,
    pub mature_count: u32,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ButtonRow {
    pub button: u32,
    pub learning: u32,
    pub young: u32,
    pub mature: u32,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TrueRetention {
    pub young_passed: u32,
    pub young_failed: u32,
    pub mature_passed: u32,
    pub mature_failed: u32,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct GraphsData {
    pub reviews: Vec<DayReviews>,
    pub added: Vec<DayCount>,
    pub intervals: Vec<IntervalBucket>,
    pub eases: Vec<EaseBucket>,
    pub average_ease: f32,
    pub hours: Vec<HourBucket>,
    pub future_due: Vec<DayCount>,
    pub daily_load: u32,
    pub card_counts: CardCountsBreakdown,
    pub today: TodayStats,
    pub buttons: Vec<ButtonRow>,
    pub true_retention: TrueRetention,
    pub retrievability: f32,
    pub fsrs: bool,
    pub rollover_hour: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeckOptions {
    pub deck_id: i64,
    pub preset_id: i64,
    pub preset_name: String,
    pub new_per_day: u32,
    pub reviews_per_day: u32,
    pub learn_steps: String,
    pub relearn_steps: String,
    pub graduating_interval_good: u32,
    pub graduating_interval_easy: u32,
    pub easy_multiplier: f32,
    pub hard_multiplier: f32,
    pub interval_multiplier: f32,
    pub maximum_review_interval: u32,
    pub minimum_lapse_interval: u32,
    pub leech_threshold: u32,
    pub leech_action: i32,
    pub desired_retention: f32,
    pub fsrs: bool,
    pub bury_new: bool,
    pub bury_reviews: bool,
    pub bury_interday_learning: bool,
    pub show_timer: bool,
    pub cap_answer_time_to_secs: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeckCounts {
    pub id: i64,
    pub name: String,
    pub level: u32,
    pub new: u32,
    pub learn: u32,
    pub review: u32,
    pub total: u32,
    pub filtered: bool,
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

    /// Decks with today's due counts, depth-first.
    pub fn deck_overview(&mut self) -> Result<Vec<DeckCounts>, CoreError> {
        self.col.deck_overview().map_err(core_err).map(|decks| {
            decks
                .into_iter()
                .map(|deck| DeckCounts {
                    id: deck.id,
                    name: deck.name,
                    level: deck.level,
                    new: deck.new,
                    learn: deck.learn,
                    review: deck.review,
                    total: deck.total,
                    filtered: deck.filtered,
                })
                .collect()
        })
    }

    /// Create a deck. Returns its id.
    pub fn add_deck(&mut self, name: &str) -> Result<i64, CoreError> {
        self.col.create_deck(name).map_err(core_err)
    }

    /// Rename a deck.
    pub fn rename_deck(&mut self, deck_id: i64, name: &str) -> Result<(), CoreError> {
        self.col.rename_deck_by_id(deck_id, name).map_err(core_err)
    }

    /// Delete a deck and its children. Returns deleted card count.
    pub fn delete_deck(&mut self, deck_id: i64) -> Result<usize, CoreError> {
        self.col.delete_deck(deck_id).map_err(core_err)
    }

    /// Make `deck_id` the deck that [`Core::next_card`] reviews.
    pub fn select_deck(&mut self, deck_id: i64) -> Result<(), CoreError> {
        self.shown = None;
        self.col
            .set_current_deck(DeckId(deck_id))
            .map(|_| ())
            .map_err(core_err)
    }

    /// Undo the last collection operation (review, edit, bulk op, …).
    pub fn undo(&mut self) -> Result<(), CoreError> {
        self.shown = None;
        self.col.undo_last().map_err(core_err)
    }

    /// Set a card flag (0 = none, 1-7).
    pub fn flag_card(&mut self, id: i64, flag: u32) -> Result<(), CoreError> {
        self.col.flag_card(id, flag).map_err(core_err)
    }

    /// Toggle Anki's `marked` tag. Returns the new state.
    pub fn toggle_mark(&mut self, card_id: i64) -> Result<bool, CoreError> {
        self.col.toggle_mark_note(card_id).map_err(core_err)
    }

    /// Whether the note behind `card_id` is marked.
    pub fn is_marked(&mut self, card_id: i64) -> Result<bool, CoreError> {
        self.col.note_is_marked(card_id).map_err(core_err)
    }

    /// Data behind the stock Anki graphs screen.
    pub fn graphs(&mut self, search: &str, days: u32) -> Result<GraphsData, CoreError> {
        let graphs = self.col.graphs_for_search(search, days).map_err(core_err)?;
        Ok(GraphsData {
            reviews: graphs
                .reviews
                .into_iter()
                .map(|day| DayReviews {
                    day: day.day,
                    learn: day.learn,
                    relearn: day.relearn,
                    young: day.young,
                    mature: day.mature,
                    filtered: day.filtered,
                })
                .collect(),
            added: graphs
                .added
                .into_iter()
                .map(|day| DayCount {
                    day: day.day,
                    count: day.count,
                })
                .collect(),
            intervals: graphs
                .intervals
                .into_iter()
                .map(|bucket| IntervalBucket {
                    days: bucket.days,
                    count: bucket.count,
                })
                .collect(),
            eases: graphs
                .eases
                .into_iter()
                .map(|bucket| EaseBucket {
                    ease: bucket.ease,
                    count: bucket.count,
                })
                .collect(),
            average_ease: graphs.average_ease,
            hours: graphs
                .hours
                .into_iter()
                .map(|bucket| HourBucket {
                    hour: bucket.hour,
                    total: bucket.total,
                    correct: bucket.correct,
                })
                .collect(),
            future_due: graphs
                .future_due
                .into_iter()
                .map(|day| DayCount {
                    day: day.day,
                    count: day.count,
                })
                .collect(),
            daily_load: graphs.daily_load,
            card_counts: CardCountsBreakdown {
                new: graphs.card_counts.new,
                learn: graphs.card_counts.learn,
                relearn: graphs.card_counts.relearn,
                young: graphs.card_counts.young,
                mature: graphs.card_counts.mature,
                suspended: graphs.card_counts.suspended,
                buried: graphs.card_counts.buried,
            },
            today: TodayStats {
                answer_count: graphs.today.answer_count,
                answer_millis: graphs.today.answer_millis,
                correct_count: graphs.today.correct_count,
                learn_count: graphs.today.learn_count,
                review_count: graphs.today.review_count,
                relearn_count: graphs.today.relearn_count,
                mature_correct: graphs.today.mature_correct,
                mature_count: graphs.today.mature_count,
            },
            buttons: graphs
                .buttons
                .into_iter()
                .map(|row| ButtonRow {
                    button: row.button,
                    learning: row.learning,
                    young: row.young,
                    mature: row.mature,
                })
                .collect(),
            true_retention: TrueRetention {
                young_passed: graphs.true_retention.young_passed,
                young_failed: graphs.true_retention.young_failed,
                mature_passed: graphs.true_retention.mature_passed,
                mature_failed: graphs.true_retention.mature_failed,
            },
            retrievability: graphs.retrievability,
            fsrs: graphs.fsrs,
            rollover_hour: graphs.rollover_hour,
        })
    }

    /// Regex find & replace across notes matching `search`.
    pub fn find_and_replace(
        &mut self,
        search: &str,
        regex: &str,
        replacement: &str,
        field: Option<&str>,
    ) -> Result<usize, CoreError> {
        self.col
            .find_and_replace_notes(search, regex, replacement, field)
            .map_err(core_err)
    }

    /// Options of the preset used by `deck_id`.
    pub fn deck_options(&mut self, deck_id: i64) -> Result<DeckOptions, CoreError> {
        self.col
            .deck_options_for_deck(deck_id)
            .map_err(core_err)
            .map(|options| DeckOptions {
                deck_id: options.deck_id,
                preset_id: options.preset_id,
                preset_name: options.preset_name,
                new_per_day: options.new_per_day,
                reviews_per_day: options.reviews_per_day,
                learn_steps: options.learn_steps,
                relearn_steps: options.relearn_steps,
                graduating_interval_good: options.graduating_interval_good,
                graduating_interval_easy: options.graduating_interval_easy,
                easy_multiplier: options.easy_multiplier,
                hard_multiplier: options.hard_multiplier,
                interval_multiplier: options.interval_multiplier,
                maximum_review_interval: options.maximum_review_interval,
                minimum_lapse_interval: options.minimum_lapse_interval,
                leech_threshold: options.leech_threshold,
                leech_action: options.leech_action,
                desired_retention: options.desired_retention,
                fsrs: options.fsrs,
                bury_new: options.bury_new,
                bury_reviews: options.bury_reviews,
                bury_interday_learning: options.bury_interday_learning,
                show_timer: options.show_timer,
                cap_answer_time_to_secs: options.cap_answer_time_to_secs,
            })
    }

    /// Save deck options to the shared preset.
    pub fn save_deck_options(&mut self, options: &DeckOptions) -> Result<(), CoreError> {
        let options = anki::klepci::DeckOptions {
            deck_id: options.deck_id,
            preset_id: options.preset_id,
            preset_name: options.preset_name.clone(),
            new_per_day: options.new_per_day,
            reviews_per_day: options.reviews_per_day,
            learn_steps: options.learn_steps.clone(),
            relearn_steps: options.relearn_steps.clone(),
            graduating_interval_good: options.graduating_interval_good,
            graduating_interval_easy: options.graduating_interval_easy,
            easy_multiplier: options.easy_multiplier,
            hard_multiplier: options.hard_multiplier,
            interval_multiplier: options.interval_multiplier,
            maximum_review_interval: options.maximum_review_interval,
            minimum_lapse_interval: options.minimum_lapse_interval,
            leech_threshold: options.leech_threshold,
            leech_action: options.leech_action,
            desired_retention: options.desired_retention,
            fsrs: options.fsrs,
            bury_new: options.bury_new,
            bury_reviews: options.bury_reviews,
            bury_interday_learning: options.bury_interday_learning,
            show_timer: options.show_timer,
            cap_answer_time_to_secs: options.cap_answer_time_to_secs,
        };
        self.col.save_deck_options(&options).map_err(core_err)
    }

    /// Import an `.apkg`/`.colpkg`. Returns the number of new notes.
    pub fn import_apkg(&mut self, path: &str) -> Result<u32, CoreError> {
        self.shown = None;
        self.col.import_apkg_file(path).map_err(core_err)
    }

    /// Export notes matching `search` (empty = all) to `.apkg`.
    pub fn export_apkg(&mut self, path: &str, search: &str) -> Result<u32, CoreError> {
        self.col.export_apkg_file(path, search).map_err(core_err)
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
    /// Invalidates the reviewer's pending card.
    pub fn suspend(&mut self, ids: &[i64], suspend: bool) -> Result<(), CoreError> {
        self.shown = None;
        self.col.suspend_cards(ids, suspend).map_err(core_err)
    }

    /// Bury cards until tomorrow. Invalidates the reviewer's pending card.
    pub fn bury(&mut self, ids: &[i64]) -> Result<(), CoreError> {
        self.shown = None;
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
    fn graphs_after_review() {
        let (mut core, _guard) = test_core("graphs");
        add_basic_note(&mut core);
        let card = core.next_card().unwrap().expect("due card");
        core.answer(card.id, 3, 4).unwrap();

        let graphs = core.graphs("", 0).unwrap();
        assert_eq!(graphs.today.answer_count, 1);
        assert!(graphs.today.answer_millis >= 4000);
        assert!(graphs.reviews.iter().map(DayReviews::total).sum::<u32>() >= 1);
        assert_eq!(graphs.hours.len(), 24);
        assert_eq!(graphs.buttons.len(), 4);
        assert_eq!(graphs.card_counts.total(), 1);

        let scoped = core.graphs("card:1", 0).unwrap();
        assert_eq!(scoped.card_counts.total(), 1);
    }

    #[test]
    fn reviewer_ops() {
        let (mut core, _guard) = test_core("reviewer_ops");
        add_basic_note(&mut core);
        let card = core.next_card().unwrap().expect("due card");
        core.flag_card(card.id, 2).unwrap();
        assert_eq!(core.search("flag:2", 10).unwrap().len(), 1);
        assert!(core.toggle_mark(card.id).unwrap());
        assert_eq!(core.search("tag:marked", 10).unwrap().len(), 1);
        assert!(!core.toggle_mark(card.id).unwrap());
        core.suspend(&[card.id], true).unwrap();
        assert_eq!(core.search("is:suspended", 10).unwrap().len(), 1);
        core.undo().unwrap();
        assert_eq!(core.search("is:suspended", 10).unwrap().len(), 0);
        core.bury(&[card.id]).unwrap();
        assert_eq!(core.search("is:buried", 10).unwrap().len(), 1);
    }

    #[test]
    fn deck_crud_and_counts() {
        let (mut core, _guard) = test_core("deck_crud");
        add_basic_note(&mut core);
        let overview = core.deck_overview().unwrap();
        let default = overview.iter().find(|deck| deck.id == 1).unwrap();
        assert!(default.total >= 1);

        let id = core.add_deck("Languages::Japanese").unwrap();
        assert!(core.add_deck("Languages::Japanese").is_err());
        let overview = core.deck_overview().unwrap();
        let child = overview
            .iter()
            .find(|deck| deck.id == id)
            .expect("child deck");
        assert_eq!(child.name, "Languages::Japanese");
        assert!(overview.iter().any(|deck| deck.name == "Languages"));

        core.rename_deck(id, "Languages::日本語").unwrap();
        assert!(core
            .deck_overview()
            .unwrap()
            .iter()
            .any(|deck| deck.name == "Languages::日本語"));

        core.select_deck(id).unwrap();
        assert!(
            core.next_card().unwrap().is_none(),
            "empty deck has no cards"
        );
        core.select_deck(1).unwrap();
        assert!(
            core.next_card().unwrap().is_some(),
            "default deck still has cards"
        );

        assert_eq!(core.delete_deck(id).unwrap(), 0);
        assert!(!core
            .deck_overview()
            .unwrap()
            .iter()
            .any(|deck| deck.name == "Languages::日本語"));
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
