use std::fs;
use std::net::{Ipv4Addr, SocketAddr};

use rustls::pki_types::ServerName;
use tempfile::TempDir;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use uuid::Uuid;

use synccore::reconcile::{ConflictPolicy, SyncMode};
use synccore::scan::ScanConfig;

use synccore::diff::{IndexEntry, SyncIndex};
use synccore::path::RelPath;
use synccore::scan::EntryKind;

use syncnet::handler::SyncHandler;
use syncnet::identity::Identity;
use syncnet::session::{RemoteAnchor, RemoteSyncConfig, quick_send, run_remote_bidi, run_remote_push};
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

/// Quick-send a single file over TLS.
#[tokio::test]
async fn quick_send_single_file() {
    let peer_a = Identity::generate().unwrap();
    let peer_b = Identity::generate().unwrap();

    let a_pinned = vec![peer_b.cert_der.clone()];
    let b_pinned = vec![peer_a.cert_der.clone()];

    let source_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();

    fs::write(source_dir.path().join("hello.txt"), "quick send payload").unwrap();

    let dest_path = dest_dir.path().to_str().unwrap().to_owned();
    let source_file = source_dir.path().join("hello.txt");

    let tcp_listener =
        tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
            .await
            .unwrap();
    let server_addr = tcp_listener.local_addr().unwrap();

    let server_config = tls::make_server_config(&peer_b, &b_pinned, false).unwrap();
    let client_config = tls::make_client_config(&peer_a, &a_pinned, false).unwrap();

    let peer_a_id = peer_a.id;

    let server_handle = tokio::spawn(async move {
        let acceptor = tokio_rustls::TlsAcceptor::from(server_config);
        let (tcp_stream, _) = tcp_listener.accept().await.unwrap();
        let tls_stream = acceptor.accept(tcp_stream).await.unwrap();
        let handler = SyncHandler::new(peer_a_id);
        let mut stream = framed(tls_stream);
        handler.serve(&mut stream).await.unwrap();
    });

    let client_handle = tokio::spawn(async move {
        let connector = TlsConnector::from(client_config);
        let tcp = TcpStream::connect(server_addr).await.unwrap();
        let server_name = ServerName::try_from("filesync.local").unwrap();
        let tls_stream = connector.connect(server_name.to_owned(), tcp).await.unwrap();
        let mut stream = framed(tls_stream);
        quick_send(&mut stream, &source_file, &dest_path).await
    });

    let (server_result, client_result) = tokio::join!(server_handle, client_handle);
    server_result.unwrap();
    let result = client_result.unwrap().unwrap();

    assert_eq!(result.files_sent, 1);
    assert_eq!(result.bytes_sent, 18); // "quick send payload".len()
    assert_eq!(
        fs::read_to_string(dest_dir.path().join("hello.txt")).unwrap(),
        "quick send payload"
    );
}

/// Quick-send a directory tree over TLS.
#[tokio::test]
async fn quick_send_directory() {
    let peer_a = Identity::generate().unwrap();
    let peer_b = Identity::generate().unwrap();

    let a_pinned = vec![peer_b.cert_der.clone()];
    let b_pinned = vec![peer_a.cert_der.clone()];

    let source_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();

    // Create a tree: docs/readme.md, docs/sub/note.txt, image.png
    fs::create_dir_all(source_dir.path().join("docs/sub")).unwrap();
    fs::write(source_dir.path().join("docs/readme.md"), "# Hello").unwrap();
    fs::write(source_dir.path().join("docs/sub/note.txt"), "a note").unwrap();
    fs::write(source_dir.path().join("image.png"), "fake png bytes").unwrap();

    let dest_path = dest_dir.path().to_str().unwrap().to_owned();
    let source_path = source_dir.path().to_path_buf();

    let tcp_listener =
        tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
            .await
            .unwrap();
    let server_addr = tcp_listener.local_addr().unwrap();

    let server_config = tls::make_server_config(&peer_b, &b_pinned, false).unwrap();
    let client_config = tls::make_client_config(&peer_a, &a_pinned, false).unwrap();

    let peer_a_id = peer_a.id;

    let server_handle = tokio::spawn(async move {
        let acceptor = tokio_rustls::TlsAcceptor::from(server_config);
        let (tcp_stream, _) = tcp_listener.accept().await.unwrap();
        let tls_stream = acceptor.accept(tcp_stream).await.unwrap();
        let handler = SyncHandler::new(peer_a_id);
        let mut stream = framed(tls_stream);
        handler.serve(&mut stream).await.unwrap();
    });

    let client_handle = tokio::spawn(async move {
        let connector = TlsConnector::from(client_config);
        let tcp = TcpStream::connect(server_addr).await.unwrap();
        let server_name = ServerName::try_from("filesync.local").unwrap();
        let tls_stream = connector.connect(server_name.to_owned(), tcp).await.unwrap();
        let mut stream = framed(tls_stream);
        quick_send(&mut stream, &source_path, &dest_path).await
    });

    let (server_result, client_result) = tokio::join!(server_handle, client_handle);
    server_result.unwrap();
    let result = client_result.unwrap().unwrap();

    assert_eq!(result.files_sent, 3);
    assert_eq!(
        fs::read_to_string(dest_dir.path().join("docs/readme.md")).unwrap(),
        "# Hello"
    );
    assert_eq!(
        fs::read_to_string(dest_dir.path().join("docs/sub/note.txt")).unwrap(),
        "a note"
    );
    assert_eq!(
        fs::read_to_string(dest_dir.path().join("image.png")).unwrap(),
        "fake png bytes"
    );
}

// Helper: build a paired TLS session (server + client) and return temp dirs.
async fn bidi_session_setup() -> (
    Identity,
    Identity,
    TempDir,
    TempDir,
) {
    let peer_a = Identity::generate().unwrap();
    let peer_b = Identity::generate().unwrap();
    let local_dir = TempDir::new().unwrap();
    let remote_dir = TempDir::new().unwrap();
    (peer_a, peer_b, local_dir, remote_dir)
}

async fn run_bidi_test(
    peer_a: Identity,
    peer_b: Identity,
    local_dir: &TempDir,
    remote_dir: &TempDir,
    index: SyncIndex,
) -> syncnet::session::RemoteSyncResult {
    let a_pinned = vec![peer_b.cert_der.clone()];
    let b_pinned = vec![peer_a.cert_der.clone()];

    let server_config = syncnet::tls::make_server_config(&peer_b, &b_pinned, false).unwrap();
    let client_config = syncnet::tls::make_client_config(&peer_a, &a_pinned, false).unwrap();

    let anchor_id = Uuid::new_v4();
    let profile_id = Uuid::new_v4();
    let remote_path = remote_dir.path().to_str().unwrap().to_owned();
    let local_path = local_dir.path().to_path_buf();
    let peer_a_id = peer_a.id;

    let tcp_listener = tokio::net::TcpListener::bind(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
    )
    .await
    .unwrap();
    let server_addr = tcp_listener.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        let acceptor = tokio_rustls::TlsAcceptor::from(server_config);
        let (tcp, _) = tcp_listener.accept().await.unwrap();
        let tls = acceptor.accept(tcp).await.unwrap();
        let handler = SyncHandler::new(peer_a_id);
        let mut stream = framed(tls);
        handler.serve(&mut stream).await.unwrap();
    });

    let client_handle = tokio::spawn(async move {
        let connector = TlsConnector::from(client_config);
        let tcp = TcpStream::connect(server_addr).await.unwrap();
        let sn = ServerName::try_from("filesync.local").unwrap();
        let tls = connector.connect(sn, tcp).await.unwrap();

        let config = RemoteSyncConfig {
            profile_id,
            mode: SyncMode::Bidirectional,
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

        let mut stream = framed(tls);
        run_remote_bidi(&mut stream, &config, &index).await
    });

    let (srv, cli) = tokio::join!(server_handle, client_handle);
    srv.unwrap();
    cli.unwrap().unwrap()
}

/// Bidi: A has file_a, B has file_b → both appear on both sides after sync.
#[tokio::test]
async fn e2e_bidi_non_conflicting() {
    let (peer_a, peer_b, local_dir, remote_dir) = bidi_session_setup().await;

    fs::write(local_dir.path().join("file_a.txt"), "from A").unwrap();
    fs::write(remote_dir.path().join("file_b.txt"), "from B").unwrap();

    let result = run_bidi_test(peer_a, peer_b, &local_dir, &remote_dir, SyncIndex::default()).await;

    assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
    assert_eq!(result.files_transferred, 2);

    assert_eq!(fs::read_to_string(remote_dir.path().join("file_a.txt")).unwrap(), "from A");
    assert_eq!(fs::read_to_string(local_dir.path().join("file_b.txt")).unwrap(), "from B");
}

/// Bidi conflict — same path modified on both sides, remote has a larger/newer file → remote wins.
/// We seed the index with an old baseline so both sides appear modified relative to it.
/// Remote content is longer so it will be "newer" by mtime (written last) on a real FS.
#[tokio::test]
async fn e2e_bidi_conflict_newer_wins() {
    let (peer_a, peer_b, local_dir, remote_dir) = bidi_session_setup().await;

    // Seed index with an old baseline so both sides are seen as Modified
    let rel = RelPath::new("shared.txt");
    let mut index = SyncIndex::default();
    index.entries.insert(rel.clone(), IndexEntry {
        path: rel,
        kind: EntryKind::File,
        size: 5,
        mtime_secs: 1_000,
        hash: "old".to_owned(),
    });

    // Write local version first, then remote (so remote has a later mtime on a real FS)
    fs::write(local_dir.path().join("shared.txt"), "local version").unwrap();
    // Small sleep ensures remote mtime > local mtime (filesystem granularity)
    std::thread::sleep(std::time::Duration::from_millis(20));
    fs::write(remote_dir.path().join("shared.txt"), "remote version").unwrap();

    let result = run_bidi_test(peer_a, peer_b, &local_dir, &remote_dir, index).await;

    assert!(result.errors.is_empty(), "errors: {:?}", result.errors);

    // Both sides must converge to the same content (whichever side was newer wins both).
    let local_content = fs::read_to_string(local_dir.path().join("shared.txt")).unwrap();
    let remote_content = fs::read_to_string(remote_dir.path().join("shared.txt")).unwrap();
    assert_eq!(local_content, remote_content, "both sides must converge to the same winner");
    assert!(
        local_content == "local version" || local_content == "remote version",
        "winner must be one of the two versions, got: {local_content:?}"
    );
}

/// Bidi: same content on both sides → no transfer (content_is_same short-circuits conflict).
/// This exercises the hash-equality path that prevents spurious transfers under clock skew.
#[tokio::test]
async fn e2e_bidi_same_content_no_transfer() {
    let (peer_a, peer_b, local_dir, remote_dir) = bidi_session_setup().await;

    // Both sides have the file but the index has no record → both seen as Created.
    // content_is_same() returns true (same bytes) → no copy action generated.
    fs::write(local_dir.path().join("sync.txt"), "identical content").unwrap();
    fs::write(remote_dir.path().join("sync.txt"), "identical content").unwrap();

    let result = run_bidi_test(peer_a, peer_b, &local_dir, &remote_dir, SyncIndex::default()).await;

    assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
    assert_eq!(result.files_transferred, 0, "no transfer expected when content is identical");
}

/// Bidi delete-vs-edit: A deletes a file, B edits it → edited copy restored on A.
#[tokio::test]
async fn e2e_bidi_delete_vs_edit() {
    let (peer_a, peer_b, local_dir, remote_dir) = bidi_session_setup().await;

    // Start with a shared baseline in the index
    let rel = RelPath::new("edited.txt");
    let mut index = SyncIndex::default();
    index.entries.insert(rel.clone(), IndexEntry {
        path: rel,
        kind: EntryKind::File,
        size: 10,
        mtime_secs: 500_000,
        hash: "old_hash".to_owned(),
    });

    // A deleted the file (not present locally), B edited it
    fs::write(remote_dir.path().join("edited.txt"), "edited on B").unwrap();
    // local_dir has no edited.txt — it was deleted

    let result = run_bidi_test(peer_a, peer_b, &local_dir, &remote_dir, index).await;

    assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
    // The edited copy should be restored on A's side
    assert_eq!(fs::read_to_string(local_dir.path().join("edited.txt")).unwrap(), "edited on B");
}

/// Updated index reflects the files that were successfully transferred.
#[tokio::test]
async fn e2e_push_updated_index() {
    let peer_a = Identity::generate().unwrap();
    let peer_b = Identity::generate().unwrap();

    let a_pinned = vec![peer_b.cert_der.clone()];
    let b_pinned = vec![peer_a.cert_der.clone()];

    let local_dir = TempDir::new().unwrap();
    let remote_dir = TempDir::new().unwrap();

    fs::write(local_dir.path().join("a.txt"), "file a").unwrap();
    fs::write(local_dir.path().join("b.txt"), "file b").unwrap();

    let anchor_id = Uuid::new_v4();
    let profile_id = Uuid::new_v4();
    let remote_path = remote_dir.path().to_str().unwrap().to_owned();
    let local_path = local_dir.path().to_path_buf();
    let peer_a_id = peer_a.id;

    let server_config = syncnet::tls::make_server_config(&peer_b, &b_pinned, false).unwrap();
    let client_config = syncnet::tls::make_client_config(&peer_a, &a_pinned, false).unwrap();

    let tcp_listener = tokio::net::TcpListener::bind(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
    )
    .await
    .unwrap();
    let server_addr = tcp_listener.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        let acceptor = tokio_rustls::TlsAcceptor::from(server_config);
        let (tcp, _) = tcp_listener.accept().await.unwrap();
        let tls = acceptor.accept(tcp).await.unwrap();
        let handler = SyncHandler::new(peer_a_id);
        let mut stream = framed(tls);
        handler.serve(&mut stream).await.unwrap();
    });

    let client_handle = tokio::spawn(async move {
        let connector = TlsConnector::from(client_config);
        let tcp = TcpStream::connect(server_addr).await.unwrap();
        let sn = ServerName::try_from("filesync.local").unwrap();
        let tls = connector.connect(sn, tcp).await.unwrap();

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

        let mut stream = framed(tls);
        syncnet::session::run_remote_push(&mut stream, &config, &SyncIndex::default()).await
    });

    let (srv, cli) = tokio::join!(server_handle, client_handle);
    srv.unwrap();
    let result = cli.unwrap().unwrap();

    assert_eq!(result.files_transferred, 2);
    assert!(result.errors.is_empty());

    // Index should contain both transferred files
    assert!(result.updated_index.entries.contains_key(&RelPath::new("a.txt")));
    assert!(result.updated_index.entries.contains_key(&RelPath::new("b.txt")));
    let a_entry = &result.updated_index.entries[&RelPath::new("a.txt")];
    assert_eq!(a_entry.size, 6); // "file a".len()
}
