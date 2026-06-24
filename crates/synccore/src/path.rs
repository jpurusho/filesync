use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

/// A relative path normalized for macOS APFS/HFS+ semantics.
///
/// Stores the original display form (preserves casing and Unicode composition)
/// but normalizes to NFD + case-folded for all comparisons and hashing.
/// This ensures the engine never treats the same file as two different paths
/// on a case-insensitive, NFD-normalized filesystem.
#[derive(Clone, Serialize, Deserialize)]
pub struct RelPath {
    /// Original path as seen on disk (for display and filesystem operations)
    display: String,
    /// NFD-normalized, case-folded form (for comparison and map keys)
    normalized: String,
}

impl RelPath {
    pub fn new(path: &str) -> Self {
        let display = path.to_owned();
        let normalized = normalize(path);
        Self {
            display,
            normalized,
        }
    }

    /// Returns true if this relative path is safe (no traversal components).
    /// A safe path has no `..` segments and is not absolute.
    pub fn is_safe(&self) -> bool {
        !self.display.starts_with('/')
            && !self.display.contains("../")
            && !self.display.ends_with("..")
            && self.display != ".."
    }

    /// Join this path onto a root directory, returning None if the result
    /// would escape the root (path traversal attack).
    pub fn safe_resolve(&self, root: &Path) -> Option<PathBuf> {
        if !self.is_safe() {
            return None;
        }
        let joined = root.join(&self.display);
        // Canonicalize both paths and verify containment.
        // If root doesn't exist yet, use lexical check (still safe due to is_safe guard).
        if let (Ok(canonical_root), Ok(canonical_joined)) =
            (root.canonicalize(), joined.canonicalize())
        {
            if canonical_joined.starts_with(&canonical_root) {
                return Some(canonical_joined);
            }
            return None;
        }
        // Fallback: lexical check (root may not exist in tests)
        Some(joined)
    }

    pub fn from_path(path: &Path) -> Self {
        let s = path.to_string_lossy();
        Self::new(&s)
    }

    /// The original path as it appeared on disk (preserves casing).
    pub fn display(&self) -> &str {
        &self.display
    }

    /// The normalized form used for comparison (NFD + lowercase).
    pub fn normalized(&self) -> &str {
        &self.normalized
    }

    /// Convert to a `PathBuf` using the display form (for filesystem ops).
    pub fn to_path_buf(&self) -> PathBuf {
        PathBuf::from(&self.display)
    }

    /// Join a child component onto this path.
    #[must_use]
    pub fn join(&self, child: &str) -> Self {
        let joined = if self.display.is_empty() {
            child.to_owned()
        } else {
            format!("{}/{child}", self.display)
        };
        Self::new(&joined)
    }

    /// Get the parent path, or None if this is a root-level entry.
    #[must_use]
    pub fn parent(&self) -> Option<Self> {
        let p = Path::new(&self.display).parent()?;
        let s = p.to_string_lossy();
        if s.is_empty() {
            return None;
        }
        Some(Self::new(&s))
    }

    /// Get the file stem (name without extension).
    #[must_use]
    pub fn stem(&self) -> Option<&str> {
        Path::new(&self.display)
            .file_stem()
            .and_then(|s| s.to_str())
    }

    /// Get the extension.
    #[must_use]
    pub fn extension(&self) -> Option<&str> {
        Path::new(&self.display)
            .extension()
            .and_then(|s| s.to_str())
    }
}

fn normalize(s: &str) -> String {
    // NFD decomposition then case-fold (lowercase for ASCII+Latin; sufficient for macOS)
    s.nfd().collect::<String>().to_lowercase()
}

impl PartialEq for RelPath {
    fn eq(&self, other: &Self) -> bool {
        self.normalized == other.normalized
    }
}

impl Eq for RelPath {}

impl PartialOrd for RelPath {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RelPath {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.normalized.cmp(&other.normalized)
    }
}

impl std::hash::Hash for RelPath {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.normalized.hash(state);
    }
}

impl fmt::Debug for RelPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RelPath({:?})", self.display)
    }
}

impl fmt::Display for RelPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.display)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_insensitive_equality() {
        let a = RelPath::new("Photos/Trip.JPG");
        let b = RelPath::new("photos/trip.jpg");
        assert_eq!(a, b);
    }

    #[test]
    fn nfd_normalization() {
        // 'é' can be encoded as U+00E9 (NFC) or U+0065 U+0301 (NFD)
        let nfc = RelPath::new("caf\u{00E9}.txt");
        let nfd = RelPath::new("caf\u{0065}\u{0301}.txt");
        assert_eq!(nfc, nfd);
    }

    #[test]
    fn ordering_is_deterministic() {
        let a = RelPath::new("alpha.txt");
        let b = RelPath::new("Beta.txt");
        // 'a' < 'b' in normalized form
        assert!(a < b);
    }

    #[test]
    fn preserves_display_form() {
        let p = RelPath::new("MyFolder/README.md");
        assert_eq!(p.display(), "MyFolder/README.md");
    }

    #[test]
    fn parent_and_join() {
        let p = RelPath::new("a/b/c.txt");
        let parent = p.parent().unwrap();
        assert_eq!(parent.display(), "a/b");
        let joined = parent.join("d.txt");
        assert_eq!(joined.display(), "a/b/d.txt");
    }

    #[test]
    fn root_level_has_no_parent() {
        let p = RelPath::new("file.txt");
        assert!(p.parent().is_none());
    }

    #[test]
    fn is_safe_rejects_traversal() {
        assert!(!RelPath::new("../etc/passwd").is_safe());
        assert!(!RelPath::new("foo/../../etc/passwd").is_safe());
        assert!(!RelPath::new("..").is_safe());
        assert!(!RelPath::new("/absolute/path").is_safe());
    }

    #[test]
    fn is_safe_accepts_normal_paths() {
        assert!(RelPath::new("file.txt").is_safe());
        assert!(RelPath::new("dir/subdir/file.txt").is_safe());
        assert!(RelPath::new(".hidden/file").is_safe());
        assert!(RelPath::new("a...b/c").is_safe());
    }

    #[test]
    fn safe_resolve_rejects_escape() {
        let root = Path::new("/tmp/anchor");
        assert!(RelPath::new("../escape").safe_resolve(root).is_none());
        assert!(RelPath::new("foo/../../escape").safe_resolve(root).is_none());
    }
}
