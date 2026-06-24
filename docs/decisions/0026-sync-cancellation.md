# ADR-0026: Sync Cancellation Implementation

**Status:** Accepted  
**Date:** 2026-06-24  
**Requirement:** FR-UI-4 (second part) — "During a run, the UI **shall** show progress ... and **allow cancel**."

## Context

After implementing progress tracking (ADR-0025), the remaining piece of FR-UI-4 was cancellation support. Users need the ability to stop a long-running sync operation.

Challenges:
- Sync operations are async tasks spawned in tokio runtime
- Need graceful cancellation that doesn't corrupt state
- Must work across the command → executor → session layers
- UI needs immediate feedback on cancellation request

## Decision

Implemented **tokio-based cancellation** using `tokio_util::sync::CancellationToken`:

### 1. Sync Tracker (`src-tauri/src/sync_tracker.rs`)

New module to track active syncs:
```rust
pub struct SyncTracker {
    inner: Arc<Mutex<HashMap<Uuid, CancellationToken>>>,
}
```

- `register(run_id)` → creates and stores a cancellation token
- `cancel(run_id)` → triggers the stored token
- `unregister(run_id)` → cleanup on completion

Managed as Tauri state, shared across all commands.

### 2. Command Layer (`src-tauri/src/commands.rs`)

**`start_sync` updated:**
- Registers run with tracker before spawning
- Uses `tokio::select!` to race sync vs cancellation
- Emits `sync:cancelled` event when cancelled
- Unregisters run on any completion path

**New `cancel_sync` command:**
- Takes `run_id` parameter
- Calls `tracker.cancel(run_id)`
- Returns boolean (true if run was found and cancelled)

### 3. UI (`ui/src/pages/ActivityPage.tsx`)

**Cancel button:**
- Shows next to status badge for running syncs
- Calls `commands.cancelSync(run.runId)`
- Styled in red with hover effects

**Cancelled event handling:**
- Listens to `sync:cancelled` event
- Updates run status to "cancelled"
- Shows partial progress: "Cancelled after X files · Y MB"

**Status badge:**
- Added "cancelled" state with gray styling
- Distinguishes from error (red) and complete (green)

## Consequences

### Positive
- **Completes FR-UI-4** — both progress tracking and cancellation now implemented
- **Graceful cancellation** — tokio::select! ensures clean shutdown
- **Immediate feedback** — UI shows cancelled status instantly
- **State tracking** — SyncTracker prevents cancelling non-existent runs
- **Partial progress preserved** — users see how much completed before cancel

### Negative
- **Not fully granular** — cancellation happens at sync-level, not per-file
- **Race condition** — if sync completes just as cancel is called, behavior depends on tokio::select! timing (acceptable — either outcome is valid)
- **No cleanup of partial transfers** — cancelled mid-file leaves temp file on disk (acceptable for MVP; syncnet atomic writes handle this)

### Known Limitations
- Cancellation doesn't propagate into file I/O operations themselves (files complete or fail atomically per syncnet design)
- If network is very slow, cancellation might take a few seconds to be observed
- No "pause/resume" — cancellation is permanent

## Version Display Fix

Also resolved version display issue showing "0.0.0":

**Problem:** `@tauri-apps/api/app::getVersion()` was unreliable in dev builds.

**Solution:** Added `get_app_version` command that reads from `CARGO_PKG_VERSION` environment variable (set at compile time). This always returns the correct version from Cargo.toml.

## How to Test

1. Create profile with many files (>100)
2. Start sync (push/pull/bidi)
3. Click "Cancel" button on running sync
4. Verify:
   - Status changes to "cancelled" (gray badge)
   - `sync:cancelled` event received
   - Partial progress shown
   - File count and bytes at cancellation point preserved
5. Verify version displays correctly in status bar (v0.3.1)

## References

- **Spec:** FileSync_Requirements_Spec.md § FR-UI-4
- **Prior work:** ADR-0025 (progress tracking)
- **Files changed:**
  - `src-tauri/src/sync_tracker.rs` — new tracker module
  - `src-tauri/src/lib.rs` — register tracker as state
  - `src-tauri/src/commands.rs` — cancellation logic + version command
  - `src-tauri/Cargo.toml` — added tokio-util dependency
  - `ui/src/lib/tauri.ts` — cancelSync command + getAppVersion
  - `ui/src/pages/ActivityPage.tsx` — cancel button + event handling
  - `ui/src/components/StatusBar.tsx` — use getAppVersion command
