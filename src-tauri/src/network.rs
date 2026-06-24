use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use tokio::sync::{RwLock, broadcast};
use tracing::{error, info, warn};

use syncnet::discovery::{DiscoveredPeer, DiscoveryService};
use syncnet::identity::Identity;
use syncnet::listener::{ListenerEvent, PeerListener};
use syncstore::Db;

const DEFAULT_PORT: u16 = 5300;

/// Managed network state for the Tauri app.
pub struct NetworkState {
    pub listen_addr: SocketAddr,
    pub identity_fingerprint: String,
    pub identity_name: String,
    discovery: Arc<DiscoveryService>,
}

impl NetworkState {
    pub async fn discovered_peers(&self) -> Vec<DiscoveredPeer> {
        self.discovery.peers().await
    }
}

/// Thread-safe wrapper managed by Tauri state.
/// Initialized as empty, then filled once network starts.
#[derive(Clone, Default)]
pub struct SharedNetworkState {
    inner: Arc<RwLock<Option<NetworkState>>>,
}

impl SharedNetworkState {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn set(&self, state: NetworkState) {
        *self.inner.write().await = Some(state);
    }

    pub async fn get_info(&self) -> Option<(SocketAddr, String, String)> {
        let guard = self.inner.read().await;
        guard.as_ref().map(|s| {
            (
                s.listen_addr,
                s.identity_fingerprint.clone(),
                s.identity_name.clone(),
            )
        })
    }

    pub async fn discovered_peers(&self) -> Vec<DiscoveredPeer> {
        let guard = self.inner.read().await;
        match guard.as_ref() {
            Some(s) => s.discovery.peers().await,
            None => Vec::new(),
        }
    }
}

/// Start the pairing listener and mDNS discovery service.
/// Returns the managed NetworkState.
pub async fn start_network(identity: &Identity, db: &Db) -> Result<NetworkState, String> {
    let listen_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), DEFAULT_PORT);

    // Start the pairing listener (accepts incoming pairing requests)
    let (event_tx, mut event_rx) = broadcast::channel::<ListenerEvent>(32);
    let bound_addr =
        PeerListener::listen_for_pairing(identity.clone(), listen_addr, event_tx.clone())
            .await
            .map_err(|e| format!("Failed to start pairing listener: {e}"))?;

    info!(addr = %bound_addr, "pairing listener started");

    // Spawn a task to handle pairing events and restart listener
    let identity_for_events = identity.clone();
    let db_for_events = db.clone();
    let event_tx_for_restart = event_tx.clone();
    tokio::spawn(async move {
        loop {
            match event_rx.recv().await {
                Ok(ListenerEvent::PairingComplete(result)) => {
                    info!(
                        peer_id = %result.peer_id,
                        peer_name = %result.peer_name,
                        "incoming pairing completed"
                    );

                    let peer_row = syncstore::peers::PeerRow {
                        id: result.peer_id,
                        name: result.peer_name.clone(),
                        cert_pem: result.peer_cert_pem.clone(),
                        fingerprint: result.peer_fingerprint.to_string(),
                        paired_at: chrono::Utc::now().to_rfc3339(),
                        last_seen: None,
                        is_online: false,
                    };

                    if let Err(e) = db_for_events.insert_peer(&peer_row) {
                        error!("failed to save incoming peer: {e}");
                    }

                    // Restart pairing listener for next connection
                    let restart_addr =
                        SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), DEFAULT_PORT);
                    match PeerListener::listen_for_pairing(
                        identity_for_events.clone(),
                        restart_addr,
                        event_tx_for_restart.clone(),
                    )
                    .await
                    {
                        Ok(addr) => info!(addr = %addr, "pairing listener restarted"),
                        Err(e) => error!("failed to restart pairing listener: {e}"),
                    }
                }
                Ok(ListenerEvent::PairingFailed(reason)) => {
                    warn!("pairing attempt failed: {reason}");

                    let restart_addr =
                        SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), DEFAULT_PORT);
                    match PeerListener::listen_for_pairing(
                        identity_for_events.clone(),
                        restart_addr,
                        event_tx_for_restart.clone(),
                    )
                    .await
                    {
                        Ok(addr) => {
                            info!(addr = %addr, "pairing listener restarted after failure");
                        }
                        Err(e) => error!("failed to restart pairing listener: {e}"),
                    }
                }
                Ok(_) => {}
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!("listener event receiver lagged by {n}");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    info!("listener event channel closed");
                    break;
                }
            }
        }
    });

    // Determine the display name (hostname)
    let display_name = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "FileSync".to_owned());

    // Start mDNS discovery service
    let discovery = DiscoveryService::new(identity.clone(), display_name.clone(), DEFAULT_PORT)
        .map_err(|e| format!("Failed to create discovery service: {e}"))?;

    discovery
        .start()
        .map_err(|e| format!("Failed to start discovery: {e}"))?;

    info!("mDNS discovery started (advertising + browsing)");

    // Determine the actual local IP to show in UI
    let local_ip = get_local_ip();
    let display_addr = SocketAddr::new(local_ip, bound_addr.port());

    Ok(NetworkState {
        listen_addr: display_addr,
        identity_fingerprint: identity.fingerprint.short(),
        identity_name: display_name,
        discovery: Arc::new(discovery),
    })
}

/// Get the primary local LAN IP address.
fn get_local_ip() -> IpAddr {
    if let Ok(addrs) = get_if_addrs::get_if_addrs() {
        for iface in &addrs {
            if !iface.is_loopback() {
                if let std::net::IpAddr::V4(ipv4) = iface.ip() {
                    if ipv4.is_private() {
                        return IpAddr::V4(ipv4);
                    }
                }
            }
        }
        for iface in &addrs {
            if !iface.is_loopback() {
                return iface.ip();
            }
        }
    }
    IpAddr::V4(Ipv4Addr::LOCALHOST)
}
