# ADR-0025: Sync Progress Tracking Implementation

**Status:** Accepted  
**Date:** 2026-06-24  
**Requirement:** FR-UI-4 — "During a run, the UI **shall** show progress (current file, files/bytes done vs. remaining) and allow cancel."

## Context

The spec requires real-time progress reporting during sync operations (FR-UI-4). The M7 plan marked "sync progress events" as complete after wiring real sync instead of fake timer events, but granular file-by-file progress tracking with current file names, counts, and byte totals was not yet implemented.

Users need to see:
- Current file being synced
- Files completed / total files
- Bytes transferred / total bytes estimate
- Progress percentage

This visibility is critical for large syncs where users need feedback that the operation is proceeding and an estimate of time remaining.

## Decision

Implemented a **callback-based progress tracking system** that flows from the sync engine through to the UI:

### 1. Syncnet progress API (`crates/syncnet/src/session.rs`)

Added:
```rust
pub struct SyncProgress {
    pub profile_id: Uuid,
    pub run_id: Uuid,
    pub current_file: Option<String>,
    pub files_completed: u64,
    pub files_total: u64,
    pub bytes_transferred: u64,
    pub bytes_total: u64,
}

pub type ProgressCallback = Box<dyn Fn(SyncProgress) + Send + Sync>;
```

Updated all three sync functions (`run_remote_push`, `run_remote_pull`, `run_remote_bidi`) to:
1. Accept an optional `progress_cb: Option<&ProgressCallback>` parameter
2. Calculate `files_total` and `bytes_total` from the sync plan before execution
3. Emit progress before each file transfer action with current file path

### 2. Sync executor bridge (`src-tauri/src/sync_executor.rs`)

- Added `progress_cb` parameter to `execute_sync`
- Threads the callback through to the appropriate session function based on sync mode

### 3. Tauri command layer (`src-tauri/src/commands.rs`)

In `start_sync` command:
- Creates a `ProgressCallback` that emits Tauri `sync:progress` events
- Passes the callback to `sync_executor::execute_sync`
- Progress events include all fields: profile_id, run_id, current_file, files_completed/total, bytes_transferred/total

### 4. UI event and display (`ui/src/pages/ActivityPage.tsx`)

Updated `SyncRun` interface and event handlers to:
- Listen to `sync:progress` events (fixed event name from `sync-progress`)
- Match by `run_id` instead of `profile_id` for accuracy
- Display:
  - Current file name with truncation
  - Progress bar calculated from `files_completed / files_total`
  - File count: "X / Y files"
  - Bytes transferred with MB formatting (and total if known)
  - Completion status with checkmark and final counts

## Consequences

### Positive
- **Meets FR-UI-4 requirement** for progress visibility (cancel not yet implemented)
- **Real-time feedback** — users see each file as it's being transferred
- **Accurate progress** — calculated from actual sync plan, not timer-based estimates
- **Minimal overhead** — progress callback is optional; no performance impact when not used
- **Extensible** — callback pattern makes it easy to add logging, metrics, or other side effects

### Negative
- **bytes_total estimation** is approximate for Pull/Bidi modes (remote file sizes not known until transfer)
- **Cancel functionality** (also required by FR-UI-4) still needs implementation — requires abort signal plumbing through async tasks

### Known Limitations
- Pull/Bidi: `bytes_total` remains 0 until files are received (no pre-scan of remote file sizes to avoid extra round-trip)
- Large file chunking: progress updates happen per-file, not per-chunk (acceptable for MVP; chunk-level progress deferred to Phase 2 delta sync)
- No progress for non-file actions (mkdir, delete) — these are fast enough to not require feedback

## How to Apply

This pattern is now the standard for long-running operations:
1. Define a progress struct and callback type in the core crate
2. Accept `Option<&Callback>` in the function signature
3. Emit progress at key milestones (before actions, after completions)
4. Wire callback creation in the Tauri command layer to emit events
5. Handle events in the UI with real-time state updates

## Outstanding Work

To fully satisfy FR-UI-4:
- **Sync cancellation** — add abort signal to async tasks, wire to a Cancel button in UI
- **History persistence** — current runs are in-memory only; consider persisting to syncstore run_history table

## References

- **Spec:** FileSync_Requirements_Spec.md § FR-UI-4
- **Plan:** docs/plans/M7-mvp-readiness.md § 1.1 (Sync progress events)
- **Files changed:**
  - `crates/syncnet/src/session.rs` — progress API + emission
  - `src-tauri/src/sync_executor.rs` — callback threading
  - `src-tauri/src/commands.rs` — event emission
  - `ui/src/lib/tauri.ts` — type definitions
  - `ui/src/pages/ActivityPage.tsx` — event handling and display
