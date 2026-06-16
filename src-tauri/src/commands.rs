use crate::views::{
    AnchorInput, AnchorView, DriftSummary, PeerView, ProfileDetail, ProfileInput, ProfileView,
    SyncStatus,
};
use std::sync::Mutex;
use syncstore::{profiles::ProfileRow, Db};
use tauri::State;
use uuid::Uuid;

type DbState<'a> = State<'a, Mutex<Db>>;

/// List all active profiles (excluding pending deletions)
#[tauri::command]
pub fn list_profiles(db: DbState) -> Result<Vec<ProfileView>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let profiles = db.list_profiles().map_err(|e| e.to_string())?;

    Ok(profiles
        .into_iter()
        .map(|p| ProfileView {
            id: p.id.to_string(),
            name: p.name,
            mode: p.mode,
            peer_name: p.peer_name,
            delete_propagation: p.delete_propagation,
            conflict_policy: p.conflict_policy,
            updated_at: p.updated_at,
            version: p.version,
            pending_deletion: p.pending_deletion,
        })
        .collect())
}

/// Get full profile detail including anchors
#[tauri::command]
pub fn get_profile(id: String, db: DbState) -> Result<ProfileDetail, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;

    let profile = db
        .get_profile(uuid)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Profile not found".to_string())?;

    let anchors = db.get_anchors(uuid).map_err(|e| e.to_string())?;

    Ok(ProfileDetail {
        id: profile.id.to_string(),
        name: profile.name,
        mode: profile.mode,
        peer_name: profile.peer_name,
        peer_id: profile.peer_id,
        delete_propagation: profile.delete_propagation,
        conflict_policy: profile.conflict_policy,
        created_at: profile.created_at,
        updated_at: profile.updated_at,
        version: profile.version,
        origin_instance_id: profile.origin_instance_id,
        pending_deletion: profile.pending_deletion,
        anchors: anchors
            .into_iter()
            .map(|a| AnchorView {
                id: a.id,
                local_path: a.local_path,
                remote_path: a.remote_path,
                max_depth: a.max_depth,
                include_hidden: a.include_hidden,
                ignore_patterns: a.ignore_patterns,
            })
            .collect(),
    })
}

/// Create a new profile
#[tauri::command]
pub fn create_profile(input: ProfileInput, db: DbState) -> Result<ProfileView, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let id = Uuid::new_v4();
    let now = chrono::Utc::now().to_rfc3339();

    let profile = ProfileRow {
        id,
        name: input.name.clone(),
        mode: input.mode.clone(),
        delete_propagation: input.delete_propagation,
        conflict_policy: input.conflict_policy.clone(),
        peer_name: input.peer_name.clone(),
        created_at: now.clone(),
        updated_at: now.clone(),
        version: 1,
        peer_id: input.peer_id.clone(),
        origin_instance_id: String::new(), // Set by local instance ID
        pending_deletion: false,
    };

    db.insert_profile(&profile).map_err(|e| e.to_string())?;

    // Insert anchors
    for anchor_input in input.anchors {
        let anchor = syncstore::profiles::AnchorRow {
            id: 0, // Auto-generated
            profile_id: id,
            local_path: anchor_input.local_path,
            remote_path: anchor_input.remote_path,
            max_depth: anchor_input.max_depth,
            include_hidden: anchor_input.include_hidden,
            ignore_patterns: anchor_input.ignore_patterns,
        };
        db.insert_anchor(&anchor).map_err(|e| e.to_string())?;
    }

    Ok(ProfileView {
        id: profile.id.to_string(),
        name: profile.name,
        mode: profile.mode,
        peer_name: profile.peer_name,
        delete_propagation: profile.delete_propagation,
        conflict_policy: profile.conflict_policy,
        updated_at: profile.updated_at,
        version: profile.version,
        pending_deletion: profile.pending_deletion,
    })
}

/// Update an existing profile
#[tauri::command]
pub fn update_profile(id: String, input: ProfileInput, db: DbState) -> Result<ProfileView, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;

    let existing = db
        .get_profile(uuid)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Profile not found".to_string())?;

    let now = chrono::Utc::now().to_rfc3339();
    let updated = ProfileRow {
        id: uuid,
        name: input.name.clone(),
        mode: input.mode.clone(),
        delete_propagation: input.delete_propagation,
        conflict_policy: input.conflict_policy.clone(),
        peer_name: input.peer_name.clone(),
        created_at: existing.created_at,
        updated_at: now.clone(),
        version: existing.version + 1,
        peer_id: input.peer_id.clone(),
        origin_instance_id: existing.origin_instance_id,
        pending_deletion: false,
    };

    db.update_profile(&updated).map_err(|e| e.to_string())?;

    // Replace anchors: delete all and re-insert
    db.delete_anchors_for_profile(uuid).map_err(|e| e.to_string())?;
    for anchor_input in input.anchors {
        let anchor = syncstore::profiles::AnchorRow {
            id: 0,
            profile_id: uuid,
            local_path: anchor_input.local_path,
            remote_path: anchor_input.remote_path,
            max_depth: anchor_input.max_depth,
            include_hidden: anchor_input.include_hidden,
            ignore_patterns: anchor_input.ignore_patterns,
        };
        db.insert_anchor(&anchor).map_err(|e| e.to_string())?;
    }

    Ok(ProfileView {
        id: updated.id.to_string(),
        name: updated.name,
        mode: updated.mode,
        peer_name: updated.peer_name,
        delete_propagation: updated.delete_propagation,
        conflict_policy: updated.conflict_policy,
        updated_at: updated.updated_at,
        version: updated.version,
        pending_deletion: updated.pending_deletion,
    })
}

/// Delete a profile and queue tombstone
#[tauri::command]
pub fn delete_profile(id: String, db: DbState) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;

    // Delete anchors first (foreign key cascade should handle this, but be explicit)
    db.delete_anchors_for_profile(uuid).map_err(|e| e.to_string())?;

    // Delete profile
    db.delete_profile(uuid).map_err(|e| e.to_string())?;

    // Queue tombstone
    let now = chrono::Utc::now().to_rfc3339();
    db.insert_profile_tombstone(uuid, &now)
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// List all paired peers
#[tauri::command]
pub fn list_peers(db: DbState) -> Result<Vec<PeerView>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let peers = db.list_peers().map_err(|e| e.to_string())?;

    Ok(peers
        .into_iter()
        .map(|p| PeerView {
            id: p.id.to_string(),
            name: p.name,
            fingerprint: p.fingerprint,
            paired_at: p.paired_at,
            last_seen: p.last_seen,
            is_online: p.is_online,
        })
        .collect())
}

/// List profiles pending deletion (for prompt)
#[tauri::command]
pub fn list_pending_deletions(db: DbState) -> Result<Vec<ProfileView>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let profiles = db.list_pending_deletions().map_err(|e| e.to_string())?;

    Ok(profiles
        .into_iter()
        .map(|p| ProfileView {
            id: p.id.to_string(),
            name: p.name,
            mode: p.mode,
            peer_name: p.peer_name,
            delete_propagation: p.delete_propagation,
            conflict_policy: p.conflict_policy,
            updated_at: p.updated_at,
            version: p.version,
            pending_deletion: p.pending_deletion,
        })
        .collect())
}

/// Confirm deletion of a pending profile
#[tauri::command]
pub fn confirm_deletion(id: String, db: DbState) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;

    db.delete_anchors_for_profile(uuid).map_err(|e| e.to_string())?;
    db.delete_profile(uuid).map_err(|e| e.to_string())?;

    Ok(())
}

/// Reject deletion and restore profile to active state
#[tauri::command]
pub fn reject_deletion(id: String, db: DbState) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;

    db.clear_pending_deletion(uuid)
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Get sync status for a profile (stub for now)
#[tauri::command]
pub fn get_sync_status(profile_id: String, _db: DbState) -> SyncStatus {
    // TODO: Implement actual status tracking via runs table
    SyncStatus {
        profile_id,
        last_sync_at: None,
        last_sync_direction: None,
        files_synced: None,
        status: "idle".to_string(),
        error_message: None,
    }
}

/// Get drift summary for a profile (stub for now)
#[tauri::command]
pub fn get_drift_summary(profile_id: String, _db: DbState) -> DriftSummary {
    // TODO: Implement actual drift calculation from index
    DriftSummary {
        profile_id,
        files_tracked: 0,
        pending_local_changes: 0,
        last_scan_at: chrono::Utc::now().to_rfc3339(),
    }
}
