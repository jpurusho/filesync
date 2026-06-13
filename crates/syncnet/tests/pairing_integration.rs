use std::net::{Ipv4Addr, SocketAddr};

use rustls::pki_types::ServerName;
use syncnet::identity::Identity;
use syncnet::listener::PeerListener;
use syncnet::pairing;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio_rustls::{TlsAcceptor, TlsConnector};

/// Two instances complete a pairing handshake in-process.
#[tokio::test]
async fn two_instances_pair_successfully() {
    let id_a = Identity::generate().unwrap();
    let id_b = Identity::generate().unwrap();

    // Start instance B's pairing listener
    let (event_tx, _) = broadcast::channel(16);
    let addr_b = PeerListener::listen_for_pairing(
        id_b.clone(),
        SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 0),
        event_tx,
    )
    .await
    .unwrap();

    // Give the listener a moment to start accepting
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Instance A initiates pairing with B (auto-confirm)
    let result = pairing::initiate_pairing(&id_a, addr_b, |_fp| true).await;

    let pairing_result = result.unwrap();
    assert_eq!(pairing_result.peer_fingerprint, id_b.fingerprint);
    assert!(!pairing_result.peer_cert_pem.is_empty());
}

/// Pairing fails if the initiator rejects.
#[tokio::test]
async fn pairing_rejected_by_initiator() {
    let id_a = Identity::generate().unwrap();
    let id_b = Identity::generate().unwrap();

    let (event_tx, _) = broadcast::channel(16);
    let addr_b = PeerListener::listen_for_pairing(
        id_b.clone(),
        SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 0),
        event_tx,
    )
    .await
    .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Initiator rejects
    let result = pairing::initiate_pairing(&id_a, addr_b, |_fp| false).await;

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("rejected"), "unexpected error: {err}");
}

/// Pinned cert verification: a paired instance accepts connections only from the pinned peer.
#[tokio::test]
async fn pinned_certs_reject_unknown_peer() {
    let id_a = Identity::generate().unwrap();
    let id_b = Identity::generate().unwrap();
    let id_stranger = Identity::generate().unwrap();

    // Test TLS config directly — B trusts only A
    let server_config =
        syncnet::tls::make_server_config(&id_b, std::slice::from_ref(&id_a.cert_der), false)
            .unwrap();
    let client_config =
        syncnet::tls::make_client_config(&id_stranger, std::slice::from_ref(&id_b.cert_der), false)
            .unwrap();

    let tcp_listener = TcpListener::bind(SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 0))
        .await
        .unwrap();
    let listen_addr = tcp_listener.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        let (tcp, _) = tcp_listener.accept().await.unwrap();
        let acceptor = TlsAcceptor::from(server_config);
        acceptor.accept(tcp).await
    });

    let client_handle = tokio::spawn(async move {
        let tcp = TcpStream::connect(listen_addr).await.unwrap();
        let connector = TlsConnector::from(client_config);
        let server_name = ServerName::try_from("filesync.local").unwrap();
        connector.connect(server_name.to_owned(), tcp).await
    });

    let (server_result, client_result) = tokio::join!(server_handle, client_handle);

    // At least one side should fail due to cert verification
    let server_failed = server_result.unwrap().is_err();
    let client_failed = client_result.unwrap().is_err();
    assert!(
        server_failed || client_failed,
        "expected TLS to fail for unpinned peer"
    );
}
