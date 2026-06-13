use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;
use std::time::SystemTime;

use blake3::Hasher;
use tracing::warn;
use walkdir::WalkDir;

use crate::path::RelPath;

/// Configuration for scanning a single anchor.
#[derive(Debug, Clone)]
pub struct ScanConfig {
    /// Maximum recursion depth. -1 means unlimited.
    pub max_depth: i32,
    /// Whether to include hidden files (dotfiles on Unix).
    pub include_hidden: bool,
    /// Glob patterns to ignore (simple suffix/prefix matching for now).
    pub ignore_patterns: Vec<String>,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            max_depth: -1,
            include_hidden: false,
            ignore_patterns: vec![],
        }
    }
}

/// Kind of filesystem entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EntryKind {
    File,
    Dir,
}

/// A single filesystem entry captured during a scan.
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: RelPath,
    pub kind: EntryKind,
    pub size: u64,
    pub mtime: SystemTime,
    pub hash: Option<String>,
}

/// The result of scanning a directory tree — a snapshot of all entries.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub entries: BTreeMap<RelPath, FileEntry>,
}

/// Scan a directory tree rooted at `root` and produce a `Snapshot`.
///
/// Only files and directories are included. Symlinks and special files are skipped.
/// Hashes are NOT computed during scan — they are computed lazily during diff
/// when needed (size+mtime shortcut, per FR-SE-4).
pub fn scan_tree(root: &Path, config: &ScanConfig) -> io::Result<Snapshot> {
    if !root.exists() {
        return Ok(Snapshot {
            entries: BTreeMap::new(),
        });
    }

    let walker_depth = if config.max_depth < 0 {
        usize::MAX
    } else {
        // +1 because walkdir counts root as depth 0
        (config.max_depth as usize).saturating_add(1)
    };

    let walker = WalkDir::new(root)
        .max_depth(walker_depth)
        .follow_links(false);

    let mut entries = BTreeMap::new();

    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                warn!("scan error: {e}");
                continue;
            }
        };

        // Skip the root itself
        if entry.path() == root {
            continue;
        }

        let rel = entry
            .path()
            .strip_prefix(root)
            .expect("entry is under root");
        let rel_str = rel.to_string_lossy().to_string();

        // Skip hidden files if not included
        if !config.include_hidden && is_hidden(&rel_str) {
            continue;
        }

        // Skip ignored patterns
        if matches_ignore(&rel_str, &config.ignore_patterns) {
            continue;
        }

        let file_type = entry.file_type();

        // Skip symlinks and special files
        if file_type.is_symlink() {
            continue;
        }

        let kind = if file_type.is_dir() {
            EntryKind::Dir
        } else if file_type.is_file() {
            EntryKind::File
        } else {
            continue; // skip special files
        };

        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(e) => {
                warn!("metadata error for {}: {e}", entry.path().display());
                continue;
            }
        };

        let size = metadata.len();
        let mtime = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);

        let rel_path = RelPath::new(&rel_str);
        entries.insert(
            rel_path.clone(),
            FileEntry {
                path: rel_path,
                kind,
                size,
                mtime,
                hash: None, // lazy — computed during diff
            },
        );
    }

    Ok(Snapshot { entries })
}

/// Compute the BLAKE3 hash of a file.
pub fn hash_file(path: &Path) -> io::Result<String> {
    let mut hasher = Hasher::new();
    let mut file = fs::File::open(path)?;
    io::copy(&mut file, &mut hasher)?;
    Ok(hasher.finalize().to_hex().to_string())
}

fn is_hidden(rel_path: &str) -> bool {
    rel_path
        .split('/')
        .any(|component| component.starts_with('.'))
}

fn matches_ignore(rel_path: &str, patterns: &[String]) -> bool {
    let filename = rel_path.rsplit('/').next().unwrap_or(rel_path);
    patterns.iter().any(|pat| match_glob(filename, pat))
}

/// Simple glob matching: supports `*` prefix (e.g. `*.tmp`) and `*` suffix (e.g. `~$*`).
/// Also supports exact match.
fn match_glob(name: &str, pattern: &str) -> bool {
    if let Some(suffix) = pattern.strip_prefix('*') {
        name.ends_with(suffix)
    } else if let Some(prefix) = pattern.strip_suffix('*') {
        name.starts_with(prefix)
    } else {
        name == pattern
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn make_tree(dir: &Path) {
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::write(dir.join("hello.txt"), "hello").unwrap();
        fs::write(dir.join("sub/nested.txt"), "nested").unwrap();
        fs::write(dir.join(".hidden"), "secret").unwrap();
        fs::write(dir.join("sub/.also_hidden"), "also secret").unwrap();
        fs::write(dir.join("temp.tmp"), "temporary").unwrap();
    }

    #[test]
    fn scan_excludes_hidden_by_default() {
        let tmp = TempDir::new().unwrap();
        make_tree(tmp.path());

        let snap = scan_tree(tmp.path(), &ScanConfig::default()).unwrap();
        let paths: Vec<_> = snap
            .entries
            .values()
            .filter(|e| e.kind == EntryKind::File)
            .map(|e| e.path.display().to_string())
            .collect();

        assert!(paths.contains(&"hello.txt".to_string()));
        assert!(paths.contains(&"sub/nested.txt".to_string()));
        assert!(paths.contains(&"temp.tmp".to_string()));
        assert!(!paths.contains(&".hidden".to_string()));
        assert!(!paths.contains(&"sub/.also_hidden".to_string()));
    }

    #[test]
    fn scan_includes_hidden_when_configured() {
        let tmp = TempDir::new().unwrap();
        make_tree(tmp.path());

        let config = ScanConfig {
            include_hidden: true,
            ..Default::default()
        };
        let snap = scan_tree(tmp.path(), &config).unwrap();
        let paths: Vec<_> = snap
            .entries
            .values()
            .filter(|e| e.kind == EntryKind::File)
            .map(|e| e.path.display().to_string())
            .collect();

        assert!(paths.contains(&".hidden".to_string()));
        assert!(paths.contains(&"sub/.also_hidden".to_string()));
    }

    #[test]
    fn scan_respects_depth_limit() {
        let tmp = TempDir::new().unwrap();
        make_tree(tmp.path());

        let config = ScanConfig {
            max_depth: 0,
            ..Default::default()
        };
        let snap = scan_tree(tmp.path(), &config).unwrap();
        let paths: Vec<_> = snap
            .entries
            .keys()
            .map(|p| p.display().to_string())
            .collect();

        // depth 0 = only immediate children of root
        assert!(paths.contains(&"hello.txt".to_string()));
        assert!(paths.contains(&"temp.tmp".to_string()));
        // sub dir appears (it's at depth 1 from walkdir's perspective which is depth 0 for us)
        // but nested.txt inside sub should NOT appear
        assert!(!paths.contains(&"sub/nested.txt".to_string()));
    }

    #[test]
    fn scan_applies_ignore_patterns() {
        let tmp = TempDir::new().unwrap();
        make_tree(tmp.path());

        let config = ScanConfig {
            ignore_patterns: vec!["*.tmp".to_string()],
            ..Default::default()
        };
        let snap = scan_tree(tmp.path(), &config).unwrap();
        let paths: Vec<_> = snap
            .entries
            .values()
            .filter(|e| e.kind == EntryKind::File)
            .map(|e| e.path.display().to_string())
            .collect();

        assert!(!paths.contains(&"temp.tmp".to_string()));
        assert!(paths.contains(&"hello.txt".to_string()));
    }

    #[test]
    fn hash_file_works() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("test.bin");
        fs::write(&p, b"hello world").unwrap();

        let h = hash_file(&p).unwrap();
        // BLAKE3 hash of "hello world" is known
        assert!(!h.is_empty());
        assert_eq!(h.len(), 64); // hex-encoded 256-bit hash
    }

    #[test]
    fn scan_nonexistent_returns_empty() {
        let snap = scan_tree(
            Path::new("/nonexistent/path/that/does/not/exist"),
            &ScanConfig::default(),
        )
        .unwrap();
        assert!(snap.entries.is_empty());
    }
}
