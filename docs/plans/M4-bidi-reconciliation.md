# M4 — Bidirectional Reconciliation over the Wire + Clock-skew Handling

**Status:** complete
**Owner model:** Opus (correctness-critical reconciler and protocol changes)

## Goal

Make the bidirectional sync mode fully operational end-to-end over the network. After M4:
- A bidirectional profile run exchanges scans from both sides, reconciles correctly, executes a mixed-side plan (push some files, pull others, resolve conflicts), and commits an accurate updated index.
- Clock skew between two machines does not corrupt conflict resolution.

## Requirements covered

- FR-SM-3: Bidirectional mode — both sides converge to the same content
- FR-CR-2..7: Conflict detection, resolution, delete-vs-edit, first-run union
- FR-CR-5: Clock skew handling (offset exchange, hash-first equality)
- FR-ST-3: Sync index updated after a successful run
- NFR-REL-1: Interruptible, resumable without corruption (index only committed on success)

## Non-goals (M4)

- UI for conflict surfacing (M6)
- Profile replication (M5)
- Delta/chunk transfer (Phase 2)
- Drift reporting (M6)

---

## What already exists and works

The reconciler in `synccore` already handles bidirectional logic correctly — both-modified (newer-wins + keep-both), delete-vs-edit, one-side-only changes, delete propagation. Tests pass. M3 gave us ScanRemote, PutFile, GetFiles, MkdirRemote, DeleteRemote RPCs.

**What's missing:**
1. No `run_remote_bidi` function — the push/pull functions only execute one-sided plans.
2. `RenameConflict { on: Remote }` has no RPC implementation (handler would silently skip it).
3. `pick_newer` in reconcile.rs uses raw mtimes with no clock-skew compensation.
4. No remote RPC variant for renaming a file (needed for KeepBoth conflict copy).
5. No index update returned from any `run_remote_*` function — callers have no way to commit the new index after a run.

---

## Protocol changes

### 1. Clock offset in StartSession

Change `StartSession` request to carry the initiator's wall-clock time:

```rust
RpcRequest::StartSession {
    profile_id: Uuid,
    mode: SyncMode,
    anchors: Vec<AnchorSpec>,
    initiator_unix_secs: i64,   // NEW — initiator's current Unix timestamp
}
```

Change the responder's reply from `RpcResponse::Ok` to a new variant:

```rust
RpcResponse::SessionStarted {
    clock_offset_secs: i64,  // responder_time - initiator_time
},
```

The initiator reads this offset and passes it into `ReconcileContext`. When comparing mtimes across sides, the reconciler adjusts the remote mtime: `adjusted_remote_mtime = remote_mtime - offset`. This makes both timestamps "in the initiator's clock domain." Hash equality is checked first (FR-CR-5: hash before timestamp).

### 2. RenameRemote RPC

Add one new RPC variant needed for KeepBoth conflict resolution:

```rust
RpcRequest::RenameRemote {
    anchor_id: Uuid,
    path: RelPath,
    new_name: String,   // full relative path of the new name
},
```

Handler renames the file at `anchor_root/path` to `anchor_root/new_name` using `std::fs::rename`. Returns `RpcResponse::Ok`.

---

## ReconcileContext changes (synccore)

Add `clock_offset_secs: i64` to `ReconcileContext`. Update `pick_newer` to:

```rust
fn pick_newer(path: &RelPath, ctx: &ReconcileContext<'_>) -> Side {
    // Apply clock offset: remote mtime is adjusted into initiator's clock domain
    // Equality within CLOCK_SKEW_TOLERANCE → prefer hash comparison (done in content_is_same)
    let local_mtime_secs = ... // as i64
    let remote_mtime_secs = ... // as i64
    let adjusted_remote = remote_mtime_secs - ctx.clock_offset_secs;
    if (local_mtime_secs - adjusted_remote).abs() <= CLOCK_SKEW_TOLERANCE_SECS {
        // Too close to call by timestamp; hash already checked in content_is_same — tie-break local
        Side::Local
    } else if local_mtime_secs >= adjusted_remote {
        Side::Local
    } else {
        Side::Remote
    }
}
```

`CLOCK_SKEW_TOLERANCE_SECS = 5` (configurable later; 5s covers typical LAN drift).

Add `clock_offset_secs: i64` to `ReconcileContext`. Existing callers (M3 push/pull, tests) pass `0` — no behavioral change for unidirectional modes.

---

## Index commit

Change the return type of all `run_remote_*` functions to include an updated index:

```rust
pub struct RemoteSyncResult {
    pub run_id: Uuid,
    pub files_transferred: u64,
    pub bytes_transferred: u64,
    pub errors: Vec<String>,
    pub updated_index: SyncIndex,   // NEW — callers persist this to syncstore
}
```

After executing the plan, build `updated_index` by applying each successfully-executed action to a clone of the input index:

- `CopyFile { from: Local, path }` → upsert index entry from `local_snap` entry
- `CopyFile { from: Remote, path }` → upsert index entry from `remote_snap` entry
- `Delete { on: _, path }` → remove from index (both sides agree)
- `RenameConflict { on: Local, path, new_name }` → rename entry in index (path → new_name) using local_snap data
- `RenameConflict { on: Remote, path, new_name }` → rename entry in index similarly using remote_snap data
- `CreateDir` → skip (dirs are implicit)
- On per-file error: **do not** update index for that path (leave old entry; next run re-detects)

---

## New function: `run_remote_bidi`

```rust
pub async fn run_remote_bidi<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut FramedStream<S>,
    config: &RemoteSyncConfig,
    index: &SyncIndex,
) -> Result<RemoteSyncResult>
```

Session flow:

```
Initiator (A)                              Responder (B)
    |                                           |
    |-- StartSession {bidi, initiator_time} --> |
    |<-- SessionStarted {clock_offset} -------- |
    |                                           |
    |  (for each anchor:)                       |
    |-- ScanRemote {anchor_id} --------------> |
    |<-- Snapshot(remote_snap) ---------------- |
    |  [local scan]                             |
    |  [diff(local_snap, remote_snap, index)]   |
    |  [reconcile(bidi, policy, clock_offset)]  |
    |  [plan]                                   |
    |                                           |
    |  (for each action in plan:)               |
    |  CopyFile{from:Local}  → PutFile + data  |
    |  CopyFile{from:Remote} → GetFiles         |
    |  CreateDir{on:Remote}  → MkdirRemote     |
    |  CreateDir{on:Local}   → local fs::create_dir_all
    |  Delete{on:Remote}     → DeleteRemote    |
    |  Delete{on:Local}      → local fs::remove
    |  RenameConflict{on:Remote} → RenameRemote|
    |  RenameConflict{on:Local}  → local rename|
    |                                           |
    |-- EndSession {run_id} -----------------> |
    |<-- Ok ----------------------------------- |
```

Key implementation note: `GetFiles` takes a list of paths and the responder streams them back. For bidi, the list of paths-to-pull comes from `CopyFile { from: Remote }` actions, batched and sent after all push/mkdir/delete/rename actions for that anchor.

Ordering: for correctness, process actions in the order `plan::order_actions` produces:
1. CreateDir (both sides)
2. CopyFile (interleaved push/pull — push first, pull in a batch via GetFiles)
3. Delete (both sides)
4. RenameConflict (both sides, before the conflicting CopyFile that follows it)

`plan::order_actions` already handles mkdir-before-copy and rename-before-copy ordering. Verify it handles RenameConflict correctly (rename must precede the CopyFile that writes the winner).

---

## Execution order

1. **ReconcileContext + pick_newer** — add `clock_offset_secs` field, update `pick_newer` with tolerance. Update all callers to pass `0`. Add unit tests for skew scenarios.

2. **RPC protocol** — add `initiator_unix_secs` to `StartSession`, add `SessionStarted`/`RenameRemote` variants. Update `handler.rs` to respond with `SessionStarted` and implement `handle_rename_remote`. Update `session.rs` `expect_ok` after `StartSession` to `expect_session_started` that returns `i64`.

3. **`run_remote_bidi`** — implement the full mixed-side plan executor. Reuse `execute_push_action` helpers; add local-side action executors for CreateDir/Delete/Rename.

4. **Index update logic** — implement `build_updated_index` helper that takes `(old_index, plan, executed_paths, local_snap, remote_snap)` and produces the new `SyncIndex`. Integrate into all three `run_remote_*` functions.

5. **Integration tests** — add to `e2e_sync.rs`:
   - `e2e_bidi_non_conflicting`: A has file_a, B has file_b → both land on both sides
   - `e2e_bidi_conflict_newer_wins`: both modify same file, B's is newer → B wins on both sides
   - `e2e_bidi_conflict_keep_both`: same file modified on both → both sides get both copies
   - `e2e_bidi_delete_vs_edit`: A deletes, B edits → edited copy restored on A
   - `e2e_bidi_clock_skew`: inject `clock_offset_secs = 60` → hash equality takes precedence, no spurious conflict
   - `e2e_index_updated`: verify `updated_index` from push run reflects transferred files

---

## Files touched

| File | Change |
|---|---|
| `crates/synccore/src/reconcile.rs` | Add `clock_offset_secs` to `ReconcileContext`, update `pick_newer` |
| `crates/syncnet/src/rpc.rs` | Add `initiator_unix_secs` to `StartSession`, `SessionStarted`, `RenameRemote` |
| `crates/syncnet/src/handler.rs` | Respond with `SessionStarted`, add `handle_rename_remote` |
| `crates/syncnet/src/session.rs` | Add `run_remote_bidi`, update `run_remote_push`/`run_remote_pull` to return `updated_index`, add `expect_session_started` |
| `crates/syncnet/tests/e2e_sync.rs` | New bidi + clock-skew + index tests |

No new crates or migrations needed.

---

## ADRs to write during execution

- **0012 — Clock skew compensation strategy**: Why offset-on-start-session vs NTP, why 5s tolerance, hash-first equality.
- **0013 — Index commit responsibility**: Why caller (not session function) persists to DB; why per-file error leaves old index entry.

---

## Done when

- `run_remote_bidi` correctly propagates changes in both directions over TLS.
- Conflict detection fires for same-path edits on both sides; newer-wins and keep-both both work.
- Clock offset of ±60 seconds does not cause a spurious conflict when files have identical content.
- `updated_index` reflects exactly the files that were successfully transferred; failed files retain old entries.
- `cargo test --workspace` passes.
- `cargo clippy --workspace --all-targets` clean.
