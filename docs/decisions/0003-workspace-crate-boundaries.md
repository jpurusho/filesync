# 0003 — Workspace structure and crate boundaries

**Status:** accepted
**Date:** 2026-06-12

## Context

The spec calls for a Rust core with "OS-specific concerns isolated" (NFR-PORT-1) so Windows/Linux ports reuse the engine. Tauri 2 is the shell. How do we split the code into crates to enforce this modularity and make the engine reusable (e.g., as a future CLI/daemon)?

## Decision

Cargo workspace with **four library crates** plus the Tauri shell:

```
crates/
├─ synccore/      # pure engine: scan, diff, plan, apply, reconcile
├─ syncnet/       # transport + discovery (TLS, mDNS)
├─ syncstore/     # SQLite schema, migrations, repos
└─ syncplatform/  # OS-specific: NFD normalization, FSEvents, bookmarks
src-tauri/        # Tauri 2 shell, command handlers, event bus
```

**Dependency rules:**
- `synccore` depends on **nothing** outside `std` and pure Rust libraries (serde, blake3, uuid). No `tokio`, no Tauri, no OS-specific APIs.
- `syncplatform` provides the OS abstractions `synccore` needs (filename normalization, path canonicalization). macOS-only for MVP; Windows/Linux impls go here later.
- `syncnet` uses `tokio` + TLS/mDNS libraries; isolated from `synccore` so the engine can run in-process (M1 test harness) or networked (M3+).
- `syncstore` wraps SQLite; uses sync API (see ADR-0002).
- `src-tauri` is the **only** place Tauri APIs appear. It wires up commands, translates between Tauri events and engine callbacks, and owns the `tokio` runtime entry point.

**Why four crates:**
- **Reusability.** `synccore` + `syncstore` + `syncplatform` can be linked into a future CLI (`filesync-cli`) or daemon without dragging in Tauri.
- **Testability.** `synccore` unit tests run without spawning a GUI or network stack.
- **Boundary enforcement.** Cargo's visibility rules prevent accidental Tauri/UI coupling in the engine.

## Consequences

- Porting to Windows/Linux: replace `syncplatform/src/macos.rs` with `windows.rs` / `linux.rs`; everything else compiles unchanged.
- Adding a CLI: `cargo new filesync-cli`, depend on the four library crates, skip `src-tauri`.
- M1 in-process testing: `synccore` tests can call the engine directly, no IPC layer needed.
- Slightly more `Cargo.toml` boilerplate upfront, but the boundaries pay for themselves by M3.
