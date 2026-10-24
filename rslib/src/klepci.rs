// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Bridge between a live collection and `klepci-sync` snapshots.
//!
//! Export is read-only and never touches the collection format.
//! Tombstones are NOT read from the `graves` table (it keeps numeric oids,
//! so a deleted note's guid is unrecoverable there) — deletions are derived
//! by diffing snapshots, like git does.

use std::collections::BTreeSet;

use klepci_sync::NoteSnapshot;

use crate::prelude::*;
use crate::search::SortMode;

/// Card HTML to readable plain text. Canonical version shared by the Rust
/// frontends (mobile ports keep their own one-liner copies).
pub fn html_to_text(html: &str) -> String {
    let mut out = html
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("</p>", "\n")
        .replace("</div>", "\n")
        .replace("</li>", "\n")
        .replace("<li>", "\n• ");
    // [sound:name] -> placeholder
    while let Some(start) = out.find("[sound:") {
        let Some(end) = out[start..].find(']') else {
            break;
        };
        out.replace_range(start..start + end + 1, "🔊");
    }
    // <img ...> -> placeholder
    while let Some(start) = out.find("<img") {
        let Some(end) = out[start..].find('>') else {
            break;
        };
        out.replace_range(start..start + end + 1, "[image]");
    }
    // strip remaining tags
    let mut text = String::with_capacity(out.len());
    let mut inside = false;
    for ch in out.chars() {
        match ch {
            '<' => inside = true,
            '>' => inside = false,
            _ if !inside => text.push(ch),
            _ => {}
        }
    }
    text.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// One browser row. Public klepci API.
#[derive(Debug, Clone)]
pub struct BrowserRow {
    pub id: i64,
    pub deck: String,
    pub preview: String,
}

/// Data for the note editor. Public klepci API.
#[derive(Debug, Clone)]
pub struct EditorData {
    pub note_id: i64,
    pub notetype_name: String,
    pub field_names: Vec<String>,
    pub fields: Vec<String>,
    pub tags: String,
}

/// One day's review counts, grouped by card maturity like stock Anki.
#[derive(Debug, Clone, Default)]
pub struct DayReviews {
    /// Days since Unix epoch (local day).
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

#[derive(Debug, Clone, Default)]
pub struct DayCount {
    pub day: i64,
    pub count: u32,
}

#[derive(Debug, Clone, Default)]
pub struct IntervalBucket {
    pub days: u32,
    pub count: u32,
}

#[derive(Debug, Clone, Default)]
pub struct EaseBucket {
    /// Ease in percent, e.g. 250.
    pub ease: u32,
    pub count: u32,
}

#[derive(Debug, Clone, Default)]
pub struct HourBucket {
    pub hour: u32,
    pub total: u32,
    pub correct: u32,
}

#[derive(Debug, Clone, Default)]
pub struct CardCountsBreakdown {
    pub new: u32,
    pub learn: u32,
    pub relearn: u32,
    pub young: u32,
    pub mature: u32,
    pub suspended: u32,
    pub buried: u32,
}

#[derive(Debug, Clone, Default)]
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

#[derive(Debug, Clone, Default)]
pub struct ButtonRow {
    /// 1 = again, 2 = hard, 3 = good, 4 = easy.
    pub button: u32,
    pub learning: u32,
    pub young: u32,
    pub mature: u32,
}

#[derive(Debug, Clone, Default)]
pub struct TrueRetention {
    pub young_passed: u32,
    pub young_failed: u32,
    pub mature_passed: u32,
    pub mature_failed: u32,
}

/// Everything the stock Anki graph screen draws, in a frontend-friendly flat
/// shape. Maps are sorted so charts are deterministic.
#[derive(Debug, Clone, Default)]
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

/// Editable subset of a deck preset (stock Anki's options screen).
#[derive(Debug, Clone)]
pub struct DeckOptions {
    pub deck_id: i64,
    pub preset_id: i64,
    pub preset_name: String,
    pub new_per_day: u32,
    pub reviews_per_day: u32,
    /// Space-separated minutes, e.g. `"1 10"`.
    pub learn_steps: String,
    /// Space-separated minutes, e.g. `"10"`.
    pub relearn_steps: String,
    pub graduating_interval_good: u32,
    pub graduating_interval_easy: u32,
    pub easy_multiplier: f32,
    pub hard_multiplier: f32,
    pub interval_multiplier: f32,
    pub maximum_review_interval: u32,
    pub minimum_lapse_interval: u32,
    pub leech_threshold: u32,
    /// 0 = suspend, 1 = tag only, 2 = bury.
    pub leech_action: i32,
    pub desired_retention: f32,
    pub fsrs: bool,
    pub bury_new: bool,
    pub bury_reviews: bool,
    pub bury_interday_learning: bool,
    pub show_timer: bool,
    pub cap_answer_time_to_secs: u32,
}

/// A deck with its live due counts, flattened from the deck tree.
#[derive(Debug, Clone)]
pub struct DeckCounts {
    pub id: i64,
    /// Full `Parent::Child` name.
    pub name: String,
    pub level: u32,
    pub new: u32,
    pub learn: u32,
    pub review: u32,
    /// Cards directly in this deck (no children).
    pub total: u32,
    pub filtered: bool,
}

impl Collection {
    /// Export notes matching `search` as guid-keyed snapshots, sorted by guid.
    /// Public klepci API (read-only; the collection format is untouched).
    pub fn export_notes_for_search(&mut self, search: &str) -> Result<Vec<NoteSnapshot>> {
        let guard = self.search_cards_into_table(search, SortMode::NoOrder)?;
        let col = &mut *guard.col;
        let note_ids: BTreeSet<NoteId> = col
            .storage
            .all_searched_cards()?
            .iter()
            .map(|card| card.note_id)
            .collect();
        let mut out = Vec::with_capacity(note_ids.len());
        for nid in note_ids {
            let note = col.storage.get_note(nid)?.or_not_found(nid)?;
            let nt = col
                .get_notetype(note.notetype_id)?
                .or_not_found(note.notetype_id)?;
            let mut tags = note.tags.clone();
            tags.sort();
            out.push(NoteSnapshot {
                guid: note.guid.clone(),
                notetype: nt.name.clone(),
                fields: note.fields().clone(),
                tags,
                mtime_secs: note.mtime.0,
            });
        }
        out.sort_by(|a, b| a.guid.cmp(&b.guid));
        Ok(out)
    }

    /// Apply a merged snapshot set. Existing notes keep their cards and
    /// decks; only fields/tags change. New notes land in `target_deck`.
    /// Schema drift (field count, notetype) is reported, never forced.
    /// Public klepci API.
    pub fn import_merged_notes(
        &mut self,
        target_deck: DeckId,
        merged: &[NoteSnapshot],
        deleted: &[String],
    ) -> Result<BridgeReport> {
        let mut report = BridgeReport::default();
        let guid_map = self.storage.all_notes_by_guid()?;
        for snap in merged {
            match guid_map.get(&snap.guid) {
                Some(nid) => {
                    let mut note = self.storage.get_note(*nid)?.or_not_found(*nid)?;
                    if note.fields().len() != snap.fields.len() {
                        report.skipped.push(SkippedNote {
                            guid: snap.guid.clone(),
                            reason: format!(
                                "field count {} != snapshot {}",
                                note.fields().len(),
                                snap.fields.len()
                            ),
                        });
                        continue;
                    }
                    let nt = self
                        .get_notetype(note.notetype_id)?
                        .or_not_found(note.notetype_id)?;
                    if nt.name != snap.notetype {
                        report.skipped.push(SkippedNote {
                            guid: snap.guid.clone(),
                            reason: format!("notetype {} != snapshot {}", nt.name, snap.notetype),
                        });
                        continue;
                    }
                    if *note.fields() != snap.fields || note.tags != snap.tags {
                        *note.fields_mut() = snap.fields.clone();
                        note.tags = snap.tags.clone();
                        self.update_note(&mut note)?;
                        report.updated += 1;
                    }
                }
                None => {
                    let Some(nt) = self.get_notetype_by_name(&snap.notetype)? else {
                        report.skipped.push(SkippedNote {
                            guid: snap.guid.clone(),
                            reason: format!("unknown notetype {}", snap.notetype),
                        });
                        continue;
                    };
                    let mut note = nt.new_note();
                    if note.fields().len() != snap.fields.len() {
                        report.skipped.push(SkippedNote {
                            guid: snap.guid.clone(),
                            reason: format!(
                                "field count {} != snapshot {}",
                                note.fields().len(),
                                snap.fields.len()
                            ),
                        });
                        continue;
                    }
                    // Preserve the guid: otherwise the next sync treats
                    // this note as brand new on every side.
                    note.guid = snap.guid.clone();
                    *note.fields_mut() = snap.fields.clone();
                    note.tags = snap.tags.clone();
                    self.add_note(&mut note, target_deck)?;
                    report.added += 1;
                }
            }
        }
        let to_delete: Vec<NoteId> = deleted
            .iter()
            .filter_map(|guid| guid_map.get(guid).copied())
            .collect();
        if !to_delete.is_empty() {
            self.remove_notes(&to_delete)?;
            report.deleted = to_delete.len() as u32;
        }
        Ok(report)
    }

    /// Unique sorted note ids for the given card ids.
    fn note_id_for_cid(&mut self, cid: i64) -> Result<NoteId> {
        let card = self
            .storage
            .get_card(CardId(cid))?
            .or_not_found(CardId(cid))?;
        Ok(card.note_id)
    }

    fn note_ids_for_cids(&mut self, cids: &[i64]) -> Result<Vec<NoteId>> {
        let mut nids = BTreeSet::new();
        for cid in cids {
            let card = self
                .storage
                .get_card(CardId(*cid))?
                .or_not_found(CardId(*cid))?;
            nids.insert(card.note_id);
        }
        Ok(nids.into_iter().collect())
    }

    /// Browser rows for `search`, capped at `limit`. Preview is plain text
    /// truncated to 120 chars.
    pub fn browser_cards_for_search(
        &mut self,
        search: &str,
        limit: usize,
    ) -> Result<Vec<BrowserRow>> {
        let cids = self.search_cards(search, SortMode::NoOrder)?;
        let mut rows = Vec::new();
        for cid in cids.into_iter().take(limit) {
            let card = self.storage.get_card(cid)?.or_not_found(cid)?;
            let deck = self
                .storage
                .get_deck(card.deck_id)?
                .or_not_found(card.deck_id)?;
            let question = self
                .render_existing_card(cid, true, false)?
                .question()
                .into_owned();
            let preview: String = html_to_text(&question).chars().take(120).collect();
            rows.push(BrowserRow {
                id: cid.0,
                deck: deck.human_name(),
                preview,
            });
        }
        Ok(rows)
    }

    /// Suspend (`suspend=true`) or unsuspend the given cards.
    pub fn suspend_cards(&mut self, cids: &[i64], suspend: bool) -> Result<()> {
        use anki_proto::scheduler::bury_or_suspend_cards_request::Mode;
        let ids: Vec<CardId> = cids.iter().map(|cid| CardId(*cid)).collect();
        if suspend {
            self.bury_or_suspend_cards(&ids, Mode::Suspend)
                .map(|_| ())?;
        } else {
            self.unbury_or_unsuspend_cards(&ids).map(|_| ())?;
        }
        Ok(())
    }

    /// Bury the given cards until tomorrow.
    pub fn bury_cards(&mut self, cids: &[i64]) -> Result<()> {
        use anki_proto::scheduler::bury_or_suspend_cards_request::Mode;
        let ids: Vec<CardId> = cids.iter().map(|cid| CardId(*cid)).collect();
        self.bury_or_suspend_cards(&ids, Mode::BuryUser)
            .map(|_| ())?;
        Ok(())
    }

    /// Delete the notes behind the given cards. Returns notes removed.
    pub fn delete_cards(&mut self, cids: &[i64]) -> Result<u32> {
        let nids = self.note_ids_for_cids(cids)?;
        if nids.is_empty() {
            return Ok(0);
        }
        self.remove_notes(&nids).map(|_| ())?;
        Ok(nids.len() as u32)
    }

    /// Move the given cards to another deck.
    pub fn set_deck_for_cards(&mut self, cids: &[i64], deck_id: i64) -> Result<()> {
        let ids: Vec<CardId> = cids.iter().map(|cid| CardId(*cid)).collect();
        self.set_deck(&ids, DeckId(deck_id)).map(|_| ())?;
        Ok(())
    }

    /// Add (`add=true`) or remove a whitespace-separated tag on the notes
    /// behind the given cards.
    pub fn tag_cards(&mut self, cids: &[i64], tags: &str, add: bool) -> Result<()> {
        let nids = self.note_ids_for_cids(cids)?;
        if nids.is_empty() {
            return Ok(());
        }
        if add {
            self.add_tags_to_notes(&nids, tags).map(|_| ())?;
        } else {
            self.remove_tags_from_notes(&nids, tags).map(|_| ())?;
        }
        Ok(())
    }

    /// Data behind the graphs screen. `search` is an Anki query (empty = all),
    /// `days` limits history (0 = all time).
    pub fn graphs_for_search(&mut self, search: &str, days: u32) -> Result<GraphsData> {
        let proto = self.graph_data_for_search(search, days)?;
        Ok(graphs_data_from_proto(proto))
    }

    /// Regex find & replace across notes matching `search`. `field` limits
    /// the replacement to a single field name. Returns changed note count.
    pub fn find_and_replace_notes(
        &mut self,
        search: &str,
        regex: &str,
        replacement: &str,
        field: Option<&str>,
    ) -> Result<usize> {
        let nids = self.search_notes_unordered(search)?;
        self.find_and_replace(nids, regex, replacement, field.map(str::to_string))
            .map(|out| out.output)
    }

    /// Options for the preset used by `deck_id`.
    pub fn deck_options_for_deck(&mut self, deck_id: i64) -> Result<DeckOptions> {
        let deck = self
            .storage
            .get_deck(DeckId(deck_id))?
            .or_not_found(DeckId(deck_id))?;
        let config_id = deck
            .config_id()
            .or_invalid("filtered decks have no preset")?;
        let config = self
            .get_deck_config(config_id, true)?
            .or_not_found(config_id)?;
        let fsrs = self.get_config_bool(BoolKey::Fsrs);
        Ok(DeckOptions {
            deck_id,
            preset_id: config.id.0,
            preset_name: config.name.clone(),
            new_per_day: config.inner.new_per_day,
            reviews_per_day: config.inner.reviews_per_day,
            learn_steps: join_steps(&config.inner.learn_steps),
            relearn_steps: join_steps(&config.inner.relearn_steps),
            graduating_interval_good: config.inner.graduating_interval_good,
            graduating_interval_easy: config.inner.graduating_interval_easy,
            easy_multiplier: config.inner.easy_multiplier,
            hard_multiplier: config.inner.hard_multiplier,
            interval_multiplier: config.inner.interval_multiplier,
            maximum_review_interval: config.inner.maximum_review_interval,
            minimum_lapse_interval: config.inner.minimum_lapse_interval,
            leech_threshold: config.inner.leech_threshold,
            leech_action: config.inner.leech_action,
            desired_retention: config.inner.desired_retention,
            fsrs,
            bury_new: config.inner.bury_new,
            bury_reviews: config.inner.bury_reviews,
            bury_interday_learning: config.inner.bury_interday_learning,
            show_timer: config.inner.show_timer,
            cap_answer_time_to_secs: config.inner.cap_answer_time_to_secs,
        })
    }

    /// Save options back to the deck's preset. The preset is shared by every
    /// deck that uses it, exactly like stock Anki.
    pub fn save_deck_options(&mut self, options: &DeckOptions) -> Result<()> {
        let mut config = self
            .get_deck_config(DeckConfigId(options.preset_id), false)?
            .or_not_found(DeckConfigId(options.preset_id))?;
        config.inner.new_per_day = options.new_per_day;
        config.inner.reviews_per_day = options.reviews_per_day;
        config.inner.learn_steps = parse_steps(&options.learn_steps)?;
        config.inner.relearn_steps = parse_steps(&options.relearn_steps)?;
        config.inner.graduating_interval_good = options.graduating_interval_good.max(1);
        config.inner.graduating_interval_easy = options
            .graduating_interval_easy
            .max(options.graduating_interval_good)
            .max(1);
        config.inner.easy_multiplier = options.easy_multiplier.max(1.0);
        config.inner.hard_multiplier = options.hard_multiplier.clamp(0.5, 1.0);
        config.inner.interval_multiplier = options.interval_multiplier.clamp(0.5, 2.0);
        config.inner.maximum_review_interval = options.maximum_review_interval.max(1);
        config.inner.minimum_lapse_interval = options.minimum_lapse_interval.max(1);
        config.inner.leech_threshold = options.leech_threshold.max(1);
        config.inner.leech_action = options.leech_action;
        config.inner.desired_retention = options.desired_retention.clamp(0.7, 0.99);
        config.inner.bury_new = options.bury_new;
        config.inner.bury_reviews = options.bury_reviews;
        config.inner.bury_interday_learning = options.bury_interday_learning;
        config.inner.show_timer = options.show_timer;
        config.inner.cap_answer_time_to_secs = options.cap_answer_time_to_secs;
        self.transact(crate::ops::Op::UpdateDeckConfig, |col| {
            col.add_or_update_deck_config(&mut config)
        })?;
        self.set_config_bool(BoolKey::Fsrs, options.fsrs, false)?;
        Ok(())
    }

    /// Import an `.apkg`/`.colpkg` into this collection. Returns new notes.
    /// Uses stock Anki's importer, so the package format stays fully
    /// compatible.
    pub fn import_apkg_file(&mut self, path: &str) -> Result<u32> {
        let options = anki_proto::import_export::ImportAnkiPackageOptions {
            merge_notetypes: true,
            with_scheduling: true,
            with_deck_configs: true,
            ..Default::default()
        };
        let output = self.import_apkg(path, options)?;
        Ok(output.output.new.len() as u32)
    }

    /// Export notes matching `search` (empty = whole collection) to `.apkg`.
    /// Returns the number of exported notes.
    pub fn export_apkg_file(&mut self, path: &str, search: &str) -> Result<u32> {
        let options = anki_proto::import_export::ExportAnkiPackageOptions {
            with_scheduling: true,
            with_deck_configs: true,
            with_media: true,
            legacy: false,
        };
        self.export_apkg(path, options, search, None)
            .map(|count| count as u32)
    }

    /// Undo the last collection operation.
    pub fn undo_last(&mut self) -> Result<()> {
        self.undo().map(|_| ())
    }

    /// Whether the note behind `cid` carries the `marked` tag.
    pub fn note_is_marked(&mut self, cid: i64) -> Result<bool> {
        let note_id = self.note_id_for_cid(cid)?;
        let note = self.storage.get_note(note_id)?.or_not_found(note_id)?;
        Ok(note.tags.iter().any(|tag| tag == "marked"))
    }

    /// Toggle Anki's `marked` tag on the note behind `cid`.
    /// Returns the new marked state.
    pub fn toggle_mark_note(&mut self, cid: i64) -> Result<bool> {
        let note_id = self.note_id_for_cid(cid)?;
        let note = self.storage.get_note(note_id)?.or_not_found(note_id)?;
        let marked = note.tags.iter().any(|tag| tag == "marked");
        let nids = [note_id];
        if marked {
            self.remove_tags_from_notes(&nids, "marked")?;
        } else {
            self.add_tags_to_notes(&nids, "marked")?;
        }
        Ok(!marked)
    }

    /// Set a card flag (0 = none, 1-7).
    pub fn flag_card(&mut self, cid: i64, flag: u32) -> Result<()> {
        self.set_card_flag(&[CardId(cid)], flag).map(|_| ())
    }

    /// All notetype names, sorted.
    pub fn notetype_names(&mut self) -> Result<Vec<String>> {
        let mut names: Vec<String> = self
            .get_all_notetypes()?
            .iter()
            .map(|nt| nt.name.clone())
            .collect();
        names.sort();
        Ok(names)
    }

    /// Field names of a notetype, in order.
    pub fn field_names_for_notetype(&mut self, notetype_name: &str) -> Result<Vec<String>> {
        let nt = self
            .get_notetype_by_name(notetype_name)?
            .or_not_found(notetype_name)?;
        Ok(nt.fields.iter().map(|field| field.name.clone()).collect())
    }

    /// Load a card's note for editing.
    pub fn note_editor_data(&mut self, cid: i64) -> Result<EditorData> {
        let card = self
            .storage
            .get_card(CardId(cid))?
            .or_not_found(CardId(cid))?;
        let note = self
            .storage
            .get_note(card.note_id)?
            .or_not_found(card.note_id)?;
        let nt = self
            .get_notetype(note.notetype_id)?
            .or_not_found(note.notetype_id)?;
        Ok(EditorData {
            note_id: note.id.0,
            notetype_name: nt.name.clone(),
            field_names: nt.fields.iter().map(|field| field.name.clone()).collect(),
            fields: note.fields().clone(),
            tags: note.tags.join(" "),
        })
    }

    /// Save edited fields + tags (space-separated). Field count must match.
    pub fn update_note_fields(
        &mut self,
        note_id: i64,
        fields: Vec<String>,
        tags: &str,
    ) -> Result<()> {
        let mut note = self
            .storage
            .get_note(NoteId(note_id))?
            .or_not_found(NoteId(note_id))?;
        if note.fields().len() != fields.len() {
            return None.or_invalid(format!(
                "field count {} != {}",
                note.fields().len(),
                fields.len()
            ));
        }
        *note.fields_mut() = fields;
        note.tags = tags.split_whitespace().map(str::to_string).collect();
        self.update_note(&mut note).map(|_| ())
    }

    /// Add a new note. Returns its id.
    pub fn add_note_to_deck(
        &mut self,
        notetype_name: &str,
        deck_id: i64,
        fields: Vec<String>,
        tags: &str,
    ) -> Result<i64> {
        let nt = self
            .get_notetype_by_name(notetype_name)?
            .or_not_found(notetype_name)?;
        let mut note = nt.new_note();
        if note.fields().len() != fields.len() {
            return None.or_invalid(format!(
                "field count {} != {}",
                note.fields().len(),
                fields.len()
            ));
        }
        *note.fields_mut() = fields;
        note.tags = tags.split_whitespace().map(str::to_string).collect();
        self.add_note(&mut note, DeckId(deck_id)).map(|_| ())?;
        Ok(note.id.0)
    }

    /// All decks with today's due counts, depth-first, root excluded.
    pub fn deck_overview(&mut self) -> Result<Vec<DeckCounts>> {
        let tree = self.deck_tree(Some(TimestampSecs::now()))?;
        let mut out = Vec::new();
        for child in tree.children {
            flatten_deck_node(child, "", &mut out);
        }
        Ok(out)
    }

    /// Create a deck. Returns its id. Returns an error for duplicate names.
    pub fn create_deck(&mut self, name: &str) -> Result<i64> {
        if self.get_deck_id(name)?.is_some() {
            return None.or_invalid(format!("deck already exists: {name}"));
        }
        let mut deck = crate::decks::Deck::new_normal();
        deck.name = crate::decks::NativeDeckName::from_human_name(name);
        self.add_deck(&mut deck).map(|_| ())?;
        Ok(deck.id.0)
    }

    /// Rename a deck (children follow automatically).
    pub fn rename_deck_by_id(&mut self, deck_id: i64, new_name: &str) -> Result<()> {
        self.rename_deck(DeckId(deck_id), new_name).map(|_| ())
    }

    /// Delete a deck and all child decks. Returns deleted card count.
    pub fn delete_deck(&mut self, deck_id: i64) -> Result<usize> {
        self.remove_decks_and_child_decks(&[DeckId(deck_id)])
            .map(|out| out.output)
    }
}

fn graphs_data_from_proto(proto: anki_proto::stats::GraphsResponse) -> GraphsData {
    let mut reviews: Vec<DayReviews> = proto
        .reviews
        .map(|reviews| reviews.count)
        .unwrap_or_default()
        .into_iter()
        .map(|(day, counts)| DayReviews {
            day: day as i64,
            learn: counts.learn,
            relearn: counts.relearn,
            young: counts.young,
            mature: counts.mature,
            filtered: counts.filtered,
        })
        .collect();
    reviews.sort_by_key(|day| day.day);

    let mut added: Vec<DayCount> = proto
        .added
        .map(|added| {
            added
                .added
                .into_iter()
                .map(|(day, count)| DayCount {
                    day: day as i64,
                    count,
                })
                .collect()
        })
        .unwrap_or_default();
    added.sort_by_key(|day| day.day);

    let mut intervals: Vec<IntervalBucket> = proto
        .intervals
        .map(|intervals| {
            intervals
                .intervals
                .into_iter()
                .map(|(days, count)| IntervalBucket { days, count })
                .collect()
        })
        .unwrap_or_default();
    intervals.sort_by_key(|bucket| bucket.days);

    let eases = proto.eases.unwrap_or_default();
    let mut ease_buckets: Vec<EaseBucket> = eases
        .eases
        .into_iter()
        .map(|(ease, count)| EaseBucket { ease, count })
        .collect();
    ease_buckets.sort_by_key(|bucket| bucket.ease);

    let hours = proto
        .hours
        .map(|hours| {
            hours
                .all_time
                .into_iter()
                .enumerate()
                .map(|(hour, bucket)| HourBucket {
                    hour: hour as u32,
                    total: bucket.total,
                    correct: bucket.correct,
                })
                .collect()
        })
        .unwrap_or_default();

    let future_due_proto = proto.future_due.unwrap_or_default();
    let mut future_due: Vec<DayCount> = future_due_proto
        .future_due
        .into_iter()
        .map(|(day, count)| DayCount {
            day: day as i64,
            count,
        })
        .collect();
    future_due.sort_by_key(|day| day.day);

    let counts = proto.card_counts.unwrap_or_default().excluding_inactive;
    let card_counts = counts
        .map(|counts| CardCountsBreakdown {
            new: counts.new_cards,
            learn: counts.learn,
            relearn: counts.relearn,
            young: counts.young,
            mature: counts.mature,
            suspended: counts.suspended,
            buried: counts.buried,
        })
        .unwrap_or_default();

    let today = proto.today.unwrap_or_default();
    let today = TodayStats {
        answer_count: today.answer_count,
        answer_millis: today.answer_millis,
        correct_count: today.correct_count,
        learn_count: today.learn_count,
        review_count: today.review_count,
        relearn_count: today.relearn_count,
        mature_correct: today.mature_correct,
        mature_count: today.mature_count,
    };

    let buttons = proto.buttons.map(|buttons| {
        let counts = buttons.all_time.unwrap_or_default();
        let mut rows: Vec<ButtonRow> = (1..=4)
            .map(|button| ButtonRow {
                button,
                ..Default::default()
            })
            .collect();
        for (button, count) in counts.learning.iter().enumerate() {
            if let Some(row) = rows.get_mut(button) {
                row.learning = *count;
            }
        }
        for (button, count) in counts.young.iter().enumerate() {
            if let Some(row) = rows.get_mut(button) {
                row.young = *count;
            }
        }
        for (button, count) in counts.mature.iter().enumerate() {
            if let Some(row) = rows.get_mut(button) {
                row.mature = *count;
            }
        }
        rows
    });
    let buttons = buttons.unwrap_or_default();

    let retention = proto.true_retention.unwrap_or_default().all_time;
    let true_retention = retention
        .map(|retention| TrueRetention {
            young_passed: retention.young_passed,
            young_failed: retention.young_failed,
            mature_passed: retention.mature_passed,
            mature_failed: retention.mature_failed,
        })
        .unwrap_or_default();

    GraphsData {
        reviews,
        added,
        intervals,
        eases: ease_buckets,
        average_ease: eases.average,
        hours,
        future_due,
        daily_load: future_due_proto.daily_load,
        card_counts,
        today,
        buttons,
        true_retention,
        retrievability: proto.retrievability.map(|r| r.average).unwrap_or_default(),
        fsrs: proto.fsrs,
        rollover_hour: proto.rollover_hour,
    }
}

fn join_steps(steps: &[f32]) -> String {
    steps
        .iter()
        .map(|step| {
            if step.fract() == 0.0 {
                format!("{}", *step as i64)
            } else {
                format!("{step}")
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_steps(text: &str) -> Result<Vec<f32>> {
    let mut steps = Vec::new();
    for part in text.split_whitespace() {
        let Ok(step) = part.parse::<f32>() else {
            return None.or_invalid(format!("invalid step: {part}"));
        };
        if !(0.0..=10_080.0).contains(&step) {
            return None.or_invalid(format!("step out of range: {part}"));
        }
        steps.push(step);
    }
    if steps.is_empty() {
        return None.or_invalid("at least one step is required");
    }
    Ok(steps)
}

fn flatten_deck_node(
    node: anki_proto::decks::DeckTreeNode,
    parent: &str,
    out: &mut Vec<DeckCounts>,
) {
    let name = if parent.is_empty() {
        node.name.clone()
    } else {
        format!("{parent}::{}", node.name)
    };
    out.push(DeckCounts {
        id: node.deck_id,
        name: name.clone(),
        level: node.level,
        new: node.new_count,
        learn: node.learn_count,
        review: node.review_count,
        total: node.total_in_deck,
        filtered: node.filtered,
    });
    for child in node.children {
        flatten_deck_node(child, &name, out);
    }
}

/// Outcome of [`Collection::import_merged_notes`].
#[derive(Debug, Default)]
pub struct BridgeReport {
    pub added: u32,
    pub updated: u32,
    pub deleted: u32,
    pub skipped: Vec<SkippedNote>,
}

#[derive(Debug)]
pub struct SkippedNote {
    pub guid: String,
    pub reason: String,
}

#[cfg(test)]
mod test {
    use super::*;

    fn two_note_collection() -> Result<Collection> {
        let mut col = Collection::new();
        let nt = col.get_notetype_by_name("Basic")?.unwrap();
        for word in ["alpha", "beta"] {
            let mut note = nt.new_note();
            note.fields_mut()[0] = format!("q-{word}");
            note.fields_mut()[1] = format!("a-{word}");
            col.add_note(&mut note, DeckId(1))?;
        }
        Ok(col)
    }

    #[test]
    fn export_roundtrip_through_merge() -> Result<()> {
        let mut col = two_note_collection()?;
        let base = col.export_notes_for_search("")?;
        assert_eq!(base.len(), 2);

        // ours: edit first note's field; theirs: delete second, add third
        let mut ours = base.clone();
        ours[0].fields[0] = "edited".to_string();
        let mut theirs = base.clone();
        let added = NoteSnapshot {
            guid: "klepci-test-new".to_string(),
            notetype: "Basic".to_string(),
            fields: vec!["q".to_string(), "a".to_string()],
            tags: vec![],
            mtime_secs: 2,
        };
        theirs.push(added);
        let removed_guid = base[1].guid.clone();
        theirs.retain(|snap| snap.guid != removed_guid);

        let merged = klepci_sync::merge_notes(&base, &ours, &theirs);
        assert!(merged.conflicts.is_empty());
        assert_eq!(merged.deleted, vec![removed_guid.clone()]);

        let report = col.import_merged_notes(DeckId(1), &merged.merged, &merged.deleted)?;
        assert_eq!(report.added, 1);
        assert_eq!(report.updated, 1);
        assert_eq!(report.deleted, 1);
        assert!(report.skipped.is_empty());

        let after = col.export_notes_for_search("")?;
        assert_eq!(after.len(), 2);
        assert!(after.iter().any(|snap| snap.fields[0] == "edited"));
        assert!(after.iter().any(|snap| snap.guid == "klepci-test-new"));
        assert!(!after.iter().any(|snap| snap.guid == removed_guid));
        Ok(())
    }

    #[test]
    fn import_skips_schema_drift() -> Result<()> {
        let mut col = two_note_collection()?;
        let mut snaps = col.export_notes_for_search("")?;
        snaps[0].notetype = "Nope".to_string();
        snaps[1].fields.pop();
        let report = col.import_merged_notes(DeckId(1), &snaps, &[])?;
        assert_eq!(report.updated, 0);
        assert_eq!(report.skipped.len(), 2);
        assert!(report
            .skipped
            .iter()
            .any(|skip| skip.reason.contains("notetype")));
        assert!(report
            .skipped
            .iter()
            .any(|skip| skip.reason.contains("field count")));
        assert!(report.skipped.iter().all(|skip| !skip.guid.is_empty()));
        Ok(())
    }

    #[test]
    fn import_unknown_notetype_skipped() -> Result<()> {
        let mut col = two_note_collection()?;
        let report = col.import_merged_notes(
            DeckId(1),
            &[NoteSnapshot {
                guid: "ghost".to_string(),
                notetype: "Ghost".to_string(),
                fields: vec!["q".to_string()],
                tags: vec![],
                mtime_secs: 1,
            }],
            &[],
        )?;
        assert_eq!(report.added, 0);
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].guid, "ghost");
        assert!(report.skipped[0].reason.contains("Ghost"));
        Ok(())
    }

    #[test]
    fn browser_search_and_bulk() -> Result<()> {
        let mut col = two_note_collection()?;
        let rows = col.browser_cards_for_search("", 100)?;
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|row| !row.preview.is_empty()));
        let ids: Vec<i64> = rows.iter().map(|row| row.id).collect();

        col.suspend_cards(&ids[0..1], true)?;
        col.tag_cards(&ids, "klepci", true)?;
        let snaps = col.export_notes_for_search("")?;
        assert!(snaps
            .iter()
            .all(|snap| snap.tags.contains(&"klepci".to_string())));

        col.suspend_cards(&ids[0..1], false)?;
        let removed = col.delete_cards(&ids[0..1])?;
        assert_eq!(removed, 1);
        assert_eq!(col.export_notes_for_search("")?.len(), 1);
        Ok(())
    }

    #[test]
    fn html_to_text_basics() {
        assert_eq!(html_to_text("<div>a<br>b&nbsp;&amp;</div>"), "a\nb &");
    }

    fn file_collection(name: &str) -> Result<Collection> {
        let dir = std::env::temp_dir().join(format!("klepci-apkg-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)?;
        let media = dir.join("media");
        std::fs::create_dir_all(&media)?;
        let mut col = crate::collection::CollectionBuilder::new(dir.join("collection.anki2"))
            .set_media_paths(media, dir.join("media.db"))
            .build()?;
        let nt = col.get_notetype_by_name("Basic")?.unwrap();
        for word in ["alpha", "beta"] {
            let mut note = nt.new_note();
            note.fields_mut()[0] = format!("q-{word}");
            note.fields_mut()[1] = format!("a-{word}");
            col.add_note(&mut note, DeckId(1))?;
        }
        Ok(col)
    }

    fn review_first_card(col: &mut Collection) -> Result<()> {
        let queued = col.get_next_card()?.unwrap();
        let mut answer = crate::scheduler::answering::CardAnswer {
            card_id: queued.card.id(),
            current_state: queued.states.current,
            new_state: queued.states.good,
            rating: crate::scheduler::answering::Rating::Good,
            answered_at: TimestampMillis::now(),
            milliseconds_taken: 2500,
            custom_data: None,
            from_queue: true,
        };
        col.answer_card(&mut answer)?;
        Ok(())
    }

    #[test]
    fn graphs_expose_review_data() -> Result<()> {
        let mut col = two_note_collection()?;
        let empty = col.graphs_for_search("", 0)?;
        let all_cards: u32 = [
            empty.card_counts.new,
            empty.card_counts.learn,
            empty.card_counts.relearn,
            empty.card_counts.young,
            empty.card_counts.mature,
            empty.card_counts.suspended,
            empty.card_counts.buried,
        ]
        .into_iter()
        .sum();
        assert_eq!(all_cards, 2);
        assert_eq!(empty.today.answer_count, 0);

        review_first_card(&mut col)?;
        let graphs = col.graphs_for_search("", 0)?;
        assert_eq!(graphs.today.answer_count, 1);
        assert!(graphs.today.answer_millis >= 2500);
        assert!(graphs.reviews.iter().map(DayReviews::total).sum::<u32>() >= 1);
        assert_eq!(graphs.buttons.len(), 4);
        assert_eq!(graphs.hours.len(), 24);
        assert!(
            graphs.buttons[2].learning + graphs.buttons[2].young + graphs.buttons[2].mature >= 1
        );
        assert!(!graphs.added.is_empty());

        let scoped = col.graphs_for_search("q-alpha", 0)?;
        assert_eq!(scoped.card_counts.new, 0);
        assert_eq!(
            scoped.card_counts.learn
                + scoped.card_counts.young
                + scoped.card_counts.mature
                + scoped.card_counts.relearn,
            1
        );
        Ok(())
    }

    #[test]
    fn find_and_replace_works() -> Result<()> {
        let mut col = two_note_collection()?;
        assert_eq!(
            col.find_and_replace_notes("", "(?i)alpha", "ALPHA", Some("Front"))?,
            1
        );
        assert_eq!(col.browser_cards_for_search("ALPHA", 10)?.len(), 1);
        assert_eq!(
            col.find_and_replace_notes("deck:Default", "q-", "question-", None)?,
            2
        );
        assert_eq!(col.browser_cards_for_search("question-alpha", 10)?.len(), 1);
        Ok(())
    }

    #[test]
    fn deck_options_roundtrip() -> Result<()> {
        let mut col = two_note_collection()?;
        let mut opts = col.deck_options_for_deck(1)?;
        assert_eq!(opts.new_per_day, 20);
        assert_eq!(opts.learn_steps, "1 10");
        assert_eq!(opts.relearn_steps, "10");

        opts.new_per_day = 7;
        opts.learn_steps = "5 25".to_string();
        opts.desired_retention = 0.85;
        opts.fsrs = true;
        col.save_deck_options(&opts)?;

        let opts = col.deck_options_for_deck(1)?;
        assert_eq!(opts.new_per_day, 7);
        assert_eq!(opts.learn_steps, "5 25");
        assert!((opts.desired_retention - 0.85).abs() < 1e-6);
        assert!(opts.fsrs);

        col.undo_last()?;
        let opts = col.deck_options_for_deck(1)?;
        assert_eq!(opts.new_per_day, 20);
        assert_eq!(opts.learn_steps, "1 10");
        Ok(())
    }

    #[test]
    fn apkg_export_import_roundtrip() -> Result<()> {
        let mut src = file_collection("src")?;
        let path =
            std::env::temp_dir().join(format!("klepci-apkg-test-{}.apkg", std::process::id()));
        let path_str = path.to_str().unwrap();
        assert_eq!(src.export_apkg_file(path_str, "")?, 2);

        let mut dst = file_collection("dst")?;
        assert_eq!(dst.import_apkg_file(path_str)?, 2);
        assert_eq!(dst.browser_cards_for_search("", 10)?.len(), 4);
        let _ = std::fs::remove_file(&path);
        Ok(())
    }

    #[test]
    fn reviewer_ops() -> Result<()> {
        let mut col = two_note_collection()?;
        let cid = col.browser_cards_for_search("q-alpha", 10)?[0].id;

        assert!(col.toggle_mark_note(cid)?);
        assert_eq!(col.browser_cards_for_search("tag:marked", 10)?.len(), 1);
        assert!(!col.toggle_mark_note(cid)?);
        assert_eq!(col.browser_cards_for_search("tag:marked", 10)?.len(), 0);

        col.flag_card(cid, 3)?;
        assert_eq!(col.browser_cards_for_search("flag:3", 10)?.len(), 1);
        col.flag_card(cid, 0)?;
        assert_eq!(col.browser_cards_for_search("flag:3", 10)?.len(), 0);

        col.suspend_cards(&[cid], true)?;
        assert_eq!(col.browser_cards_for_search("is:suspended", 10)?.len(), 1);
        col.undo_last()?;
        assert_eq!(col.browser_cards_for_search("is:suspended", 10)?.len(), 0);
        Ok(())
    }

    #[test]
    fn deck_overview_and_crud() -> Result<()> {
        let mut col = two_note_collection()?;
        let child = col.create_deck("Parent::Child")?;
        assert!(child > 0);
        assert!(col.create_deck("Parent::Child").is_err());

        let overview = col.deck_overview()?;
        let parent = overview
            .iter()
            .find(|deck| deck.name == "Parent")
            .expect("parent deck");
        assert!(parent.id > 1, "auto-created parent is not the default deck");
        let child = overview
            .iter()
            .find(|deck| deck.name == "Parent::Child")
            .expect("child deck");
        assert_eq!(child.level, 2);
        assert_eq!(child.total, 0);

        col.rename_deck_by_id(child.id, "Parent::Renamed")?;
        let names: Vec<String> = col.deck_overview()?.into_iter().map(|d| d.name).collect();
        assert!(names.contains(&"Parent::Renamed".to_string()));

        let default = overview
            .iter()
            .find(|deck| deck.id == 1)
            .expect("default deck");
        assert_eq!(default.total, 2);

        assert_eq!(col.delete_deck(child.id)?, 0);
        assert!(!col
            .deck_overview()?
            .iter()
            .any(|deck| deck.name == "Parent::Renamed"));
        Ok(())
    }

    #[test]
    fn editor_load_save_and_add() -> Result<()> {
        let mut col = two_note_collection()?;
        assert_eq!(
            col.field_names_for_notetype("Basic")?,
            vec!["Front".to_string(), "Back".to_string()]
        );

        let rows = col.browser_cards_for_search("q-alpha", 10)?;
        assert_eq!(rows.len(), 1);
        let data = col.note_editor_data(rows[0].id)?;
        assert_eq!(data.notetype_name, "Basic");
        assert_eq!(data.fields[0], "q-alpha");

        col.update_note_fields(
            data.note_id,
            vec!["Q!".to_string(), "A!".to_string()],
            "x y",
        )?;
        let data = col.note_editor_data(rows[0].id)?;
        assert_eq!(data.fields, vec!["Q!".to_string(), "A!".to_string()]);
        assert_eq!(data.tags, "x y");

        let nid =
            col.add_note_to_deck("Basic", 1, vec!["nq".to_string(), "na".to_string()], "new")?;
        assert!(nid > 0);
        assert_eq!(col.browser_cards_for_search("nq", 10)?.len(), 1);
        Ok(())
    }
}
