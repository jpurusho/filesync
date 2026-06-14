use std::collections::{BTreeMap, BTreeSet};

use crate::diff::{Change, DiffResult, SyncIndex};
use crate::path::RelPath;
use crate::scan::FileEntry;

/// Which side of the sync pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Side {
    Local,
    Remote,
}

/// Sync mode for a profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SyncMode {
    Push,
    Pull,
    Bidirectional,
}

/// Conflict resolution policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ConflictPolicy {
    NewerWins,
    KeepBoth,
}

/// What kind of conflict occurred.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictKind {
    BothModified,
    DeleteVsEdit,
}

/// A detected conflict and its resolution.
#[derive(Debug, Clone)]
pub struct Conflict {
    pub path: RelPath,
    pub kind: ConflictKind,
    pub resolution: ConflictResolution,
    pub winner: Side,
}

/// How a conflict was resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictResolution {
    NewerWins,
    KeepBoth,
}

/// An action to be executed during the apply phase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    CopyFile {
        from: Side,
        path: RelPath,
    },
    CreateDir {
        on: Side,
        path: RelPath,
    },
    Delete {
        on: Side,
        path: RelPath,
    },
    RenameConflict {
        on: Side,
        path: RelPath,
        new_name: String,
    },
}

/// The result of reconciliation: a set of actions to execute and conflicts encountered.
#[derive(Debug, Clone)]
pub struct SyncPlan {
    pub actions: Vec<Action>,
    pub conflicts: Vec<Conflict>,
}

/// Context needed for conflict resolution (file metadata from both sides).
pub struct ReconcileContext<'a> {
    pub local_entries: &'a BTreeMap<RelPath, FileEntry>,
    pub remote_entries: &'a BTreeMap<RelPath, FileEntry>,
    pub delete_propagation: bool,
    pub peer_name: String,
    /// Responder clock minus initiator clock in seconds (positive = responder is ahead).
    /// Used to adjust remote mtimes into initiator's clock domain before comparing.
    pub clock_offset_secs: i64,
}

const CLOCK_SKEW_TOLERANCE_SECS: i64 = 5;

/// Reconcile a diff into a sync plan.
///
/// This is the core decision logic of the sync engine. It examines what changed
/// on each side and produces ordered actions respecting the sync mode and conflict policy.
pub fn reconcile(
    diff: &DiffResult,
    index: &SyncIndex,
    mode: SyncMode,
    policy: ConflictPolicy,
    ctx: &ReconcileContext<'_>,
) -> SyncPlan {
    match mode {
        SyncMode::Push => reconcile_unidirectional(diff, Side::Local, Side::Remote, ctx),
        SyncMode::Pull => reconcile_unidirectional(diff, Side::Remote, Side::Local, ctx),
        SyncMode::Bidirectional => reconcile_bidirectional(diff, index, policy, ctx),
    }
}

/// Push/Pull: source is authoritative, no conflicts possible.
fn reconcile_unidirectional(
    diff: &DiffResult,
    source: Side,
    dest: Side,
    ctx: &ReconcileContext<'_>,
) -> SyncPlan {
    let source_changes = match source {
        Side::Local => &diff.local,
        Side::Remote => &diff.remote,
    };

    let mut actions = Vec::new();

    for change in source_changes {
        match change {
            Change::Created(path) | Change::Modified(path) => {
                // Ensure parent dirs exist on dest
                collect_parent_dirs(path, dest, &mut actions);
                actions.push(Action::CopyFile {
                    from: source,
                    path: path.clone(),
                });
            }
            Change::Deleted(path) => {
                if ctx.delete_propagation {
                    actions.push(Action::Delete {
                        on: dest,
                        path: path.clone(),
                    });
                }
            }
        }
    }

    SyncPlan {
        actions,
        conflicts: vec![],
    }
}

/// Bidirectional reconciliation: the core correctness logic.
fn reconcile_bidirectional(
    diff: &DiffResult,
    _index: &SyncIndex,
    policy: ConflictPolicy,
    ctx: &ReconcileContext<'_>,
) -> SyncPlan {
    let mut actions = Vec::new();
    let mut conflicts = Vec::new();

    // Build lookup maps for changes by path
    let local_changes: BTreeMap<&RelPath, &Change> =
        diff.local.iter().map(|c| (c.path(), c)).collect();
    let remote_changes: BTreeMap<&RelPath, &Change> =
        diff.remote.iter().map(|c| (c.path(), c)).collect();

    // All paths that have any change on either side
    let all_changed_paths: BTreeSet<&RelPath> = local_changes
        .keys()
        .copied()
        .chain(remote_changes.keys().copied())
        .collect();

    for path in all_changed_paths {
        let local_change = local_changes.get(path).copied();
        let remote_change = remote_changes.get(path).copied();

        match (local_change, remote_change) {
            // Changed on local only → propagate to remote
            (Some(change), None) => {
                handle_one_side_change(change, Side::Local, Side::Remote, ctx, &mut actions);
            }
            // Changed on remote only → propagate to local
            (None, Some(change)) => {
                handle_one_side_change(change, Side::Remote, Side::Local, ctx, &mut actions);
            }
            // Changed on both sides → potential conflict
            (Some(local), Some(remote)) => {
                handle_both_changed(
                    path,
                    local,
                    remote,
                    policy,
                    ctx,
                    &mut actions,
                    &mut conflicts,
                );
            }
            (None, None) => unreachable!(),
        }
    }

    SyncPlan { actions, conflicts }
}

fn handle_one_side_change(
    change: &Change,
    changed_side: Side,
    other_side: Side,
    ctx: &ReconcileContext<'_>,
    actions: &mut Vec<Action>,
) {
    match change {
        Change::Created(path) | Change::Modified(path) => {
            collect_parent_dirs(path, other_side, actions);
            actions.push(Action::CopyFile {
                from: changed_side,
                path: path.clone(),
            });
        }
        Change::Deleted(path) => {
            if ctx.delete_propagation {
                actions.push(Action::Delete {
                    on: other_side,
                    path: path.clone(),
                });
            }
        }
    }
}

fn handle_both_changed(
    path: &RelPath,
    local: &Change,
    remote: &Change,
    policy: ConflictPolicy,
    ctx: &ReconcileContext<'_>,
    actions: &mut Vec<Action>,
    conflicts: &mut Vec<Conflict>,
) {
    match (local, remote) {
        // Both created or modified: check if content is same (converged)
        (Change::Created(_) | Change::Modified(_), Change::Created(_) | Change::Modified(_)) => {
            if content_is_same(path, ctx) {
                // Already converged — no action needed
                return;
            }
            // Genuine conflict
            resolve_content_conflict(path, policy, ctx, actions, conflicts);
        }
        // Both deleted: no conflict, both agree
        (Change::Deleted(_), Change::Deleted(_)) => {
            // Nothing to do — both sides already deleted
        }
        // Delete on one side, edit on the other: delete-vs-edit conflict
        (Change::Deleted(_), Change::Created(_) | Change::Modified(_)) => {
            // Remote has the edited copy, local deleted
            resolve_delete_vs_edit(
                path,
                Side::Remote,
                Side::Local,
                policy,
                ctx,
                actions,
                conflicts,
            );
        }
        (Change::Created(_) | Change::Modified(_), Change::Deleted(_)) => {
            // Local has the edited copy, remote deleted
            resolve_delete_vs_edit(
                path,
                Side::Local,
                Side::Remote,
                policy,
                ctx,
                actions,
                conflicts,
            );
        }
    }
}

fn content_is_same(path: &RelPath, ctx: &ReconcileContext<'_>) -> bool {
    let local_entry = ctx.local_entries.get(path);
    let remote_entry = ctx.remote_entries.get(path);

    match (local_entry, remote_entry) {
        (Some(l), Some(r)) => {
            // If hashes are available, compare them
            if let (Some(lh), Some(rh)) = (&l.hash, &r.hash) {
                return lh == rh;
            }
            // Fallback: size must match at minimum
            l.size == r.size
        }
        _ => false,
    }
}

fn resolve_content_conflict(
    path: &RelPath,
    policy: ConflictPolicy,
    ctx: &ReconcileContext<'_>,
    actions: &mut Vec<Action>,
    conflicts: &mut Vec<Conflict>,
) {
    match policy {
        ConflictPolicy::NewerWins => {
            let winner = pick_newer(path, ctx);
            let loser = match winner {
                Side::Local => Side::Remote,
                Side::Remote => Side::Local,
            };
            collect_parent_dirs(path, loser, actions);
            actions.push(Action::CopyFile {
                from: winner,
                path: path.clone(),
            });
            conflicts.push(Conflict {
                path: path.clone(),
                kind: ConflictKind::BothModified,
                resolution: ConflictResolution::NewerWins,
                winner,
            });
        }
        ConflictPolicy::KeepBoth => {
            // Rename the remote version on the local side with a conflict suffix
            let conflict_name = make_conflict_name(path, &ctx.peer_name);
            actions.push(Action::RenameConflict {
                on: Side::Local,
                path: path.clone(),
                new_name: conflict_name,
            });
            // Copy remote version to local
            collect_parent_dirs(path, Side::Local, actions);
            actions.push(Action::CopyFile {
                from: Side::Remote,
                path: path.clone(),
            });
            // Copy local version to remote (so both sides have both copies)
            collect_parent_dirs(path, Side::Remote, actions);
            actions.push(Action::CopyFile {
                from: Side::Local,
                path: path.clone(),
            });
            conflicts.push(Conflict {
                path: path.clone(),
                kind: ConflictKind::BothModified,
                resolution: ConflictResolution::KeepBoth,
                winner: Side::Local, // arbitrary for KeepBoth
            });
        }
    }
}

fn resolve_delete_vs_edit(
    path: &RelPath,
    edited_side: Side,
    _deleted_side: Side,
    _policy: ConflictPolicy,
    ctx: &ReconcileContext<'_>,
    actions: &mut Vec<Action>,
    conflicts: &mut Vec<Conflict>,
) {
    // FR-CR-6: delete-vs-edit → keep the edited copy (regardless of policy)
    let dest = match edited_side {
        Side::Local => Side::Remote,
        Side::Remote => Side::Local,
    };
    collect_parent_dirs(path, dest, actions);
    actions.push(Action::CopyFile {
        from: edited_side,
        path: path.clone(),
    });
    conflicts.push(Conflict {
        path: path.clone(),
        kind: ConflictKind::DeleteVsEdit,
        resolution: ConflictResolution::NewerWins, // keep edited is effectively "newer wins"
        winner: edited_side,
    });

    // If the other side was supposed to delete, we DON'T propagate the deletion
    // because we're keeping the edited copy. Log it in peer_name for context.
    let _ = &ctx.peer_name;
}

fn pick_newer(path: &RelPath, ctx: &ReconcileContext<'_>) -> Side {
    let to_secs = |t: std::time::SystemTime| -> i64 {
        t.duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64)
    };

    let local_secs = ctx.local_entries.get(path).map(|e| to_secs(e.mtime));
    let remote_secs = ctx.remote_entries.get(path).map(|e| to_secs(e.mtime));

    match (local_secs, remote_secs) {
        (Some(l), Some(r)) => {
            // Adjust remote mtime into initiator's clock domain.
            let adjusted_r = r - ctx.clock_offset_secs;
            if (l - adjusted_r).abs() <= CLOCK_SKEW_TOLERANCE_SECS {
                // Within tolerance — hashes already confirmed unequal at this point,
                // so we cannot determine which is newer. Tie-break: prefer local.
                Side::Local
            } else if l >= adjusted_r {
                Side::Local
            } else {
                Side::Remote
            }
        }
        (None, Some(_)) => Side::Remote,
        // (Some, None) or (None, None): prefer local (deterministic fallback)
        _ => Side::Local,
    }
}

fn make_conflict_name(path: &RelPath, peer_name: &str) -> String {
    let stem = path.stem().unwrap_or("file");
    let ext = path.extension();
    let date = chrono::Utc::now().format("%Y-%m-%d");
    match ext {
        Some(e) => format!("{stem} (conflict from {peer_name} {date}).{e}"),
        None => format!("{stem} (conflict from {peer_name} {date})"),
    }
}

fn collect_parent_dirs(path: &RelPath, on: Side, actions: &mut Vec<Action>) {
    // Collect all ancestor dirs that might need to be created
    let mut current = path.parent();
    let mut parents = Vec::new();
    while let Some(p) = current {
        parents.push(p.clone());
        current = p.parent();
    }
    // Add in root-to-leaf order
    for parent in parents.into_iter().rev() {
        let action = Action::CreateDir { on, path: parent };
        if !actions.contains(&action) {
            actions.push(action);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use crate::scan::FileEntry;

    use super::*;

    fn entry(path: &str, size: u64, mtime_secs: u64) -> (RelPath, FileEntry) {
        let rel = RelPath::new(path);
        (
            rel.clone(),
            FileEntry {
                path: rel,
                kind: crate::scan::EntryKind::File,
                size,
                mtime: UNIX_EPOCH + Duration::from_secs(mtime_secs),
                hash: None,
            },
        )
    }

    fn entry_with_hash(path: &str, size: u64, mtime_secs: u64, hash: &str) -> (RelPath, FileEntry) {
        let rel = RelPath::new(path);
        (
            rel.clone(),
            FileEntry {
                path: rel,
                kind: crate::scan::EntryKind::File,
                size,
                mtime: UNIX_EPOCH + Duration::from_secs(mtime_secs),
                hash: Some(hash.to_owned()),
            },
        )
    }

    fn ctx<'a>(
        local: &'a BTreeMap<RelPath, FileEntry>,
        remote: &'a BTreeMap<RelPath, FileEntry>,
        delete_propagation: bool,
    ) -> ReconcileContext<'a> {
        ReconcileContext {
            local_entries: local,
            remote_entries: remote,
            delete_propagation,
            peer_name: "PeerB".to_owned(),
            clock_offset_secs: 0,
        }
    }

    #[test]
    fn push_propagates_source_changes() {
        let diff = DiffResult {
            local: vec![Change::Created(RelPath::new("new.txt"))],
            remote: vec![],
        };
        let local = BTreeMap::from([entry("new.txt", 100, 1000)]);
        let remote = BTreeMap::new();
        let context = ctx(&local, &remote, false);

        let plan = reconcile(
            &diff,
            &SyncIndex::default(),
            SyncMode::Push,
            ConflictPolicy::NewerWins,
            &context,
        );
        assert!(plan.conflicts.is_empty());
        assert!(plan.actions.iter().any(|a| matches!(a, Action::CopyFile { from: Side::Local, path } if path.display() == "new.txt")));
    }

    #[test]
    fn push_additive_no_delete() {
        let diff = DiffResult {
            local: vec![Change::Deleted(RelPath::new("old.txt"))],
            remote: vec![],
        };
        let local = BTreeMap::new();
        let remote = BTreeMap::from([entry("old.txt", 50, 500)]);
        let context = ctx(&local, &remote, false);

        let plan = reconcile(
            &diff,
            &SyncIndex::default(),
            SyncMode::Push,
            ConflictPolicy::NewerWins,
            &context,
        );
        assert!(plan.actions.is_empty());
    }

    #[test]
    fn push_mirror_propagates_delete() {
        let diff = DiffResult {
            local: vec![Change::Deleted(RelPath::new("old.txt"))],
            remote: vec![],
        };
        let local = BTreeMap::new();
        let remote = BTreeMap::from([entry("old.txt", 50, 500)]);
        let context = ctx(&local, &remote, true);

        let plan = reconcile(
            &diff,
            &SyncIndex::default(),
            SyncMode::Push,
            ConflictPolicy::NewerWins,
            &context,
        );
        assert!(plan.actions.iter().any(|a| matches!(a, Action::Delete { on: Side::Remote, path } if path.display() == "old.txt")));
    }

    #[test]
    fn bidi_non_conflicting_propagates_both_ways() {
        let diff = DiffResult {
            local: vec![Change::Modified(RelPath::new("file_a.txt"))],
            remote: vec![Change::Modified(RelPath::new("file_b.txt"))],
        };
        let local = BTreeMap::from([entry("file_a.txt", 200, 2000)]);
        let remote = BTreeMap::from([entry("file_b.txt", 300, 3000)]);
        let index = SyncIndex::default();
        let context = ctx(&local, &remote, false);

        let plan = reconcile(
            &diff,
            &index,
            SyncMode::Bidirectional,
            ConflictPolicy::NewerWins,
            &context,
        );
        assert!(plan.conflicts.is_empty());
        // file_a should copy from local to remote
        assert!(plan.actions.iter().any(|a| matches!(a, Action::CopyFile { from: Side::Local, path } if path.display() == "file_a.txt")));
        // file_b should copy from remote to local
        assert!(plan.actions.iter().any(|a| matches!(a, Action::CopyFile { from: Side::Remote, path } if path.display() == "file_b.txt")));
    }

    #[test]
    fn bidi_same_content_no_conflict() {
        let diff = DiffResult {
            local: vec![Change::Modified(RelPath::new("same.txt"))],
            remote: vec![Change::Modified(RelPath::new("same.txt"))],
        };
        let local = BTreeMap::from([entry_with_hash("same.txt", 100, 1000, "abc123")]);
        let remote = BTreeMap::from([entry_with_hash("same.txt", 100, 1000, "abc123")]);
        let index = SyncIndex::default();
        let context = ctx(&local, &remote, false);

        let plan = reconcile(
            &diff,
            &index,
            SyncMode::Bidirectional,
            ConflictPolicy::NewerWins,
            &context,
        );
        assert!(plan.conflicts.is_empty());
        // No copy actions needed — already converged
        assert!(
            !plan
                .actions
                .iter()
                .any(|a| matches!(a, Action::CopyFile { .. }))
        );
    }

    #[test]
    fn bidi_conflict_newer_wins() {
        let diff = DiffResult {
            local: vec![Change::Modified(RelPath::new("conflict.txt"))],
            remote: vec![Change::Modified(RelPath::new("conflict.txt"))],
        };
        // Remote is newer
        let local = BTreeMap::from([entry_with_hash("conflict.txt", 100, 1000, "local_hash")]);
        let remote = BTreeMap::from([entry_with_hash("conflict.txt", 150, 2000, "remote_hash")]);
        let index = SyncIndex::default();
        let context = ctx(&local, &remote, false);

        let plan = reconcile(
            &diff,
            &index,
            SyncMode::Bidirectional,
            ConflictPolicy::NewerWins,
            &context,
        );
        assert_eq!(plan.conflicts.len(), 1);
        assert_eq!(plan.conflicts[0].winner, Side::Remote);
        assert!(plan.actions.iter().any(|a| matches!(a, Action::CopyFile { from: Side::Remote, path } if path.display() == "conflict.txt")));
    }

    #[test]
    fn bidi_delete_vs_edit_keeps_edited() {
        let diff = DiffResult {
            local: vec![Change::Deleted(RelPath::new("edited.txt"))],
            remote: vec![Change::Modified(RelPath::new("edited.txt"))],
        };
        let local = BTreeMap::new();
        let remote = BTreeMap::from([entry("edited.txt", 200, 2000)]);
        let index = SyncIndex::default();
        let context = ctx(&local, &remote, true);

        let plan = reconcile(
            &diff,
            &index,
            SyncMode::Bidirectional,
            ConflictPolicy::NewerWins,
            &context,
        );
        assert_eq!(plan.conflicts.len(), 1);
        assert_eq!(plan.conflicts[0].kind, ConflictKind::DeleteVsEdit);
        assert_eq!(plan.conflicts[0].winner, Side::Remote);
        // The edited copy should be restored to the local side
        assert!(plan.actions.iter().any(|a| matches!(a, Action::CopyFile { from: Side::Remote, path } if path.display() == "edited.txt")));
    }

    #[test]
    fn bidi_delete_propagates_when_enabled() {
        let diff = DiffResult {
            local: vec![Change::Deleted(RelPath::new("removed.txt"))],
            remote: vec![],
        };
        let local = BTreeMap::new();
        let remote = BTreeMap::from([entry("removed.txt", 50, 500)]);
        let index = SyncIndex::default();
        let context = ctx(&local, &remote, true);

        let plan = reconcile(
            &diff,
            &index,
            SyncMode::Bidirectional,
            ConflictPolicy::NewerWins,
            &context,
        );
        assert!(plan.conflicts.is_empty());
        assert!(plan.actions.iter().any(|a| matches!(a, Action::Delete { on: Side::Remote, path } if path.display() == "removed.txt")));
    }

    #[test]
    fn bidi_delete_no_propagation_when_disabled() {
        let diff = DiffResult {
            local: vec![Change::Deleted(RelPath::new("removed.txt"))],
            remote: vec![],
        };
        let local = BTreeMap::new();
        let remote = BTreeMap::from([entry("removed.txt", 50, 500)]);
        let index = SyncIndex::default();
        let context = ctx(&local, &remote, false);

        let plan = reconcile(
            &diff,
            &index,
            SyncMode::Bidirectional,
            ConflictPolicy::NewerWins,
            &context,
        );
        assert!(plan.conflicts.is_empty());
        assert!(plan.actions.is_empty());
    }
}
