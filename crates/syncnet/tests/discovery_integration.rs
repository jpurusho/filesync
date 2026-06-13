use syncnet::discovery::{DiscoveryEvent, DiscoveryService};
use syncnet::identity::Identity;

/// Two instances discover each other via mDNS on localhost.
/// This test requires mDNS to work on the local machine (not sandboxed).
#[tokio::test]
#[ignore = "mDNS requires non-sandboxed network access"]
async fn two_instances_discover_each_other() {
    let id_a = Identity::generate().unwrap();
    let id_b = Identity::generate().unwrap();

    let svc_a = DiscoveryService::new(id_a.clone(), "NodeA".to_owned(), 9001).unwrap();
    let svc_b = DiscoveryService::new(id_b.clone(), "NodeB".to_owned(), 9002).unwrap();

    let mut rx_a = svc_a.subscribe();

    svc_a.start().unwrap();
    svc_b.start().unwrap();

    // Wait for A to discover B (with timeout)
    let discovered = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let Ok(event) = rx_a.recv().await else {
                continue;
            };
            if let DiscoveryEvent::PeerDiscovered(peer) = event {
                if peer.id == id_b.id {
                    return peer;
                }
            }
        }
    })
    .await;

    assert!(discovered.is_ok(), "timed out waiting for peer discovery");
    let peer = discovered.unwrap();
    assert_eq!(peer.name, "NodeB");
    assert_eq!(peer.protocol_version, 1);

    // Cleanup
    let _ = svc_a.stop();
    let _ = svc_b.stop();
}
