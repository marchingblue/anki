// Copyright: klepci contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Local git operations for snapshot repos, via `gix` (pure Rust, no
//! system git — this must also run on mobile later).
//!
//! Strategy: the tree is rebuilt purely from the workdir on every commit
//! (no index involved), so deletions are automatic and the result is
//! deterministic. A commit happens only when the tree differs from HEAD.
//!
//! v0.2 scope: init/open/commit/merge-base/log. Push/fetch/pull (network
//! protocol) is v0.3.

use std::ffi::OsStr;
use std::fmt::Display;
use std::fs;
use std::io;
use std::path::Path;

use gix::objs::bstr::BStr;
use gix::objs::bstr::BString;

/// Author of klepci commits.
#[derive(Debug, Clone)]
pub struct Author {
    pub name: String,
    pub email: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitError(pub String);

impl Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for GitError {}

impl From<io::Error> for GitError {
    fn from(err: io::Error) -> Self {
        GitError(err.to_string())
    }
}

fn git_err<E: Display>(err: E) -> GitError {
    GitError(err.to_string())
}

/// A snapshot repo: `<path>` is the workdir, `<path>/.git` the store.
pub struct SnapshotRepo {
    repo: gix::Repository,
}

impl SnapshotRepo {
    fn workdir(&self) -> Result<&Path, GitError> {
        self.repo
            .workdir()
            .ok_or_else(|| GitError("bare repo".to_string()))
    }
}

/// Open the repo at `path`, or init a new one if there is none.
pub fn open_or_init(path: &Path) -> Result<SnapshotRepo, GitError> {
    let repo = match gix::open(path) {
        Ok(repo) => repo,
        Err(_) => gix::init(path).map_err(git_err)?,
    };
    Ok(SnapshotRepo { repo })
}

/// Hex of the HEAD commit, or `None` on a fresh repo.
pub fn head_id(repo: &SnapshotRepo) -> Result<Option<String>, GitError> {
    Ok(repo
        .repo
        .head()
        .map_err(git_err)?
        .id()
        .map(|id| id.detach().to_hex().to_string()))
}

fn head_tree_id(repo: &SnapshotRepo) -> Result<Option<gix::ObjectId>, GitError> {
    if repo.repo.head().map_err(git_err)?.is_unborn() {
        return Ok(None);
    }
    Ok(Some(
        repo.repo
            .rev_parse_single(BStr::new("HEAD^{tree}"))
            .map_err(git_err)?
            .detach(),
    ))
}

fn collect_files(
    dir: &Path,
    base: &Path,
    out: &mut Vec<(String, Vec<u8>)>,
) -> Result<(), GitError> {
    let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        if path.file_name() == Some(OsStr::new(".git")) {
            continue;
        }
        if path.is_dir() {
            collect_files(&path, base, out)?;
        } else {
            let rel = path
                .strip_prefix(base)
                .map_err(|err| GitError(err.to_string()))?;
            let name = rel
                .to_str()
                .ok_or_else(|| GitError(format!("non-UTF8 path: {}", rel.display())))?;
            out.push((name.replace('\\', "/"), fs::read(&path)?));
        }
    }
    Ok(())
}

fn write_tree_at(
    repo: &gix::Repository,
    prefix: &str,
    files: &[(String, Vec<u8>)],
) -> Result<gix::ObjectId, GitError> {
    use std::collections::BTreeMap;
    let mut blobs: Vec<(&str, &[u8])> = Vec::new();
    let mut subdirs: BTreeMap<&str, Vec<(&str, &[u8])>> = BTreeMap::new();
    for (path, bytes) in files {
        let rest = &path[prefix.len()..];
        match rest.find('/') {
            None => blobs.push((rest, bytes)),
            Some(slash) => subdirs
                .entry(&rest[..slash])
                .or_default()
                .push((&rest[slash + 1..], bytes)),
        }
    }
    let blob_mode = gix::objs::tree::EntryMode::try_from(0o100644)
        .map_err(|mode| GitError(mode.to_string()))?;
    let tree_mode = gix::objs::tree::EntryMode::try_from(0o040000)
        .map_err(|mode| GitError(mode.to_string()))?;
    let mut entries: Vec<gix::objs::tree::Entry> = Vec::new();
    for (name, bytes) in blobs {
        let oid = repo.write_blob(bytes).map_err(git_err)?.detach();
        entries.push(gix::objs::tree::Entry {
            mode: blob_mode,
            filename: BString::from(name),
            oid,
        });
    }
    for (name, nested) in &subdirs {
        // rebuild full relative paths for the recursion
        let nested_full: Vec<(String, Vec<u8>)> = nested
            .iter()
            .map(|(rest, bytes)| (format!("{prefix}{name}/{rest}"), (*bytes).to_vec()))
            .collect();
        let oid = write_tree_at(repo, &format!("{prefix}{name}/"), &nested_full)?;
        entries.push(gix::objs::tree::Entry {
            mode: tree_mode,
            filename: BString::from(*name),
            oid,
        });
    }
    entries.sort();
    let tree = gix::objs::Tree { entries };
    Ok(repo.write_object(&tree).map_err(git_err)?.detach())
}

/// Hash every file under the workdir (minus `.git`) into a tree.
/// Deterministic: same content always yields the same tree.
pub fn write_workdir_tree(repo: &SnapshotRepo) -> Result<String, GitError> {
    let workdir = repo.workdir()?.to_path_buf();
    let mut files = Vec::new();
    collect_files(&workdir, &workdir, &mut files)?;
    let refs: Vec<(String, Vec<u8>)> = files;
    Ok(write_tree_at(&repo.repo, "", &refs)?.to_hex().to_string())
}

fn parse_id(repo: &SnapshotRepo, hex: &str) -> Result<gix::ObjectId, GitError> {
    Ok(repo
        .repo
        .rev_parse_single(BStr::new(hex))
        .map_err(git_err)?
        .detach())
}

/// Create a commit for `tree_hex` on `refname` (e.g. `HEAD`).
/// Returns the new commit hex.
pub fn commit_tree(
    repo: &SnapshotRepo,
    refname: &str,
    message: &str,
    author: &Author,
    tree_hex: &str,
    parents: &[String],
) -> Result<String, GitError> {
    let tree = parse_id(repo, tree_hex)?;
    let mut parent_ids = Vec::with_capacity(parents.len());
    for parent in parents {
        parent_ids.push(parse_id(repo, parent)?);
    }
    let time = gix::date::Time::now_local_or_utc().to_string();
    let signature = gix::actor::SignatureRef {
        name: BStr::new(&author.name),
        email: BStr::new(&author.email),
        time: &time,
    };
    Ok(repo
        .repo
        .commit_as(signature, signature, refname, message, tree, parent_ids)
        .map_err(git_err)?
        .detach()
        .to_hex()
        .to_string())
}

/// Commit the current workdir state on HEAD if it differs.
/// Returns the new head hex, or `None` when nothing changed.
pub fn commit_snapshot(
    repo: &SnapshotRepo,
    message: &str,
    author: &Author,
) -> Result<Option<String>, GitError> {
    let tree_hex = write_workdir_tree(repo)?;
    let head_tree = head_tree_id(repo)?.map(|id| id.to_hex().to_string());
    if head_tree.as_deref() == Some(tree_hex.as_str()) {
        return Ok(None);
    }
    let parents = head_id(repo)?.into_iter().collect::<Vec<_>>();
    Ok(Some(commit_tree(
        repo, "HEAD", message, author, &tree_hex, &parents,
    )?))
}

/// Best merge-base of two commits (hex), or `None` if unrelated.
pub fn merge_base_hex(
    repo: &SnapshotRepo,
    ours_hex: &str,
    theirs_hex: &str,
) -> Result<Option<String>, GitError> {
    let ours = parse_id(repo, ours_hex)?;
    let theirs = parse_id(repo, theirs_hex)?;
    Ok(repo
        .repo
        .merge_base(ours, theirs)
        .map_err(git_err)?
        .map(|id| id.detach().to_hex().to_string()))
}

/// One log line, newest first, following first parents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    pub hex: String,
    pub message: String,
}

pub fn log(repo: &SnapshotRepo, max: usize) -> Result<Vec<LogEntry>, GitError> {
    let mut out = Vec::new();
    let mut next = head_id(repo)?;
    while let Some(hex) = next {
        if out.len() >= max {
            break;
        }
        let id = parse_id(repo, &hex)?;
        let commit = repo
            .repo
            .find_object(id)
            .map_err(git_err)?
            .try_into_commit()
            .map_err(git_err)?;
        out.push(LogEntry {
            hex: hex.clone(),
            message: commit.message_raw_sloppy().to_string(),
        });
        next = commit
            .parent_ids()
            .next()
            .map(|parent| parent.detach().to_hex().to_string());
    }
    Ok(out)
}

#[cfg(test)]
mod test {
    use super::*;

    fn author() -> Author {
        Author {
            name: "klepci-test".to_string(),
            email: "test@example.com".to_string(),
        }
    }

    fn fresh_repo(name: &str) -> (SnapshotRepo, tempfile_path::TempPath) {
        let dir = std::env::temp_dir().join(format!(
            "klepci-git-{name}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let repo = open_or_init(&dir).unwrap();
        let path = tempfile_path::TempPath(dir);
        (repo, path)
    }

    /// Minimal tempdir guard without a new dependency.
    mod tempfile_path {
        use std::path::PathBuf;
        pub struct TempPath(pub PathBuf);
        impl Drop for TempPath {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    #[test]
    fn init_commit_noop() {
        let (repo, _guard) = fresh_repo("noop");
        assert_eq!(head_id(&repo).unwrap(), None);
        fs::write(repo.workdir().unwrap().join("f.txt"), "v1").unwrap();
        let head = commit_snapshot(&repo, "first", &author())
            .unwrap()
            .expect("should commit");
        assert_eq!(head_id(&repo).unwrap().as_deref(), Some(head.as_str()));
        assert_eq!(commit_snapshot(&repo, "noop", &author()).unwrap(), None);
    }

    #[test]
    fn modify_recommit_and_log() {
        let (repo, _guard) = fresh_repo("log");
        fs::write(repo.workdir().unwrap().join("f.txt"), "v1").unwrap();
        let first = commit_snapshot(&repo, "first", &author()).unwrap().unwrap();
        fs::write(repo.workdir().unwrap().join("f.txt"), "v2").unwrap();
        let second = commit_snapshot(&repo, "second", &author())
            .unwrap()
            .unwrap();
        assert_ne!(first, second);
        assert_eq!(
            merge_base_hex(&repo, &first, &second).unwrap().as_deref(),
            Some(first.as_str())
        );
        let entries = log(&repo, 10).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].hex, second);
        assert!(entries[0].message.contains("second"));
    }

    #[test]
    fn divergent_branches_share_base() {
        let (repo, _guard) = fresh_repo("diverge");
        fs::write(repo.workdir().unwrap().join("f.txt"), "base").unwrap();
        let base = commit_snapshot(&repo, "base", &author()).unwrap().unwrap();
        fs::write(repo.workdir().unwrap().join("f.txt"), "ours").unwrap();
        let ours = commit_snapshot(&repo, "ours", &author()).unwrap().unwrap();
        // their side: same base content, committed on another ref
        fs::write(repo.workdir().unwrap().join("f.txt"), "base").unwrap();
        let tree = write_workdir_tree(&repo).unwrap();
        let theirs = commit_tree(
            &repo,
            "refs/heads/other",
            "theirs",
            &author(),
            &tree,
            &[base.clone()],
        )
        .unwrap();
        // restore HEAD content: no new commit expected
        fs::write(repo.workdir().unwrap().join("f.txt"), "ours").unwrap();
        assert_eq!(commit_snapshot(&repo, "noop", &author()).unwrap(), None);
        assert_eq!(
            merge_base_hex(&repo, &ours, &theirs).unwrap().as_deref(),
            Some(base.as_str())
        );
    }

    #[test]
    fn deletions_commit() {
        let (repo, _guard) = fresh_repo("del");
        let workdir = repo.workdir().unwrap().to_path_buf();
        fs::write(workdir.join("gone.txt"), "x").unwrap();
        commit_snapshot(&repo, "add", &author()).unwrap().unwrap();
        fs::remove_file(workdir.join("gone.txt")).unwrap();
        assert!(commit_snapshot(&repo, "remove", &author())
            .unwrap()
            .is_some());
        assert_eq!(commit_snapshot(&repo, "noop", &author()).unwrap(), None);
    }
}
