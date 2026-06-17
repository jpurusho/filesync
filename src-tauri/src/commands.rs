use crate::views::{
    AnchorView, DriftSummary, PairingConfirmation, PeerView, ProfileDetail, ProfileInput,
    ProfileView, StartSyncResult, SyncStatus,
};
use std::net::SocketAddr;
use std::sync::Mutex;
use syncnet::identity::Identity;
use syncnet::pairing;
use syncstore::{Db, profiles::ProfileRow};
use tauri::{Emitter, State};
use uuid::Uuid;

type DbState<'a> = State<'a, Mutex<Db>>;
type IdentityState<'a> = State<'a, Mutex<Identity>>;

/// List all active profiles (excluding pending deletions)
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
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
#[allow(clippy::needless_pass_by_value)]
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
#[allow(clippy::needless_pass_by_value)]
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
#[allow(clippy::needless_pass_by_value)]
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
    db.delete_anchors_for_profile(uuid)
        .map_err(|e| e.to_string())?;
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
#[allow(clippy::needless_pass_by_value)]
pub fn delete_profile(id: String, db: DbState) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;

    // Delete anchors first (foreign key cascade should handle this, but be explicit)
    db.delete_anchors_for_profile(uuid)
        .map_err(|e| e.to_string())?;

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
#[allow(clippy::needless_pass_by_value)]
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
#[allow(clippy::needless_pass_by_value)]
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
#[allow(clippy::needless_pass_by_value)]
pub fn confirm_deletion(id: String, db: DbState) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;

    db.delete_anchors_for_profile(uuid)
        .map_err(|e| e.to_string())?;
    db.delete_profile(uuid).map_err(|e| e.to_string())?;

    Ok(())
}

/// Reject deletion and restore profile to active state
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn reject_deletion(id: String, db: DbState) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;

    db.clear_pending_deletion(uuid).map_err(|e| e.to_string())?;

    Ok(())
}

/// Get sync status for a profile (stub for now)
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
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
#[allow(clippy::needless_pass_by_value)]
pub fn get_drift_summary(profile_id: String, _db: DbState) -> DriftSummary {
    // TODO: Implement actual drift calculation from index
    DriftSummary {
        profile_id,
        files_tracked: 0,
        pending_local_changes: 0,
        last_scan_at: chrono::Utc::now().to_rfc3339(),
    }
}

/// Initiate pairing with a peer (async handshake).
///
/// M6 implementation: Auto-confirms the pairing and returns the peer fingerprint
/// for post-hoc verification in the UI. A more sophisticated implementation would
/// pause the handshake and wait for user confirmation before sending Confirm message.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub async fn pair_peer(
    address: String,
    identity: IdentityState<'_>,
    db: DbState<'_>,
) -> Result<PairingConfirmation, String> {
    let peer_addr: SocketAddr = address.parse().map_err(|e| format!("Invalid address: {e}"))?;

    // Clone identity to avoid holding lock across await
    let identity_clone = {
        let identity = identity.lock().map_err(|e| e.to_string())?;
        identity.clone()
    };

    // Perform pairing handshake (auto-confirm for M6)
    let result = pairing::initiate_pairing(&identity_clone, peer_addr, |_fingerprint| true)
        .await
        .map_err(|e| e.to_string())?;

    // Store peer in database
    let db = db.lock().map_err(|e| e.to_string())?;
    let peer_row = syncstore::peers::PeerRow {
        id: result.peer_id,
        name: result.peer_name.clone(),
        cert_pem: result.peer_cert_pem.clone(),
        fingerprint: result.peer_fingerprint.to_string(),
        paired_at: chrono::Utc::now().to_rfc3339(),
        last_seen: None,
        is_online: false,
    };

    db.insert_peer(&peer_row).map_err(|e| e.to_string())?;

    Ok(PairingConfirmation {
        peer_id: result.peer_id.to_string(),
        peer_name: result.peer_name,
        peer_fingerprint: result.peer_fingerprint.to_string(),
    })
}

/// Unpair a peer (remove from database)
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn unpair_peer(peer_id: String, db: DbState) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let uuid = Uuid::parse_str(&peer_id).map_err(|e| e.to_string())?;

    db.delete_peer(uuid).map_err(|e| e.to_string())?;

    Ok(())
}

/// Start a sync session (stub for M6 - returns immediately, no actual sync yet)
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub async fn start_sync(
    profile_id: String,
    direction: String, // "push" | "pull" | "bidi"
    _db: DbState<'_>,
    _identity: IdentityState<'_>,
    app_handle: tauri::AppHandle,
) -> Result<StartSyncResult, String> {
    let profile_uuid = Uuid::parse_str(&profile_id).map_err(|e| e.to_string())?;
    let run_id = Uuid::new_v4();

    // M6 stub: emit fake progress events
    let app_clone = app_handle.clone();
    let profile_id_clone = profile_id.clone();
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        let _ = app_clone.emit("sync-progress", serde_json::json!({
            "profile_id": profile_id_clone,
            "status": "scanning",
            "progress": 0.2,
        }));

        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
        let _ = app_clone.emit("sync-progress", serde_json::json!({
            "profile_id": profile_id_clone,
            "status": "transferring",
            "progress": 0.6,
        }));

        tokio::time::sleep(tokio::time::Duration::from_millis(800)).await;
        let _ = app_clone.emit("sync-complete", serde_json::json!({
            "profile_id": profile_id_clone,
            "run_id": run_id.to_string(),
            "files_transferred": 42,
            "bytes_transferred": 123456,
        }));
    });

    Ok(StartSyncResult {
        run_id: run_id.to_string(),
        profile_id: profile_uuid.to_string(),
        direction,
    })
}
