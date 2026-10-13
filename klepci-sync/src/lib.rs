// Copyright: klepci contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Operation log, guid-keyed snapshots and 3-way merge for klepci
//! collaboration.
//!
//! Identity is the Anki note `guid` (stable across export/import); numeric
//! ids are never used as keys. Scheduling state (`cards`, `revlog`) is out
//! of scope — only note content, notetypes, tags and tombstones sync.
//!
//! v0.1: pure data + merge. Git transport (`gix`) lands in v0.2.

pub mod git;
pub mod merge;
pub mod ops;
pub mod snapshot;

pub use git::{commit_snapshot, commit_tree, head_id, log, merge_base_hex, open_or_init};
pub use git::{Author, LogEntry, SnapshotRepo};
pub use merge::{merge_notes, Conflict, ConflictKind, MergeResult, Side};
pub use ops::{apply_op, NoteOp, NoteSnapshot};
