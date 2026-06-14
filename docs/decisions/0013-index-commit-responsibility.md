# 0013 — Index Commit Responsibility

## Context

The sync index must be updated after a run to reflect the new state, or incremental syncs will be incorrect (all files re-detected as changed). The question is: who updates the index, and when?

## Decision

Session functions (`run_remote_push`, `run_remote_pull`, `run_remote_bidi`) return an `updated_index: SyncIndex` in `RemoteSyncResult`. The caller is responsible for persisting this to syncstore.

Per-file errors leave the old index entry in place: only successfully-executed actions mutate the in-memory index before it is returned. A failed file is re-detected on the next run and retried.

The index is returned (not persisted inside the session function) because:
1. Session functions live in `syncnet` and have no dependency on `syncstore` — keeping that separation clean.
2. The caller can decide whether to commit transactionally alongside the run record.
3. If the session itself fails (connection dropped before `EndSession`), the caller receives an error, never gets an `updated_index`, and the old index remains — the next run is safe and idempotent.

## Consequences

- Callers (Tauri command handlers in M6) must call `syncstore::Db::save_index` with `result.updated_index` on success.
- If the caller crashes between receiving the result and persisting the index, the next run may re-transfer some files. This is safe (idempotent) but not perfectly efficient.
- The `SyncIndex` type in `synccore::diff` must be `Clone` (it already is).
