# M3 — Networked Push/Pull over TLS-over-TCP

**Status:** in-progress
**Owner model:** Opus (protocol design, correctness of remote apply)

## Goal

Enable actual file transfer between two paired peers over the encrypted TLS channel established in M2. After M3, a sync profile targeting a remote peer performs the full engine pipeline (scan → diff → reconcile → plan → apply) with files flowing over the network instead of local filesystem paths.

## Requirements covered

- FR-SM-1: Push/Pull/Bidirectional (over network)
- FR-SM-2: Retain hierarchical structure on remote
- FR-SM-4: Incremental — only changed files transfer
- FR-DT-1: Whole-file granularity transfer
- FR-DT-3: Resumable/interruptible — no partial files at dest (atomic writes on remote)
- FR-FH-7: Atomic writes on destination (temp+fsync+rename, now over RPC)
- NFR-SEC-1: TLS 1.3 (via M2's rustls setup)
- NFR-SEC-2: Only paired peers can initiate sync
- NFR-SEC-3: Only expose folders in a profile targeting the requesting peer

## Non-goals (M3)

- Bidirectional reconciliation requiring BOTH sides' scans to travel (that's M4 — clock-skew + remote scan exchange)
- Quick-send (M3.5 — reuses M3 transport but different flow)
- UI for progress, profile management (M6)
- Delta/chunk transfer (Phase 2)
- Compression (Phase 2, FR-DT-4)

## Key design decision: push-based architecture

For M3, we implement **push-only** (initiator → responder) and **pull-only** (responder → initiator). The initiator drives the session:

- **Push:** Initiator scans local, scans remote via RPC, diffs, reconciles, plans, then sends files to responder.
- **Pull:** Initiator tells responder "give me files for these paths", responder scans and sends them.

Bidirectional is deferred to M4 because it requires exchanging scans from BOTH sides and reconciling them centrally, plus clock-skew compensation.

## Architecture

### Framing protocol (syncnet::transport)

Replace the current placeholder with a length-prefixed binary framing layer over the TLS stream. Every message is:

```
┌────────────┬──────────────┬───────────────────┐
│ len: u32   │ type: u8     │ payload: [u8; len] │
│ (big-endian)│              │ (MessagePack)      │
└────────────┴──────────────┴───────────────────┘
```

Message types:
- `0x01` — RPC Request
- `0x02` — RPC Response
- `0x03` — File Data (streaming chunk)
- `0x04` — Error
- `0xFF` — Shutdown

Max message size: 16 MiB (large files stream as multiple chunks).

### RPC model (syncnet::rpc)

Lightweight request-response RPC layered on the framed connection. Each request has a `u32` request_id for multiplexing (even though M3 is single-stream, this future-proofs).

```rust
#[derive(Serialize, Deserialize)]
pub enum RpcRequest {
    /// Get a snapshot of a remote directory (relative to an allowed anchor).
    ScanRemote { anchor_id: Uuid, config: ScanConfig },
    /// Request file contents for a set of paths.
    GetFiles { anchor_id: Uuid, paths: Vec<RelPath> },
    /// Send a file to the remote (push mode).
    PutFile { anchor_id: Uuid, path: RelPath, size: u64, mtime_secs: i64 },
    /// Create a directory on the remote.
    MkdirRemote { anchor_id: Uuid, path: RelPath },
    /// Delete a path on the remote.
    DeleteRemote { anchor_id: Uuid, path: RelPath },
    /// Start a sync session — negotiate profile and direction.
    StartSession { profile_id: Uuid, mode: SyncMode },
    /// End sync session, commit index.
    EndSession { run_id: Uuid },
}

#[derive(Serialize, Deserialize)]
pub enum RpcResponse {
    Ok,
    Snapshot(Snapshot),
    FileChunk { path: RelPath, offset: u64, data: Vec<u8>, is_last: bool },
    Error { code: u32, message: String },
}
```

### Session flow (Push mode)

```
Initiator (A)                          Responder (B)
    |                                       |
    |-- StartSession {profile, Push} ------>|
    |<----- Ok (anchor access validated) ---|
    |                                       |
    |-- ScanRemote {anchor_id, config} ---->|
    |<----- Snapshot(remote_snap) ----------|
    |                                       |
    |  [local: scan → diff → reconcile → plan]
    |                                       |
    |-- MkdirRemote {path} --------------->|  (for each CreateDir)
    |<----- Ok -----------------------------|
    |                                       |
    |-- PutFile {path, size, mtime} ------->|  (for each CopyFile)
    |-- [file data chunks] ---------------->|
    |<----- Ok (written atomically) --------|
    |                                       |
    |-- DeleteRemote {path} --------------->|  (for each Delete, if mirror)
    |<----- Ok -----------------------------|
    |                                       |
    |-- EndSession {run_id} --------------->|
    |<----- Ok -----------------------------|
```

### Session flow (Pull mode)

```
Initiator (A)                          Responder (B)
    |                                       |
    |-- StartSession {profile, Pull} ------>|
    |<----- Ok -----------------------------|
    |                                       |
    |-- ScanRemote {anchor_id, config} ---->|
    |<----- Snapshot(remote_snap) ----------|
    |                                       |
    |  [local: scan → diff → reconcile → plan]
    |  (plan says: copy FROM Remote to Local)
    |                                       |
    |-- GetFiles {anchor_id, paths} ------->|
    |<----- FileChunk (per file) -----------|  (received and written atomically locally)
    |                                       |
    |-- EndSession {run_id} --------------->|
    |<----- Ok -----------------------------|
```

### Access control (NFR-SEC-3)

The responder validates every RPC against allowed anchors:
- On `StartSession`, responder checks that the requesting peer is paired AND the profile's anchor targets them.
- `ScanRemote`/`GetFiles`/`PutFile`/`MkdirRemote`/`DeleteRemote` all require a valid `anchor_id` that resolves to an allowed filesystem path for the authenticated peer.

This is enforced via a `SessionContext` struct on the responder:
```rust
pub struct SessionContext {
    pub peer_id: Uuid,
    pub profile_id: Uuid,
    pub allowed_anchors: HashMap<Uuid, PathBuf>,  // anchor_id → local path
    pub mode: SyncMode,
}
```

### Network-aware apply (synccore abstraction)

Instead of modifying the pure `synccore` crate, we introduce a **network apply** module in `syncnet` that consumes a `SyncPlan` and executes it over RPC. The engine boundary becomes:

- `synccore::run_sync` — still used for local-only (testing, future CLI).
- New: `syncnet::session::run_remote_sync` — orchestrates scan→diff→reconcile→plan locally, then executes the plan over RPC.

This keeps `synccore` pure and network-free.

### File streaming

Large files are streamed in 256 KiB chunks. The responder (on PutFile) writes each chunk to a temp file, fsyncs on last chunk, then atomically renames. This satisfies FR-FH-7 over the network.

On GetFiles, the responder reads and streams chunks back. The initiator writes atomically.

### Error handling

- Per-file errors do NOT abort the session (NFR-REL-2). The initiator logs the error and continues with remaining files.
- Connection-level errors (TLS teardown, timeout) abort the entire session. The next run is idempotent — the index is only committed on `EndSession` success.
- If a PutFile is interrupted mid-transfer, the temp file is cleaned up on the responder (orphan temp cleanup on session end).

## New modules

| Path | Responsibility |
|---|---|
| `crates/syncnet/src/transport.rs` | Length-prefixed framing, send/recv primitives |
| `crates/syncnet/src/rpc.rs` | RPC types, request/response codec |
| `crates/syncnet/src/session.rs` | Session management: initiator logic (drive plan over RPC) |
| `crates/syncnet/src/handler.rs` | Responder-side RPC handler (dispatch + access control) |

## New dependencies

| Crate | Why |
|---|---|
| `rmp-serde` | MessagePack serialization (compact binary, schema-flexible) |
| `tokio-util` (codec) | Framed read/write adapter for length-delimited protocol |

## Execution order

1. **Transport framing** — implement length-prefixed codec in `transport.rs` using `tokio_util::codec::LengthDelimitedCodec`. Define `Frame` type and send/recv helpers over `TlsStream`.
2. **RPC types** — define `RpcRequest` / `RpcResponse` enums with MessagePack serde in `rpc.rs`. Add request_id and message type tags.
3. **Handler (responder)** — implement `SyncHandler` that dispatches incoming RPC requests. Wire up `ScanRemote` (calls synccore::scan), `MkdirRemote`, `PutFile` (atomic write), `DeleteRemote`, `GetFiles` (stream chunks), session lifecycle.
4. **Session (initiator)** — implement `run_remote_push` and `run_remote_pull`. Orchestrates: connect → StartSession → ScanRemote → local engine → execute plan over RPC → EndSession.
5. **Listener integration** — extend `PeerListener` to route authenticated connections to the handler (currently only handles pairing).
6. **Integration tests** — two instances in-process, push a directory from A to B, verify contents match. Pull the same back. Test error cases (denied anchor, file not found).

## ADRs to write during execution

- **0010 — Framing protocol (length-prefixed MessagePack over TLS-TCP):** Why not QUIC yet, why MessagePack over JSON, why 16 MiB max.
- **0011 — Sync session lifecycle:** Why initiator-driven, why index commit is at EndSession, idempotency guarantees.

## Done when

- Push: A creates files locally, pushes to B over TLS. Files appear atomically on B with correct content and preserved mtime.
- Pull: A pulls files from B over TLS. Files appear atomically on A.
- Unpaired peer cannot initiate a session.
- Invalid anchor_id is rejected.
- Interrupted mid-file leaves no partial file at destination.
- `cargo test --workspace` passes.
- `cargo clippy --workspace --all-targets` clean.
- Transport and session code has zero UI dependencies.
