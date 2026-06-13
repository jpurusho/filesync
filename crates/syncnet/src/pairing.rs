use std::net::SocketAddr;

use rustls::pki_types::ServerName;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tracing::info;
use uuid::Uuid;

use crate::Error;
use crate::identity::{Fingerprint, Identity};
use crate::tls;

/// Messages exchanged during the pairing handshake.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PairingMessage {
    Request {
        id: String,
        cert_pem: String,
        name: String,
    },
    Response {
        id: String,
        cert_pem: String,
        name: String,
    },
    Confirm,
    Reject,
}

/// Result of a pairing attempt.
#[derive(Debug, Clone)]
pub struct PairingResult {
    pub peer_id: Uuid,
    pub peer_name: String,
    pub peer_cert_pem: String,
    pub peer_fingerprint: Fingerprint,
}

/// State of a pairing flow (for UI integration in M6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairingState {
    Idle,
    AwaitingConfirmation { peer_fingerprint: String },
    Confirmed { peer_id: Uuid },
    Rejected,
}

/// Initiate pairing with a peer at the given address.
/// Returns the peer's identity info if the handshake succeeds.
/// The `confirm_fn` is called with the peer's fingerprint — return true to accept.
pub async fn initiate_pairing<F>(
    identity: &Identity,
    peer_addr: SocketAddr,
    confirm_fn: F,
) -> Result<PairingResult, Error>
where
    F: FnOnce(&str) -> bool,
{
    let client_config = tls::make_client_config(identity, &[], true)?;
    let connector = TlsConnector::from(client_config);

    let tcp = TcpStream::connect(peer_addr)
        .await
        .map_err(|e| Error::Pairing(format!("connect to {peer_addr}: {e}")))?;

    let server_name = ServerName::try_from("filesync.local")
        .map_err(|e| Error::Pairing(format!("server name: {e}")))?;

    let mut tls_stream = connector
        .connect(server_name.to_owned(), tcp)
        .await
        .map_err(|e| Error::Pairing(format!("TLS handshake: {e}")))?;

    // Send pairing request
    let request = PairingMessage::Request {
        id: identity.id.to_string(),
        cert_pem: identity.cert_pem.clone(),
        name: hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "Unknown".to_owned()),
    };
    send_message(&mut tls_stream, &request).await?;

    // Receive response
    let response = recv_message(&mut tls_stream).await?;
    let (peer_id, peer_cert_pem, peer_name) = match response {
        PairingMessage::Response { id, cert_pem, name } => {
            let peer_id =
                Uuid::parse_str(&id).map_err(|e| Error::Pairing(format!("invalid UUID: {e}")))?;
            (peer_id, cert_pem, name)
        }
        PairingMessage::Reject => {
            return Err(Error::Pairing("peer rejected pairing".to_owned()));
        }
        _ => {
            return Err(Error::Pairing(
                "unexpected message during pairing".to_owned(),
            ));
        }
    };

    // Compute peer fingerprint
    let peer_pem_parsed =
        pem::parse(&peer_cert_pem).map_err(|e| Error::Pairing(format!("parse peer cert: {e}")))?;
    let peer_fingerprint = Fingerprint::from_cert_der(peer_pem_parsed.contents());

    // Ask user to confirm
    let fp_display = peer_fingerprint.short();
    if confirm_fn(&fp_display) {
        send_message(&mut tls_stream, &PairingMessage::Confirm).await?;
    } else {
        send_message(&mut tls_stream, &PairingMessage::Reject).await?;
        return Err(Error::Pairing("user rejected pairing".to_owned()));
    }

    // Wait for peer's confirmation
    let peer_confirm = recv_message(&mut tls_stream).await?;
    match peer_confirm {
        PairingMessage::Confirm => {
            info!(peer_id = %peer_id, peer_name = %peer_name, "pairing confirmed");
            Ok(PairingResult {
                peer_id,
                peer_name,
                peer_cert_pem,
                peer_fingerprint,
            })
        }
        PairingMessage::Reject => Err(Error::Pairing("peer rejected after confirm".to_owned())),
        _ => Err(Error::Pairing("unexpected message".to_owned())),
    }
}

/// Handle an incoming pairing request (server side).
/// The `confirm_fn` is called with the initiator's fingerprint — return true to accept.
pub async fn handle_pairing_request<S, F>(
    identity: &Identity,
    stream: &mut S,
    confirm_fn: F,
) -> Result<PairingResult, Error>
where
    S: AsyncReadExt + AsyncWriteExt + Unpin,
    F: FnOnce(&str) -> bool,
{
    // Receive request
    let request = recv_message_from(stream).await?;
    let (peer_id, peer_cert_pem, peer_name) = match request {
        PairingMessage::Request { id, cert_pem, name } => {
            let peer_id =
                Uuid::parse_str(&id).map_err(|e| Error::Pairing(format!("invalid UUID: {e}")))?;
            (peer_id, cert_pem, name)
        }
        _ => {
            return Err(Error::Pairing(
                "expected PairRequest, got something else".to_owned(),
            ));
        }
    };

    // Compute peer fingerprint
    let peer_pem_parsed =
        pem::parse(&peer_cert_pem).map_err(|e| Error::Pairing(format!("parse peer cert: {e}")))?;
    let peer_fingerprint = Fingerprint::from_cert_der(peer_pem_parsed.contents());

    // Send response (our identity)
    let response = PairingMessage::Response {
        id: identity.id.to_string(),
        cert_pem: identity.cert_pem.clone(),
        name: hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "Unknown".to_owned()),
    };
    send_message_to(stream, &response).await?;

    // Ask user to confirm
    let fp_display = peer_fingerprint.short();
    if !confirm_fn(&fp_display) {
        send_message_to(stream, &PairingMessage::Reject).await?;
        return Err(Error::Pairing("user rejected pairing".to_owned()));
    }

    // Wait for initiator's confirmation
    let peer_confirm = recv_message_from(stream).await?;
    match peer_confirm {
        PairingMessage::Confirm => {
            send_message_to(stream, &PairingMessage::Confirm).await?;
            info!(peer_id = %peer_id, peer_name = %peer_name, "pairing confirmed (responder)");
            Ok(PairingResult {
                peer_id,
                peer_name,
                peer_cert_pem,
                peer_fingerprint,
            })
        }
        PairingMessage::Reject => {
            send_message_to(stream, &PairingMessage::Reject).await?;
            Err(Error::Pairing("initiator rejected pairing".to_owned()))
        }
        _ => Err(Error::Pairing("unexpected message".to_owned())),
    }
}

// --- Wire format: length-prefixed JSON ---

async fn send_message<S: AsyncWriteExt + Unpin>(
    stream: &mut S,
    msg: &PairingMessage,
) -> Result<(), Error> {
    send_message_to(stream, msg).await
}

async fn send_message_to<S: AsyncWriteExt + Unpin>(
    stream: &mut S,
    msg: &PairingMessage,
) -> Result<(), Error> {
    let json = serde_json::to_vec(msg).map_err(|e| Error::Pairing(format!("serialize: {e}")))?;
    let len = (json.len() as u32).to_be_bytes();
    stream.write_all(&len).await?;
    stream.write_all(&json).await?;
    stream.flush().await?;
    Ok(())
}

async fn recv_message<S: AsyncReadExt + Unpin>(stream: &mut S) -> Result<PairingMessage, Error> {
    recv_message_from(stream).await
}

async fn recv_message_from<S: AsyncReadExt + Unpin>(
    stream: &mut S,
) -> Result<PairingMessage, Error> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;

    if len > 1_048_576 {
        return Err(Error::Pairing("message too large".to_owned()));
    }

    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await?;

    serde_json::from_slice(&buf).map_err(|e| Error::Pairing(format!("deserialize: {e}")))
}
