use rusqlite_migration::{M, Migrations};

const INIT: &str = r"
CREATE TABLE meta (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);

INSERT INTO meta (key, value) VALUES ('schema_version', '1');
";

const PROFILES: &str = r"
CREATE TABLE profiles (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    mode TEXT NOT NULL CHECK (mode IN ('push', 'pull', 'bidirectional')),
    delete_propagation INTEGER NOT NULL DEFAULT 0,
    conflict_policy TEXT NOT NULL DEFAULT 'newer_wins' CHECK (conflict_policy IN ('newer_wins', 'keep_both')),
    peer_name TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE anchors (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    local_path TEXT NOT NULL,
    remote_path TEXT NOT NULL,
    max_depth INTEGER NOT NULL DEFAULT -1,
    include_hidden INTEGER NOT NULL DEFAULT 0,
    ignore_patterns TEXT NOT NULL DEFAULT '[]'
);

CREATE INDEX idx_anchors_profile ON anchors(profile_id);

UPDATE meta SET value = '2' WHERE key = 'schema_version';
";

const SYNC_INDEX: &str = r"
CREATE TABLE sync_index (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    anchor_idx INTEGER NOT NULL,
    rel_path TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('file', 'dir')),
    size INTEGER NOT NULL,
    mtime_secs INTEGER NOT NULL,
    hash TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (profile_id, anchor_idx, rel_path)
);

CREATE INDEX idx_sync_index_profile ON sync_index(profile_id, anchor_idx);

UPDATE meta SET value = '3' WHERE key = 'schema_version';
";

const RUN_RECORDS: &str = r"
CREATE TABLE run_records (
    id TEXT PRIMARY KEY NOT NULL,
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    started_at TEXT NOT NULL,
    finished_at TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('success', 'partial', 'failed')),
    files_transferred INTEGER NOT NULL DEFAULT 0,
    files_deleted INTEGER NOT NULL DEFAULT 0,
    conflicts_count INTEGER NOT NULL DEFAULT 0,
    errors_count INTEGER NOT NULL DEFAULT 0,
    error_summary TEXT
);

CREATE INDEX idx_run_records_profile ON run_records(profile_id, started_at);

UPDATE meta SET value = '4' WHERE key = 'schema_version';
";

const PEERS: &str = r"
CREATE TABLE peers (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    cert_pem TEXT NOT NULL,
    paired_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    last_seen TEXT,
    is_online INTEGER NOT NULL DEFAULT 0
);

UPDATE meta SET value = '5' WHERE key = 'schema_version';
";

const QUICK_SEND_RECORDS: &str = r"
CREATE TABLE quick_send_records (
    id TEXT PRIMARY KEY NOT NULL,
    peer_id TEXT NOT NULL,
    direction TEXT NOT NULL CHECK (direction IN ('send', 'receive')),
    destination_dir TEXT NOT NULL,
    started_at TEXT NOT NULL,
    finished_at TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('success', 'partial', 'failed')),
    files_transferred INTEGER NOT NULL DEFAULT 0,
    bytes_transferred INTEGER NOT NULL DEFAULT 0,
    error_summary TEXT
);

CREATE INDEX idx_quick_send_peer ON quick_send_records(peer_id, started_at);

UPDATE meta SET value = '6' WHERE key = 'schema_version';
";

#[must_use]
pub fn get_migrations() -> Migrations<'static> {
    Migrations::new(vec![
        M::up(INIT),
        M::up(PROFILES),
        M::up(SYNC_INDEX),
        M::up(RUN_RECORDS),
        M::up(PEERS),
        M::up(QUICK_SEND_RECORDS),
    ])
}
