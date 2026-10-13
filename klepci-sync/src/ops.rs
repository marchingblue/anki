// Copyright: klepci contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Sync operations over guid-keyed note snapshots.

use serde::Deserialize;
use serde::Serialize;

/// Content of one note. `tags` must be sorted/deduped by the caller;
/// [`crate::merge`] normalizes on merge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteSnapshot {
    pub guid: String,
    pub notetype: String,
    pub fields: Vec<String>,
    pub tags: Vec<String>,
    pub mtime_secs: i64,
}

/// One local change, exchanged on push/pull (transport in v0.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoteOp {
    Upsert { snap: NoteSnapshot, actor: String },
    Delete { guid: String, actor: String },
}

/// Apply a single op to an in-memory snapshot set.
/// Returns true if anything changed.
pub fn apply_op(notes: &mut Vec<NoteSnapshot>, graves: &mut Vec<String>, op: NoteOp) -> bool {
    match op {
        NoteOp::Upsert { snap, .. } => {
            graves.retain(|guid| *guid != snap.guid);
            match notes.iter_mut().find(|note| note.guid == snap.guid) {
                Some(note) if *note == snap => false,
                Some(note) => {
                    *note = snap;
                    true
                }
                None => {
                    notes.push(snap);
                    true
                }
            }
        }
        NoteOp::Delete { guid, .. } => {
            let before = notes.len();
            notes.retain(|note| note.guid != guid);
            let removed = notes.len() != before;
            if removed && !graves.contains(&guid) {
                graves.push(guid);
            }
            removed
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn snap(guid: &str) -> NoteSnapshot {
        NoteSnapshot {
            guid: guid.to_string(),
            notetype: "Basic".to_string(),
            fields: vec!["q".to_string(), "a".to_string()],
            tags: vec![],
            mtime_secs: 1,
        }
    }

    #[test]
    fn upsert_adds_updates_and_resurrects() {
        let (mut notes, mut graves) = (vec![], vec!["g1".to_string()]);
        assert!(apply_op(
            &mut notes,
            &mut graves,
            NoteOp::Upsert {
                snap: snap("g1"),
                actor: "a".to_string(),
            }
        ));
        assert!(graves.is_empty());
        // identical upsert is a no-op
        assert!(!apply_op(
            &mut notes,
            &mut graves,
            NoteOp::Upsert {
                snap: snap("g1"),
                actor: "a".to_string(),
            }
        ));
    }

    #[test]
    fn delete_tombstones() {
        let (mut notes, mut graves) = (vec![snap("g1")], vec![]);
        assert!(apply_op(
            &mut notes,
            &mut graves,
            NoteOp::Delete {
                guid: "g1".to_string(),
                actor: "a".to_string(),
            }
        ));
        assert_eq!(graves, vec!["g1".to_string()]);
        // deleting again is a no-op
        assert!(!apply_op(
            &mut notes,
            &mut graves,
            NoteOp::Delete {
                guid: "g1".to_string(),
                actor: "a".to_string(),
            }
        ));
    }
}
