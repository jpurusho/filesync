use std::fs;
use std::io::{self, Write};
use std::path::Path;

use tracing::{error, info};
use uuid::Uuid;

use crate::reconcile::{Action, Side, SyncPlan};

/// Result of applying a sync plan.
#[derive(Debug)]
pub struct ApplyResult {
    pub applied: Vec<AppliedAction>,
    pub errors: Vec<ApplyError>,
}

#[derive(Debug)]
pub struct AppliedAction {
    pub action: Action,
}

#[derive(Debug)]
pub struct ApplyError {
    pub action: Action,
    pub error: String,
}

/// Context for applying actions.
pub struct ApplyContext<'a> {
    pub local_root: &'a Path,
    pub remote_root: &'a Path,
}

impl ApplyContext<'_> {
    fn root_for(&self, side: Side) -> &Path {
        match side {
            Side::Local => self.local_root,
            Side::Remote => self.remote_root,
        }
    }
}

/// Apply a sync plan to the filesystem.
///
/// Per NFR-REL-2: failure on one file does not abort the run.
/// Each action is attempted; failures are recorded and the run continues.
pub fn apply_plan(plan: &SyncPlan, ctx: &ApplyContext<'_>) -> ApplyResult {
    let mut applied = Vec::new();
    let mut errors = Vec::new();

    for action in &plan.actions {
        match execute_action(action, ctx) {
            Ok(()) => {
                applied.push(AppliedAction {
                    action: action.clone(),
                });
            }
            Err(e) => {
                error!("apply error for {action:?}: {e}");
                errors.push(ApplyError {
                    action: action.clone(),
                    error: e.to_string(),
                });
            }
        }
    }

    ApplyResult { applied, errors }
}

fn execute_action(action: &Action, ctx: &ApplyContext<'_>) -> io::Result<()> {
    match action {
        Action::CreateDir { on, path } => {
            let full = ctx.root_for(*on).join(path.to_path_buf());
            fs::create_dir_all(&full)?;
            info!("created dir: {}", full.display());
            Ok(())
        }
        Action::CopyFile { from, path } => {
            let source_root = ctx.root_for(*from);
            let dest_root = ctx.root_for(from.opposite());
            let source_path = source_root.join(path.to_path_buf());
            let dest_path = dest_root.join(path.to_path_buf());
            atomic_copy(&source_path, &dest_path)?;
            info!(
                "copied: {} -> {}",
                source_path.display(),
                dest_path.display()
            );
            Ok(())
        }
        Action::Delete { on, path } => {
            let full = ctx.root_for(*on).join(path.to_path_buf());
            if full.is_dir() {
                fs::remove_dir_all(&full)?;
            } else {
                fs::remove_file(&full)?;
            }
            info!("deleted: {}", full.display());
            Ok(())
        }
        Action::RenameConflict { on, path, new_name } => {
            let root = ctx.root_for(*on);
            let original = root.join(path.to_path_buf());
            let parent = original.parent().unwrap_or(root);
            let renamed = parent.join(new_name);
            if original.exists() {
                fs::rename(&original, &renamed)?;
                info!(
                    "renamed conflict: {} -> {}",
                    original.display(),
                    renamed.display()
                );
            }
            Ok(())
        }
    }
}

/// Atomic file copy: write to temp file on same volume, fsync, rename into place.
/// Ensures an interrupted write never leaves a partially-written file at the target path.
fn atomic_copy(source: &Path, dest: &Path) -> io::Result<()> {
    let dest_dir = dest.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dest_dir)?;

    let tmp_name = format!(".filesync-tmp-{}", Uuid::new_v4());
    let tmp_path = dest_dir.join(&tmp_name);

    // Write to temp file
    {
        let source_file = fs::File::open(source)?;
        let mut reader = io::BufReader::new(source_file);
        let mut tmp_file = fs::File::create(&tmp_path)?;
        io::copy(&mut reader, &mut tmp_file)?;
        tmp_file.flush()?;
        tmp_file.sync_all()?; // fsync
    }

    // Preserve mtime from source
    if let Ok(meta) = fs::metadata(source) {
        if let Ok(mtime) = meta.modified() {
            set_file_mtime(&tmp_path, mtime);
        }
    }

    // Atomic rename
    fs::rename(&tmp_path, dest)?;

    Ok(())
}

fn set_file_mtime(path: &Path, mtime: std::time::SystemTime) {
    // TODO: use filetime crate for cross-platform mtime preservation
    let _ = (path, mtime);
}

trait SideExt {
    fn opposite(self) -> Self;
}

impl SideExt for Side {
    fn opposite(self) -> Self {
        match self {
            Side::Local => Side::Remote,
            Side::Remote => Side::Local,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;
    use crate::path::RelPath;
    use crate::reconcile::SyncPlan;

    #[test]
    fn atomic_copy_produces_correct_file() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("source.txt");
        let dst = tmp.path().join("dest.txt");
        fs::write(&src, "hello world").unwrap();

        atomic_copy(&src, &dst).unwrap();

        assert_eq!(fs::read_to_string(&dst).unwrap(), "hello world");
    }

    #[test]
    fn apply_creates_dirs_and_copies() {
        let local = TempDir::new().unwrap();
        let remote = TempDir::new().unwrap();

        // Create a file on local side
        fs::create_dir_all(local.path().join("sub")).unwrap();
        fs::write(local.path().join("sub/file.txt"), "content").unwrap();

        let plan = SyncPlan {
            actions: vec![
                Action::CreateDir {
                    on: Side::Remote,
                    path: RelPath::new("sub"),
                },
                Action::CopyFile {
                    from: Side::Local,
                    path: RelPath::new("sub/file.txt"),
                },
            ],
            conflicts: vec![],
        };

        let ctx = ApplyContext {
            local_root: local.path(),
            remote_root: remote.path(),
        };

        let result = apply_plan(&plan, &ctx);
        assert!(result.errors.is_empty());
        assert_eq!(result.applied.len(), 2);

        // Verify file exists on remote
        let remote_file = remote.path().join("sub/file.txt");
        assert!(remote_file.exists());
        assert_eq!(fs::read_to_string(&remote_file).unwrap(), "content");
    }

    #[test]
    fn apply_delete_removes_file() {
        let local = TempDir::new().unwrap();
        let remote = TempDir::new().unwrap();

        // Create a file on remote side to be deleted
        fs::write(remote.path().join("old.txt"), "doomed").unwrap();

        let plan = SyncPlan {
            actions: vec![Action::Delete {
                on: Side::Remote,
                path: RelPath::new("old.txt"),
            }],
            conflicts: vec![],
        };

        let ctx = ApplyContext {
            local_root: local.path(),
            remote_root: remote.path(),
        };

        let result = apply_plan(&plan, &ctx);
        assert!(result.errors.is_empty());
        assert!(!remote.path().join("old.txt").exists());
    }

    #[test]
    fn apply_continues_on_error() {
        let local = TempDir::new().unwrap();
        let remote = TempDir::new().unwrap();

        // First action will fail (copy from nonexistent file)
        // Second action should still succeed
        fs::write(local.path().join("good.txt"), "ok").unwrap();

        let plan = SyncPlan {
            actions: vec![
                Action::CopyFile {
                    from: Side::Local,
                    path: RelPath::new("nonexistent.txt"),
                },
                Action::CopyFile {
                    from: Side::Local,
                    path: RelPath::new("good.txt"),
                },
            ],
            conflicts: vec![],
        };

        let ctx = ApplyContext {
            local_root: local.path(),
            remote_root: remote.path(),
        };

        let result = apply_plan(&plan, &ctx);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.applied.len(), 1);
        assert!(remote.path().join("good.txt").exists());
    }
}
