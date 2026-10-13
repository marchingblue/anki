// Copyright: klepci contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! 3-way merge of guid-keyed snapshots (base = merge-base).
//!
//! Rules mirror the `.apkg` import semantics (`new/updated/conflicting`),
//! except both-sides-changed is a [`ConflictKind`] instead of silent
//! last-writer-wins:
//!
//! - same guid, same notetype, change on one side only → fast-forward
//! - same guid, both sides changed different fields → merged per field
//! - same guid, both sides changed the same field differently → conflict
//! - same guid, different notetypes, neither equals base → conflict
//!   (like `.apkg` `conflicting`)
//! - delete vs modify → conflict; delete on both sides → tombstone
//! - tags merge by union (v0.1 limitation: tag removals don't propagate;
//!   tag tombstones come with the op-log transport in v0.2)

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use crate::ops::NoteSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Ours,
    Theirs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictKind {
    Field {
        index: usize,
        base: String,
        ours: String,
        theirs: String,
    },
    FieldCount {
        base: usize,
        ours: usize,
        theirs: usize,
    },
    Notetype {
        base: String,
        ours: String,
        theirs: String,
    },
    BothAdded {
        ours: NoteSnapshot,
        theirs: NoteSnapshot,
    },
    DeleteVsModify {
        deleted_by: Side,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub guid: String,
    pub kind: ConflictKind,
}

#[derive(Debug, Default)]
pub struct MergeResult {
    /// Merged live notes (sorted by guid for deterministic output).
    pub merged: Vec<NoteSnapshot>,
    /// Guids deleted on both sides, or whose delete was accepted.
    pub deleted: Vec<String>,
    pub conflicts: Vec<Conflict>,
}

fn union_tags(ours: &[String], theirs: &[String]) -> Vec<String> {
    let mut tags: BTreeSet<&str> = BTreeSet::new();
    tags.extend(ours.iter().map(String::as_str));
    tags.extend(theirs.iter().map(String::as_str));
    tags.into_iter().map(str::to_string).collect()
}

fn three_way(
    guid: &str,
    base: &NoteSnapshot,
    ours: &NoteSnapshot,
    theirs: &NoteSnapshot,
    result: &mut MergeResult,
) {
    // Notetype first: a schema change needs an explicit decision.
    let notetype = if ours.notetype == theirs.notetype {
        ours.notetype.clone()
    } else if ours.notetype == base.notetype {
        theirs.notetype.clone()
    } else if theirs.notetype == base.notetype {
        ours.notetype.clone()
    } else {
        result.conflicts.push(Conflict {
            guid: guid.to_string(),
            kind: ConflictKind::Notetype {
                base: base.notetype.clone(),
                ours: ours.notetype.clone(),
                theirs: theirs.notetype.clone(),
            },
        });
        return;
    };
    if ours.fields.len() != theirs.fields.len() {
        result.conflicts.push(Conflict {
            guid: guid.to_string(),
            kind: ConflictKind::FieldCount {
                base: base.fields.len(),
                ours: ours.fields.len(),
                theirs: theirs.fields.len(),
            },
        });
        return;
    }

    let mut fields = Vec::with_capacity(ours.fields.len());
    for (index, ((base_field, our_field), their_field)) in base
        .fields
        .iter()
        .zip(&ours.fields)
        .zip(&theirs.fields)
        .enumerate()
    {
        if our_field == their_field {
            fields.push(our_field.clone());
        } else if our_field == base_field {
            fields.push(their_field.clone());
        } else if their_field == base_field {
            fields.push(our_field.clone());
        } else {
            result.conflicts.push(Conflict {
                guid: guid.to_string(),
                kind: ConflictKind::Field {
                    index,
                    base: base_field.clone(),
                    ours: our_field.clone(),
                    theirs: their_field.clone(),
                },
            });
            return;
        }
    }
    result.merged.push(NoteSnapshot {
        guid: guid.to_string(),
        notetype,
        fields,
        tags: union_tags(&ours.tags, &theirs.tags),
        mtime_secs: ours.mtime_secs.max(theirs.mtime_secs),
    });
}

/// Merge `ours` and `theirs` against common ancestor `base`.
pub fn merge_notes(
    base: &[NoteSnapshot],
    ours: &[NoteSnapshot],
    theirs: &[NoteSnapshot],
) -> MergeResult {
    let mut result = MergeResult::default();
    fn index(notes: &[NoteSnapshot]) -> BTreeMap<&str, &NoteSnapshot> {
        notes
            .iter()
            .map(|note| (note.guid.as_str(), note))
            .collect()
    }
    let (base_map, our_map, their_map) = (index(base), index(ours), index(theirs));

    let mut guids: BTreeSet<&str> = BTreeSet::new();
    guids.extend(base_map.keys().copied());
    guids.extend(our_map.keys().copied());
    guids.extend(their_map.keys().copied());

    for guid in guids {
        let base = base_map.get(guid).copied();
        let ours = our_map.get(guid).copied();
        let theirs = their_map.get(guid).copied();
        match (ours, theirs) {
            (Some(ours), Some(theirs)) => match base {
                Some(base) => three_way(guid, base, ours, theirs, &mut result),
                None if ours == theirs => result.merged.push((*ours).clone()),
                None => result.conflicts.push(Conflict {
                    guid: guid.to_string(),
                    kind: ConflictKind::BothAdded {
                        ours: (*ours).clone(),
                        theirs: (*theirs).clone(),
                    },
                }),
            },
            (Some(ours), None) => match base {
                // deleted by them: accept only if we didn't touch it
                Some(base) if *ours == *base => result.deleted.push(guid.to_string()),
                Some(_) => result.conflicts.push(Conflict {
                    guid: guid.to_string(),
                    kind: ConflictKind::DeleteVsModify {
                        deleted_by: Side::Theirs,
                    },
                }),
                // added by us
                None => result.merged.push((*ours).clone()),
            },
            (None, Some(theirs)) => match base {
                Some(base) if *theirs == *base => result.deleted.push(guid.to_string()),
                Some(_) => result.conflicts.push(Conflict {
                    guid: guid.to_string(),
                    kind: ConflictKind::DeleteVsModify {
                        deleted_by: Side::Ours,
                    },
                }),
                None => result.merged.push((*theirs).clone()),
            },
            // only in base: deleted on both sides
            (None, None) => result.deleted.push(guid.to_string()),
        }
    }
    result.merged.sort_by(|a, b| a.guid.cmp(&b.guid));
    result.deleted.sort();
    result
}

#[cfg(test)]
mod test {
    use super::*;

    fn snap(guid: &str, fields: &[&str], mtime: i64) -> NoteSnapshot {
        NoteSnapshot {
            guid: guid.to_string(),
            notetype: "Basic".to_string(),
            fields: fields.iter().map(|s| s.to_string()).collect(),
            tags: vec!["t".to_string()],
            mtime_secs: mtime,
        }
    }

    #[test]
    fn fast_forward_one_side() {
        let base = vec![snap("g", &["q", "a"], 1)];
        let ours = vec![snap("g", &["q!", "a"], 2)];
        let res = merge_notes(&base, &ours, &base);
        assert!(res.conflicts.is_empty());
        assert_eq!(res.merged[0].fields, vec!["q!", "a"]);
        assert_eq!(res.merged[0].mtime_secs, 2);
    }

    #[test]
    fn disjoint_field_edits_merge() {
        let base = vec![snap("g", &["q", "a"], 1)];
        let ours = vec![snap("g", &["q!", "a"], 2)];
        let theirs = vec![snap("g", &["q", "a!"], 3)];
        let res = merge_notes(&base, &ours, &theirs);
        assert!(res.conflicts.is_empty());
        assert_eq!(res.merged[0].fields, vec!["q!", "a!"]);
    }

    #[test]
    fn same_field_both_sides_conflicts() {
        let base = vec![snap("g", &["q", "a"], 1)];
        let ours = vec![snap("g", &["q1", "a"], 2)];
        let theirs = vec![snap("g", &["q2", "a"], 3)];
        let res = merge_notes(&base, &ours, &theirs);
        assert_eq!(res.merged.len(), 0);
        assert_eq!(res.conflicts.len(), 1);
        assert!(matches!(
            res.conflicts[0].kind,
            ConflictKind::Field { index: 0, .. }
        ));
    }

    #[test]
    fn notetype_change_needs_decision() {
        let mut theirs = snap("g", &["q", "a"], 3);
        theirs.notetype = "Cloze".to_string();
        let mut ours = snap("g", &["q", "a"], 2);
        ours.notetype = "Basic (2)".to_string();
        let base = vec![snap("g", &["q", "a"], 1)];
        let res = merge_notes(&base, &[ours], &[theirs]);
        assert!(matches!(
            res.conflicts[0].kind,
            ConflictKind::Notetype { .. }
        ));
    }

    #[test]
    fn delete_vs_modify_conflicts_accept_clean_delete() {
        let base = vec![snap("g", &["q", "a"], 1), snap("h", &["q", "a"], 1)];
        let ours = vec![snap("g", &["q!", "a"], 2), snap("h", &["q", "a"], 1)];
        let theirs = vec![];
        let res = merge_notes(&base, &ours, &theirs);
        // g: modified by us, deleted by them -> conflict; h: untouched -> deleted
        assert_eq!(res.conflicts.len(), 1);
        assert!(matches!(
            res.conflicts[0].kind,
            ConflictKind::DeleteVsModify {
                deleted_by: Side::Theirs
            }
        ));
        assert_eq!(res.deleted, vec!["h".to_string()]);
    }
}
