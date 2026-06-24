# ADR 0019 — Tauri Sync Integration Blocker (Db + Async)

**Date:** 2026-06-18  
**Status:** Resolved (see ADR-0024 for implementation)  
**Resolution Date:** 2026-06-23

## Context

When implementing real sync integration in the Tauri commands layer (`start_sync`), we hit a fundamental architectural issue:

1. **Tauri commands are async** — the `#[tauri::command]` macro requires async functions to return `Send` futures
2. **Db is behind `Mutex<Db>` in managed state** — guards are `!Send`
3. **`run_remote_push/pull/bidi` are async** and need to call Db methods during execution (load index, save index, query anchors)

This creates a `MutexGuard` across `.await` problem:
```rust
let db_guard = db.lock()?;          // MutexGuard<Db>
let result = execute_sync(..., &db_guard).await;  // ERROR: guard held across await
```

The Rust compiler rejects this because `MutexGuard` is `!Send`, and async functions that hold non-Send values across await points cannot themselves be Send.

## Decision

For MVP (M7), **defer real sync integration** in the Tauri layer and keep the stub implementation. Document the blocker and three viable solutions.

The drift_summary implementation (synchronous Db access) works fine and has been implemented.

## Consequences

### Immediate (MVP blocking)

- Sync button in UI does not trigger real file transfer
- Profile drift reporting shows tracked file count but not pending changes (would need filesystem scan)
- Users cannot test end-to-end sync via the UI

### Solutions (post-MVP)

Three architectural approaches to unblock real sync integration:

#### Option 1: Make Db cloneable via Arc
Change `syncstore::Db` to wrap `rusqlite::Connection` in `Arc`:
```rust
pub struct Db {
    conn: Arc<rusqlite::Connection>,
}

impl Clone for Db {
    fn clone(&self) -> Self {
        Self { conn: Arc::clone(&self.conn) }
    }
}
```

**Pros:** Minimal API changes, Db can be cloned and passed into async blocks  
**Cons:** rusqlite::Connection isn't thread-safe out-of-the-box; need to verify WAL mode handles concurrent access correctly

#### Option 2: Connection pool
Use `r2d2` or similar to pool connections:
```rust
type DbPool = r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>;
// Pass pool to async executor, grab connection on demand
```

**Pros:** Standard pattern for async + database  
**Cons:** More invasive change, all Db callers need refactor

#### Option 3: Pre-load all data, then go async
Extract all needed data from Db synchronously before the async operation:
```rust
let (profile, anchors, peer, index) = {
    let db = db.lock()?;
    (db.get_profile(id)?, db.get_anchors(id)?, ...)
}; // guard dropped here

// Now call async sync with pre-loaded data
let result = run_remote_push_with_data(profile, anchors, peer, index).await;
```

**Pros:** No Db changes needed  
**Cons:** Sync functions need new signatures that take owned data; can't call Db mid-sync (e.g., incremental index updates)

### Recommendation

**Option 1 (Arc-wrapped Db) is the simplest** for unblocking MVP testing. Rusqlite with WAL mode already supports concurrent readers, and writes are serialized at the SQLite level.

Changes required:
1. `crates/syncstore/src/lib.rs`: wrap `Connection` in `Arc`, derive `Clone`
2. `src-tauri/src/lib.rs`: remove `Mutex<Db>`, just store `Db` (it's already safe to clone)
3. `src-tauri/src/commands.rs`: clone Db before async calls

Estimated effort: ~1 hour (change + test).

## Why This Wasn't Caught Earlier

The network layer (`syncnet`) and sync engine (`synccore`) are pure Rust with no UI/async boundary. Tests use in-memory Db and don't cross the managed-state + Tauri-command barrier. The issue only surfaces when integrating into Tauri's async command layer.

## Resolution

**Implemented:** 2026-06-23 via ADR-0024

Option 1 (Arc-wrapped Db) was implemented successfully:
- Changed to `SQLITE_OPEN_FULL_MUTEX` for thread-safe connection
- Created `SendableConnection` wrapper with `unsafe impl Send + Sync`
- Replaced `Arc<Mutex<Connection>>` with `Arc<SendableConnection>`
- Removed all `#[allow(dead_code)]` from sync executor

All tests pass. Sync button now triggers real file transfers.

See ADR-0024 for full implementation details and consequences.

## Related

- ADR-0024: Implementation of Option 1 resolution
- M7 plan (`docs/plans/M7-mvp-readiness.md`) — Phase 1 sync integration task
- `src-tauri/src/sync_executor.rs` — now fully functional
- FR-SM-1..6 in spec — sync modes now functional in UI
