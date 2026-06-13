# 0002 — Use `rusqlite` over `sqlx`

**Status:** accepted
**Date:** 2026-06-12

## Context

The spec requires SQLite for state storage (FR-ST-5). Two main Rust SQLite drivers exist:
- `rusqlite`: synchronous API, bundled SQLite option, simpler model
- `sqlx`: async API, connection pool, compile-time query checking

The sync engine's hot paths (scan, diff, apply) need to read/write the index frequently. A profile's sync index is single-writer by design — each profile runs one sync at a time.

## Decision

Use `rusqlite` with the `bundled` feature for `syncstore`.

**Why:**
1. **Sync API fits the model.** Scan and apply loops already walk the filesystem synchronously. Adding async overhead (spawn tasks, poll futures, manage a connection pool) doesn't buy anything when the workload is sequential reads/writes.
2. **No pool needed.** One sync run = one writer. Connection pooling solves a multi-client problem we don't have.
3. **Simpler error handling.** `rusqlite::Error` integrates cleanly with `thiserror`; no need for `sqlx`'s compile-time macro setup or runtime migration executor.
4. **Bundled SQLite.** Guarantees version consistency across macOS/Windows/Linux; no system lib linkage issues.

**Trade-offs:**
- If we later add a long-lived daemon with concurrent profile runs, a shared pool might help. That's solvable by wrapping `rusqlite` connections in `Arc<Mutex<_>>` per profile or adding `sqlx` at that point. For MVP, `rusqlite`'s simplicity wins.

## Consequences

- `syncstore` uses `rusqlite` + `rusqlite_migration` for schema management.
- No async runtime dependency in `syncstore` itself (though `syncnet` uses `tokio`).
- WAL mode and `NORMAL` synchronous setting chosen for durability + performance balance.
- If performance profiling in M1+ shows index writes are a bottleneck, consider batching writes in a transaction or switching to `sqlx`. Revisit with data, not speculation.
