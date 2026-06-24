use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};
use std::{fs, io};

use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tracing::{error, info, warn};
use uuid::Uuid;

use synccore::path::RelPath;
use synccore::reconcile::{ConflictPolicy, SyncMode};
use synccore::scan::{self, ScanConfig};
use syncstore::profiles::{AnchorRow, ProfileRow};

use crate::Error;
use crate::rpc::{
    AnchorSpec, ErrorCode, RpcBody, RpcRequest, RpcResponse, WireAnchor, WireProfile,
    WireProfileSummary, decode_message, encode_response,
};
use crate::transport::{Frame, FramedStream, MessageType};

struct SessionState {
    #[allow(dead_code)]
    peer_id: Uuid,
    #[allow(dead_code)]
    profile_id: Uuid,
    allowed_anchors: HashMap<Uuid, PathBuf>,
}

fn validate_rel_path(path: &RelPath) -> Result<(), RpcResponse> {
    if !path.is_safe() {
        return Err(RpcResponse::Error {
            code: ErrorCode::AccessDenied,
            message: format!("path traversal rejected: {}", path.display()),
        });
    }
    Ok(())
}


pub struct SyncHandler {
    peer_id: Uuid,
    instance_id: Uuid,
    db_path: Option<PathBuf>,
}

#[allow(clippy::unused_self, clippy::ref_option)]
impl SyncHandler {
    pub fn new(peer_id: Uuid, instance_id: Uuid) -> Self {
        Self {
            peer_id,
            instance_id,
            db_path: None,
        }
    }

    pub fn with_db_path(peer_id: Uuid, instance_id: Uuid, db_path: PathBuf) -> Self {
        Self {
            peer_id,
            instance_id,
            db_path: Some(db_path),
        }
    }

    fn open_db(&self) -> Option<syncstore::Db> {
        self.db_path
            .as_ref()
            .and_then(|p| syncstore::Db::open(p).ok())
    }

    #[allow(clippy::too_many_lines)]
    pub async fn serve<S: AsyncRead + AsyncWrite + Unpin>(
        &self,
        stream: &mut FramedStream<S>,
    ) -> Result<(), Error> {
        let mut session: Option<SessionState> = None;

        loop {
            let frame = match stream.next().await {
                Some(Ok(f)) => f,
                Some(Err(e)) => {
                    if is_connection_closed(&e) {
                        info!("peer disconnected");
                        return Ok(());
                    }
                    error!("handler frame error: {e}");
                    return Err(e);
                }
                None => {
                    info!("peer disconnected");
                    return Ok(());
                }
            };

            match frame.msg_type {
                MessageType::Shutdown => {
                    info!("peer sent shutdown");
                    return Ok(());
                }
                MessageType::RpcRequest => {
                    let msg = decode_message(&frame.payload)?;
                    let RpcBody::Request(request) = msg.body else {
                        warn!("expected request, got response");
                        continue;
                    };
                    let req_id = msg.id;

                    match request {
                        RpcRequest::PutFile {
                            anchor_id,
                            path,
                            size: _,
                            mtime_secs,
                        } => {
                            let prep = self.put_file_validate(anchor_id, &path, &session);
                            match prep {
                                Ok(dest_path) => {
                                    let resp_bytes = encode_response(req_id, RpcResponse::Ok)?;
                                    send_frame(stream, MessageType::RpcResponse, resp_bytes)
                                        .await?;

                                    let write_result = self
                                        .receive_and_write_file(stream, &dest_path, mtime_secs)
                                        .await;

                                    let final_resp = match write_result {
                                        Ok(()) => RpcResponse::Ok,
                                        Err(e) => RpcResponse::Error {
                                            code: ErrorCode::IoError,
                                            message: e.to_string(),
                                        },
                                    };
                                    let resp_bytes = encode_response(req_id + 1, final_resp)?;
                                    send_frame(stream, MessageType::RpcResponse, resp_bytes)
                                        .await?;
                                }
                                Err(resp) => {
                                    let resp_bytes = encode_response(req_id, resp)?;
                                    send_frame(stream, MessageType::RpcResponse, resp_bytes)
                                        .await?;
                                }
                            }
                        }
                        RpcRequest::GetFiles { anchor_id, paths } => {
                            let validate = self.get_files_validate(anchor_id, &paths, &session);
                            match validate {
                                Ok(root) => {
                                    let resp_bytes = encode_response(req_id, RpcResponse::Ok)?;
                                    send_frame(stream, MessageType::RpcResponse, resp_bytes)
                                        .await?;

                                    for path in &paths {
                                        self.send_file(stream, &root, path).await?;
                                    }
                                }
                                Err(resp) => {
                                    let resp_bytes = encode_response(req_id, resp)?;
                                    send_frame(stream, MessageType::RpcResponse, resp_bytes)
                                        .await?;
                                }
                            }
                        }
                        RpcRequest::QuickSend {
                            transfer_id,
                            destination_dir,
                            entries,
                        } => {
                            let result = self
                                .handle_quick_send(stream, &destination_dir, &entries)
                                .await;
                            let resp = match result {
                                Ok(count) => RpcResponse::QuickSendAck {
                                    transfer_id,
                                    files_written: count,
                                },
                                Err(e) => RpcResponse::Error {
                                    code: ErrorCode::IoError,
                                    message: e.to_string(),
                                },
                            };
                            let resp_bytes = encode_response(req_id, resp)?;
                            send_frame(stream, MessageType::RpcResponse, resp_bytes).await?;
                        }
                        RpcRequest::RenameRemote {
                            anchor_id,
                            path,
                            new_name,
                        } => {
                            let response =
                                self.rename_remote(anchor_id, &path, &new_name, &session);
                            let resp_bytes = encode_response(req_id, response)?;
                            send_frame(stream, MessageType::RpcResponse, resp_bytes).await?;
                        }
                        RpcRequest::ListProfiles => {
                            let response = self.handle_list_profiles();
                            let resp_bytes = encode_response(req_id, response)?;
                            send_frame(stream, MessageType::RpcResponse, resp_bytes).await?;
                        }
                        RpcRequest::GetProfile { profile_id } => {
                            let response = self.handle_get_profile(profile_id);
                            let resp_bytes = encode_response(req_id, response)?;
                            send_frame(stream, MessageType::RpcResponse, resp_bytes).await?;
                        }
                        RpcRequest::ReplicateProfile { profile } => {
                            let response = self.handle_replicate_profile(&profile);
                            let resp_bytes = encode_response(req_id, response)?;
                            send_frame(stream, MessageType::RpcResponse, resp_bytes).await?;
                        }
                        RpcRequest::ProfileDeleted {
                            profile_id,
                            deleted_at,
                        } => {
                            let response = self.handle_profile_deleted(profile_id, &deleted_at);
                            let resp_bytes = encode_response(req_id, response)?;
                            send_frame(stream, MessageType::RpcResponse, resp_bytes).await?;
                        }
                        other => {
                            let response = self.handle_request(other, &mut session);
                            let resp_bytes = encode_response(req_id, response)?;
                            send_frame(stream, MessageType::RpcResponse, resp_bytes).await?;
                        }
                    }
                }
                _ => {
                    warn!("unexpected frame type in handler: {:?}", frame.msg_type);
                }
            }
        }
    }

    fn handle_request(
        &self,
        request: RpcRequest,
        session: &mut Option<SessionState>,
    ) -> RpcResponse {
        match request {
            RpcRequest::StartSession {
                profile_id,
                mode: _,
                anchors,
                initiator_unix_secs,
            } => self.start_session(profile_id, anchors, initiator_unix_secs, session),

            RpcRequest::ScanRemote { anchor_id, config } => {
                self.scan_remote(anchor_id, &config, session)
            }

            RpcRequest::MkdirRemote { anchor_id, path } => {
                self.mkdir_remote(anchor_id, &path, session)
            }

            RpcRequest::DeleteRemote { anchor_id, path } => {
                self.delete_remote(anchor_id, &path, session)
            }

            RpcRequest::EndSession { run_id } => {
                info!(run_id = %run_id, "session ended");
                *session = None;
                RpcResponse::Ok
            }

            // Handled separately in serve() for streaming or direct dispatch
            RpcRequest::PutFile { .. }
            | RpcRequest::GetFiles { .. }
            | RpcRequest::QuickSend { .. }
            | RpcRequest::RenameRemote { .. }
            | RpcRequest::ListProfiles
            | RpcRequest::GetProfile { .. }
            | RpcRequest::ReplicateProfile { .. }
            | RpcRequest::ProfileDeleted { .. } => RpcResponse::Error {
                code: ErrorCode::Internal,
                message: "bug: this RPC is handled in serve loop".to_owned(),
            },
        }
    }

    fn start_session(
        &self,
        profile_id: Uuid,
        anchors: Vec<AnchorSpec>,
        initiator_unix_secs: i64,
        session: &mut Option<SessionState>,
    ) -> RpcResponse {
        let mut allowed = HashMap::new();
        for anchor in anchors {
            let path = PathBuf::from(&anchor.remote_path);
            if !path.exists() {
                return RpcResponse::Error {
                    code: ErrorCode::NotFound,
                    message: format!("anchor path does not exist: {}", anchor.remote_path),
                };
            }
            allowed.insert(anchor.id, path);
        }

        *session = Some(SessionState {
            peer_id: self.peer_id,
            profile_id,
            allowed_anchors: allowed,
        });

        let responder_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let clock_offset_secs = responder_secs - initiator_unix_secs;

        info!(
            profile_id = %profile_id,
            peer_id = %self.peer_id,
            clock_offset_secs,
            "session started"
        );
        RpcResponse::SessionStarted { clock_offset_secs }
    }

    fn scan_remote(
        &self,
        anchor_id: Uuid,
        config: &ScanConfig,
        session: &Option<SessionState>,
    ) -> RpcResponse {
        let Some(sess) = session else {
            return RpcResponse::Error {
                code: ErrorCode::InvalidSession,
                message: "no active session".to_owned(),
            };
        };

        let Some(root) = sess.allowed_anchors.get(&anchor_id) else {
            return RpcResponse::Error {
                code: ErrorCode::AccessDenied,
                message: format!("anchor {anchor_id} not allowed"),
            };
        };

        match scan::scan_tree(root, config) {
            Ok(snapshot) => RpcResponse::Snapshot(snapshot),
            Err(e) => RpcResponse::Error {
                code: ErrorCode::IoError,
                message: format!("scan failed: {e}"),
            },
        }
    }

    fn get_files_validate(
        &self,
        anchor_id: Uuid,
        paths: &[RelPath],
        session: &Option<SessionState>,
    ) -> Result<PathBuf, RpcResponse> {
        let sess = session.as_ref().ok_or(RpcResponse::Error {
            code: ErrorCode::InvalidSession,
            message: "no active session".to_owned(),
        })?;

        let root = sess
            .allowed_anchors
            .get(&anchor_id)
            .ok_or(RpcResponse::Error {
                code: ErrorCode::AccessDenied,
                message: format!("anchor {anchor_id} not allowed"),
            })?;

        for path in paths {
            validate_rel_path(path)?;
            let full = root.join(path.to_path_buf());
            if !full.is_file() {
                return Err(RpcResponse::Error {
                    code: ErrorCode::NotFound,
                    message: format!("file not found: {}", path.display()),
                });
            }
        }

        Ok(root.clone())
    }

    fn put_file_validate(
        &self,
        anchor_id: Uuid,
        path: &RelPath,
        session: &Option<SessionState>,
    ) -> Result<PathBuf, RpcResponse> {
        let sess = session.as_ref().ok_or(RpcResponse::Error {
            code: ErrorCode::InvalidSession,
            message: "no active session".to_owned(),
        })?;

        let root = sess
            .allowed_anchors
            .get(&anchor_id)
            .ok_or(RpcResponse::Error {
                code: ErrorCode::AccessDenied,
                message: format!("anchor {anchor_id} not allowed"),
            })?;

        validate_rel_path(path)?;
        let dest = root.join(path.to_path_buf());
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| RpcResponse::Error {
                code: ErrorCode::IoError,
                message: format!("create parent dirs: {e}"),
            })?;
        }

        Ok(dest)
    }

    fn mkdir_remote(
        &self,
        anchor_id: Uuid,
        path: &RelPath,
        session: &Option<SessionState>,
    ) -> RpcResponse {
        let Some(sess) = session else {
            return RpcResponse::Error {
                code: ErrorCode::InvalidSession,
                message: "no active session".to_owned(),
            };
        };

        let Some(root) = sess.allowed_anchors.get(&anchor_id) else {
            return RpcResponse::Error {
                code: ErrorCode::AccessDenied,
                message: format!("anchor {anchor_id} not allowed"),
            };
        };

        if !path.is_safe() {
            return RpcResponse::Error {
                code: ErrorCode::AccessDenied,
                message: format!("path traversal rejected: {}", path.display()),
            };
        }
        let full = root.join(path.to_path_buf());
        if let Err(e) = fs::create_dir_all(&full) {
            return RpcResponse::Error {
                code: ErrorCode::IoError,
                message: format!("mkdir: {e}"),
            };
        }

        RpcResponse::Ok
    }

    fn delete_remote(
        &self,
        anchor_id: Uuid,
        path: &RelPath,
        session: &Option<SessionState>,
    ) -> RpcResponse {
        let Some(sess) = session else {
            return RpcResponse::Error {
                code: ErrorCode::InvalidSession,
                message: "no active session".to_owned(),
            };
        };

        let Some(root) = sess.allowed_anchors.get(&anchor_id) else {
            return RpcResponse::Error {
                code: ErrorCode::AccessDenied,
                message: format!("anchor {anchor_id} not allowed"),
            };
        };

        if !path.is_safe() {
            return RpcResponse::Error {
                code: ErrorCode::AccessDenied,
                message: format!("path traversal rejected: {}", path.display()),
            };
        }
        let full = root.join(path.to_path_buf());
        let result = if full.is_dir() {
            fs::remove_dir_all(&full)
        } else if full.exists() {
            fs::remove_file(&full)
        } else {
            Ok(())
        };

        match result {
            Ok(()) => RpcResponse::Ok,
            Err(e) => RpcResponse::Error {
                code: ErrorCode::IoError,
                message: format!("delete: {e}"),
            },
        }
    }

    fn rename_remote(
        &self,
        anchor_id: Uuid,
        path: &RelPath,
        new_name: &str,
        session: &Option<SessionState>,
    ) -> RpcResponse {
        let Some(sess) = session else {
            return RpcResponse::Error {
                code: ErrorCode::InvalidSession,
                message: "no active session".to_owned(),
            };
        };

        let Some(root) = sess.allowed_anchors.get(&anchor_id) else {
            return RpcResponse::Error {
                code: ErrorCode::AccessDenied,
                message: format!("anchor {anchor_id} not allowed"),
            };
        };

        if !path.is_safe() {
            return RpcResponse::Error {
                code: ErrorCode::AccessDenied,
                message: format!("path traversal rejected: {}", path.display()),
            };
        }
        if new_name.contains("../") || new_name.starts_with('/') || new_name.ends_with("..") || new_name == ".." {
            return RpcResponse::Error {
                code: ErrorCode::AccessDenied,
                message: format!("path traversal rejected: {new_name}"),
            };
        }
        let from = root.join(path.to_path_buf());
        let to = root.join(new_name);

        if let Some(parent) = to.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                return RpcResponse::Error {
                    code: ErrorCode::IoError,
                    message: format!("create parent for rename: {e}"),
                };
            }
        }

        match fs::rename(&from, &to) {
            Ok(()) => RpcResponse::Ok,
            Err(e) => RpcResponse::Error {
                code: ErrorCode::IoError,
                message: format!("rename: {e}"),
            },
        }
    }

    async fn receive_and_write_file<S: AsyncRead + AsyncWrite + Unpin>(
        &self,
        stream: &mut FramedStream<S>,
        dest: &Path,
        mtime_secs: i64,
    ) -> Result<(), Error> {
        let mut data = Vec::new();

        loop {
            let frame = stream
                .next()
                .await
                .ok_or_else(|| {
                    Error::Transport("connection closed during file receive".to_owned())
                })?
                .map_err(|e| Error::Transport(format!("recv file data: {e}")))?;

            if frame.msg_type != MessageType::FileData {
                return Err(Error::Transport(format!(
                    "expected FileData, got {:?}",
                    frame.msg_type
                )));
            }

            if frame.payload.is_empty() {
                break;
            }

            data.extend_from_slice(&frame.payload);
        }

        atomic_write_file(dest, &data, mtime_secs)
            .map_err(|e| Error::Session(format!("atomic write: {e}")))?;

        Ok(())
    }

    async fn send_file<S: AsyncRead + AsyncWrite + Unpin>(
        &self,
        stream: &mut FramedStream<S>,
        root: &Path,
        path: &RelPath,
    ) -> Result<(), Error> {
        const CHUNK_SIZE: usize = 256 * 1024;

        let full = root.join(path.to_path_buf());
        let data =
            fs::read(&full).map_err(|e| Error::Session(format!("read {}: {e}", full.display())))?;

        for chunk in data.chunks(CHUNK_SIZE) {
            send_frame(stream, MessageType::FileData, chunk.to_vec()).await?;
        }

        // Empty frame signals end of file
        send_frame(stream, MessageType::FileData, Vec::new()).await?;

        Ok(())
    }

    async fn handle_quick_send<S: AsyncRead + AsyncWrite + Unpin>(
        &self,
        stream: &mut FramedStream<S>,
        destination_dir: &str,
        entries: &[crate::rpc::QuickSendEntry],
    ) -> Result<u64, Error> {
        let dest_root = PathBuf::from(destination_dir);
        if dest_root.to_string_lossy().contains("..") {
            return Err(Error::Session("path traversal in destination_dir".to_owned()));
        }
        if !dest_root.exists() {
            fs::create_dir_all(&dest_root)
                .map_err(|e| Error::Session(format!("create dest dir: {e}")))?;
        }

        let ack_bytes = encode_response(0, RpcResponse::Ok)?;
        send_frame(stream, MessageType::RpcResponse, ack_bytes).await?;

        let mut files_written: u64 = 0;

        for entry in entries {
            if !entry.rel_path.is_safe() {
                warn!("quick_send: rejecting unsafe path: {}", entry.rel_path.display());
                continue;
            }
            let dest_path = dest_root.join(entry.rel_path.to_path_buf());

            if entry.is_dir {
                fs::create_dir_all(&dest_path)
                    .map_err(|e| Error::Session(format!("mkdir: {e}")))?;
                continue;
            }

            self.receive_and_write_file(stream, &dest_path, entry.mtime_secs)
                .await?;
            files_written += 1;
        }

        Ok(files_written)
    }

    // --- Profile replication handlers (FR-PS) ---

    fn handle_list_profiles(&self) -> RpcResponse {
        let Some(db) = self.open_db() else {
            return RpcResponse::ProfileList {
                profiles: Vec::new(),
            };
        };

        let profiles = match db.list_profiles_for_peer(self.peer_id) {
            Ok(p) => p,
            Err(e) => {
                return RpcResponse::Error {
                    code: ErrorCode::Internal,
                    message: format!("db error: {e}"),
                };
            }
        };

        let summaries: Vec<WireProfileSummary> = profiles
            .iter()
            .map(|p| WireProfileSummary {
                id: p.id,
                name: p.name.clone(),
                mode: parse_sync_mode(&p.mode),
                version: p.version,
                updated_at: p.updated_at.clone(),
            })
            .collect();

        RpcResponse::ProfileList {
            profiles: summaries,
        }
    }

    fn handle_get_profile(&self, profile_id: Uuid) -> RpcResponse {
        let Some(db) = self.open_db() else {
            return RpcResponse::Error {
                code: ErrorCode::Internal,
                message: "no database available".to_owned(),
            };
        };

        let profile = match db.get_profile(profile_id) {
            Ok(Some(p)) => p,
            Ok(None) => {
                return RpcResponse::Error {
                    code: ErrorCode::NotFound,
                    message: "profile not found".to_owned(),
                };
            }
            Err(e) => {
                return RpcResponse::Error {
                    code: ErrorCode::Internal,
                    message: format!("db error: {e}"),
                };
            }
        };

        let anchors = match db.get_anchors(profile_id) {
            Ok(a) => a,
            Err(e) => {
                return RpcResponse::Error {
                    code: ErrorCode::Internal,
                    message: format!("db error loading anchors: {e}"),
                };
            }
        };

        let wire = profile_to_wire(&profile, &anchors, self.instance_id);
        RpcResponse::ProfileData { profile: wire }
    }

    fn handle_replicate_profile(&self, incoming: &WireProfile) -> RpcResponse {
        let Some(db) = self.open_db() else {
            return RpcResponse::ProfileAccepted;
        };

        let existing = match db.get_profile(incoming.id) {
            Ok(p) => p,
            Err(e) => {
                return RpcResponse::Error {
                    code: ErrorCode::Internal,
                    message: format!("db error: {e}"),
                };
            }
        };

        match existing {
            None => {
                // New profile — insert with paths mapped to our perspective
                let (row, anchors) = wire_to_profile(incoming, self.instance_id);
                if let Err(e) = db.insert_profile(&row) {
                    return RpcResponse::Error {
                        code: ErrorCode::Internal,
                        message: format!("insert profile: {e}"),
                    };
                }
                for anchor in &anchors {
                    if let Err(e) = db.insert_anchor(anchor) {
                        return RpcResponse::Error {
                            code: ErrorCode::Internal,
                            message: format!("insert anchor: {e}"),
                        };
                    }
                }
                info!(profile_id = %incoming.id, version = incoming.version, "accepted new profile");
                RpcResponse::ProfileAccepted
            }
            Some(local) => match incoming.version.cmp(&local.version) {
                std::cmp::Ordering::Greater => {
                    // Incoming is newer — update local
                    let (row, anchors) = wire_to_profile(incoming, self.instance_id);
                    if let Err(e) = db.delete_anchors_for_profile(row.id) {
                        return RpcResponse::Error {
                            code: ErrorCode::Internal,
                            message: format!("clear anchors: {e}"),
                        };
                    }
                    if let Err(e) = db.update_profile(&row) {
                        return RpcResponse::Error {
                            code: ErrorCode::Internal,
                            message: format!("update profile: {e}"),
                        };
                    }
                    for anchor in &anchors {
                        if let Err(e) = db.insert_anchor(anchor) {
                            return RpcResponse::Error {
                                code: ErrorCode::Internal,
                                message: format!("insert anchor: {e}"),
                            };
                        }
                    }
                    info!(
                        profile_id = %incoming.id,
                        incoming_version = incoming.version,
                        local_version = local.version,
                        "accepted newer profile version"
                    );
                    RpcResponse::ProfileAccepted
                }
                std::cmp::Ordering::Less => {
                    // Local is newer — tell initiator to update
                    let local_anchors = match db.get_anchors(local.id) {
                        Ok(a) => a,
                        Err(e) => {
                            return RpcResponse::Error {
                                code: ErrorCode::Internal,
                                message: format!("db error: {e}"),
                            };
                        }
                    };
                    let local_wire = profile_to_wire(&local, &local_anchors, self.instance_id);
                    info!(
                        profile_id = %incoming.id,
                        incoming_version = incoming.version,
                        local_version = local.version,
                        "rejected: local version is newer"
                    );
                    RpcResponse::ProfileConflict {
                        local_version: local_wire,
                    }
                }
                std::cmp::Ordering::Equal => {
                    // Same version — no-op
                    RpcResponse::ProfileAccepted
                }
            },
        }
    }

    fn handle_profile_deleted(&self, profile_id: Uuid, deleted_at: &str) -> RpcResponse {
        let Some(db) = self.open_db() else {
            return RpcResponse::Ok;
        };

        // Mark as pending deletion rather than hard-deleting
        match db.get_profile(profile_id) {
            Ok(Some(mut profile)) => {
                profile.pending_deletion = true;
                if let Err(e) = db.update_profile(&profile) {
                    return RpcResponse::Error {
                        code: ErrorCode::Internal,
                        message: format!("mark pending deletion: {e}"),
                    };
                }
                info!(profile_id = %profile_id, deleted_at, "profile marked as pending deletion");
                RpcResponse::Ok
            }
            Ok(None) => RpcResponse::Ok,
            Err(e) => RpcResponse::Error {
                code: ErrorCode::Internal,
                message: format!("db error: {e}"),
            },
        }
    }
}

// --- Conversion helpers ---

/// Convert a local ProfileRow + anchors into a WireProfile for sending to a peer.
/// `local_instance_id` is this instance's UUID — it becomes the origin in the wire format.
pub fn profile_to_wire(
    profile: &ProfileRow,
    anchors: &[AnchorRow],
    local_instance_id: Uuid,
) -> WireProfile {
    let origin = Uuid::parse_str(&profile.origin_instance_id).unwrap_or(local_instance_id);

    let wire_anchors: Vec<WireAnchor> = anchors
        .iter()
        .map(|a| {
            // If we are the origin instance, local_path is side_a; otherwise it's side_b
            if origin == local_instance_id {
                WireAnchor {
                    side_a_path: a.local_path.clone(),
                    side_b_path: a.remote_path.clone(),
                    max_depth: a.max_depth,
                    include_hidden: a.include_hidden,
                    ignore_patterns: a.ignore_patterns.clone(),
                }
            } else {
                WireAnchor {
                    side_a_path: a.remote_path.clone(),
                    side_b_path: a.local_path.clone(),
                    max_depth: a.max_depth,
                    include_hidden: a.include_hidden,
                    ignore_patterns: a.ignore_patterns.clone(),
                }
            }
        })
        .collect();

    WireProfile {
        id: profile.id,
        name: profile.name.clone(),
        mode: parse_sync_mode(&profile.mode),
        delete_propagation: profile.delete_propagation,
        conflict_policy: parse_conflict_policy(&profile.conflict_policy),
        version: profile.version,
        updated_at: profile.updated_at.clone(),
        origin_instance_id: origin,
        anchors: wire_anchors,
    }
}

/// Convert a WireProfile received from a peer into a local ProfileRow + anchors.
/// `local_instance_id` is this instance's UUID — used to determine which side of the
/// path mapping is "local" to us.
pub fn wire_to_profile(
    wire: &WireProfile,
    local_instance_id: Uuid,
) -> (ProfileRow, Vec<AnchorRow>) {
    let i_am_origin = wire.origin_instance_id == local_instance_id;

    let row = ProfileRow {
        id: wire.id,
        name: wire.name.clone(),
        mode: sync_mode_to_str(wire.mode).to_owned(),
        delete_propagation: wire.delete_propagation,
        conflict_policy: conflict_policy_to_str(wire.conflict_policy).to_owned(),
        peer_name: String::new(),
        created_at: String::new(),
        updated_at: wire.updated_at.clone(),
        version: wire.version,
        peer_id: String::new(), // Caller should fill in the peer's UUID
        origin_instance_id: wire.origin_instance_id.to_string(),
        pending_deletion: false,
    };

    let anchors: Vec<AnchorRow> = wire
        .anchors
        .iter()
        .map(|wa| {
            let (local_path, remote_path) = if i_am_origin {
                (wa.side_a_path.clone(), wa.side_b_path.clone())
            } else {
                (wa.side_b_path.clone(), wa.side_a_path.clone())
            };
            AnchorRow {
                id: 0,
                profile_id: wire.id,
                local_path,
                remote_path,
                max_depth: wa.max_depth,
                include_hidden: wa.include_hidden,
                ignore_patterns: wa.ignore_patterns.clone(),
            }
        })
        .collect();

    (row, anchors)
}

fn parse_sync_mode(s: &str) -> SyncMode {
    match s {
        "push" => SyncMode::Push,
        "pull" => SyncMode::Pull,
        _ => SyncMode::Bidirectional,
    }
}

fn parse_conflict_policy(s: &str) -> ConflictPolicy {
    match s {
        "keep_both" => ConflictPolicy::KeepBoth,
        _ => ConflictPolicy::NewerWins,
    }
}

fn sync_mode_to_str(mode: SyncMode) -> &'static str {
    match mode {
        SyncMode::Push => "push",
        SyncMode::Pull => "pull",
        SyncMode::Bidirectional => "bidirectional",
    }
}

fn conflict_policy_to_str(policy: ConflictPolicy) -> &'static str {
    match policy {
        ConflictPolicy::NewerWins => "newer_wins",
        ConflictPolicy::KeepBoth => "keep_both",
    }
}

// --- Frame / file utilities ---

async fn send_frame<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut FramedStream<S>,
    msg_type: MessageType,
    payload: Vec<u8>,
) -> Result<(), Error> {
    stream
        .send(Frame { msg_type, payload })
        .await
        .map_err(|e| Error::Transport(format!("send frame: {e}")))
}

/// Write file data atomically to the destination path.
pub fn atomic_write_file(dest: &Path, data: &[u8], mtime_secs: i64) -> io::Result<()> {
    let parent = dest.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;

    let tmp_name = format!(".filesync-tmp-{}", Uuid::new_v4());
    let tmp_path = parent.join(&tmp_name);

    {
        let mut f = fs::File::create(&tmp_path)?;
        f.write_all(data)?;
        f.flush()?;
        f.sync_all()?;
    }

    if mtime_secs > 0 {
        let mtime = UNIX_EPOCH + Duration::from_secs(mtime_secs as u64);
        filetime_set(&tmp_path, mtime);
    }

    fs::rename(&tmp_path, dest)?;
    Ok(())
}

fn filetime_set(_path: &Path, _mtime: std::time::SystemTime) {
    // TODO: use filetime crate
}

fn is_connection_closed(e: &Error) -> bool {
    if let Error::Io(io_err) = e {
        matches!(
            io_err.kind(),
            io::ErrorKind::UnexpectedEof
                | io::ErrorKind::ConnectionReset
                | io::ErrorKind::BrokenPipe
        )
    } else {
        let msg = e.to_string();
        msg.contains("close_notify") || msg.contains("UnexpectedEof")
    }
}
