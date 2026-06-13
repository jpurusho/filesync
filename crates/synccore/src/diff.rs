use std::collections::BTreeMap;
use std::io;
use std::path::Path;

use crate::path::RelPath;
use crate::scan::{self, EntryKind, FileEntry, Snapshot};

/// The last-synced state of a file, as recorded in the sync index.
#[derive(Debug, Clone)]
pub struct IndexEntry {
    pub path: RelPath,
    pub kind: EntryKind,
    pub size: u64,
    pub mtime_secs: i64,
    pub hash: String,
}

/// A sync index: the last-known-synced state of all paths for a profile.
#[derive(Debug, Clone, Default)]
pub struct SyncIndex {
    pub entries: BTreeMap<RelPath, IndexEntry>,
}

/// What changed on one side relative to the sync index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Created(RelPath),
    Modified(RelPath),
    Deleted(RelPath),
}

impl Change {
    pub fn path(&self) -> &RelPath {
        match self {
            Self::Created(p) | Self::Modified(p) | Self::Deleted(p) => p,
        }
    }
}

/// The diff result for both sides.
#[derive(Debug, Clone)]
pub struct DiffResult {
    pub local: Vec<Change>,
    pub remote: Vec<Change>,
}

/// Compute the diff between a snapshot and the sync index for one side.
/// `root` is the filesystem root for this side (needed to hash files on demand).
///
/// Returns the list of changes (Created, Modified, Deleted) detected on this side.
pub fn diff_side(snapshot: &Snapshot, index: &SyncIndex, root: &Path) -> io::Result<Vec<Change>> {
    let mut changes = Vec::new();

    // Check each entry in the snapshot against the index
    for (rel_path, entry) in &snapshot.entries {
        // Only diff files (dirs are implicit from file paths)
        if entry.kind != EntryKind::File {
            continue;
        }

        match index.entries.get(rel_path) {
            None => {
                // Path not in index → new file
                changes.push(Change::Created(rel_path.clone()));
            }
            Some(idx_entry) => {
                // Check if modified: size/mtime shortcut first
                if entry.size != idx_entry.size {
                    changes.push(Change::Modified(rel_path.clone()));
                } else if has_mtime_changed(entry, idx_entry) {
                    // Size same but mtime differs → confirm with hash
                    let full_path = root.join(entry.path.to_path_buf());
                    let current_hash = scan::hash_file(&full_path)?;
                    if current_hash != idx_entry.hash {
                        changes.push(Change::Modified(rel_path.clone()));
                    }
                }
                // else: size and mtime match → unchanged
            }
        }
    }

    // Check for deletions: paths in index but not in snapshot
    for rel_path in index.entries.keys() {
        if index.entries[rel_path].kind != EntryKind::File {
            continue;
        }
        if !snapshot.entries.contains_key(rel_path) {
            changes.push(Change::Deleted(rel_path.clone()));
        }
    }

    Ok(changes)
}

/// Compute the diff for both sides against the shared index.
pub fn compute_diff(
    local_snapshot: &Snapshot,
    remote_snapshot: &Snapshot,
    index: &SyncIndex,
    local_root: &Path,
    remote_root: &Path,
) -> io::Result<DiffResult> {
    let local = diff_side(local_snapshot, index, local_root)?;
    let remote = diff_side(remote_snapshot, index, remote_root)?;
    Ok(DiffResult { local, remote })
}

fn has_mtime_changed(entry: &FileEntry, idx_entry: &IndexEntry) -> bool {
    let entry_secs = entry
        .mtime
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    entry_secs != idx_entry.mtime_secs
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::*;

    fn make_snapshot(files: &[(&str, u64, i64)]) -> Snapshot {
        let mut entries = BTreeMap::new();
        for &(path, size, mtime_secs) in files {
            let rel = RelPath::new(path);
            entries.insert(
                rel.clone(),
                FileEntry {
                    path: rel,
                    kind: EntryKind::File,
                    size,
                    mtime: UNIX_EPOCH + Duration::from_secs(mtime_secs as u64),
                    hash: None,
                },
            );
        }
        Snapshot { entries }
    }

    fn make_index(files: &[(&str, u64, i64, &str)]) -> SyncIndex {
        let mut entries = BTreeMap::new();
        for &(path, size, mtime_secs, hash) in files {
            let rel = RelPath::new(path);
            entries.insert(
                rel.clone(),
                IndexEntry {
                    path: rel,
                    kind: EntryKind::File,
                    size,
                    mtime_secs,
                    hash: hash.to_owned(),
                },
            );
        }
        SyncIndex { entries }
    }

    #[test]
    fn detects_new_file() {
        let snap = make_snapshot(&[("new.txt", 100, 1000)]);
        let index = SyncIndex::default();

        let changes = diff_side(&snap, &index, Path::new("/unused")).unwrap();
        assert_eq!(changes.len(), 1);
        assert!(matches!(&changes[0], Change::Created(p) if p.display() == "new.txt"));
    }

    #[test]
    fn detects_deletion() {
        let snap = make_snapshot(&[]);
        let index = make_index(&[("gone.txt", 50, 500, "abc123")]);

        let changes = diff_side(&snap, &index, Path::new("/unused")).unwrap();
        assert_eq!(changes.len(), 1);
        assert!(matches!(&changes[0], Change::Deleted(p) if p.display() == "gone.txt"));
    }

    #[test]
    fn detects_size_change() {
        let snap = make_snapshot(&[("file.txt", 200, 1000)]);
        let index = make_index(&[("file.txt", 100, 1000, "hash_old")]);

        let changes = diff_side(&snap, &index, Path::new("/unused")).unwrap();
        assert_eq!(changes.len(), 1);
        assert!(matches!(&changes[0], Change::Modified(p) if p.display() == "file.txt"));
    }

    #[test]
    fn unchanged_file_no_diff() {
        let snap = make_snapshot(&[("same.txt", 100, 1000)]);
        let index = make_index(&[("same.txt", 100, 1000, "hash_same")]);

        let changes = diff_side(&snap, &index, Path::new("/unused")).unwrap();
        assert!(changes.is_empty());
    }

    #[test]
    fn empty_index_treats_all_as_created() {
        let snap = make_snapshot(&[("a.txt", 10, 100), ("b.txt", 20, 200)]);
        let index = SyncIndex::default();

        let changes = diff_side(&snap, &index, Path::new("/unused")).unwrap();
        assert_eq!(changes.len(), 2);
        assert!(changes.iter().all(|c| matches!(c, Change::Created(_))));
    }
}
