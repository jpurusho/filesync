use std::fs;
use std::path::Path;

use synccore::diff::SyncIndex;
use synccore::engine::{Anchor, Profile, RunResult};
use synccore::reconcile::{ConflictPolicy, SyncMode};
use synccore::scan::ScanConfig;
use tempfile::TempDir;
use uuid::Uuid;

fn make_profile(local: &Path, remote: &Path, mode: SyncMode, config: ScanConfig) -> Profile {
    Profile {
        id: Uuid::new_v4(),
        name: "test-profile".to_owned(),
        mode,
        delete_propagation: false,
        conflict_policy: ConflictPolicy::NewerWins,
        anchors: vec![Anchor {
            local_path: local.to_path_buf(),
            remote_path: remote.to_path_buf(),
            config,
        }],
        peer_name: "PeerB".to_owned(),
    }
}

fn run(profile: &Profile, index: &SyncIndex) -> RunResult {
    synccore::engine::run_sync(profile, index).expect("sync should succeed")
}

fn new_index_from(result: &RunResult) -> SyncIndex {
    result.anchor_results[0].new_index.clone()
}

// AC-1: Push A→B of a nested folder with hidden files excluded
#[test]
fn ac1_push_nested_hidden_excluded() {
    let local = TempDir::new().unwrap();
    let remote = TempDir::new().unwrap();

    // Create nested tree with hidden files
    fs::create_dir_all(local.path().join("photos/2024")).unwrap();
    fs::write(local.path().join("photos/2024/img001.jpg"), "image data").unwrap();
    fs::write(
        local.path().join("photos/2024/img002.jpg"),
        "more image data",
    )
    .unwrap();
    fs::write(local.path().join("photos/.hidden"), "hidden file").unwrap();
    fs::write(local.path().join("photos/2024/.thumbs"), "thumbs db").unwrap();

    let profile = make_profile(
        local.path(),
        remote.path(),
        SyncMode::Push,
        ScanConfig::default(), // hidden=false by default
    );

    let result = run(&profile, &SyncIndex::default());

    // Remote should have visible files only
    assert!(remote.path().join("photos/2024/img001.jpg").exists());
    assert!(remote.path().join("photos/2024/img002.jpg").exists());
    assert!(!remote.path().join("photos/.hidden").exists());
    assert!(!remote.path().join("photos/2024/.thumbs").exists());

    // No conflicts
    assert!(result.anchor_results[0].plan.conflicts.is_empty());
}

// AC-2: Re-run with no changes → zero transfers
#[test]
fn ac2_rerun_no_changes() {
    let local = TempDir::new().unwrap();
    let remote = TempDir::new().unwrap();

    fs::write(local.path().join("file.txt"), "content").unwrap();

    let profile = make_profile(
        local.path(),
        remote.path(),
        SyncMode::Push,
        ScanConfig::default(),
    );

    // First run
    let result1 = run(&profile, &SyncIndex::default());
    let index = new_index_from(&result1);

    // Copy file to remote to simulate first sync success
    // (apply already did this)
    assert!(remote.path().join("file.txt").exists());

    // Second run with the index from first run
    let result2 = run(&profile, &index);

    // No actions in second run
    assert!(
        result2.anchor_results[0].plan.actions.is_empty(),
        "expected zero actions on re-run, got: {:?}",
        result2.anchor_results[0].plan.actions
    );
}

// AC-3: Modify one file on A, re-run Push → only that file transferred
#[test]
fn ac3_incremental_push() {
    let local = TempDir::new().unwrap();
    let remote = TempDir::new().unwrap();

    fs::write(local.path().join("a.txt"), "original a").unwrap();
    fs::write(local.path().join("b.txt"), "original b").unwrap();

    let profile = make_profile(
        local.path(),
        remote.path(),
        SyncMode::Push,
        ScanConfig::default(),
    );

    // First run
    let result1 = run(&profile, &SyncIndex::default());
    let index = new_index_from(&result1);

    // Modify only a.txt (different size ensures diff detects the change
    // even if mtime resolution is too coarse in fast tests)
    fs::write(
        local.path().join("a.txt"),
        "modified a — now longer content",
    )
    .unwrap();

    // Second run
    let result2 = run(&profile, &index);
    let copy_actions: Vec<_> = result2.anchor_results[0]
        .plan
        .actions
        .iter()
        .filter(|a| matches!(a, synccore::reconcile::Action::CopyFile { .. }))
        .collect();

    assert_eq!(copy_actions.len(), 1, "only one file should be copied");

    // Remote should have modified content
    assert_eq!(
        fs::read_to_string(remote.path().join("a.txt")).unwrap(),
        "modified a — now longer content"
    );
    // b.txt should be unchanged
    assert_eq!(
        fs::read_to_string(remote.path().join("b.txt")).unwrap(),
        "original b"
    );
}

// AC-4: Bidirectional — edit file X on A, edit file Y on B → both propagate
#[test]
fn ac4_bidi_non_conflicting() {
    let local = TempDir::new().unwrap();
    let remote = TempDir::new().unwrap();

    // Initial state: same content on both sides
    fs::write(local.path().join("x.txt"), "original x").unwrap();
    fs::write(local.path().join("y.txt"), "original y").unwrap();
    fs::write(remote.path().join("x.txt"), "original x").unwrap();
    fs::write(remote.path().join("y.txt"), "original y").unwrap();

    let profile = make_profile(
        local.path(),
        remote.path(),
        SyncMode::Bidirectional,
        ScanConfig::default(),
    );

    // Build an index representing the initial synced state
    let initial_result = run(&profile, &SyncIndex::default());
    let index = new_index_from(&initial_result);

    // Now edit x on local, y on remote
    fs::write(local.path().join("x.txt"), "modified x on A").unwrap();
    fs::write(remote.path().join("y.txt"), "modified y on B").unwrap();

    let result = run(&profile, &index);

    // No conflicts — different files
    assert!(
        result.anchor_results[0].plan.conflicts.is_empty(),
        "expected no conflicts, got: {:?}",
        result.anchor_results[0].plan.conflicts
    );

    // x.txt propagated to remote
    assert_eq!(
        fs::read_to_string(remote.path().join("x.txt")).unwrap(),
        "modified x on A"
    );
    // y.txt propagated to local
    assert_eq!(
        fs::read_to_string(local.path().join("y.txt")).unwrap(),
        "modified y on B"
    );
}

// AC-5: Bidirectional — edit same file on both → conflict resolved per policy
#[test]
fn ac5_bidi_conflict() {
    let local = TempDir::new().unwrap();
    let remote = TempDir::new().unwrap();

    fs::write(local.path().join("doc.txt"), "original").unwrap();
    fs::write(remote.path().join("doc.txt"), "original").unwrap();

    let profile = make_profile(
        local.path(),
        remote.path(),
        SyncMode::Bidirectional,
        ScanConfig::default(),
    );

    // Build baseline index
    let initial = run(&profile, &SyncIndex::default());
    let index = new_index_from(&initial);

    // Edit same file on both sides with different content
    fs::write(local.path().join("doc.txt"), "local edit").unwrap();
    fs::write(remote.path().join("doc.txt"), "remote edit").unwrap();

    let result = run(&profile, &index);

    // Should have exactly one conflict
    assert_eq!(result.anchor_results[0].plan.conflicts.len(), 1);
    // Newer wins — one side overwrites the other (nothing silently lost)
}

// AC-6: Bidirectional — delete a file on A (unchanged on B) → file deleted on B
#[test]
fn ac6_bidi_delete_propagates() {
    let local = TempDir::new().unwrap();
    let remote = TempDir::new().unwrap();

    fs::write(local.path().join("remove_me.txt"), "content").unwrap();
    fs::write(remote.path().join("remove_me.txt"), "content").unwrap();

    let mut profile = make_profile(
        local.path(),
        remote.path(),
        SyncMode::Bidirectional,
        ScanConfig::default(),
    );
    profile.delete_propagation = true;

    // Build baseline
    let initial = run(&profile, &SyncIndex::default());
    let index = new_index_from(&initial);

    // Delete on local
    fs::remove_file(local.path().join("remove_me.txt")).unwrap();

    let result = run(&profile, &index);

    // No conflict (only one side changed)
    assert!(result.anchor_results[0].plan.conflicts.is_empty());
    // File should be deleted on remote
    assert!(!remote.path().join("remove_me.txt").exists());
}

// AC-7: Delete on A while edited on B (delete-vs-edit) → conflict, edited copy preserved
#[test]
fn ac7_delete_vs_edit() {
    let local = TempDir::new().unwrap();
    let remote = TempDir::new().unwrap();

    fs::write(local.path().join("contested.txt"), "original").unwrap();
    fs::write(remote.path().join("contested.txt"), "original").unwrap();

    let mut profile = make_profile(
        local.path(),
        remote.path(),
        SyncMode::Bidirectional,
        ScanConfig::default(),
    );
    profile.delete_propagation = true;

    // Build baseline
    let initial = run(&profile, &SyncIndex::default());
    let index = new_index_from(&initial);

    // Delete on local, edit on remote
    fs::remove_file(local.path().join("contested.txt")).unwrap();
    fs::write(remote.path().join("contested.txt"), "remote edited").unwrap();

    let result = run(&profile, &index);

    // Should be a conflict
    assert_eq!(result.anchor_results[0].plan.conflicts.len(), 1);
    // The edited copy should be preserved (copied to local)
    assert!(
        local.path().join("contested.txt").exists(),
        "edited copy should be preserved on local side"
    );
    assert_eq!(
        fs::read_to_string(local.path().join("contested.txt")).unwrap(),
        "remote edited"
    );
}

// AC-11: Depth set to 1 → only anchor + first level synced
#[test]
fn ac11_depth_limiting() {
    let local = TempDir::new().unwrap();
    let remote = TempDir::new().unwrap();

    fs::create_dir_all(local.path().join("level1/level2")).unwrap();
    fs::write(local.path().join("root.txt"), "root").unwrap();
    fs::write(local.path().join("level1/a.txt"), "level1").unwrap();
    fs::write(local.path().join("level1/level2/deep.txt"), "deep").unwrap();

    let config = ScanConfig {
        max_depth: 1,
        ..Default::default()
    };
    let profile = make_profile(local.path(), remote.path(), SyncMode::Push, config);

    run(&profile, &SyncIndex::default());

    // Root level and level 1 should be synced
    assert!(remote.path().join("root.txt").exists());
    assert!(remote.path().join("level1/a.txt").exists());
    // Level 2 should NOT be synced
    assert!(
        !remote.path().join("level1/level2/deep.txt").exists(),
        "files beyond depth 1 should not be synced"
    );
}

// AC-14: Reset profile → rescan, no deletes (first-sync behavior)
#[test]
fn ac14_reset_profile() {
    let local = TempDir::new().unwrap();
    let remote = TempDir::new().unwrap();

    fs::write(local.path().join("keep.txt"), "keep this").unwrap();
    fs::write(remote.path().join("keep.txt"), "keep this").unwrap();
    fs::write(remote.path().join("extra.txt"), "extra on remote").unwrap();

    let profile = make_profile(
        local.path(),
        remote.path(),
        SyncMode::Bidirectional,
        ScanConfig::default(),
    );

    // Simulate "reset": run with empty index (as if index was cleared)
    let result = run(&profile, &SyncIndex::default());

    // No deletes should happen (first-sync treats everything as created → union)
    assert!(remote.path().join("extra.txt").exists());
    assert!(remote.path().join("keep.txt").exists());
    // extra.txt should be copied to local (union behavior)
    assert!(
        local.path().join("extra.txt").exists(),
        "first sync should copy remote-only files to local"
    );

    // No conflicts for identical content
    let conflicts_on_keep: Vec<_> = result.anchor_results[0]
        .plan
        .conflicts
        .iter()
        .filter(|c| c.path.display() == "keep.txt")
        .collect();
    assert!(
        conflicts_on_keep.is_empty(),
        "identical files should not conflict on first sync"
    );
}
