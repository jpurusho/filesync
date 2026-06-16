# 16. Handler DB Access via Path (Not Reference)

Date: 2026-06-16

## Context

The `SyncHandler` runs inside `tokio::spawn` and holds `&self` across `.await` points. `rusqlite::Connection` contains `RefCell` (not `Sync`), so `&Db` is not `Send` — holding a `Db` in the handler or passing `&Db` to async session functions fails to compile.

Options considered:
1. Wrap `Db` in `Arc<Mutex<Db>>` — adds contention, complicates the API.
2. Store `Option<PathBuf>` and open a short-lived connection per operation.
3. Use `spawn_blocking` for all DB calls — heavy boilerplate.

## Decision

The handler stores `Option<PathBuf>` (the DB path) and calls `syncstore::Db::open(path)` for each profile-replication RPC. Session-side functions (`replicate_profile`, `deliver_tombstones`) also take `Option<&Path>` and open briefly.

## Consequences

- Each profile RPC opens and closes a SQLite connection. With WAL mode this is cheap (~100μs) and correct under concurrency.
- No `Send`/`Sync` issues — the `Db` never lives across an `.await`.
- The handler can still be created without a DB path (tests, quick-send) — profile RPCs gracefully degrade (return empty/accepted).
- If profile RPCs become hot-path (they won't — called once per sync session), we could move to a dedicated blocking thread with a channel.
