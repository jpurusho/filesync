use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use tokio::sync::{RwLock, broadcast};
use tracing::{debug, info};
use uuid::Uuid;

use crate::Error;
use crate::identity::Identity;

const SERVICE_TYPE: &str = "_filesync._tcp.local.";
const TXT_ID: &str = "id";
const TXT_NAME: &str = "name";
const TXT_VER: &str = "ver";
const TXT_FP: &str = "fp";

/// A peer discovered on the network.
#[derive(Debug, Clone)]
pub struct DiscoveredPeer {
    pub id: Uuid,
    pub name: String,
    pub addrs: Vec<SocketAddr>,
    pub protocol_version: u32,
    pub fingerprint_short: String,
    pub last_seen: Instant,
}

/// Events emitted by the discovery service.
#[derive(Debug, Clone)]
pub enum DiscoveryEvent {
    PeerDiscovered(DiscoveredPeer),
    PeerUpdated(DiscoveredPeer),
    PeerLost(Uuid),
}

/// Manages mDNS advertisement and browsing.
pub struct DiscoveryService {
    daemon: ServiceDaemon,
    identity: Identity,
    display_name: String,
    port: u16,
    peers: Arc<RwLock<HashMap<Uuid, DiscoveredPeer>>>,
    event_tx: broadcast::Sender<DiscoveryEvent>,
}

impl DiscoveryService {
    pub fn new(identity: Identity, display_name: String, port: u16) -> Result<Self, Error> {
        let daemon = ServiceDaemon::new()
            .map_err(|e| Error::Discovery(format!("failed to create mDNS daemon: {e}")))?;
        let (event_tx, _) = broadcast::channel(64);

        Ok(Self {
            daemon,
            identity,
            display_name,
            port,
            peers: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<DiscoveryEvent> {
        self.event_tx.subscribe()
    }

    pub async fn peers(&self) -> Vec<DiscoveredPeer> {
        self.peers.read().await.values().cloned().collect()
    }

    pub fn start(&self) -> Result<(), Error> {
        self.advertise()?;
        self.browse()?;
        Ok(())
    }

    pub fn stop(&self) -> Result<(), Error> {
        let instance_name = format!("filesync-{}", self.identity.id);
        self.daemon
            .unregister(&format!("{instance_name}.{SERVICE_TYPE}"))
            .map_err(|e| Error::Discovery(format!("unregister failed: {e}")))?;
        Ok(())
    }

    fn advertise(&self) -> Result<(), Error> {
        let instance_name = format!("filesync-{}", self.identity.id);

        let properties = [
            (TXT_ID, self.identity.id.to_string()),
            (TXT_NAME, self.display_name.clone()),
            (TXT_VER, "1".to_owned()),
            (TXT_FP, self.identity.fingerprint.short()),
        ];

        let hostname = format!("{}.local.", get_hostname());

        let service = ServiceInfo::new(
            SERVICE_TYPE,
            &instance_name,
            &hostname,
            "",
            self.port,
            &properties[..],
        )
        .map_err(|e| Error::Discovery(format!("service info creation failed: {e}")))?;

        self.daemon
            .register(service)
            .map_err(|e| Error::Discovery(format!("mDNS registration failed: {e}")))?;

        info!(
            id = %self.identity.id,
            name = %self.display_name,
            port = self.port,
            "advertising via mDNS"
        );

        Ok(())
    }

    fn browse(&self) -> Result<(), Error> {
        let receiver = self
            .daemon
            .browse(SERVICE_TYPE)
            .map_err(|e| Error::Discovery(format!("mDNS browse failed: {e}")))?;

        let peers = Arc::clone(&self.peers);
        let event_tx = self.event_tx.clone();
        let own_id = self.identity.id;

        tokio::spawn(async move {
            while let Ok(event) = receiver.recv_async().await {
                match event {
                    ServiceEvent::ServiceResolved(info) => {
                        if let Some(peer) = parse_service_info(&info) {
                            if peer.id == own_id {
                                continue;
                            }
                            let mut map = peers.write().await;
                            let is_new = !map.contains_key(&peer.id);
                            map.insert(peer.id, peer.clone());
                            let event = if is_new {
                                debug!(peer_id = %peer.id, name = %peer.name, "peer discovered");
                                DiscoveryEvent::PeerDiscovered(peer)
                            } else {
                                DiscoveryEvent::PeerUpdated(peer)
                            };
                            let _ = event_tx.send(event);
                        }
                    }
                    ServiceEvent::ServiceRemoved(_, fullname) => {
                        let mut map = peers.write().await;
                        let removed_id = map
                            .iter()
                            .find(|(_, p)| fullname.contains(&p.id.to_string()))
                            .map(|(id, _)| *id);
                        if let Some(id) = removed_id {
                            map.remove(&id);
                            debug!(peer_id = %id, "peer lost");
                            let _ = event_tx.send(DiscoveryEvent::PeerLost(id));
                        }
                    }
                    _ => {}
                }
            }
        });

        Ok(())
    }
}

fn parse_service_info(info: &ServiceInfo) -> Option<DiscoveredPeer> {
    let properties = info.get_properties();
    let id_str = properties.get_property_val_str(TXT_ID)?;
    let id = Uuid::parse_str(id_str).ok()?;
    let name = properties
        .get_property_val_str(TXT_NAME)
        .unwrap_or("Unknown")
        .to_owned();
    let ver = properties
        .get_property_val_str(TXT_VER)
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let fp = properties
        .get_property_val_str(TXT_FP)
        .unwrap_or("")
        .to_owned();

    let addrs: Vec<SocketAddr> = info
        .get_addresses()
        .iter()
        .map(|addr| SocketAddr::new(*addr, info.get_port()))
        .collect();

    Some(DiscoveredPeer {
        id,
        name,
        addrs,
        protocol_version: ver,
        fingerprint_short: fp,
        last_seen: Instant::now(),
    })
}

fn get_hostname() -> String {
    hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "filesync-host".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostname_fallback() {
        let name = get_hostname();
        assert!(!name.is_empty());
    }
}
