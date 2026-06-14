use syncstore::Db;
use syncstore::index::IndexEntryRow;
use syncstore::profiles::{AnchorRow, ProfileRow};
use syncstore::quick_sends::QuickSendRecordRow;
use syncstore::runs::RunRecordRow;
use uuid::Uuid;

fn test_db() -> Db {
    Db::in_memory().expect("failed to create test db")
}

fn sample_profile() -> ProfileRow {
    ProfileRow {
        id: Uuid::new_v4(),
        name: "Test Profile".to_owned(),
        mode: "bidirectional".to_owned(),
        delete_propagation: false,
        conflict_policy: "newer_wins".to_owned(),
        peer_name: "MacBook".to_owned(),
        created_at: String::new(),
        updated_at: String::new(),
    }
}

#[test]
fn profile_crud() {
    let db = test_db();
    let mut profile = sample_profile();

    // Insert
    db.insert_profile(&profile).unwrap();

    // Read
    let loaded = db.get_profile(profile.id).unwrap().unwrap();
    assert_eq!(loaded.name, "Test Profile");
    assert_eq!(loaded.mode, "bidirectional");
    assert!(!loaded.delete_propagation);

    // Update
    profile.name = "Renamed".to_owned();
    profile.delete_propagation = true;
    db.update_profile(&profile).unwrap();
    let updated = db.get_profile(profile.id).unwrap().unwrap();
    assert_eq!(updated.name, "Renamed");
    assert!(updated.delete_propagation);

    // List
    let all = db.list_profiles().unwrap();
    assert_eq!(all.len(), 1);

    // Delete
    db.delete_profile(profile.id).unwrap();
    assert!(db.get_profile(profile.id).unwrap().is_none());
}

#[test]
fn anchors_linked_to_profile() {
    let db = test_db();
    let profile = sample_profile();
    db.insert_profile(&profile).unwrap();

    let anchor = AnchorRow {
        id: 0,
        profile_id: profile.id,
        local_path: "/Users/me/docs".to_owned(),
        remote_path: "/Users/peer/docs".to_owned(),
        max_depth: 3,
        include_hidden: true,
        ignore_patterns: vec!["*.tmp".to_owned(), ".DS_Store".to_owned()],
    };
    db.insert_anchor(&anchor).unwrap();

    let anchors = db.get_anchors(profile.id).unwrap();
    assert_eq!(anchors.len(), 1);
    assert_eq!(anchors[0].local_path, "/Users/me/docs");
    assert_eq!(anchors[0].max_depth, 3);
    assert!(anchors[0].include_hidden);
    assert_eq!(anchors[0].ignore_patterns, vec!["*.tmp", ".DS_Store"]);

    // Cascade delete
    db.delete_profile(profile.id).unwrap();
    let anchors_after = db.get_anchors(profile.id).unwrap();
    assert!(anchors_after.is_empty());
}

#[test]
fn sync_index_save_and_load() {
    let db = test_db();
    let profile = sample_profile();
    db.insert_profile(&profile).unwrap();

    let entries = vec![
        IndexEntryRow {
            profile_id: profile.id,
            anchor_idx: 0,
            rel_path: "docs/readme.md".to_owned(),
            kind: "file".to_owned(),
            size: 1024,
            mtime_secs: 1_700_000_000,
            hash: "abc123def456".to_owned(),
        },
        IndexEntryRow {
            profile_id: profile.id,
            anchor_idx: 0,
            rel_path: "src/main.rs".to_owned(),
            kind: "file".to_owned(),
            size: 2048,
            mtime_secs: 1_700_001_000,
            hash: "789xyz".to_owned(),
        },
    ];

    db.save_index(profile.id, 0, &entries).unwrap();

    let loaded = db.load_index(profile.id, 0).unwrap();
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded[0].rel_path, "docs/readme.md");
    assert_eq!(loaded[0].size, 1024);
    assert_eq!(loaded[1].hash, "789xyz");

    // Overwrite with new data
    let new_entries = vec![IndexEntryRow {
        profile_id: profile.id,
        anchor_idx: 0,
        rel_path: "only_one.txt".to_owned(),
        kind: "file".to_owned(),
        size: 512,
        mtime_secs: 1_700_002_000,
        hash: "new_hash".to_owned(),
    }];
    db.save_index(profile.id, 0, &new_entries).unwrap();
    let reloaded = db.load_index(profile.id, 0).unwrap();
    assert_eq!(reloaded.len(), 1);
    assert_eq!(reloaded[0].rel_path, "only_one.txt");

    // Clear
    db.clear_index(profile.id).unwrap();
    let empty = db.load_index(profile.id, 0).unwrap();
    assert!(empty.is_empty());
}

#[test]
fn run_records() {
    let db = test_db();
    let profile = sample_profile();
    db.insert_profile(&profile).unwrap();

    let run1 = RunRecordRow {
        id: Uuid::new_v4(),
        profile_id: profile.id,
        started_at: "2026-06-12T10:00:00Z".to_owned(),
        finished_at: "2026-06-12T10:00:05Z".to_owned(),
        status: "success".to_owned(),
        files_transferred: 15,
        files_deleted: 2,
        conflicts_count: 0,
        errors_count: 0,
        error_summary: None,
    };
    let run2 = RunRecordRow {
        id: Uuid::new_v4(),
        profile_id: profile.id,
        started_at: "2026-06-12T11:00:00Z".to_owned(),
        finished_at: "2026-06-12T11:00:03Z".to_owned(),
        status: "partial".to_owned(),
        files_transferred: 8,
        files_deleted: 0,
        conflicts_count: 1,
        errors_count: 2,
        error_summary: Some("permission denied on 2 files".to_owned()),
    };

    db.insert_run(&run1).unwrap();
    db.insert_run(&run2).unwrap();

    // Most recent first
    let history = db.get_runs(profile.id, 10).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].status, "partial"); // newer
    assert_eq!(history[1].status, "success");

    // Latest
    let latest = db.get_latest_run(profile.id).unwrap().unwrap();
    assert_eq!(latest.files_transferred, 8);
    assert_eq!(
        latest.error_summary.as_deref(),
        Some("permission denied on 2 files")
    );

    // Cascade delete
    db.delete_profile(profile.id).unwrap();
    let after = db.get_runs(profile.id, 10).unwrap();
    assert!(after.is_empty());
}

#[test]
fn quick_send_records() {
    let db = test_db();
    let peer_id = Uuid::new_v4();

    let record = QuickSendRecordRow {
        id: Uuid::new_v4(),
        peer_id,
        direction: "send".to_owned(),
        destination_dir: "/Users/peer/Downloads".to_owned(),
        started_at: "2026-06-13T09:00:00Z".to_owned(),
        finished_at: "2026-06-13T09:00:02Z".to_owned(),
        status: "success".to_owned(),
        files_transferred: 3,
        bytes_transferred: 10240,
        error_summary: None,
    };
    db.insert_quick_send(&record).unwrap();

    let all = db.get_quick_sends(10).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].direction, "send");
    assert_eq!(all[0].files_transferred, 3);
    assert_eq!(all[0].bytes_transferred, 10240);

    let by_peer = db.get_quick_sends_for_peer(peer_id, 10).unwrap();
    assert_eq!(by_peer.len(), 1);

    let other_peer = Uuid::new_v4();
    let none = db.get_quick_sends_for_peer(other_peer, 10).unwrap();
    assert!(none.is_empty());
}
