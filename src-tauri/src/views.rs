use serde::{Deserialize, Serialize};

/// Simplified profile view for list display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileView {
    pub id: String,
    pub name: String,
    pub mode: String,
    pub peer_name: String,
    pub delete_propagation: bool,
    pub conflict_policy: String,
    pub updated_at: String,
    pub version: u64,
    pub pending_deletion: bool,
}

/// Full profile detail including anchors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileDetail {
    pub id: String,
    pub name: String,
    pub mode: String,
    pub peer_name: String,
    pub peer_id: String,
    pub delete_propagation: bool,
    pub conflict_policy: String,
    pub created_at: String,
    pub updated_at: String,
    pub version: u64,
    pub origin_instance_id: String,
    pub pending_deletion: bool,
    pub anchors: Vec<AnchorView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnchorView {
    pub id: i64,
    pub local_path: String,
    pub remote_path: String,
    pub max_depth: i32,
    pub include_hidden: bool,
    pub ignore_patterns: Vec<String>,
}

/// Peer view for list and detail
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerView {
    pub id: String,
    pub name: String,
    pub fingerprint: String,
    pub paired_at: String,
    pub last_seen: Option<String>,
    pub is_online: bool,
}

/// Sync status for a profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatus {
    pub profile_id: String,
    pub last_sync_at: Option<String>,
    pub last_sync_direction: Option<String>,
    pub files_synced: Option<u64>,
    pub status: String, // "idle" | "running" | "error"
    pub error_message: Option<String>,
}

/// Input for creating/updating a profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileInput {
    pub name: String,
    pub mode: String,
    pub peer_name: String,
    pub peer_id: String,
    pub delete_propagation: bool,
    pub conflict_policy: String,
    pub anchors: Vec<AnchorInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnchorInput {
    pub local_path: String,
    pub remote_path: String,
    pub max_depth: i32,
    pub include_hidden: bool,
    pub ignore_patterns: Vec<String>,
}

/// Drift summary for a profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftSummary {
    pub profile_id: String,
    pub files_tracked: u64,
    pub pending_local_changes: u64,
    pub last_scan_at: String,
}

/// Pairing confirmation result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::struct_field_names)] // peer_ prefix is intentional for clarity
pub struct PairingConfirmation {
    pub peer_id: String,
    pub peer_name: String,
    pub peer_fingerprint: String,
}

/// Result of starting a sync session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartSyncResult {
    pub run_id: String,
    pub profile_id: String,
    pub direction: String,
}

/// Network info for display in UI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInfoView {
    pub listen_address: String,
    pub listen_port: u16,
    pub fingerprint: String,
    pub hostname: String,
}

/// A peer discovered via mDNS
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredPeerView {
    pub id: String,
    pub name: String,
    pub addresses: Vec<String>,
    pub fingerprint_short: String,
}
