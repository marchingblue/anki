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

impl Collection {
    /// Export notes matching `search` as guid-keyed snapshots, sorted by guid.
    pub(crate) fn export_notes_for_search(&mut self, search: &str) -> Result<Vec<NoteSnapshot>> {
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
    pub(crate) fn import_merged_notes(
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
        for _ in 0..2 {
            let mut note = nt.new_note();
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
}
