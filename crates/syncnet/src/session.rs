use std::path::PathBuf;

use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tracing::{error, info};
use uuid::Uuid;

use synccore::diff::{self, SyncIndex};
use synccore::path::RelPath;
use synccore::plan;
use synccore::reconcile::{self, Action, ConflictPolicy, ReconcileContext, Side, SyncMode};
use synccore::scan::{self, ScanConfig, Snapshot};

use crate::handler::atomic_write_file;
use crate::rpc::{
    AnchorSpec, RpcBody, RpcRequest, RpcResponse, decode_message, encode_request,
};
use crate::transport::{Frame, FramedStream, MessageType};
use crate::Error;

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
    send_request(stream, req_id, RpcRequest::StartSession {
        profile_id: config.profile_id,
        mode: config.mode,
        anchors: anchor_specs,
    })
    .await?;
    expect_ok(stream).await?;

    info!(run_id = %run_id, "remote push session started");

    for anchor in &config.anchors {
        // 2. Scan remote
        req_id += 1;
        send_request(stream, req_id, RpcRequest::ScanRemote {
            anchor_id: anchor.id,
            config: anchor.scan_config.clone(),
        })
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
            &anchor.local_path, // remote_root is not used for hash computation in push
        )
        .map_err(|e| Error::Session(format!("diff: {e}")))?;

        let ctx = ReconcileContext {
            local_entries: &local_snap.entries,
            remote_entries: &remote_snap.entries,
            delete_propagation: config.delete_propagation,
            peer_name: config.peer_name.clone(),
        };

        let mut sync_plan = reconcile::reconcile(
            &diff,
            index,
            config.mode,
            config.conflict_policy,
            &ctx,
        );
        plan::dedup_dirs(&mut sync_plan);
        plan::order_actions(&mut sync_plan);

        // 5. Execute plan over RPC
        for action in &sync_plan.actions {
            let result = execute_push_action(
                stream,
                action,
                anchor.id,
                &anchor.local_path,
                &mut req_id,
            )
            .await;

            match result {
                Ok(bytes) => {
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
    send_request(stream, req_id, RpcRequest::StartSession {
        profile_id: config.profile_id,
        mode: config.mode,
        anchors: anchor_specs,
    })
    .await?;
    expect_ok(stream).await?;

    info!(run_id = %run_id, "remote pull session started");

    for anchor in &config.anchors {
        // 2. Scan remote
        req_id += 1;
        send_request(stream, req_id, RpcRequest::ScanRemote {
            anchor_id: anchor.id,
            config: anchor.scan_config.clone(),
        })
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
        };

        let mut sync_plan = reconcile::reconcile(
            &diff,
            index,
            config.mode,
            config.conflict_policy,
            &ctx,
        );
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

        // Create dirs locally
        for action in &sync_plan.actions {
            if let Action::CreateDir {
                on: Side::Local,
                path,
            } = action
            {
                let full = anchor.local_path.join(path.to_path_buf());
                if let Err(e) = std::fs::create_dir_all(&full) {
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
                if let Err(e) = result {
                    errors.push(format!("delete {}: {e}", path.display()));
                }
            }
        }

        // 6. Request files from remote
        if !paths_to_pull.is_empty() {
            req_id += 1;
            send_request(stream, req_id, RpcRequest::GetFiles {
                anchor_id: anchor.id,
                paths: paths_to_pull.clone(),
            })
            .await?;
            expect_ok(stream).await?;

            // Receive file data frames
            for path in &paths_to_pull {
                match receive_file(stream, &anchor.local_path, path).await {
                    Ok(size) => {
                        files_transferred += 1;
                        bytes_transferred += size;
                    }
                    Err(e) => {
                        errors.push(format!("recv {}: {e}", path.display()));
                    }
                }
            }
        }
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
        Action::CreateDir { on: Side::Remote, path } => {
            *req_id += 1;
            send_request(stream, *req_id, RpcRequest::MkdirRemote {
                anchor_id,
                path: path.clone(),
            })
            .await?;
            expect_ok(stream).await?;
            Ok(0)
        }
        Action::CopyFile { from: Side::Local, path } => {
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
            send_request(stream, *req_id, RpcRequest::PutFile {
                anchor_id,
                path: path.clone(),
                size,
                mtime_secs,
            })
            .await?;
            expect_ok(stream).await?;

            // Send file data as FileData frames
            send_file_data(stream, &data).await?;

            // Wait for write confirmation
            expect_ok(stream).await?;

            Ok(size)
        }
        Action::Delete { on: Side::Remote, path } => {
            *req_id += 1;
            send_request(stream, *req_id, RpcRequest::DeleteRemote {
                anchor_id,
                path: path.clone(),
            })
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
