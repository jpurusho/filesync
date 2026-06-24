use std::net::SocketAddr;
use std::path::PathBuf;

use rustls::pki_types::ServerName;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use uuid::Uuid;

use synccore::diff::{IndexEntry, SyncIndex};
use synccore::path::RelPath;
use synccore::reconcile::{ConflictPolicy, SyncMode};
use synccore::scan::{EntryKind, ScanConfig};
use syncnet::identity::Identity;
use syncnet::session::{
    RemoteAnchor, RemoteSyncConfig, RemoteSyncResult, run_remote_bidi, run_remote_pull,
    run_remote_push,
};
use syncnet::tls;
use syncnet::transport::framed;
use syncstore::Db;

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("Profile not found: {0}")]
    ProfileNotFound(Uuid),
    #[error("Peer not found for profile: {0}")]
    PeerNotFound(String),
    #[error("Invalid peer address: {0}")]
    InvalidAddress(String),
    #[error("Network error: {0}")]
    Network(String),
    #[error("TLS error: {0}")]
    Tls(String),
    #[error("Sync error: {0}")]
    Sync(String),
    #[error("Database error: {0}")]
    Database(String),
}

impl From<syncnet::Error> for SyncError {
    fn from(e: syncnet::Error) -> Self {
        Self::Sync(e.to_string())
    }
}

impl From<syncstore::StoreError> for SyncError {
    fn from(e: syncstore::StoreError) -> Self {
        Self::Database(e.to_string())
    }
}

/// Execute a sync session for the given profile
pub async fn execute_sync(
    profile_id: Uuid,
    peer_address: SocketAddr,
    mode: SyncMode,
    identity: &Identity,
    db: &Db,
) -> Result<RemoteSyncResult, SyncError> {
    // 1. Load profile and anchors
    let profile = db
        .get_profile(profile_id)?
        .ok_or(SyncError::ProfileNotFound(profile_id))?;

    let anchors_rows = db.get_anchors(profile_id)?;
    if anchors_rows.is_empty() {
        return Err(SyncError::Sync("Profile has no anchors".to_string()));
    }

    // 2. Load peer info
    let peer_id_uuid = Uuid::parse_str(&profile.peer_id)
        .map_err(|e| SyncError::Database(format!("Invalid peer_id in profile: {e}")))?;

    let peer = db
        .get_peer(peer_id_uuid)?
        .ok_or_else(|| SyncError::PeerNotFound(profile.peer_name.clone()))?;

    // 3. Build RemoteSyncConfig
    let anchors: Vec<RemoteAnchor> = anchors_rows
        .iter()
        .map(|a| {
            RemoteAnchor {
                id: Uuid::new_v4(), // Anchors use i64 id in DB, generate UUID for protocol
                local_path: PathBuf::from(&a.local_path),
                remote_path: a.remote_path.clone(),
                scan_config: ScanConfig {
                    max_depth: a.max_depth,
                    include_hidden: a.include_hidden,
                    ignore_patterns: a.ignore_patterns.clone(),
                },
            }
        })
        .collect();

    let conflict_policy = match profile.conflict_policy.as_str() {
        "keep_both" => ConflictPolicy::KeepBoth,
        _ => ConflictPolicy::NewerWins, // "newer_wins" or unknown defaults to NewerWins
    };

    let config = RemoteSyncConfig {
        profile_id,
        mode,
        conflict_policy,
        delete_propagation: profile.delete_propagation,
        peer_name: peer.name.clone(),
        anchors,
    };

    // 4. Load sync index (simplified - load all anchors into one index)
    let index = load_sync_index(db, profile_id, &anchors_rows)?;

    // 5. Establish TLS connection
    let peer_cert = rustls::pki_types::CertificateDer::from(peer.cert_pem.as_bytes().to_vec());
    let pinned_certs = vec![peer_cert];

    let client_config = tls::make_client_config(identity, &pinned_certs, false)
        .map_err(|e| SyncError::Tls(e.to_string()))?;

    let tcp = TcpStream::connect(peer_address)
        .await
        .map_err(|e| SyncError::Network(format!("Failed to connect to {peer_address}: {e}")))?;

    let connector = TlsConnector::from(client_config);
    let server_name = ServerName::try_from("filesync.local")
        .map_err(|e| SyncError::Tls(format!("Invalid server name: {e}")))?;

    let tls_stream = connector
        .connect(server_name.to_owned(), tcp)
        .await
        .map_err(|e| SyncError::Tls(format!("TLS handshake failed: {e}")))?;

    let mut stream = framed(tls_stream);

    // 6. Run sync based on mode
    let result = match mode {
        SyncMode::Push => run_remote_push(&mut stream, &config, &index).await?,
        SyncMode::Pull => run_remote_pull(&mut stream, &config, &index).await?,
        SyncMode::Bidirectional => run_remote_bidi(&mut stream, &config, &index).await?,
    };

    // 7. Persist updated index
    save_sync_index(db, profile_id, &result.updated_index, &anchors_rows)?;

    Ok(result)
}

/// Load the sync index for a profile from the database
fn load_sync_index(
    db: &Db,
    profile_id: Uuid,
    anchors: &[syncstore::profiles::AnchorRow],
) -> Result<SyncIndex, SyncError> {
    let mut index = SyncIndex::default();

    // Load entries for each anchor
    for (anchor_idx, _anchor) in anchors.iter().enumerate() {
        let entries = db.load_index(profile_id, anchor_idx)?;

        for entry in entries {
            let rel_path = RelPath::new(&entry.rel_path);

            let kind = match entry.kind.as_str() {
                "dir" => EntryKind::Dir,
                _ => EntryKind::File, // "file" or unknown defaults to File
            };

            index.entries.insert(
                rel_path.clone(),
                IndexEntry {
                    path: rel_path,
                    kind,
                    size: entry.size,
                    mtime_secs: entry.mtime_secs,
                    hash: entry.hash.clone(),
                },
            );
        }
    }

    Ok(index)
}

/// Save the updated sync index to the database
fn save_sync_index(
    db: &Db,
    profile_id: Uuid,
    index: &SyncIndex,
    _anchors: &[syncstore::profiles::AnchorRow],
) -> Result<(), SyncError> {
    // For simplicity, save all entries to anchor_idx 0
    // TODO: properly track which anchor each path belongs to
    let entries: Vec<syncstore::index::IndexEntryRow> = index
        .entries
        .iter()
        .map(|(rel_path, entry)| {
            let kind = match entry.kind {
                EntryKind::File => "file",
                EntryKind::Dir => "dir",
            };

            syncstore::index::IndexEntryRow {
                profile_id,
                anchor_idx: 0,
                rel_path: rel_path.display().to_string(),
                kind: kind.to_string(),
                size: entry.size,
                mtime_secs: entry.mtime_secs,
                hash: entry.hash.clone(),
            }
        })
        .collect();

    db.save_index(profile_id, 0, &entries)?;

    Ok(())
}
