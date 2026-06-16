use std::path::PathBuf;

use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tracing::{error, info};
use uuid::Uuid;

use synccore::diff::{self, IndexEntry, SyncIndex};
use synccore::path::RelPath;
use synccore::plan;
use synccore::reconcile::{self, Action, ConflictPolicy, ReconcileContext, Side, SyncMode};
use synccore::scan::{self, EntryKind, ScanConfig, Snapshot};

use crate::Error;
use crate::handler::{atomic_write_file, profile_to_wire, wire_to_profile};
use crate::rpc::{
    AnchorSpec, RpcBody, RpcRequest, RpcResponse, WireProfile, WireProfileSummary, decode_message,
    encode_request,
};
use crate::transport::{Frame, FramedStream, MessageType};

use syncstore::profiles::{AnchorRow, ProfileRow};

const FILE_CHUNK_SIZE: usize = 256 * 1024; // 256 KiB

pub struct RemoteSyncConfig {
    pub profile_id: Uuid,
    pub mode: SyncMode,
    pub conflict_policy: ConflictPolicy,
    pub delete_propagation: bool,
    pub peer_name: String,
    pub anchors: Vec<RemoteAnchor>,
}

pub struct RemoteAnchor {
    pub id: Uuid,
    pub local_path: PathBuf,
    pub remote_path: String,
    pub scan_config: ScanConfig,
}

#[derive(Debug)]
pub struct RemoteSyncResult {
    pub run_id: Uuid,
    pub files_transferred: u64,
    pub bytes_transferred: u64,
    pub errors: Vec<String>,
    /// Updated sync index reflecting all successfully transferred files.
    /// Callers should persist this to syncstore on success.
    pub updated_index: SyncIndex,
}

/// Run a push sync: initiator sends files to responder.
pub async fn run_remote_push<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut FramedStream<S>,
    config: &RemoteSyncConfig,
    index: &SyncIndex,
) -> Result<RemoteSyncResult, Error> {
    let run_id = Uuid::new_v4();
    let mut req_id: u32 = 0;
    let mut files_transferred: u64 = 0;
    let mut bytes_transferred: u64 = 0;
    let mut errors: Vec<String> = Vec::new();
    let mut updated_index = index.clone();

    // 1. Start session
    let anchor_specs: Vec<AnchorSpec> = config
        .anchors
        .iter()
        .map(|a| AnchorSpec {
            id: a.id,
            remote_path: a.remote_path.clone(),
        })
        .collect();

    req_id += 1;
    send_request(
        stream,
        req_id,
        RpcRequest::StartSession {
            profile_id: config.profile_id,
            mode: config.mode,
            anchors: anchor_specs,
            initiator_unix_secs: now_unix_secs(),
        },
    )
    .await?;
    // clock_offset not used for push, but we must read the SessionStarted response
    let _ = expect_session_started(stream).await?;

    info!(run_id = %run_id, "remote push session started");

    for anchor in &config.anchors {
        // 2. Scan remote
        req_id += 1;
        send_request(
            stream,
            req_id,
            RpcRequest::ScanRemote {
                anchor_id: anchor.id,
                config: anchor.scan_config.clone(),
            },
        )
        .await?;
        let remote_snap = expect_snapshot(stream).await?;

        // 3. Scan local
        let local_snap = scan::scan_tree(&anchor.local_path, &anchor.scan_config)
            .map_err(|e| Error::Session(format!("local scan: {e}")))?;

        // 4. Diff + Reconcile + Plan
        let diff = diff::compute_diff(
            &local_snap,
            &remote_snap,
            index,
            &anchor.local_path,
            &anchor.local_path,
        )
        .map_err(|e| Error::Session(format!("diff: {e}")))?;

        let ctx = ReconcileContext {
            local_entries: &local_snap.entries,
            remote_entries: &remote_snap.entries,
            delete_propagation: config.delete_propagation,
            peer_name: config.peer_name.clone(),
            clock_offset_secs: 0,
        };

        let mut sync_plan =
            reconcile::reconcile(&diff, index, config.mode, config.conflict_policy, &ctx);
        plan::dedup_dirs(&mut sync_plan);
        plan::order_actions(&mut sync_plan);

        // 5. Execute plan over RPC
        let mut executed: Vec<&Action> = Vec::new();
        for action in &sync_plan.actions {
            let result =
                execute_push_action(stream, action, anchor.id, &anchor.local_path, &mut req_id)
                    .await;

            match result {
                Ok(bytes) => {
                    executed.push(action);
                    if bytes > 0 {
                        files_transferred += 1;
                        bytes_transferred += bytes;
                    }
                }
                Err(e) => {
                    error!("action error: {e}");
                    errors.push(e.to_string());
                }
            }
        }

        apply_actions_to_index(&mut updated_index, &executed, &local_snap, &remote_snap);
    }

    // 6. End session
    req_id += 1;
    send_request(stream, req_id, RpcRequest::EndSession { run_id }).await?;
    expect_ok(stream).await?;

    info!(
        run_id = %run_id,
        files = files_transferred,
        bytes = bytes_transferred,
        errors = errors.len(),
        "remote push complete"
    );

    Ok(RemoteSyncResult {
        run_id,
        files_transferred,
        bytes_transferred,
        errors,
        updated_index,
    })
}

/// Run a pull sync: initiator receives files from responder.
#[allow(clippy::too_many_lines)]
pub async fn run_remote_pull<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut FramedStream<S>,
    config: &RemoteSyncConfig,
    index: &SyncIndex,
) -> Result<RemoteSyncResult, Error> {
    let run_id = Uuid::new_v4();
    let mut req_id: u32 = 0;
    let mut files_transferred: u64 = 0;
    let mut bytes_transferred: u64 = 0;
    let mut errors: Vec<String> = Vec::new();
    let mut updated_index = index.clone();

    // 1. Start session
    let anchor_specs: Vec<AnchorSpec> = config
        .anchors
        .iter()
        .map(|a| AnchorSpec {
            id: a.id,
            remote_path: a.remote_path.clone(),
        })
        .collect();

    req_id += 1;
    send_request(
        stream,
        req_id,
        RpcRequest::StartSession {
            profile_id: config.profile_id,
            mode: config.mode,
            anchors: anchor_specs,
            initiator_unix_secs: now_unix_secs(),
        },
    )
    .await?;
    let _ = expect_session_started(stream).await?;

    info!(run_id = %run_id, "remote pull session started");

    for anchor in &config.anchors {
        // 2. Scan remote
        req_id += 1;
        send_request(
            stream,
            req_id,
            RpcRequest::ScanRemote {
                anchor_id: anchor.id,
                config: anchor.scan_config.clone(),
            },
        )
        .await?;
        let remote_snap = expect_snapshot(stream).await?;

        // 3. Scan local
        let local_snap = scan::scan_tree(&anchor.local_path, &anchor.scan_config)
            .map_err(|e| Error::Session(format!("local scan: {e}")))?;

        // 4. Diff + Reconcile + Plan (pull mode)
        let diff = diff::compute_diff(
            &local_snap,
            &remote_snap,
            index,
            &anchor.local_path,
            &anchor.local_path,
        )
        .map_err(|e| Error::Session(format!("diff: {e}")))?;

        let ctx = ReconcileContext {
            local_entries: &local_snap.entries,
            remote_entries: &remote_snap.entries,
            delete_propagation: config.delete_propagation,
            peer_name: config.peer_name.clone(),
            clock_offset_secs: 0,
        };

        let mut sync_plan =
            reconcile::reconcile(&diff, index, config.mode, config.conflict_policy, &ctx);
        plan::dedup_dirs(&mut sync_plan);
        plan::order_actions(&mut sync_plan);

        // 5. Collect paths we need to pull
        let paths_to_pull: Vec<RelPath> = sync_plan
            .actions
            .iter()
            .filter_map(|a| match a {
                Action::CopyFile {
                    from: Side::Remote,
                    path,
                } => Some(path.clone()),
                _ => None,
            })
            .collect();

        let mut executed: Vec<&Action> = Vec::new();

        // Create dirs locally
        for action in &sync_plan.actions {
            if let Action::CreateDir {
                on: Side::Local,
                path,
            } = action
            {
                let full = anchor.local_path.join(path.to_path_buf());
                if std::fs::create_dir_all(&full).is_ok() {
                    executed.push(action);
                } else if let Err(e) = std::fs::create_dir_all(&full) {
                    errors.push(format!("mkdir {}: {e}", path.display()));
                }
            }
        }

        // Handle local deletes (mirror mode)
        for action in &sync_plan.actions {
            if let Action::Delete {
                on: Side::Local,
                path,
            } = action
            {
                let full = anchor.local_path.join(path.to_path_buf());
                let result = if full.is_dir() {
                    std::fs::remove_dir_all(&full)
                } else if full.exists() {
                    std::fs::remove_file(&full)
                } else {
                    Ok(())
                };
                match result {
                    Ok(()) => executed.push(action),
                    Err(e) => errors.push(format!("delete {}: {e}", path.display())),
                }
            }
        }

        // 6. Request files from remote
        if !paths_to_pull.is_empty() {
            req_id += 1;
            send_request(
                stream,
                req_id,
                RpcRequest::GetFiles {
                    anchor_id: anchor.id,
                    paths: paths_to_pull.clone(),
                },
            )
            .await?;
            expect_ok(stream).await?;

            // Receive file data frames
            for path in &paths_to_pull {
                match receive_file(stream, &anchor.local_path, path).await {
                    Ok(size) => {
                        files_transferred += 1;
                        bytes_transferred += size;
                        // Find and mark the corresponding CopyFile action as executed
                        if let Some(action) = sync_plan.actions.iter().find(|a| {
                            matches!(a, Action::CopyFile { from: Side::Remote, path: p } if p == path)
                        }) {
                            executed.push(action);
                        }
                    }
                    Err(e) => {
                        errors.push(format!("recv {}: {e}", path.display()));
                    }
                }
            }
        }

        apply_actions_to_index(&mut updated_index, &executed, &local_snap, &remote_snap);
    }

    // 7. End session
    req_id += 1;
    send_request(stream, req_id, RpcRequest::EndSession { run_id }).await?;
    expect_ok(stream).await?;

    info!(
        run_id = %run_id,
        files = files_transferred,
        bytes = bytes_transferred,
        errors = errors.len(),
        "remote pull complete"
    );

    Ok(RemoteSyncResult {
        run_id,
        files_transferred,
        bytes_transferred,
        errors,
        updated_index,
    })
}

async fn execute_push_action<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut FramedStream<S>,
    action: &Action,
    anchor_id: Uuid,
    local_root: &std::path::Path,
    req_id: &mut u32,
) -> Result<u64, Error> {
    match action {
        Action::CreateDir {
            on: Side::Remote,
            path,
        } => {
            *req_id += 1;
            send_request(
                stream,
                *req_id,
                RpcRequest::MkdirRemote {
                    anchor_id,
                    path: path.clone(),
                },
            )
            .await?;
            expect_ok(stream).await?;
            Ok(0)
        }
        Action::CopyFile {
            from: Side::Local,
            path,
        } => {
            let source = local_root.join(path.to_path_buf());
            let data = std::fs::read(&source)
                .map_err(|e| Error::Session(format!("read {}: {e}", source.display())))?;
            let size = data.len() as u64;

            let mtime_secs = std::fs::metadata(&source)
                .and_then(|m| m.modified())
                .map_or(0, |t| {
                    t.duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| d.as_secs() as i64)
                });

            *req_id += 1;
            send_request(
                stream,
                *req_id,
                RpcRequest::PutFile {
                    anchor_id,
                    path: path.clone(),
                    size,
                    mtime_secs,
                },
            )
            .await?;
            expect_ok(stream).await?;

            // Send file data as FileData frames
            send_file_data(stream, &data).await?;

            // Wait for write confirmation
            expect_ok(stream).await?;

            Ok(size)
        }
        Action::Delete {
            on: Side::Remote,
            path,
        } => {
            *req_id += 1;
            send_request(
                stream,
                *req_id,
                RpcRequest::DeleteRemote {
                    anchor_id,
                    path: path.clone(),
                },
            )
            .await?;
            expect_ok(stream).await?;
            Ok(0)
        }
        // Actions on the local side are not sent over the network
        _ => Ok(0),
    }
}

async fn send_request<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut FramedStream<S>,
    id: u32,
    request: RpcRequest,
) -> Result<(), Error> {
    let payload = encode_request(id, request)?;
    stream
        .send(Frame {
            msg_type: MessageType::RpcRequest,
            payload,
        })
        .await
        .map_err(|e| Error::Transport(format!("send: {e}")))
}

async fn expect_ok<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut FramedStream<S>,
) -> Result<(), Error> {
    let frame = stream
        .next()
        .await
        .ok_or_else(|| Error::Rpc("connection closed".to_owned()))?
        .map_err(|e| Error::Transport(format!("recv: {e}")))?;

    if frame.msg_type != MessageType::RpcResponse {
        return Err(Error::Rpc(format!(
            "expected RpcResponse, got {:?}",
            frame.msg_type
        )));
    }

    let msg = decode_message(&frame.payload)?;
    match msg.body {
        RpcBody::Response(RpcResponse::Ok) => Ok(()),
        RpcBody::Response(RpcResponse::Error { code, message }) => {
            Err(Error::Rpc(format!("remote error ({code:?}): {message}")))
        }
        _ => Err(Error::Rpc("unexpected response type".to_owned())),
    }
}

async fn expect_snapshot<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut FramedStream<S>,
) -> Result<Snapshot, Error> {
    let frame = stream
        .next()
        .await
        .ok_or_else(|| Error::Rpc("connection closed".to_owned()))?
        .map_err(|e| Error::Transport(format!("recv: {e}")))?;

    if frame.msg_type != MessageType::RpcResponse {
        return Err(Error::Rpc(format!(
            "expected RpcResponse, got {:?}",
            frame.msg_type
        )));
    }

    let msg = decode_message(&frame.payload)?;
    match msg.body {
        RpcBody::Response(RpcResponse::Snapshot(snap)) => Ok(snap),
        RpcBody::Response(RpcResponse::Error { code, message }) => {
            Err(Error::Rpc(format!("remote error ({code:?}): {message}")))
        }
        _ => Err(Error::Rpc("expected Snapshot response".to_owned())),
    }
}

async fn send_file_data<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut FramedStream<S>,
    data: &[u8],
) -> Result<(), Error> {
    for chunk in data.chunks(FILE_CHUNK_SIZE) {
        stream
            .send(Frame {
                msg_type: MessageType::FileData,
                payload: chunk.to_vec(),
            })
            .await
            .map_err(|e| Error::Transport(format!("send file data: {e}")))?;
    }

    // Send empty frame to signal end of file
    stream
        .send(Frame {
            msg_type: MessageType::FileData,
            payload: Vec::new(),
        })
        .await
        .map_err(|e| Error::Transport(format!("send file end: {e}")))?;

    Ok(())
}

async fn receive_file<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut FramedStream<S>,
    local_root: &std::path::Path,
    path: &RelPath,
) -> Result<u64, Error> {
    let mut data = Vec::new();

    loop {
        let frame = stream
            .next()
            .await
            .ok_or_else(|| Error::Rpc("connection closed during file transfer".to_owned()))?
            .map_err(|e| Error::Transport(format!("recv file data: {e}")))?;

        if frame.msg_type != MessageType::FileData {
            return Err(Error::Rpc(format!(
                "expected FileData, got {:?}",
                frame.msg_type
            )));
        }

        if frame.payload.is_empty() {
            break;
        }

        data.extend_from_slice(&frame.payload);
    }

    let dest = local_root.join(path.to_path_buf());
    let size = data.len() as u64;

    atomic_write_file(&dest, &data, 0)
        .map_err(|e| Error::Session(format!("write {}: {e}", dest.display())))?;

    Ok(size)
}

async fn expect_session_started<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut FramedStream<S>,
) -> Result<i64, Error> {
    let frame = stream
        .next()
        .await
        .ok_or_else(|| Error::Rpc("connection closed".to_owned()))?
        .map_err(|e| Error::Transport(format!("recv: {e}")))?;

    if frame.msg_type != MessageType::RpcResponse {
        return Err(Error::Rpc(format!(
            "expected RpcResponse, got {:?}",
            frame.msg_type
        )));
    }

    let msg = decode_message(&frame.payload)?;
    match msg.body {
        RpcBody::Response(RpcResponse::SessionStarted { clock_offset_secs }) => {
            Ok(clock_offset_secs)
        }
        RpcBody::Response(RpcResponse::Error { code, message }) => {
            Err(Error::Rpc(format!("remote error ({code:?}): {message}")))
        }
        _ => Err(Error::Rpc("expected SessionStarted response".to_owned())),
    }
}

fn now_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Update the in-memory index to reflect successfully executed actions.
/// Failed actions are not passed here — their old index entries are preserved
/// so the next run re-detects and retries them.
fn apply_actions_to_index(
    index: &mut SyncIndex,
    executed: &[&Action],
    local_snap: &synccore::scan::Snapshot,
    remote_snap: &synccore::scan::Snapshot,
) {
    for action in executed {
        match action {
            Action::CopyFile {
                from: Side::Local,
                path,
            } => {
                if let Some(entry) = local_snap.entries.get(path) {
                    upsert_index_from_entry(index, entry);
                }
            }
            Action::CopyFile {
                from: Side::Remote,
                path,
            } => {
                if let Some(entry) = remote_snap.entries.get(path) {
                    upsert_index_from_entry(index, entry);
                }
            }
            Action::Delete { path, .. } => {
                index.entries.remove(path);
            }
            Action::RenameConflict {
                on: Side::Local,
                path,
                new_name,
            } => {
                if let Some(old) = index.entries.remove(path) {
                    let new_key = RelPath::new(new_name);
                    index.entries.insert(
                        new_key.clone(),
                        IndexEntry {
                            path: new_key,
                            ..old
                        },
                    );
                }
            }
            Action::RenameConflict {
                on: Side::Remote,
                path,
                new_name,
            } => {
                // Remote renamed a file; the new name will be pulled via CopyFile — just
                // remove the old entry so the next run treats it as a fresh create.
                index.entries.remove(path);
                let _ = new_name;
            }
            Action::CreateDir { .. } => {}
        }
    }
}

fn upsert_index_from_entry(index: &mut SyncIndex, entry: &synccore::scan::FileEntry) {
    let mtime_secs = entry
        .mtime
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    index.entries.insert(
        entry.path.clone(),
        IndexEntry {
            path: entry.path.clone(),
            kind: EntryKind::File,
            size: entry.size,
            mtime_secs,
            hash: entry.hash.clone().unwrap_or_default(),
        },
    );
}

/// Run a bidirectional sync: scans both sides, reconciles, then executes a mixed-direction plan.
#[allow(clippy::too_many_lines)]
pub async fn run_remote_bidi<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut FramedStream<S>,
    config: &RemoteSyncConfig,
    index: &SyncIndex,
) -> Result<RemoteSyncResult, Error> {
    let run_id = Uuid::new_v4();
    let mut req_id: u32 = 0;
    let mut files_transferred: u64 = 0;
    let mut bytes_transferred: u64 = 0;
    let mut errors: Vec<String> = Vec::new();
    let mut updated_index = index.clone();

    // 1. Start session — exchange clock offset
    let anchor_specs: Vec<AnchorSpec> = config
        .anchors
        .iter()
        .map(|a| AnchorSpec {
            id: a.id,
            remote_path: a.remote_path.clone(),
        })
        .collect();

    req_id += 1;
    send_request(
        stream,
        req_id,
        RpcRequest::StartSession {
            profile_id: config.profile_id,
            mode: config.mode,
            anchors: anchor_specs,
            initiator_unix_secs: now_unix_secs(),
        },
    )
    .await?;
    let clock_offset_secs = expect_session_started(stream).await?;

    info!(run_id = %run_id, clock_offset_secs, "remote bidi session started");

    for anchor in &config.anchors {
        // 2. Scan remote
        req_id += 1;
        send_request(
            stream,
            req_id,
            RpcRequest::ScanRemote {
                anchor_id: anchor.id,
                config: anchor.scan_config.clone(),
            },
        )
        .await?;
        let remote_snap = expect_snapshot(stream).await?;

        // 3. Scan local
        let local_snap = scan::scan_tree(&anchor.local_path, &anchor.scan_config)
            .map_err(|e| Error::Session(format!("local scan: {e}")))?;

        // 4. Diff against shared index from both sides
        let diff = diff::compute_diff(
            &local_snap,
            &remote_snap,
            index,
            &anchor.local_path,
            &anchor.local_path,
        )
        .map_err(|e| Error::Session(format!("diff: {e}")))?;

        let ctx = ReconcileContext {
            local_entries: &local_snap.entries,
            remote_entries: &remote_snap.entries,
            delete_propagation: config.delete_propagation,
            peer_name: config.peer_name.clone(),
            clock_offset_secs,
        };

        let mut sync_plan =
            reconcile::reconcile(&diff, index, config.mode, config.conflict_policy, &ctx);
        plan::dedup_dirs(&mut sync_plan);
        plan::order_actions(&mut sync_plan);

        // 5. Execute the plan — mixed push/pull/local actions
        let mut executed: Vec<&Action> = Vec::new();
        let mut paths_to_pull: Vec<RelPath> = Vec::new();

        for action in &sync_plan.actions {
            match action {
                // --- Remote-side actions ---
                Action::CreateDir {
                    on: Side::Remote, ..
                }
                | Action::CopyFile {
                    from: Side::Local, ..
                }
                | Action::Delete {
                    on: Side::Remote, ..
                }
                | Action::RenameConflict {
                    on: Side::Remote, ..
                } => {
                    let result = execute_bidi_remote_action(
                        stream,
                        action,
                        anchor.id,
                        &anchor.local_path,
                        &mut req_id,
                        &mut paths_to_pull,
                    )
                    .await;
                    match result {
                        Ok(bytes) => {
                            executed.push(action);
                            bytes_transferred += bytes;
                            if bytes > 0 {
                                files_transferred += 1;
                            }
                        }
                        Err(e) => {
                            error!("remote action error: {e}");
                            errors.push(e.to_string());
                        }
                    }
                }
                // --- Local-side actions ---
                Action::CreateDir {
                    on: Side::Local,
                    path,
                } => {
                    let full = anchor.local_path.join(path.to_path_buf());
                    match std::fs::create_dir_all(&full) {
                        Ok(()) => {
                            executed.push(action);
                        }
                        Err(e) => errors.push(format!("mkdir {}: {e}", path.display())),
                    }
                }
                Action::Delete {
                    on: Side::Local,
                    path,
                } => {
                    let full = anchor.local_path.join(path.to_path_buf());
                    let result = if full.is_dir() {
                        std::fs::remove_dir_all(&full)
                    } else if full.exists() {
                        std::fs::remove_file(&full)
                    } else {
                        Ok(())
                    };
                    match result {
                        Ok(()) => {
                            executed.push(action);
                        }
                        Err(e) => errors.push(format!("delete {}: {e}", path.display())),
                    }
                }
                Action::RenameConflict {
                    on: Side::Local,
                    path,
                    new_name,
                } => {
                    let from = anchor.local_path.join(path.to_path_buf());
                    let to = anchor.local_path.join(new_name);
                    match std::fs::rename(&from, &to) {
                        Ok(()) => {
                            executed.push(action);
                        }
                        Err(e) => errors.push(format!("rename {}: {e}", path.display())),
                    }
                }
                // CopyFile from Remote is batched into GetFiles below
                Action::CopyFile {
                    from: Side::Remote,
                    path,
                } => {
                    paths_to_pull.push(path.clone());
                }
            }
        }

        // 6. Pull all Remote→Local files in one batched GetFiles request
        if !paths_to_pull.is_empty() {
            req_id += 1;
            send_request(
                stream,
                req_id,
                RpcRequest::GetFiles {
                    anchor_id: anchor.id,
                    paths: paths_to_pull.clone(),
                },
            )
            .await?;
            expect_ok(stream).await?;

            for path in &paths_to_pull {
                match receive_file(stream, &anchor.local_path, path).await {
                    Ok(size) => {
                        files_transferred += 1;
                        bytes_transferred += size;
                        if let Some(action) = sync_plan.actions.iter().find(|a| {
                            matches!(a, Action::CopyFile { from: Side::Remote, path: p } if p == path)
                        }) {
                            executed.push(action);
                        }
                    }
                    Err(e) => errors.push(format!("recv {}: {e}", path.display())),
                }
            }
        }

        apply_actions_to_index(&mut updated_index, &executed, &local_snap, &remote_snap);
    }

    // 7. End session
    req_id += 1;
    send_request(stream, req_id, RpcRequest::EndSession { run_id }).await?;
    expect_ok(stream).await?;

    info!(
        run_id = %run_id,
        files = files_transferred,
        bytes = bytes_transferred,
        errors = errors.len(),
        "remote bidi complete"
    );

    Ok(RemoteSyncResult {
        run_id,
        files_transferred,
        bytes_transferred,
        errors,
        updated_index,
    })
}

/// Execute one remote-side action for bidi mode.
/// CopyFile{from:Remote} is not handled here — it's batched into GetFiles by the caller.
/// Returns bytes transferred (>0 for file sends, 0 for mkdir/delete/rename).
async fn execute_bidi_remote_action<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut FramedStream<S>,
    action: &Action,
    anchor_id: Uuid,
    local_root: &std::path::Path,
    req_id: &mut u32,
    _paths_to_pull: &mut Vec<RelPath>,
) -> Result<u64, Error> {
    match action {
        Action::CreateDir {
            on: Side::Remote,
            path,
        } => {
            *req_id += 1;
            send_request(
                stream,
                *req_id,
                RpcRequest::MkdirRemote {
                    anchor_id,
                    path: path.clone(),
                },
            )
            .await?;
            expect_ok(stream).await?;
            Ok(0)
        }
        Action::CopyFile {
            from: Side::Local,
            path,
        } => {
            let source = local_root.join(path.to_path_buf());
            let data = std::fs::read(&source)
                .map_err(|e| Error::Session(format!("read {}: {e}", source.display())))?;
            let size = data.len() as u64;

            let mtime_secs = std::fs::metadata(&source)
                .and_then(|m| m.modified())
                .map_or(0, |t| {
                    t.duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| d.as_secs() as i64)
                });

            *req_id += 1;
            send_request(
                stream,
                *req_id,
                RpcRequest::PutFile {
                    anchor_id,
                    path: path.clone(),
                    size,
                    mtime_secs,
                },
            )
            .await?;
            expect_ok(stream).await?;
            send_file_data(stream, &data).await?;
            expect_ok(stream).await?;
            Ok(size)
        }
        Action::Delete {
            on: Side::Remote,
            path,
        } => {
            *req_id += 1;
            send_request(
                stream,
                *req_id,
                RpcRequest::DeleteRemote {
                    anchor_id,
                    path: path.clone(),
                },
            )
            .await?;
            expect_ok(stream).await?;
            Ok(0)
        }
        Action::RenameConflict {
            on: Side::Remote,
            path,
            new_name,
        } => {
            *req_id += 1;
            send_request(
                stream,
                *req_id,
                RpcRequest::RenameRemote {
                    anchor_id,
                    path: path.clone(),
                    new_name: new_name.clone(),
                },
            )
            .await?;
            expect_ok(stream).await?;
            Ok(0)
        }
        // Local-side and CopyFile{from:Remote} are handled by the caller
        _ => Ok(0),
    }
}

// --- Quick-send (FR-SM-6) ---

/// Result of a quick-send operation.
#[derive(Debug)]
pub struct QuickSendResult {
    pub transfer_id: Uuid,
    pub files_sent: u64,
    pub bytes_sent: u64,
}

/// Send files/folders to a paired peer without creating a profile or sync index.
///
/// `source_path` can be a single file or a directory (transferred recursively).
/// `destination_dir` is the directory on the remote where files will land.
#[allow(clippy::too_many_lines)]
pub async fn quick_send<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut FramedStream<S>,
    source_path: &std::path::Path,
    destination_dir: &str,
) -> Result<QuickSendResult, Error> {
    use crate::rpc::QuickSendEntry;
    use walkdir::WalkDir;

    let transfer_id = Uuid::new_v4();

    // Build entry manifest
    let mut entries = Vec::new();
    let base = if source_path.is_file() {
        source_path.parent().unwrap_or(source_path)
    } else {
        source_path
    };

    if source_path.is_file() {
        let meta =
            std::fs::metadata(source_path).map_err(|e| Error::Session(format!("stat: {e}")))?;
        let mtime_secs = meta.modified().map_or(0, |t| {
            t.duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as i64)
        });
        let file_name = source_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        entries.push(QuickSendEntry {
            rel_path: RelPath::new(&file_name),
            size: meta.len(),
            mtime_secs,
            is_dir: false,
        });
    } else {
        for entry in WalkDir::new(source_path).min_depth(1).sort_by_file_name() {
            let entry = entry.map_err(|e| Error::Session(format!("walk: {e}")))?;
            let rel = entry.path().strip_prefix(base).unwrap_or(entry.path());
            let rel_str = rel.to_string_lossy().to_string();

            if entry.file_type().is_dir() {
                entries.push(QuickSendEntry {
                    rel_path: RelPath::new(&rel_str),
                    size: 0,
                    mtime_secs: 0,
                    is_dir: true,
                });
            } else {
                let meta = entry
                    .metadata()
                    .map_err(|e| Error::Session(format!("stat: {e}")))?;
                let mtime_secs = meta.modified().map_or(0, |t| {
                    t.duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| d.as_secs() as i64)
                });
                entries.push(QuickSendEntry {
                    rel_path: RelPath::new(&rel_str),
                    size: meta.len(),
                    mtime_secs,
                    is_dir: false,
                });
            }
        }
    }

    // Send the QuickSend request
    send_request(
        stream,
        1,
        RpcRequest::QuickSend {
            transfer_id,
            destination_dir: destination_dir.to_owned(),
            entries: entries.clone(),
        },
    )
    .await?;

    // Wait for the ready acknowledgment
    expect_ok(stream).await?;

    // Stream file data for each non-dir entry
    let mut bytes_sent: u64 = 0;
    let mut files_sent: u64 = 0;

    for entry in &entries {
        if entry.is_dir {
            continue;
        }

        let full_path = base.join(entry.rel_path.to_path_buf());
        let data = std::fs::read(&full_path)
            .map_err(|e| Error::Session(format!("read {}: {e}", full_path.display())))?;
        bytes_sent += data.len() as u64;
        files_sent += 1;

        send_file_data(stream, &data).await?;
    }

    // Wait for final acknowledgment
    let frame = stream
        .next()
        .await
        .ok_or_else(|| Error::Rpc("connection closed".to_owned()))?
        .map_err(|e| Error::Transport(format!("recv: {e}")))?;

    if frame.msg_type != MessageType::RpcResponse {
        return Err(Error::Rpc(format!(
            "expected RpcResponse, got {:?}",
            frame.msg_type
        )));
    }

    let msg = decode_message(&frame.payload)?;
    match msg.body {
        RpcBody::Response(RpcResponse::QuickSendAck {
            transfer_id: tid, ..
        }) => {
            info!(transfer_id = %tid, files = files_sent, bytes = bytes_sent, "quick-send complete");
        }
        RpcBody::Response(RpcResponse::Error { code, message }) => {
            return Err(Error::Rpc(format!("remote error ({code:?}): {message}")));
        }
        _ => {
            return Err(Error::Rpc("unexpected response to quick-send".to_owned()));
        }
    }

    Ok(QuickSendResult {
        transfer_id,
        files_sent,
        bytes_sent,
    })
}

// --- Profile replication (FR-PS) ---

/// Replicate a profile to the peer during a sync session.
/// Returns Ok(true) if the peer accepted, Ok(false) if the peer had a newer version
/// (in which case the local profile is updated via db_path), or Err on transport failure.
pub async fn replicate_profile<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut FramedStream<S>,
    req_id: &mut u32,
    profile: &ProfileRow,
    anchors: &[AnchorRow],
    instance_id: Uuid,
    db_path: Option<&std::path::Path>,
) -> Result<bool, Error> {
    let wire = profile_to_wire(profile, anchors, instance_id);

    *req_id += 1;
    send_request(
        stream,
        *req_id,
        RpcRequest::ReplicateProfile { profile: wire },
    )
    .await?;

    let frame = stream
        .next()
        .await
        .ok_or_else(|| Error::Rpc("connection closed".to_owned()))?
        .map_err(|e| Error::Transport(format!("recv: {e}")))?;

    if frame.msg_type != MessageType::RpcResponse {
        return Err(Error::Rpc(format!(
            "expected RpcResponse, got {:?}",
            frame.msg_type
        )));
    }

    let msg = decode_message(&frame.payload)?;
    match msg.body {
        RpcBody::Response(RpcResponse::ProfileAccepted) => Ok(true),
        RpcBody::Response(RpcResponse::ProfileConflict { local_version }) => {
            if let Some(path) = db_path {
                if let Ok(db) = syncstore::Db::open(path) {
                    let (row, new_anchors) = wire_to_profile(&local_version, instance_id);
                    let _ = db.delete_anchors_for_profile(row.id);
                    let _ = db.update_profile(&row);
                    for anchor in &new_anchors {
                        let _ = db.insert_anchor(anchor);
                    }
                    info!(
                        profile_id = %row.id,
                        new_version = local_version.version,
                        "updated local profile from peer's newer version"
                    );
                }
            }
            Ok(false)
        }
        RpcBody::Response(RpcResponse::Error { code, message }) => Err(Error::Rpc(format!(
            "replicate profile error ({code:?}): {message}"
        ))),
        _ => Err(Error::Rpc(
            "unexpected response to ReplicateProfile".to_owned(),
        )),
    }
}

/// Send undelivered profile tombstones to the peer.
/// Opens the DB at `db_path` to read tombstones and mark them delivered.
pub async fn deliver_tombstones<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut FramedStream<S>,
    req_id: &mut u32,
    db_path: &std::path::Path,
) -> Result<u32, Error> {
    let db = syncstore::Db::open(db_path).map_err(|e| Error::Session(format!("open db: {e}")))?;

    let tombstones = db
        .list_undelivered_tombstones()
        .map_err(|e| Error::Session(format!("load tombstones: {e}")))?;

    drop(db); // Release before async I/O

    let mut delivered = 0u32;
    for (profile_id, deleted_at) in &tombstones {
        *req_id += 1;
        send_request(
            stream,
            *req_id,
            RpcRequest::ProfileDeleted {
                profile_id: *profile_id,
                deleted_at: deleted_at.clone(),
            },
        )
        .await?;
        expect_ok(stream).await?;

        // Re-open briefly to mark delivered
        if let Ok(db) = syncstore::Db::open(db_path) {
            let _ = db.mark_tombstone_delivered(*profile_id);
        }
        delivered += 1;
    }

    if delivered > 0 {
        info!(count = delivered, "delivered profile tombstones");
    }
    Ok(delivered)
}

/// Query the peer for profiles that target this instance (FR-PS-1).
pub async fn list_peer_profiles<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut FramedStream<S>,
    req_id: &mut u32,
) -> Result<Vec<WireProfileSummary>, Error> {
    *req_id += 1;
    send_request(stream, *req_id, RpcRequest::ListProfiles).await?;

    let frame = stream
        .next()
        .await
        .ok_or_else(|| Error::Rpc("connection closed".to_owned()))?
        .map_err(|e| Error::Transport(format!("recv: {e}")))?;

    if frame.msg_type != MessageType::RpcResponse {
        return Err(Error::Rpc(format!(
            "expected RpcResponse, got {:?}",
            frame.msg_type
        )));
    }

    let msg = decode_message(&frame.payload)?;
    match msg.body {
        RpcBody::Response(RpcResponse::ProfileList { profiles }) => Ok(profiles),
        RpcBody::Response(RpcResponse::Error { code, message }) => Err(Error::Rpc(format!(
            "list profiles error ({code:?}): {message}"
        ))),
        _ => Err(Error::Rpc("expected ProfileList response".to_owned())),
    }
}

/// Fetch full profile data from peer by ID.
pub async fn fetch_peer_profile<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut FramedStream<S>,
    req_id: &mut u32,
    profile_id: Uuid,
) -> Result<WireProfile, Error> {
    *req_id += 1;
    send_request(stream, *req_id, RpcRequest::GetProfile { profile_id }).await?;

    let frame = stream
        .next()
        .await
        .ok_or_else(|| Error::Rpc("connection closed".to_owned()))?
        .map_err(|e| Error::Transport(format!("recv: {e}")))?;

    if frame.msg_type != MessageType::RpcResponse {
        return Err(Error::Rpc(format!(
            "expected RpcResponse, got {:?}",
            frame.msg_type
        )));
    }

    let msg = decode_message(&frame.payload)?;
    match msg.body {
        RpcBody::Response(RpcResponse::ProfileData { profile }) => Ok(profile),
        RpcBody::Response(RpcResponse::Error { code, message }) => Err(Error::Rpc(format!(
            "get profile error ({code:?}): {message}"
        ))),
        _ => Err(Error::Rpc("expected ProfileData response".to_owned())),
    }
}
