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
use synccore::scan::{self, ScanConfig};

use crate::rpc::{
    AnchorSpec, ErrorCode, RpcBody, RpcRequest, RpcResponse, decode_message, encode_response,
};
use crate::transport::{Frame, FramedStream, MessageType};
use crate::Error;

struct SessionState {
    #[allow(dead_code)]
    peer_id: Uuid,
    #[allow(dead_code)]
    profile_id: Uuid,
    allowed_anchors: HashMap<Uuid, PathBuf>,
}

pub struct SyncHandler {
    peer_id: Uuid,
}

#[allow(clippy::unused_self, clippy::ref_option)]
impl SyncHandler {
    pub fn new(peer_id: Uuid) -> Self {
        Self { peer_id }
    }

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
                            // Two-phase: send OK for the header, receive data, then send final OK/Error
                            let prep = self.put_file_validate(anchor_id, &path, &session);
                            match prep {
                                Ok(dest_path) => {
                                    let resp_bytes =
                                        encode_response(req_id, RpcResponse::Ok)?;
                                    send_frame(stream, MessageType::RpcResponse, resp_bytes)
                                        .await?;

                                    // Receive file data
                                    let write_result =
                                        self.receive_and_write_file(stream, &dest_path, mtime_secs)
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
                            // Validate, respond OK, then stream files
                            let validate = self.get_files_validate(anchor_id, &paths, &session);
                            match validate {
                                Ok(root) => {
                                    let resp_bytes =
                                        encode_response(req_id, RpcResponse::Ok)?;
                                    send_frame(stream, MessageType::RpcResponse, resp_bytes)
                                        .await?;

                                    // Stream each file
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
            } => self.start_session(profile_id, anchors, session),

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

            // Handled separately in serve() for streaming
            RpcRequest::PutFile { .. } | RpcRequest::GetFiles { .. } => {
                RpcResponse::Error {
                    code: ErrorCode::Internal,
                    message: "bug: streaming RPCs handled in serve loop".to_owned(),
                }
            }
        }
    }

    fn start_session(
        &self,
        profile_id: Uuid,
        anchors: Vec<AnchorSpec>,
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

        info!(profile_id = %profile_id, peer_id = %self.peer_id, "session started");
        RpcResponse::Ok
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

        let root = sess.allowed_anchors.get(&anchor_id).ok_or(RpcResponse::Error {
            code: ErrorCode::AccessDenied,
            message: format!("anchor {anchor_id} not allowed"),
        })?;

        for path in paths {
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

        let root = sess.allowed_anchors.get(&anchor_id).ok_or(RpcResponse::Error {
            code: ErrorCode::AccessDenied,
            message: format!("anchor {anchor_id} not allowed"),
        })?;

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
                .ok_or_else(|| Error::Transport("connection closed during file receive".to_owned()))?
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
        let data = fs::read(&full)
            .map_err(|e| Error::Session(format!("read {}: {e}", full.display())))?;

        for chunk in data.chunks(CHUNK_SIZE) {
            send_frame(stream, MessageType::FileData, chunk.to_vec()).await?;
        }

        // Empty frame signals end of file
        send_frame(stream, MessageType::FileData, Vec::new()).await?;

        Ok(())
    }
}

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
