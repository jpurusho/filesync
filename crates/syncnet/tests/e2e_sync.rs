use std::fs;
use std::net::{Ipv4Addr, SocketAddr};

use rustls::pki_types::ServerName;
use tempfile::TempDir;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use uuid::Uuid;

use synccore::diff::SyncIndex;
use synccore::reconcile::{ConflictPolicy, SyncMode};
use synccore::scan::ScanConfig;

use syncnet::handler::SyncHandler;
use syncnet::identity::Identity;
use syncnet::session::{RemoteAnchor, RemoteSyncConfig, run_remote_push};
use syncnet::tls;
use syncnet::transport::framed;

fn default_scan_config() -> ScanConfig {
    ScanConfig {
        max_depth: -1,
        include_hidden: false,
        ignore_patterns: vec![],
    }
}

/// Verify that paired peers can complete mutual TLS.
#[tokio::test]
async fn paired_tls_handshake_succeeds() {
    let peer_a = Identity::generate().unwrap();
    let peer_b = Identity::generate().unwrap();

    let a_pinned = vec![peer_b.cert_der.clone()];
    let b_pinned = vec![peer_a.cert_der.clone()];

    let server_config = tls::make_server_config(&peer_b, &b_pinned, false).unwrap();
    let client_config = tls::make_client_config(&peer_a, &a_pinned, false).unwrap();

    let tcp_listener = tokio::net::TcpListener::bind(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
    )
    .await
    .unwrap();
    let server_addr = tcp_listener.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        let acceptor = tokio_rustls::TlsAcceptor::from(server_config);
        let (tcp_stream, _) = tcp_listener.accept().await.unwrap();
        acceptor.accept(tcp_stream).await
    });

    let client_handle = tokio::spawn(async move {
        let connector = TlsConnector::from(client_config);
        let tcp = TcpStream::connect(server_addr).await.unwrap();
        let server_name = ServerName::try_from("filesync.local").unwrap();
        connector.connect(server_name.to_owned(), tcp).await
    });

    let (server_result, client_result) = tokio::join!(server_handle, client_handle);
    let server_ok = server_result.unwrap();
    let client_ok = client_result.unwrap();

    if let Err(ref e) = server_ok {
        panic!("server TLS failed: {e}");
    }
    if let Err(ref e) = client_ok {
        panic!("client TLS failed: {e}");
    }
}

/// End-to-end test: two instances with paired TLS certs, push files over TCP.
#[tokio::test]
async fn e2e_push_over_tls() {
    let peer_a = Identity::generate().unwrap();
    let peer_b = Identity::generate().unwrap();

    let a_pinned = vec![peer_b.cert_der.clone()];
    let b_pinned = vec![peer_a.cert_der.clone()];

    let local_dir = TempDir::new().unwrap();
    let remote_dir = TempDir::new().unwrap();

    fs::create_dir_all(local_dir.path().join("photos")).unwrap();
    fs::write(local_dir.path().join("notes.txt"), "my notes").unwrap();
    fs::write(local_dir.path().join("photos/pic.jpg"), "fake jpeg data").unwrap();

    let remote_path = remote_dir.path().to_str().unwrap().to_owned();
    let anchor_id = Uuid::new_v4();
    let profile_id = Uuid::new_v4();

    let tcp_listener = tokio::net::TcpListener::bind(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
    )
    .await
    .unwrap();
    let server_addr = tcp_listener.local_addr().unwrap();

    let server_config = tls::make_server_config(&peer_b, &b_pinned, false).unwrap();
    let client_config = tls::make_client_config(&peer_a, &a_pinned, false).unwrap();

    let peer_a_id = peer_a.id;

    let local_path = local_dir.path().to_path_buf();

    let server_handle = tokio::spawn(async move {
        let acceptor = tokio_rustls::TlsAcceptor::from(server_config);
        let (tcp_stream, _) = tcp_listener.accept().await.unwrap();
        let tls_stream = match acceptor.accept(tcp_stream).await {
            Ok(s) => s,
            Err(e) => panic!("SERVER TLS accept failed: {e}"),
        };

        let handler = SyncHandler::new(peer_a_id);
        let mut stream = framed(tls_stream);
        if let Err(e) = handler.serve(&mut stream).await {
            panic!("SERVER handler error: {e}");
        }
    });

    let client_handle = tokio::spawn(async move {
        let connector = TlsConnector::from(client_config);
        let tcp = TcpStream::connect(server_addr).await.unwrap();
        let server_name = ServerName::try_from("filesync.local").unwrap();
        let tls_stream = match connector.connect(server_name.to_owned(), tcp).await {
            Ok(s) => s,
            Err(e) => panic!("CLIENT TLS connect failed: {e}"),
        };

        let config = RemoteSyncConfig {
            profile_id,
            mode: SyncMode::Push,
            conflict_policy: ConflictPolicy::NewerWins,
            delete_propagation: false,
            peer_name: "PeerB".to_owned(),
            anchors: vec![RemoteAnchor {
                id: anchor_id,
                local_path,
                remote_path,
                scan_config: default_scan_config(),
            }],
        };

        let mut stream = framed(tls_stream);
        run_remote_push(&mut stream, &config, &SyncIndex::default()).await
    });

    let (server_result, client_result) = tokio::join!(server_handle, client_handle);
    if let Err(ref e) = server_result {
        panic!("server task panicked: {e}");
    }
    server_result.unwrap();
    let result = client_result.unwrap().unwrap();

    assert_eq!(result.files_transferred, 2);
    assert!(result.errors.is_empty());

    assert_eq!(
        fs::read_to_string(remote_dir.path().join("notes.txt")).unwrap(),
        "my notes"
    );
    assert_eq!(
        fs::read_to_string(remote_dir.path().join("photos/pic.jpg")).unwrap(),
        "fake jpeg data"
    );
}

/// Unpaired peer is rejected by TLS (pinned verifier).
#[tokio::test]
async fn unpaired_peer_rejected() {
    let peer_a = Identity::generate().unwrap();
    let peer_b = Identity::generate().unwrap();
    let peer_c = Identity::generate().unwrap(); // unpaired intruder

    // B only trusts A
    let b_pinned = vec![peer_a.cert_der.clone()];

    let server_config = tls::make_server_config(&peer_b, &b_pinned, false).unwrap();

    let tcp_listener = tokio::net::TcpListener::bind(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
    )
    .await
    .unwrap();
    let server_addr = tcp_listener.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        let acceptor = tokio_rustls::TlsAcceptor::from(server_config);
        let (tcp_stream, _) = tcp_listener.accept().await.unwrap();
        acceptor.accept(tcp_stream).await
    });

    tokio::task::yield_now().await;

    // C tries to connect with B's cert pinned (so C trusts B), but B doesn't trust C
    let c_config =
        tls::make_client_config(&peer_c, std::slice::from_ref(&peer_b.cert_der), false).unwrap();
    let connector = TlsConnector::from(c_config);

    let tcp = TcpStream::connect(server_addr).await.unwrap();
    let server_name = ServerName::try_from("filesync.local").unwrap();
    let client_result = connector.connect(server_name.to_owned(), tcp).await;

    let server_result = server_handle.await.unwrap();

    // At least one side must fail
    let server_failed = server_result.is_err();
    let client_failed = client_result.is_err();
    assert!(
        server_failed || client_failed,
        "expected TLS to fail for unpinned peer, server={server_result:?}, client={client_result:?}"
    );
}
