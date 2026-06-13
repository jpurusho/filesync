# M1 — Local Engine Kernel

**Status:** ready to execute
**Owner model:** Opus (reconciler logic, conflict policy, correctness-critical paths)

## Goal

Implement the complete sync engine pipeline in `synccore`, operating in-process on local paths (no networking). After M1, the following acceptance scenarios pass as integration tests:

- AC-1: Push with hidden-files excluded
- AC-2: Re-run with no changes → zero transfers
- AC-3: Incremental push (only changed file transferred)
- AC-4: Bidirectional, non-conflicting edits propagate
- AC-5: Bidirectional conflict → resolved per policy
- AC-6: Bidirectional delete propagates
- AC-7: Delete-vs-edit → conflict → keep edited copy
- AC-11: Depth limiting
- AC-14: Reset profile → rescan, no deletes

## Non-goals (M1)

- No networking — both sides are local paths accessed in-process.
- No peer discovery, pairing, or transport.
- No UI interaction — engine is exercised only via Rust tests.
- No profile CRUD UI — profiles are constructed in-memory for tests.
- No profile replication (M5).
- No quick-send (M3.5).
- No clock-skew handling (M4 — requires peer time exchange).
- No rename detection (Phase 2, FR-SE-6).

## Architecture

### Data flow

```
Profile + local_path + peer_path
    │
    ▼
┌──────────┐     ┌──────────┐
│ Scan(A)  │     │ Scan(B)  │     // Walk both sides
└────┬─────┘     └────┬─────┘
     │                 │
     ▼                 ▼
  Snapshot(A)      Snapshot(B)     // Map<RelPath, FileEntry>
     │                 │
     ▼                 ▼
┌────────────────────────────────┐
│         Diff(A, B, Index)      │  // Compare against last-synced state
└────────────┬───────────────────┘
             │
             ▼
        DiffResult { local_changes, remote_changes }
             │
             ▼
┌────────────────────────────────┐
│    Reconcile(DiffResult, mode, │
│              conflict_policy)  │  // Decide what to do
└────────────┬───────────────────┘
             │
             ▼
        SyncPlan { actions: Vec<Action> }
             │
             ▼
┌────────────────────────────────┐
│         Apply(plan, ctx)       │  // Execute atomically
└────────────┬───────────────────┘
             │
             ▼
        ApplyResult { applied, skipped, errors }
             │
             ▼
┌────────────────────────────────┐
│    Commit(index, applied)      │  // Update sync index
└────────────────────────────────┘
```

### Key types (in `synccore`)

```rust
// scan.rs
pub struct Snapshot {
    pub entries: BTreeMap<RelPath, FileEntry>,
}

pub struct FileEntry {
    pub path: RelPath,
    pub kind: EntryKind,         // File, Dir, Symlink
    pub size: u64,
    pub mtime: SystemTime,
    pub hash: Option<Hash>,      // BLAKE3, lazily computed
}

// diff.rs
pub struct DiffResult {
    pub local: Vec<Change>,      // Changes on side A (local)
    pub remote: Vec<Change>,     // Changes on side B (remote/peer)
}

pub enum Change {
    Created(RelPath),
    Modified(RelPath),
    Deleted(RelPath),
}

// reconcile.rs
pub struct SyncPlan {
    pub actions: Vec<Action>,
    pub conflicts: Vec<Conflict>,
}

pub enum Action {
    CopyFile { from: Side, path: RelPath },
    CreateDir { on: Side, path: RelPath },
    Delete { on: Side, path: RelPath },
}

pub struct Conflict {
    pub path: RelPath,
    pub kind: ConflictKind,       // BothModified, DeleteVsEdit
    pub resolution: Resolution,
    pub winner: Side,
}

// apply.rs
pub struct ApplyResult {
    pub applied: Vec<AppliedAction>,
    pub errors: Vec<ApplyError>,
}
```

### Path handling (FR-FH-6)

`RelPath` is a normalized relative path type that:
- Stores the display form (original casing, for user-facing output)
- Normalizes to NFD for comparison (macOS APFS behavior)
- Compares case-insensitively (macOS default)
- Implements `Ord` for deterministic BTreeMap ordering

This is the single point where macOS filesystem semantics are encoded. The engine never does raw string comparison on paths.

### Scan strategy (FR-SE-3, FR-SE-4)

1. Walk the tree using `walkdir` crate (respects depth, hidden-file toggle).
2. For each file: record `(size, mtime)`.
3. **Lazy hashing:** only compute BLAKE3 when:
   - The index has no entry for this path (new file — need hash for index).
   - Size/mtime differ from the index entry (file may have changed — confirm with hash).
   - Both sides have the same mtime but engine needs to confirm (clock skew scenario, deferred to M4).
4. Hashing is done streaming (4KB buffer) to handle large files without allocation.

### Diff algorithm (FR-SE-5)

For each side (A, B), compare current `Snapshot` against `SyncIndex`:

```
For each path in snapshot:
    if path not in index → Created
    if path in index AND (hash differs OR size/mtime changed AND hash confirms) → Modified
    else → Unchanged

For each path in index:
    if path not in snapshot → Deleted
```

The output is `DiffResult { local: Vec<Change>, remote: Vec<Change> }`.

### Reconciliation (FR-CR-1 through FR-CR-7)

**Push/Pull:** Trivial. The source's changes become copy/delete actions on the destination.
- If `delete_propagation = false` (additive), suppress Delete actions.

**Bidirectional:** The core logic:

```
For each path that appears in local changes OR remote changes:
    if changed on one side only → propagate that change to the other
    if changed on BOTH sides:
        if both Created/Modified with SAME hash → no conflict (converged)
        if both Deleted → no conflict (both agree on deletion)
        else → CONFLICT → apply policy
    if deleted on one, unchanged on other:
        if delete_propagation → delete on other
        else → no action
    if deleted on one, modified on other → CONFLICT (delete-vs-edit)
```

**First sync (FR-CR-7):** When index is empty, treat as: everything is "Created" on both sides. Identical hashes → no conflict. Different content → conflict per policy.

**Conflict policies (MVP):**
- `NewerWins`: Compare mtime, pick newer. Tie-break: prefer local (arbitrary but deterministic).
- `KeepBoth`: Rename the loser to `name (conflict from <side> YYYY-MM-DD).ext`.

### Apply (FR-FH-7, NFR-REL-1, NFR-REL-2)

Actions are ordered: `CreateDir` first (parents before children), then `CopyFile`, then `Delete` (children before parents).

File copy is atomic:
1. Write to `<dest_dir>/.filesync-tmp-<uuid>` on the same volume.
2. `fsync` the temp file.
3. `rename` temp → final path.

Per-file error handling: if one file fails (locked, permission error), record the error and continue with remaining actions.

### Sync index + run records (syncstore)

**New SQLite migrations:**
- `0002_profiles.sql`: profiles table
- `0003_sync_index.sql`: sync_index table (profile_id, anchor_idx, rel_path, kind, size, mtime, hash, sync_version)
- `0004_run_records.sql`: run_records table + run_conflicts + run_errors

The sync index key is `(profile_id, anchor_idx, normalized_rel_path)`.

`sync_version` is a per-profile monotonic counter incremented on each successful run — used for conflict-detection baseline.

### Integration test strategy

Tests in `synccore/tests/` (integration tests, not unit tests) exercise the full pipeline:
- Create temp dirs (A and B).
- Construct a `Profile` in memory.
- Call `engine::run_sync(profile, &db)`.
- Assert filesystem state and index state.

Property tests (`proptest`) for the reconciler:
- Generate random pairs of change sets.
- Assert: no data loss (a file present on either side and not in conflict is always preserved).
- Assert: idempotence (running reconcile twice produces the same plan).
- Assert: commutativity for bidirectional (swapping A/B produces the mirror plan).

## Execution order

1. **RelPath type** — `synccore/src/path.rs`: NFD normalization (using `unicode-normalization` crate), case-insensitive Ord, display form.
2. **Scan** — `synccore/src/scan.rs`: `scan_tree(root, config) -> Snapshot`. Unit tests: hidden files excluded, depth honored, symlinks skipped.
3. **Diff** — `synccore/src/diff.rs`: `compute_diff(local_snap, remote_snap, index) -> DiffResult`. Unit tests with synthetic snapshots.
4. **Reconcile** — `synccore/src/reconcile.rs`: `reconcile(diff, mode, policy) -> SyncPlan`. Property tests here. This is the correctness kernel.
5. **Plan** — `synccore/src/plan.rs`: action ordering (mkdir before copy, delete children before parents). Sorts `SyncPlan.actions`.
6. **Apply** — `synccore/src/apply.rs`: `apply_plan(plan, local_root, remote_root) -> ApplyResult`. Atomic writes, per-file error handling.
7. **Sync index schema** — `syncstore`: migrations 0002–0004, `IndexRepo` for CRUD.
8. **Engine orchestrator** — `synccore/src/engine.rs`: wires scan→diff→reconcile→plan→apply→commit. This is the entry point tests call.
9. **Integration tests** — `synccore/tests/`: AC-1 through AC-7, AC-11, AC-14.
10. **Property tests** — `synccore/tests/reconcile_props.rs`: proptest for reconciler invariants.

## New dependencies (workspace)

| Crate | Why |
|---|---|
| `unicode-normalization` | NFD for RelPath (FR-FH-6) |
| `walkdir` | Tree walking with depth control |
| `tempfile` | Test fixtures and atomic write temp files |
| `proptest` | Property tests for reconciler |
| `chrono` | Timestamp formatting in conflict renames |

## ADRs to write during execution

- **0005 — RelPath normalization model**: NFD + case-fold for comparison, preserves original for display. Why not NFC. Why case-insensitive always (macOS-only MVP, APFS default).
- **0006 — Lazy hashing strategy**: size+mtime shortcut, hash on demand. Why not hash everything upfront.
- **0007 — Conflict resolution in first-sync**: treat as union with conflict on differing content. Why not "source wins" for first sync.

## Done when

- `cargo test --workspace` passes including integration tests for AC-1 through AC-7, AC-11, AC-14.
- `cargo clippy --workspace --all-targets -- -D warnings` passes.
- Property tests for reconciler run with at least 256 cases (configurable via `PROPTEST_CASES`).
- Running the engine on two temp directories with real files produces correct results (spot-checked manually via a test that prints the plan).
- ADRs 0005–0007 exist on disk.
- The engine has **zero** dependencies on Tauri, tokio, or network code.
