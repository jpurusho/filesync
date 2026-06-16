use serde::{Deserialize, Serialize};
use uuid::Uuid;

use synccore::path::RelPath;
use synccore::reconcile::{ConflictPolicy, SyncMode};
use synccore::scan::{ScanConfig, Snapshot};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcMessage {
    pub id: u32,
    pub body: RpcBody,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RpcBody {
    Request(RpcRequest),
    Response(RpcResponse),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RpcRequest {
    StartSession {
        profile_id: Uuid,
        mode: SyncMode,
        anchors: Vec<AnchorSpec>,
        /// Initiator's current Unix timestamp; responder uses this to compute clock offset.
        initiator_unix_secs: i64,
    },
    ScanRemote {
        anchor_id: Uuid,
        config: ScanConfig,
    },
    GetFiles {
        anchor_id: Uuid,
        paths: Vec<RelPath>,
    },
    PutFile {
        anchor_id: Uuid,
        path: RelPath,
        size: u64,
        mtime_secs: i64,
    },
    MkdirRemote {
        anchor_id: Uuid,
        path: RelPath,
    },
    DeleteRemote {
        anchor_id: Uuid,
        path: RelPath,
    },
    EndSession {
        run_id: Uuid,
    },
    /// Quick-send: profile-less one-shot transfer (FR-SM-6).
    /// The destination_dir is where files land on the responder.
    QuickSend {
        transfer_id: Uuid,
        destination_dir: String,
        entries: Vec<QuickSendEntry>,
    },
    /// Rename a file on the remote side (used for KeepBoth conflict resolution).
    RenameRemote {
        anchor_id: Uuid,
        path: RelPath,
        new_name: String,
    },
    /// List profiles that target the requesting peer (FR-PS-1).
    ListProfiles,
    /// Get full profile config by ID.
    GetProfile {
        profile_id: Uuid,
    },
    /// Replicate a profile to the peer (sent during StartSession or standalone).
    ReplicateProfile {
        profile: WireProfile,
    },
    /// Notify peer that a profile has been deleted.
    ProfileDeleted {
        profile_id: Uuid,
        deleted_at: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickSendEntry {
    pub rel_path: RelPath,
    pub size: u64,
    pub mtime_secs: i64,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RpcResponse {
    Ok,
    /// Response to StartSession carrying the clock offset for skew compensation.
    SessionStarted {
        /// responder_unix_secs - initiator_unix_secs
        clock_offset_secs: i64,
    },
    Snapshot(Snapshot),
    FileHeader {
        path: RelPath,
        size: u64,
    },
    QuickSendAck {
        transfer_id: Uuid,
        files_written: u64,
    },
    /// Response to ListProfiles.
    ProfileList {
        profiles: Vec<WireProfileSummary>,
    },
    /// Response to GetProfile.
    ProfileData {
        profile: WireProfile,
    },
    /// Response to ReplicateProfile when local version is newer.
    ProfileConflict {
        local_version: WireProfile,
    },
    /// Response to ReplicateProfile when accepted.
    ProfileAccepted,
    Error {
        code: ErrorCode,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnchorSpec {
    pub id: Uuid,
    pub remote_path: String,
}

/// Wire format for profile replication (FR-PS-2).
/// Uses neutral path naming: side_a is origin instance, side_b is peer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireProfile {
    pub id: Uuid,
    pub name: String,
    pub mode: SyncMode,
    pub delete_propagation: bool,
    pub conflict_policy: ConflictPolicy,
    pub version: u64,
    pub updated_at: String,
    pub origin_instance_id: Uuid,
    pub anchors: Vec<WireAnchor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireAnchor {
    pub side_a_path: String, // path on origin instance
    pub side_b_path: String, // path on peer
    pub max_depth: i32,
    pub include_hidden: bool,
    pub ignore_patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireProfileSummary {
    pub id: Uuid,
    pub name: String,
    pub mode: SyncMode,
    pub version: u64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCode {
    InvalidSession,
    AccessDenied,
    NotFound,
    IoError,
    Internal,
}

pub fn encode_request(id: u32, req: RpcRequest) -> Result<Vec<u8>, crate::Error> {
    let msg = RpcMessage {
        id,
        body: RpcBody::Request(req),
    };
    rmp_serde::to_vec(&msg).map_err(|e| crate::Error::Rpc(format!("encode request: {e}")))
}

pub fn encode_response(id: u32, resp: RpcResponse) -> Result<Vec<u8>, crate::Error> {
    let msg = RpcMessage {
        id,
        body: RpcBody::Response(resp),
    };
    rmp_serde::to_vec(&msg).map_err(|e| crate::Error::Rpc(format!("encode response: {e}")))
}

pub fn decode_message(data: &[u8]) -> Result<RpcMessage, crate::Error> {
    rmp_serde::from_slice(data).map_err(|e| crate::Error::Rpc(format!("decode message: {e}")))
}
