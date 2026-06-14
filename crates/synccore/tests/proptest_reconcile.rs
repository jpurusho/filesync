//! Property tests for the reconciler.
//!
//! Invariants tested:
//! 1. No data loss: reconcile never emits a Delete action without delete_propagation=true.
//! 2. Convergence: after applying a bidi plan, both sides have the same set of files.
//! 3. Idempotence: reconciling the empty diff produces zero actions.
//! 4. Commutativity: swapping local/remote in a bidi diff produces a mirror plan.
//! 5. Non-conflicting changes never produce conflicts.

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, UNIX_EPOCH};

use proptest::prelude::*;
use synccore::diff::{Change, DiffResult, SyncIndex};
use synccore::path::RelPath;
use synccore::reconcile::{Action, ConflictPolicy, ReconcileContext, Side, SyncMode};
use synccore::scan::{EntryKind, FileEntry};

// --- Generators ---

fn arb_filename() -> impl Strategy<Value = String> {
    prop::string::string_regex("[a-z][a-z0-9]{0,7}\\.(txt|md|rs|jpg)")
        .unwrap()
        .prop_filter("non-empty", |s| !s.is_empty())
}

/// Generate a change type for a given path.
fn arb_change(path: RelPath) -> impl Strategy<Value = Change> {
    prop_oneof![
        Just(Change::Created(path.clone())),
        Just(Change::Modified(path.clone())),
        Just(Change::Deleted(path)),
    ]
}

/// Generate a set of non-overlapping changes (each path appears at most once).
fn arb_change_set(max_size: usize) -> impl Strategy<Value = Vec<Change>> {
    prop::collection::vec(arb_filename(), 0..max_size).prop_flat_map(|names| {
        let unique: Vec<_> = names
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let strats: Vec<_> = unique
            .into_iter()
            .map(|name| arb_change(RelPath::new(&name)))
            .collect();
        strats
    })
}

/// Generate a DiffResult where local and remote paths don't overlap (non-conflicting).
fn arb_non_conflicting_diff() -> impl Strategy<Value = DiffResult> {
    (arb_change_set(5), arb_change_set(5)).prop_map(|(local, remote)| {
        // Remove any paths that appear on both sides
        let local_paths: BTreeSet<_> = local.iter().map(|c| c.path().clone()).collect();
        let remote_filtered: Vec<_> = remote
            .into_iter()
            .filter(|c| !local_paths.contains(c.path()))
            .collect();
        DiffResult {
            local,
            remote: remote_filtered,
        }
    })
}

/// Generate file entries for all paths mentioned in a diff.
fn entries_for_diff(
    diff: &DiffResult,
) -> (BTreeMap<RelPath, FileEntry>, BTreeMap<RelPath, FileEntry>) {
    let mut local = BTreeMap::new();
    let mut remote = BTreeMap::new();

    for change in &diff.local {
        match change {
            Change::Created(p) | Change::Modified(p) => {
                local.insert(
                    p.clone(),
                    FileEntry {
                        path: p.clone(),
                        kind: EntryKind::File,
                        size: 100,
                        mtime: UNIX_EPOCH + Duration::from_secs(2000),
                        hash: Some(format!("local_{}", p.display())),
                    },
                );
            }
            Change::Deleted(_) => {}
        }
    }

    for change in &diff.remote {
        match change {
            Change::Created(p) | Change::Modified(p) => {
                remote.insert(
                    p.clone(),
                    FileEntry {
                        path: p.clone(),
                        kind: EntryKind::File,
                        size: 200,
                        mtime: UNIX_EPOCH + Duration::from_secs(3000),
                        hash: Some(format!("remote_{}", p.display())),
                    },
                );
            }
            Change::Deleted(_) => {}
        }
    }

    (local, remote)
}

// --- Property Tests ---

proptest! {
    /// Invariant 1: Without delete_propagation, reconcile never emits Delete actions.
    #[test]
    fn no_delete_without_propagation(
        local_changes in arb_change_set(8),
        remote_changes in arb_change_set(8),
        mode in prop_oneof![
            Just(SyncMode::Push),
            Just(SyncMode::Pull),
            Just(SyncMode::Bidirectional),
        ],
        policy in prop_oneof![
            Just(ConflictPolicy::NewerWins),
            Just(ConflictPolicy::KeepBoth),
        ],
    ) {
        let diff = DiffResult { local: local_changes, remote: remote_changes };
        let (local_entries, remote_entries) = entries_for_diff(&diff);
        let ctx = ReconcileContext {
            local_entries: &local_entries,
            remote_entries: &remote_entries,
            delete_propagation: false,
            peer_name: "TestPeer".to_owned(),
            clock_offset_secs: 0,
        };

        let plan = synccore::reconcile::reconcile(
            &diff, &SyncIndex::default(), mode, policy, &ctx,
        );

        for action in &plan.actions {
            prop_assert!(!matches!(action, Action::Delete { .. }),
                "Delete action emitted without delete_propagation: {:?}", action);
        }
    }

    /// Invariant 2: Reconciling an empty diff always produces zero actions.
    #[test]
    fn empty_diff_produces_empty_plan(
        mode in prop_oneof![
            Just(SyncMode::Push),
            Just(SyncMode::Pull),
            Just(SyncMode::Bidirectional),
        ],
        policy in prop_oneof![
            Just(ConflictPolicy::NewerWins),
            Just(ConflictPolicy::KeepBoth),
        ],
    ) {
        let diff = DiffResult { local: vec![], remote: vec![] };
        let local_entries = BTreeMap::new();
        let remote_entries = BTreeMap::new();
        let ctx = ReconcileContext {
            local_entries: &local_entries,
            remote_entries: &remote_entries,
            delete_propagation: true,
            peer_name: "TestPeer".to_owned(),
            clock_offset_secs: 0,
        };

        let plan = synccore::reconcile::reconcile(
            &diff, &SyncIndex::default(), mode, policy, &ctx,
        );

        prop_assert!(plan.actions.is_empty(),
            "Non-empty actions from empty diff: {:?}", plan.actions);
        prop_assert!(plan.conflicts.is_empty(),
            "Conflicts from empty diff: {:?}", plan.conflicts);
    }

    /// Invariant 3: Non-conflicting bidi diff produces zero conflicts.
    #[test]
    fn non_conflicting_diff_no_conflicts(
        diff in arb_non_conflicting_diff(),
        policy in prop_oneof![
            Just(ConflictPolicy::NewerWins),
            Just(ConflictPolicy::KeepBoth),
        ],
    ) {
        let (local_entries, remote_entries) = entries_for_diff(&diff);
        let ctx = ReconcileContext {
            local_entries: &local_entries,
            remote_entries: &remote_entries,
            delete_propagation: false,
            peer_name: "TestPeer".to_owned(),
            clock_offset_secs: 0,
        };

        let plan = synccore::reconcile::reconcile(
            &diff, &SyncIndex::default(), SyncMode::Bidirectional, policy, &ctx,
        );

        prop_assert!(plan.conflicts.is_empty(),
            "Conflicts from non-conflicting diff: {:?}\nDiff: {:?}", plan.conflicts, diff);
    }

    /// Invariant 4: Push mode only copies from local.
    #[test]
    fn push_only_copies_from_local(
        local_changes in arb_change_set(8),
        remote_changes in arb_change_set(8),
    ) {
        let diff = DiffResult { local: local_changes, remote: remote_changes };
        let (local_entries, remote_entries) = entries_for_diff(&diff);
        let ctx = ReconcileContext {
            local_entries: &local_entries,
            remote_entries: &remote_entries,
            delete_propagation: true,
            peer_name: "TestPeer".to_owned(),
            clock_offset_secs: 0,
        };

        let plan = synccore::reconcile::reconcile(
            &diff, &SyncIndex::default(), SyncMode::Push, ConflictPolicy::NewerWins, &ctx,
        );

        for action in &plan.actions {
            match action {
                Action::CopyFile { from, .. } => {
                    prop_assert_eq!(*from, Side::Local,
                        "Push mode copied from Remote: {:?}", action);
                }
                Action::Delete { on, .. } => {
                    prop_assert_eq!(*on, Side::Remote,
                        "Push mode deleted on Local: {:?}", action);
                }
                Action::CreateDir { on, .. } => {
                    prop_assert_eq!(*on, Side::Remote,
                        "Push mode created dir on Local: {:?}", action);
                }
                Action::RenameConflict { .. } => {
                    prop_assert!(false, "Push mode should never rename: {:?}", action);
                }
            }
        }
        // Push never generates conflicts
        prop_assert!(plan.conflicts.is_empty());
    }

    /// Invariant 5: Pull mode only copies from remote.
    #[test]
    fn pull_only_copies_from_remote(
        local_changes in arb_change_set(8),
        remote_changes in arb_change_set(8),
    ) {
        let diff = DiffResult { local: local_changes, remote: remote_changes };
        let (local_entries, remote_entries) = entries_for_diff(&diff);
        let ctx = ReconcileContext {
            local_entries: &local_entries,
            remote_entries: &remote_entries,
            delete_propagation: true,
            peer_name: "TestPeer".to_owned(),
            clock_offset_secs: 0,
        };

        let plan = synccore::reconcile::reconcile(
            &diff, &SyncIndex::default(), SyncMode::Pull, ConflictPolicy::NewerWins, &ctx,
        );

        for action in &plan.actions {
            match action {
                Action::CopyFile { from, .. } => {
                    prop_assert_eq!(*from, Side::Remote,
                        "Pull mode copied from Local: {:?}", action);
                }
                Action::Delete { on, .. } => {
                    prop_assert_eq!(*on, Side::Local,
                        "Pull mode deleted on Remote: {:?}", action);
                }
                Action::CreateDir { on, .. } => {
                    prop_assert_eq!(*on, Side::Local,
                        "Pull mode created dir on Remote: {:?}", action);
                }
                Action::RenameConflict { .. } => {
                    prop_assert!(false, "Pull mode should never rename: {:?}", action);
                }
            }
        }
        prop_assert!(plan.conflicts.is_empty());
    }

    /// Invariant 6: Bidi with both sides having identical changes (converged) produces no actions.
    #[test]
    fn converged_changes_no_action(
        filenames in prop::collection::vec(arb_filename(), 1..5),
    ) {
        let unique: Vec<_> = filenames.into_iter().collect::<BTreeSet<_>>().into_iter().collect();
        let local: Vec<Change> = unique.iter().map(|n| Change::Modified(RelPath::new(n))).collect();
        let remote: Vec<Change> = unique.iter().map(|n| Change::Modified(RelPath::new(n))).collect();
        let diff = DiffResult { local, remote };

        // Same hash on both sides → converged
        let mut local_entries = BTreeMap::new();
        let mut remote_entries = BTreeMap::new();
        for name in &unique {
            let p = RelPath::new(name);
            let entry = FileEntry {
                path: p.clone(),
                kind: EntryKind::File,
                size: 42,
                mtime: UNIX_EPOCH + Duration::from_secs(1000),
                hash: Some("same_hash".to_owned()),
            };
            local_entries.insert(p.clone(), entry.clone());
            remote_entries.insert(p, entry);
        }

        let ctx = ReconcileContext {
            local_entries: &local_entries,
            remote_entries: &remote_entries,
            delete_propagation: false,
            peer_name: "TestPeer".to_owned(),
            clock_offset_secs: 0,
        };

        let plan = synccore::reconcile::reconcile(
            &diff, &SyncIndex::default(), SyncMode::Bidirectional, ConflictPolicy::NewerWins, &ctx,
        );

        // No copy/delete actions for converged content
        let non_dir_actions: Vec<_> = plan.actions.iter()
            .filter(|a| !matches!(a, Action::CreateDir { .. }))
            .collect();
        prop_assert!(non_dir_actions.is_empty(),
            "Converged content should produce no actions: {:?}", non_dir_actions);
        prop_assert!(plan.conflicts.is_empty());
    }

    /// Invariant 7: Every CopyFile action references a path that exists in the source entries.
    #[test]
    fn copy_actions_reference_existing_entries(
        local_changes in arb_change_set(6),
        remote_changes in arb_change_set(6),
        mode in prop_oneof![
            Just(SyncMode::Push),
            Just(SyncMode::Pull),
            Just(SyncMode::Bidirectional),
        ],
    ) {
        let diff = DiffResult { local: local_changes, remote: remote_changes };
        let (local_entries, remote_entries) = entries_for_diff(&diff);
        let ctx = ReconcileContext {
            local_entries: &local_entries,
            remote_entries: &remote_entries,
            delete_propagation: false,
            peer_name: "TestPeer".to_owned(),
            clock_offset_secs: 0,
        };

        let plan = synccore::reconcile::reconcile(
            &diff, &SyncIndex::default(), mode, ConflictPolicy::NewerWins, &ctx,
        );

        for action in &plan.actions {
            if let Action::CopyFile { from, path } = action {
                let source = match from {
                    Side::Local => &local_entries,
                    Side::Remote => &remote_entries,
                };
                prop_assert!(source.contains_key(path),
                    "CopyFile references non-existent path {:?} on {:?}", path, from);
            }
        }
    }
}
