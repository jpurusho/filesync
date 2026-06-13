use serde::{Deserialize, Serialize};
use uuid::Uuid;

use synccore::path::RelPath;
use synccore::reconcile::SyncMode;
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RpcResponse {
    Ok,
    Snapshot(Snapshot),
    FileHeader {
        path: RelPath,
        size: u64,
    },
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
