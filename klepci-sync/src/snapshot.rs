// Copyright: klepci contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Guid-keyed snapshot layout:
//!
//! ```text
//! <dir>/notes/<percent-encoded guid>.json
//! <dir>/graves.json          # sorted tombstone guids
//! <dir>/manifest.json        # {"version": 1}
//! ```
//!
//! Anki guids are base91 and may contain `/`, so guids are percent-encoded
//! for file names (only `[A-Za-z0-9_-]` pass through literally).

use std::fs;
use std::io;
use std::path::Path;

use crate::ops::NoteSnapshot;

const MANIFEST_VERSION: u32 = 1;

fn encode_guid(guid: &str) -> String {
    let mut out = String::with_capacity(guid.len());
    for byte in guid.bytes() {
        if byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_' {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn decode_guid(encoded: &str) -> Option<String> {
    let bytes = encoded.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = encoded.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Write `notes` + `graves` to `dir`, replacing previous content.
/// JSON is compact; notes are independent files so git diffs stay small.
pub fn write_snapshot(dir: &Path, notes: &[NoteSnapshot], graves: &[String]) -> io::Result<()> {
    let notes_dir = dir.join("notes");
    if dir.exists() {
        fs::remove_dir_all(dir)?;
    }
    fs::create_dir_all(&notes_dir)?;
    for note in notes {
        let path = notes_dir.join(format!("{}.json", encode_guid(&note.guid)));
        fs::write(path, serde_json::to_string(note).map_err(io::Error::other)?)?;
    }
    let mut graves = graves.to_vec();
    graves.sort();
    graves.dedup();
    fs::write(
        dir.join("graves.json"),
        serde_json::to_string(&graves).map_err(io::Error::other)?,
    )?;
    fs::write(
        dir.join("manifest.json"),
        format!("{{\"version\":{MANIFEST_VERSION}}}"),
    )?;
    Ok(())
}

/// Read a snapshot back. Returns `(notes sorted by guid, graves)`.
pub fn read_snapshot(dir: &Path) -> io::Result<(Vec<NoteSnapshot>, Vec<String>)> {
    let manifest = fs::read_to_string(dir.join("manifest.json"))
        .map_err(|_| io::Error::other("no manifest"))?;
    if !manifest.contains(&MANIFEST_VERSION.to_string()) {
        return Err(io::Error::other("unsupported snapshot version"));
    }
    let mut notes = Vec::new();
    for entry in fs::read_dir(dir.join("notes"))? {
        let path = entry?.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let note: NoteSnapshot = serde_json::from_slice(&fs::read(&path)?)
            .map_err(|err| io::Error::other(format!("{}: {err}", path.display())))?;
        // file name must round-trip to the guid inside
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("");
        if decode_guid(stem).as_deref() != Some(note.guid.as_str()) {
            return Err(io::Error::other(format!(
                "guid mismatch in {}",
                path.display()
            )));
        }
        notes.push(note);
    }
    notes.sort_by(|a, b| a.guid.cmp(&b.guid));
    let graves: Vec<String> =
        serde_json::from_slice(&fs::read(dir.join("graves.json"))?).map_err(io::Error::other)?;
    Ok((notes, graves))
}

#[cfg(test)]
mod test {
    use super::*;

    fn snap(guid: &str) -> NoteSnapshot {
        NoteSnapshot {
            guid: guid.to_string(),
            notetype: "Basic".to_string(),
            fields: vec!["q".to_string()],
            tags: vec![],
            mtime_secs: 1,
        }
    }

    #[test]
    fn guid_filename_roundtrip() {
        // base91 guids may contain '/' and other unsafe chars
        for guid in ["abcXYZ019", "a/b?c=d", "x%y", "ząb"] {
            assert_eq!(decode_guid(&encode_guid(guid)).as_deref(), Some(guid));
        }
    }

    #[test]
    fn write_read_roundtrip() {
        let dir = std::env::temp_dir().join("klepci-snapshot-test");
        let notes = vec![snap("g/1"), snap("g2")];
        write_snapshot(&dir, &notes, &["gone".to_string(), "gone".to_string()]).unwrap();
        let (back, graves) = read_snapshot(&dir).unwrap();
        assert_eq!(back, {
            let mut sorted = notes.clone();
            sorted.sort_by(|a, b| a.guid.cmp(&b.guid));
            sorted
        });
        assert_eq!(graves, vec!["gone".to_string()]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_dir_errors() {
        assert!(read_snapshot(Path::new("/nonexistent-klepci-dir")).is_err());
    }
}
