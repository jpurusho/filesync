use std::net::SocketAddr;

use rustls::pki_types::CertificateDer;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio_rustls::TlsAcceptor;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::Error;
use crate::handler::SyncHandler;
use crate::identity::{Fingerprint, Identity};
use crate::pairing::{self, PairingResult};
use crate::tls;
use crate::transport::framed;

/// Events emitted by the listener.
#[derive(Debug, Clone)]
pub enum ListenerEvent {
    PairingRequest {
        peer_fingerprint: String,
        peer_name: String,
    },
    PairingComplete(PairingResult),
    PairingFailed(String),
    SessionStarted {
        peer_id: Uuid,
        peer_addr: SocketAddr,
    },
    SessionEnded {
        peer_id: Uuid,
    },
}

/// A TLS listener that handles pairing and authenticated sync connections.
pub struct PeerListener {
    identity: Identity,
    pinned_certs: Vec<CertificateDer<'static>>,
    event_tx: broadcast::Sender<ListenerEvent>,
}

impl PeerListener {
    pub fn new(identity: Identity, pinned_certs: Vec<CertificateDer<'static>>) -> Self {
        let (event_tx, _) = broadcast::channel(32);
        Self {
            identity,
            pinned_certs,
            event_tx,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ListenerEvent> {
        self.event_tx.subscribe()
    }

    /// Bind to the given address and start accepting connections.
    /// Returns the actual bound address (useful when binding to port 0).
    pub async fn listen(self, addr: SocketAddr) -> Result<SocketAddr, Error> {
        let tcp_listener = TcpListener::bind(addr).await?;
        let bound_addr = tcp_listener.local_addr()?;

        info!(addr = %bound_addr, "peer listener started");

        let identity = self.identity.clone();
        let pinned_certs = self.pinned_certs.clone();
        let event_tx = self.event_tx.clone();

        tokio::spawn(async move {
            loop {
                let (tcp_stream, peer_addr) = match tcp_listener.accept().await {
                    Ok(conn) => conn,
                    Err(e) => {
                        error!("accept error: {e}");
                        continue;
                    }
                };

                let identity = identity.clone();
                let pinned_certs = pinned_certs.clone();
                let event_tx = event_tx.clone();

                tokio::spawn(async move {
                    // Try authenticated mode first (only pinned certs accepted)
                    let auth_config =
                        match tls::make_server_config(&identity, &pinned_certs, false) {
                            Ok(c) => c,
                            Err(e) => {
                                error!("TLS config error: {e}");
                                return;
                            }
                        };

                    let acceptor = TlsAcceptor::from(auth_config);
                    match acceptor.accept(tcp_stream).await {
                        Ok(tls_stream) => {
                            // Authenticated peer — extract peer identity from client cert
                            let peer_id = extract_peer_id(&tls_stream);
                            let _ = event_tx.send(ListenerEvent::SessionStarted {
                                peer_id: peer_id.unwrap_or(Uuid::nil()),
                                peer_addr,
                            });

                            // Route to sync handler
                            let handler = SyncHandler::new(
                                peer_id.unwrap_or(Uuid::nil()),
                            );
                            let mut stream = framed(tls_stream);
                            if let Err(e) = handler.serve(&mut stream).await {
                                warn!("sync session error from {peer_addr}: {e}");
                            }

                            let _ = event_tx.send(ListenerEvent::SessionEnded {
                                peer_id: peer_id.unwrap_or(Uuid::nil()),
                            });
                        }
                        Err(_tls_err) => {
                            // TLS failed with pinned verifier — might be an unpaired peer
                            // attempting to pair. Re-accept with pairing mode on a new connection.
                            // Note: we can't retry the same TCP stream, so pairing must use
                            // a separate connection attempt. The pairing initiator knows to
                            // connect without a pinned cert.
                            info!("TLS auth rejected from {peer_addr} (not a pinned peer)");
                        }
                    }
                });
            }
        });

        Ok(bound_addr)
    }

    /// Start a pairing-mode listener on the same address.
    /// Call this when the user initiates "accept pairing" in the UI.
    pub async fn listen_for_pairing(
        identity: Identity,
        addr: SocketAddr,
        event_tx: broadcast::Sender<ListenerEvent>,
    ) -> Result<SocketAddr, Error> {
        let tcp_listener = TcpListener::bind(addr).await?;
        let bound_addr = tcp_listener.local_addr()?;

        info!(addr = %bound_addr, "pairing listener started");

        tokio::spawn(async move {
            // Accept one pairing connection, then stop
            let (tcp_stream, peer_addr) = match tcp_listener.accept().await {
                Ok(conn) => conn,
                Err(e) => {
                    error!("pairing accept error: {e}");
                    return;
                }
            };

            let server_config = match tls::make_server_config(&identity, &[], true) {
                Ok(c) => c,
                Err(e) => {
                    error!("TLS config error for pairing: {e}");
                    return;
                }
            };

            let acceptor = TlsAcceptor::from(server_config);
            let mut tls_stream = match acceptor.accept(tcp_stream).await {
                Ok(s) => s,
                Err(e) => {
                    error!("TLS accept error from {peer_addr}: {e}");
                    let _ = event_tx.send(ListenerEvent::PairingFailed(e.to_string()));
                    return;
                }
            };

            let result = pairing::handle_pairing_request(
                &identity,
                &mut tls_stream,
                |_fp| true, // auto-confirm for now; UI will intercept in M6
            )
            .await;

            match result {
                Ok(pr) => {
                    let _ = event_tx.send(ListenerEvent::PairingComplete(pr));
                }
                Err(e) => {
                    let _ = event_tx.send(ListenerEvent::PairingFailed(e.to_string()));
                }
            }
        });

        Ok(bound_addr)
    }
}

fn extract_peer_id<S>(
    tls_stream: &tokio_rustls::server::TlsStream<S>,
) -> Option<Uuid> {
    let (_, conn) = tls_stream.get_ref();
    let certs = conn.peer_certificates()?;
    let cert_der = certs.first()?;
    let fp = Fingerprint::from_cert_der(cert_der.as_ref());
    // Derive UUID from fingerprint (same logic as identity loading)
    Some(Uuid::from_bytes(fp.bytes()[..16].try_into().ok()?))
}
