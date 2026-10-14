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

    #[test]
    fn editor_load_save_and_add() -> Result<()> {
        let mut col = two_note_collection()?;
        assert!(col.notetype_names()?.contains(&"Basic".to_string()));
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
