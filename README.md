# FileSync — P2P File Synchronization for macOS

**Status:** M7 (MVP readiness) in progress  
**Version:** 0.1.1  
**Platform:** macOS (MVP target)

## What This Is

FileSync is a peer-to-peer file synchronization application built with Tauri (Rust + React) that synchronizes files and folders between two computers on a local network. It features:

- **Three sync modes:** Push (A→B), Pull (B→A), and Bidirectional (A+B)
- **Profile-based sync:** Named configurations with multiple folder anchors
- **Automatic discovery:** mDNS-based peer discovery on LAN
- **Conflict resolution:** Newer-wins or keep-both policies for bidirectional sync
- **Profile replication:** Share profile configurations between paired peers
- **Incremental sync:** Only changed files are transferred
- **Trust-on-first-use (TOFU):** Self-signed TLS certificates, pinned after first pairing

## Architecture (High Level)

```
┌─────────────────────────────────────────────────────────────┐
│ UI Layer (Tauri + React)                                     │
│ - Profile management (create, edit, delete)                  │
│ - Peer pairing flow                                          │
│ - Sync triggers and status display                           │
└─────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────┐
│ Sync Engine (synccore, syncnet)                             │
│ - Filesystem scanning and diff                               │
│ - Reconciliation logic (conflict detection & resolution)     │
│ - TLS-secured network transfer                               │
│ - Profile replication protocol                               │
└─────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────┐
│ State Store (syncstore — SQLite + WAL)                      │
│ - Profiles and anchors                                       │
│ - Sync index (last-known state of all synced paths)         │
│ - Peer registry and pairing history                          │
└─────────────────────────────────────────────────────────────┘
```

### Crates

| Crate | Purpose |
|-------|---------|
| `synccore` | Core sync engine: scanning, diff, reconciliation, conflict resolution |
| `syncnet` | Network layer: mDNS discovery, TLS, pairing, RPC protocol |
| `syncstore` | SQLite persistence: profiles, peers, sync index, tombstones |
| `syncplatform` | Platform-specific utilities (paths, normalization) |
| `src-tauri` | Tauri application: UI commands, managed state |
| `ui` | React frontend: profile editor, peer pairing, sync controls |

## Build Instructions

### Prerequisites

- **Rust:** 1.83+ (install via [rustup](https://rustup.rs/))
- **Node.js:** 20+ with npm
- **macOS:** 12+ (Intel or Apple Silicon)

### Development Build

```bash
# Install dependencies and run dev mode
make dev

# Or manually:
cd ui && npm install
npm run tauri dev
```

### Production Build

```bash
# Build optimized binary + .app bundle
make build

# Or manually:
npm run tauri build

# Find the built app at:
# src-tauri/target/release/bundle/macos/filesync.app
```

### Development Tasks

The project includes a Makefile for common tasks:

```bash
make dev          # Run in development mode
make build        # Production build
make test         # Run all tests
make check        # Check formatting and linting
make fmt          # Auto-format code
make clean        # Clean build artifacts
```

## Install & Run

1. **Copy the .app** from `src-tauri/target/release/bundle/macos/` to `/Applications`
2. **Launch** `FileSync.app`
3. **Grant permissions** if macOS prompts for network/filesystem access

### Auto-Updates

FileSync can update itself automatically:

1. Click **"Check Updates"** in the header (or wait for automatic check on launch)
2. If an update is available, click **"Update Available"** to download and install
3. Restart the app after installation

Updates are fetched from GitHub releases. To publish a new version:

```bash
# Tag and push (triggers GitHub Actions to build and publish)
git tag v0.1.1 -m "Release v0.1.1"
git push origin v0.1.1

# All instances can now update via the "Check Updates" button
```

## User Guide (Quick Start)

### 1. Pair with another computer

- On **Computer A**: Click "Peers" tab → "Pair New Peer"
- Enter Computer B's IP address and port (default: `5300`)
- Verify the fingerprint shown (same on both sides)
- Pairing complete! Both computers now trust each other

### 2. Create a sync profile

- Click "Profiles" tab → "New Profile"
- Enter profile name, choose sync mode:
  - **Push:** Changes flow from this computer to peer
  - **Pull:** Changes flow from peer to this computer
  - **Bidirectional:** Both sides converge (mutual sync)
- Add folder anchors:
  - **Local path:** folder on this computer
  - **Remote path:** corresponding folder on peer
- Set conflict policy (bidirectional only):
  - **Newer Wins:** File with most recent modification time wins
  - **Keep Both:** Rename conflicting file with `.conflict-<timestamp>` suffix
- Choose delete propagation (opt-in): whether deletions sync to the other side

### 3. Run a sync

- Click "Activity" tab
- Select profile and direction
- Click "Sync Now"
- Watch progress (files transferred, bytes synced)

### 4. Handle profile conflicts

If both sides modify the same profile (e.g., add/remove anchors), version reconciliation happens automatically:
- **Newer wins:** Profile with higher version number replaces the other
- A notification shows what changed

### 5. Handle profile deletions

If a peer deletes a profile:
- You'll see a prompt: "Peer deleted profile X. Confirm or reject?"
- **Confirm:** Profile deleted locally too
- **Reject:** Profile remains active, peer's deletion ignored

## Milestones

### Completed

- **M0:** Scaffolding (Tauri + React, crate structure)
- **M1:** Local sync engine (scan, diff, reconcile, apply)
- **M2:** Discovery and pairing (mDNS, TOFU TLS)
- **M3:** Networked transfer (push/pull RPCs)
- **M3.5:** Quick-send (one-shot file transfer, no profile)
- **M4:** Bidirectional reconciliation + clock-skew handling
- **M5:** Profile replication (share configs, tombstones)
- **M6:** Tauri UI (profiles, peers, sync controls, deletion/conflict UX)

### In Progress (M7 — MVP Readiness)

- Integration testing and bug fixes
- Network services auto-start on launch (ADR-0020)
- Security hardening: path traversal protection (ADR-0021)
- Auto-updater for seamless version distribution (ADR-0022)
- Build automation with Makefile (ADR-0023)

## Current Status & Known Limitations

### Working Features

- ✅ **Automatic peer discovery** via mDNS on LAN
- ✅ **Secure pairing** with TOFU TLS (self-signed certificates)
- ✅ **Profile management** (create, edit, delete, replicate to peers)
- ✅ **Network services** auto-start on app launch
- ✅ **Auto-updates** via Tauri updater (GitHub releases)
- ✅ **Dark theme** UI with modern React components
- ✅ **Path traversal protection** for secure file operations

### Known Limitations (MVP)

- **macOS only** — Linux/Windows support deferred to Phase 2
- **Two peers only** — No multi-peer topologies (>2 nodes)
- **No delta sync** — Whole files transferred (chunk/block-level deferred)
- **No auto-trigger** — Manual sync only (filesystem watch deferred)
- ⚠️ **Real sync integration blocked** — See ADR-0019 (async Db access issue)
  - Sync button currently stubbed in UI (returns immediately without transferring files)
  - Three solutions documented: Arc-wrapped Db (recommended), connection pool, or pre-load data
  - See `docs/decisions/0019-tauri-sync-integration-blocker.md` for details
  - Core sync engine and network layer are fully functional and tested

## Testing Status

- **Sync engine:** Comprehensive tests in `crates/synccore/tests/`
- **Network layer:** Integration tests in `crates/syncnet/tests/e2e_sync.rs`
- **Profile replication:** Tests in `crates/syncnet/tests/profile_replication_tests.rs`
- **UI integration:** Manual testing only (no automated UI tests yet)

To run tests:
```bash
cargo test --all
```

## Documentation

- **Project Guide:** `CLAUDE.md` — architecture overview, working style, reading order
- **Decisions (ADRs):** `docs/decisions/` — all architectural decisions (0001-0023)
- **Milestone Plans:** `docs/plans/` — detailed implementation plans for M0-M7
- **Spec:** `FileSync_Requirements_Spec.md` — functional and non-functional requirements

## Troubleshooting

### Pairing fails
- Check firewall: allow incoming connections on port 5300
- Verify both computers are on the same network
- Try pinging the peer's IP address first

### Sync button does nothing
- Known issue (ADR-0019): real sync integration blocked by async Db access
- Workaround in progress: make Db cloneable

### Profile not syncing
- Check that profile is active (not marked for deletion)
- Verify peer is online (green indicator in Peers tab)
- Check logs in Console.app for errors

## Development

### Run tests
```bash
make test        # Run all tests
# Or manually:
cargo test --all
```

### Lint and format
```bash
make check       # Check formatting and clippy
make fmt         # Auto-format all code
# Or manually:
cargo clippy --all-targets
cargo fmt --all
```

### View database (SQLite)
```bash
# Database location:
# ~/Library/Application Support/com.filesync.app/filesync.db
sqlite3 ~/Library/Application\ Support/com.filesync.app/filesync.db
```

### Architecture

See `CLAUDE.md` for detailed architecture and working style guidance. Key decisions are recorded as ADRs in `docs/decisions/`, numbered sequentially (currently 0001-0023).

## Contributing

This is an experimental project. Issues and PRs welcome, but no guarantees on response time.

## License

MIT (see LICENSE file)

## Credits

Built with:
- [Tauri](https://tauri.app/) — Rust + web UI framework
- [Tokio](https://tokio.rs/) — Async runtime
- [Rustls](https://github.com/rustls/rustls) — TLS implementation
- [rusqlite](https://github.com/rusqlite/rusqlite) — SQLite bindings
- [React](https://react.dev/) + [Zustand](https://zustand-demo.pmnd.rs/) — UI state management
