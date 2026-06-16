#![allow(clippy::similar_names)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::unused_async)]

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
use syncnet::handler::{profile_to_wire, wire_to_profile};
use syncnet::identity::Identity;
use syncnet::session::{
    RemoteAnchor, RemoteSyncConfig, deliver_tombstones, quick_send, replicate_profile,
    run_remote_bidi, run_remote_push,
};
use syncnet::tls;
use syncnet::transport::framed;

use syncstore::Db;
use syncstore::profiles::{AnchorRow, ProfileRow};

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

    let tcp_listener = tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
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

    let tcp_listener = tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
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

        let handler = SyncHandler::new(peer_a_id, Uuid::new_v4());
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

    let tcp_listener = tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
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

    let tcp_listener = tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
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
        let handler = SyncHandler::new(peer_a_id, Uuid::new_v4());
        let mut stream = framed(tls_stream);
        handler.serve(&mut stream).await.unwrap();
    });

    let client_handle = tokio::spawn(async move {
        let connector = TlsConnector::from(client_config);
        let tcp = TcpStream::connect(server_addr).await.unwrap();
        let server_name = ServerName::try_from("filesync.local").unwrap();
        let tls_stream = connector
            .connect(server_name.to_owned(), tcp)
            .await
            .unwrap();
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

    let tcp_listener = tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
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
        let handler = SyncHandler::new(peer_a_id, Uuid::new_v4());
        let mut stream = framed(tls_stream);
        handler.serve(&mut stream).await.unwrap();
    });

    let client_handle = tokio::spawn(async move {
        let connector = TlsConnector::from(client_config);
        let tcp = TcpStream::connect(server_addr).await.unwrap();
        let server_name = ServerName::try_from("filesync.local").unwrap();
        let tls_stream = connector
            .connect(server_name.to_owned(), tcp)
            .await
            .unwrap();
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
async fn bidi_session_setup() -> (Identity, Identity, TempDir, TempDir) {
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

    let tcp_listener = tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .await
        .unwrap();
    let server_addr = tcp_listener.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        let acceptor = tokio_rustls::TlsAcceptor::from(server_config);
        let (tcp, _) = tcp_listener.accept().await.unwrap();
        let tls = acceptor.accept(tcp).await.unwrap();
        let handler = SyncHandler::new(peer_a_id, Uuid::new_v4());
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

    let result = run_bidi_test(
        peer_a,
        peer_b,
        &local_dir,
        &remote_dir,
        SyncIndex::default(),
    )
    .await;

    assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
    assert_eq!(result.files_transferred, 2);

    assert_eq!(
        fs::read_to_string(remote_dir.path().join("file_a.txt")).unwrap(),
        "from A"
    );
    assert_eq!(
        fs::read_to_string(local_dir.path().join("file_b.txt")).unwrap(),
        "from B"
    );
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
    index.entries.insert(
        rel.clone(),
        IndexEntry {
            path: rel,
            kind: EntryKind::File,
            size: 5,
            mtime_secs: 1_000,
            hash: "old".to_owned(),
        },
    );

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
    assert_eq!(
        local_content, remote_content,
        "both sides must converge to the same winner"
    );
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

    let result = run_bidi_test(
        peer_a,
        peer_b,
        &local_dir,
        &remote_dir,
        SyncIndex::default(),
    )
    .await;

    assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
    assert_eq!(
        result.files_transferred, 0,
        "no transfer expected when content is identical"
    );
}

/// Bidi delete-vs-edit: A deletes a file, B edits it → edited copy restored on A.
#[tokio::test]
async fn e2e_bidi_delete_vs_edit() {
    let (peer_a, peer_b, local_dir, remote_dir) = bidi_session_setup().await;

    // Start with a shared baseline in the index
    let rel = RelPath::new("edited.txt");
    let mut index = SyncIndex::default();
    index.entries.insert(
        rel.clone(),
        IndexEntry {
            path: rel,
            kind: EntryKind::File,
            size: 10,
            mtime_secs: 500_000,
            hash: "old_hash".to_owned(),
        },
    );

    // A deleted the file (not present locally), B edited it
    fs::write(remote_dir.path().join("edited.txt"), "edited on B").unwrap();
    // local_dir has no edited.txt — it was deleted

    let result = run_bidi_test(peer_a, peer_b, &local_dir, &remote_dir, index).await;

    assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
    // The edited copy should be restored on A's side
    assert_eq!(
        fs::read_to_string(local_dir.path().join("edited.txt")).unwrap(),
        "edited on B"
    );
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

    let tcp_listener = tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .await
        .unwrap();
    let server_addr = tcp_listener.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        let acceptor = tokio_rustls::TlsAcceptor::from(server_config);
        let (tcp, _) = tcp_listener.accept().await.unwrap();
        let tls = acceptor.accept(tcp).await.unwrap();
        let handler = SyncHandler::new(peer_a_id, Uuid::new_v4());
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
    assert!(
        result
            .updated_index
            .entries
            .contains_key(&RelPath::new("a.txt"))
    );
    assert!(
        result
            .updated_index
            .entries
            .contains_key(&RelPath::new("b.txt"))
    );
    let a_entry = &result.updated_index.entries[&RelPath::new("a.txt")];
    assert_eq!(a_entry.size, 6); // "file a".len()
}

// --- Profile Replication Tests (M5) ---

fn make_test_profile(instance_id: Uuid, peer_id: Uuid) -> (ProfileRow, Vec<AnchorRow>) {
    let profile_id = Uuid::new_v4();
    let profile = ProfileRow {
        id: profile_id,
        name: "Photos Sync".to_owned(),
        mode: "bidirectional".to_owned(),
        delete_propagation: false,
        conflict_policy: "newer_wins".to_owned(),
        peer_name: "PeerB".to_owned(),
        created_at: "2026-06-14T10:00:00Z".to_owned(),
        updated_at: "2026-06-14T10:00:00Z".to_owned(),
        version: 1,
        peer_id: peer_id.to_string(),
        origin_instance_id: instance_id.to_string(),
        pending_deletion: false,
    };
    let anchors = vec![AnchorRow {
        id: 0,
        profile_id,
        local_path: "/Users/alice/Photos".to_owned(),
        remote_path: "/Users/bob/Backup/Photos".to_owned(),
        max_depth: -1,
        include_hidden: false,
        ignore_patterns: vec![".DS_Store".to_owned()],
    }];
    (profile, anchors)
}

/// Profile replicated on sync: A sends profile to B, B stores it with paths flipped.
#[tokio::test]
async fn e2e_profile_replicated_on_sync() {
    let peer_a = Identity::generate().unwrap();
    let peer_b = Identity::generate().unwrap();

    let a_pinned = vec![peer_b.cert_der.clone()];
    let b_pinned = vec![peer_a.cert_der.clone()];

    // Create a DB for B to receive the profile
    let db_dir = TempDir::new().unwrap();
    let db_path = db_dir.path().join("b.db");
    let db_b = Db::open(&db_path).unwrap();
    drop(db_b); // Close so handler can reopen

    let instance_a_id = peer_a.id;
    let instance_b_id = peer_b.id;
    let (profile, anchors) = make_test_profile(instance_a_id, instance_b_id);
    let profile_id = profile.id;

    let tcp_listener = tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .await
        .unwrap();
    let server_addr = tcp_listener.local_addr().unwrap();

    let server_config = tls::make_server_config(&peer_b, &b_pinned, false).unwrap();
    let client_config = tls::make_client_config(&peer_a, &a_pinned, false).unwrap();

    let db_path_clone = db_path.clone();
    let server_handle = tokio::spawn(async move {
        let acceptor = tokio_rustls::TlsAcceptor::from(server_config);
        let (tcp, _) = tcp_listener.accept().await.unwrap();
        let tls = acceptor.accept(tcp).await.unwrap();
        let handler = SyncHandler::with_db_path(instance_a_id, instance_b_id, db_path_clone);
        let mut stream = framed(tls);
        handler.serve(&mut stream).await.unwrap();
    });

    let profile_clone = profile.clone();
    let anchors_clone = anchors.clone();
    let client_handle = tokio::spawn(async move {
        let connector = TlsConnector::from(client_config);
        let tcp = TcpStream::connect(server_addr).await.unwrap();
        let sn = ServerName::try_from("filesync.local").unwrap();
        let tls = connector.connect(sn, tcp).await.unwrap();
        let mut stream = framed(tls);
        let mut req_id = 0u32;

        let accepted = replicate_profile(
            &mut stream,
            &mut req_id,
            &profile_clone,
            &anchors_clone,
            instance_a_id,
            None::<&std::path::Path>,
        )
        .await
        .unwrap();

        assert!(accepted, "peer should accept new profile");
    });

    let (srv, cli) = tokio::join!(server_handle, client_handle);
    srv.unwrap();
    cli.unwrap();

    // Verify B's database has the profile with paths flipped
    let db_b = Db::open(&db_path).unwrap();
    let loaded = db_b.get_profile(profile_id).unwrap().unwrap();
    assert_eq!(loaded.name, "Photos Sync");
    assert_eq!(loaded.version, 1);
    assert_eq!(loaded.origin_instance_id, instance_a_id.to_string());

    let loaded_anchors = db_b.get_anchors(profile_id).unwrap();
    assert_eq!(loaded_anchors.len(), 1);
    // B's local_path should be what was A's remote_path (paths flipped)
    assert_eq!(loaded_anchors[0].local_path, "/Users/bob/Backup/Photos");
    assert_eq!(loaded_anchors[0].remote_path, "/Users/alice/Photos");
}

/// Profile version conflict: peer has newer version, initiator updates.
#[tokio::test]
async fn e2e_profile_version_conflict() {
    let peer_a = Identity::generate().unwrap();
    let peer_b = Identity::generate().unwrap();

    let a_pinned = vec![peer_b.cert_der.clone()];
    let b_pinned = vec![peer_a.cert_der.clone()];

    let instance_a_id = peer_a.id;
    let instance_b_id = peer_b.id;

    // B already has the profile at version 3 (newer)
    let db_dir = TempDir::new().unwrap();
    let db_path = db_dir.path().join("b.db");
    {
        let db_b = Db::open(&db_path).unwrap();
        let profile_id = Uuid::new_v4();
        let profile = ProfileRow {
            id: profile_id,
            name: "Newer Name".to_owned(),
            mode: "push".to_owned(),
            delete_propagation: true,
            conflict_policy: "keep_both".to_owned(),
            peer_name: "PeerA".to_owned(),
            created_at: "2026-06-14T10:00:00Z".to_owned(),
            updated_at: "2026-06-14T12:00:00Z".to_owned(),
            version: 3,
            peer_id: instance_a_id.to_string(),
            origin_instance_id: instance_a_id.to_string(),
            pending_deletion: false,
        };
        db_b.insert_profile(&profile).unwrap();
        db_b.insert_anchor(&AnchorRow {
            id: 0,
            profile_id,
            local_path: "/Users/bob/NewPath".to_owned(),
            remote_path: "/Users/alice/Photos".to_owned(),
            max_depth: -1,
            include_hidden: true,
            ignore_patterns: vec![],
        })
        .unwrap();

        // A tries to send version 1 (stale)
        let old_profile = ProfileRow {
            id: profile_id,
            name: "Old Name".to_owned(),
            mode: "bidirectional".to_owned(),
            delete_propagation: false,
            conflict_policy: "newer_wins".to_owned(),
            peer_name: "PeerB".to_owned(),
            created_at: "2026-06-14T10:00:00Z".to_owned(),
            updated_at: "2026-06-14T10:00:00Z".to_owned(),
            version: 1,
            peer_id: instance_b_id.to_string(),
            origin_instance_id: instance_a_id.to_string(),
            pending_deletion: false,
        };
        let old_anchors = vec![AnchorRow {
            id: 0,
            profile_id,
            local_path: "/Users/alice/Photos".to_owned(),
            remote_path: "/Users/bob/Backup/Photos".to_owned(),
            max_depth: -1,
            include_hidden: false,
            ignore_patterns: vec![".DS_Store".to_owned()],
        }];

        drop(db_b);

        let tcp_listener =
            tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
                .await
                .unwrap();
        let server_addr = tcp_listener.local_addr().unwrap();

        let server_config = tls::make_server_config(&peer_b, &b_pinned, false).unwrap();
        let client_config = tls::make_client_config(&peer_a, &a_pinned, false).unwrap();

        let db_path_clone = db_path.clone();
        let server_handle = tokio::spawn(async move {
            let acceptor = tokio_rustls::TlsAcceptor::from(server_config);
            let (tcp, _) = tcp_listener.accept().await.unwrap();
            let tls = acceptor.accept(tcp).await.unwrap();
            let handler = SyncHandler::with_db_path(instance_a_id, instance_b_id, db_path_clone);
            let mut stream = framed(tls);
            handler.serve(&mut stream).await.unwrap();
        });

        // A has a DB to receive the conflict response
        let a_db_dir = TempDir::new().unwrap();
        let a_db_path = a_db_dir.path().join("a.db");
        let db_a = Db::open(&a_db_path).unwrap();
        db_a.insert_profile(&old_profile).unwrap();
        for anchor in &old_anchors {
            db_a.insert_anchor(anchor).unwrap();
        }
        drop(db_a);

        let a_db_path_clone = a_db_path.clone();
        let client_handle = tokio::spawn(async move {
            let connector = TlsConnector::from(client_config);
            let tcp = TcpStream::connect(server_addr).await.unwrap();
            let sn = ServerName::try_from("filesync.local").unwrap();
            let tls = connector.connect(sn, tcp).await.unwrap();
            let mut stream = framed(tls);
            let mut req_id = 0u32;

            let accepted = replicate_profile(
                &mut stream,
                &mut req_id,
                &old_profile,
                &old_anchors,
                instance_a_id,
                Some(a_db_path_clone.as_path()),
            )
            .await
            .unwrap();

            assert!(!accepted, "peer should reject stale version");
            a_db_path_clone
        });

        let (srv, cli) = tokio::join!(server_handle, client_handle);
        srv.unwrap();
        let a_db_path_final = cli.unwrap();

        // Verify A's database was updated with B's newer version
        let db_a = Db::open(&a_db_path_final).unwrap();
        let updated = db_a.get_profile(profile_id).unwrap().unwrap();
        assert_eq!(updated.version, 3);
        assert_eq!(updated.name, "Newer Name");
    }
}

/// Profile tombstone delivered: A deletes profile, B marks pending_deletion.
#[tokio::test]
async fn e2e_profile_tombstone_delivered() {
    let peer_a = Identity::generate().unwrap();
    let peer_b = Identity::generate().unwrap();

    let a_pinned = vec![peer_b.cert_der.clone()];
    let b_pinned = vec![peer_a.cert_der.clone()];

    let instance_a_id = peer_a.id;
    let instance_b_id = peer_b.id;

    let profile_id = Uuid::new_v4();

    // B has the profile
    let db_dir = TempDir::new().unwrap();
    let db_path = db_dir.path().join("b.db");
    {
        let db_b = Db::open(&db_path).unwrap();
        let profile = ProfileRow {
            id: profile_id,
            name: "Doomed Profile".to_owned(),
            mode: "push".to_owned(),
            delete_propagation: false,
            conflict_policy: "newer_wins".to_owned(),
            peer_name: "PeerA".to_owned(),
            created_at: "2026-06-14T10:00:00Z".to_owned(),
            updated_at: "2026-06-14T10:00:00Z".to_owned(),
            version: 1,
            peer_id: instance_a_id.to_string(),
            origin_instance_id: instance_a_id.to_string(),
            pending_deletion: false,
        };
        db_b.insert_profile(&profile).unwrap();
        drop(db_b);
    }

    // A has the tombstone queued
    let a_db_dir = TempDir::new().unwrap();
    let a_db_path = a_db_dir.path().join("a.db");
    {
        let db_a = Db::open(&a_db_path).unwrap();
        db_a.insert_profile_tombstone(profile_id, "2026-06-15T08:00:00Z")
            .unwrap();
        drop(db_a);
    }

    let tcp_listener = tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .await
        .unwrap();
    let server_addr = tcp_listener.local_addr().unwrap();

    let server_config = tls::make_server_config(&peer_b, &b_pinned, false).unwrap();
    let client_config = tls::make_client_config(&peer_a, &a_pinned, false).unwrap();

    let db_path_clone = db_path.clone();
    let server_handle = tokio::spawn(async move {
        let acceptor = tokio_rustls::TlsAcceptor::from(server_config);
        let (tcp, _) = tcp_listener.accept().await.unwrap();
        let tls = acceptor.accept(tcp).await.unwrap();
        let handler = SyncHandler::with_db_path(instance_a_id, instance_b_id, db_path_clone);
        let mut stream = framed(tls);
        handler.serve(&mut stream).await.unwrap();
    });

    let a_db_path_clone = a_db_path.clone();
    let client_handle = tokio::spawn(async move {
        let connector = TlsConnector::from(client_config);
        let tcp = TcpStream::connect(server_addr).await.unwrap();
        let sn = ServerName::try_from("filesync.local").unwrap();
        let tls = connector.connect(sn, tcp).await.unwrap();
        let mut stream = framed(tls);
        let mut req_id = 0u32;

        let delivered = deliver_tombstones(&mut stream, &mut req_id, &a_db_path_clone)
            .await
            .unwrap();
        assert_eq!(delivered, 1);
    });

    let (srv, cli) = tokio::join!(server_handle, client_handle);
    srv.unwrap();
    cli.unwrap();

    // B should have the profile marked as pending_deletion
    let db_b = Db::open(&db_path).unwrap();
    let loaded = db_b.get_profile(profile_id).unwrap().unwrap();
    assert!(loaded.pending_deletion);

    // And it should NOT appear in list_profiles
    let all = db_b.list_profiles().unwrap();
    assert!(all.is_empty());

    // Tombstone should be marked as delivered on A's side
    let db_a = Db::open(&a_db_path).unwrap();
    let remaining = db_a.list_undelivered_tombstones().unwrap();
    assert!(remaining.is_empty());
}

/// Wire format round-trip: profile_to_wire → wire_to_profile preserves data with correct path flip.
#[test]
fn wire_profile_roundtrip_path_mapping() {
    let instance_a = Uuid::new_v4();
    let instance_b = Uuid::new_v4();

    let profile_id = Uuid::new_v4();
    let profile = ProfileRow {
        id: profile_id,
        name: "Test".to_owned(),
        mode: "bidirectional".to_owned(),
        delete_propagation: true,
        conflict_policy: "keep_both".to_owned(),
        peer_name: "Peer".to_owned(),
        created_at: String::new(),
        updated_at: "2026-06-14T10:00:00Z".to_owned(),
        version: 5,
        peer_id: instance_b.to_string(),
        origin_instance_id: instance_a.to_string(),
        pending_deletion: false,
    };
    let anchors = vec![AnchorRow {
        id: 0,
        profile_id,
        local_path: "/Users/alice/Docs".to_owned(),
        remote_path: "/Users/bob/Shared".to_owned(),
        max_depth: 2,
        include_hidden: true,
        ignore_patterns: vec!["*.tmp".to_owned()],
    }];

    // A serializes to wire (A is origin)
    let wire = profile_to_wire(&profile, &anchors, instance_a);
    assert_eq!(wire.origin_instance_id, instance_a);
    assert_eq!(wire.anchors[0].side_a_path, "/Users/alice/Docs");
    assert_eq!(wire.anchors[0].side_b_path, "/Users/bob/Shared");

    // B deserializes (B is NOT origin)
    let (row_b, anchors_b) = wire_to_profile(&wire, instance_b);
    assert_eq!(row_b.name, "Test");
    assert_eq!(row_b.version, 5);
    // B's local_path is side_b (its path), remote_path is side_a (A's path)
    assert_eq!(anchors_b[0].local_path, "/Users/bob/Shared");
    assert_eq!(anchors_b[0].remote_path, "/Users/alice/Docs");
    assert_eq!(anchors_b[0].max_depth, 2);
    assert!(anchors_b[0].include_hidden);
    assert_eq!(anchors_b[0].ignore_patterns, vec!["*.tmp"]);

    // B re-serializes back to wire (B is NOT origin, so it should reconstruct correctly)
    let wire2 = profile_to_wire(&row_b, &anchors_b, instance_b);
    assert_eq!(wire2.origin_instance_id, instance_a); // origin preserved
    assert_eq!(wire2.anchors[0].side_a_path, "/Users/alice/Docs");
    assert_eq!(wire2.anchors[0].side_b_path, "/Users/bob/Shared");
}
