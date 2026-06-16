# M5 — Profile Replication

**Status:** complete
**Owner model:** Opus (protocol design + correctness of version reconciliation)

## Goal

Make profile configurations a shared artifact: when two instances are paired, each can discover which profiles the peer has defined that target it, and sync runs replicate the profile config so both sides hold a consistent definition. Profile edits are reconciled by version/timestamp, and deletions are communicated to the peer.

After M5:
- Instance B can query instance A for "profiles that target me" and vice versa.
- Each sync run replicates the profile config to the peer; the peer upserts it into its own store.
- If both sides edit the same profile, newer-wins by `updated_at`.
- Deleting a profile locally sends a tombstone notification to the peer (the "prompt" is a UI concern for M6; M5 provides the wire protocol and local handling).

## Requirements covered

- FR-PS-1: Profiles discoverable from either instance
- FR-PS-2: Profile config replicated on sync
- FR-PS-3: Version-based reconciliation of profile edits
- FR-PS-4: Delete propagation protocol (prompt UI deferred to M6)

## Non-goals (M5)

- UI for profile conflict display (M6)
- Profile overlap detection (FR-PR-11) — already validatable at profile-save time; not a wire concern
- Auto-trigger / watch (Phase 2)
- Drift reporting on the profile itself (M6)

---

## Design decisions

### 1. Replication is piggy-backed on sync session start

Rather than a separate "profile sync" connection, the initiator sends the full profile config as part of `StartSession`. The responder:
1. Checks if it already has this profile (by UUID).
2. If new: inserts it (with path fields flipped — see §3 below).
3. If existing: compares `updated_at`; if the incoming version is newer, upserts; if local is newer, responds with its own version so the initiator can update.

**Why piggyback?** The profile is already identified in `StartSession`; sending the config there avoids a separate handshake. A standalone "list/fetch profiles" RPC is also needed for FR-PS-1 (discovery without running a sync), so we provide both.

### 2. Path-field mapping ("local" vs "remote" is relative)

On instance A, an anchor has:
```
local_path:  /Users/alice/Photos
remote_path: /Users/bob/Backup/Photos
```

When this profile is replicated to instance B, B stores it as:
```
local_path:  /Users/bob/Backup/Photos     ← was remote_path on A
remote_path: /Users/alice/Photos           ← was local_path on A
```

The wire format uses **neutral field names** (`side_a_path`, `side_b_path`) plus an `origin_instance_id` to indicate which side is A. Each instance, on receipt, maps:
- If I am `origin_instance_id` → `local_path = side_a_path, remote_path = side_b_path`
- If I am the peer → `local_path = side_b_path, remote_path = side_a_path`

This is unambiguous and survives round-trips without information loss.

### 3. Version reconciliation

Each profile carries a monotonic `version: u64` (bumped on every save) and an `updated_at` timestamp. On replication:
- If incoming `version > local version` → accept incoming.
- If incoming `version < local version` → reject; respond with local copy so initiator can update.
- If incoming `version == local version` → no-op (already in sync).

**Why version counter, not just timestamp?** Timestamps can collide (same-second edits) or be affected by clock skew. A monotonic counter breaks ties deterministically. If versions diverge (A has v5, B has v6 — both edited independently), we treat this as a conflict: higher version wins (the side that edited more recently). If versions are equal but content differs (shouldn't happen if both are disciplined), timestamp breaks the tie.

### 4. Profile deletion protocol

When a user deletes a profile locally:
1. The profile is removed from the local store.
2. A `ProfileDeleted { profile_id, deleted_at }` notification is sent to the peer if currently connected, or queued and sent on next connection.
3. The peer receives the notification and:
   - In M5: marks the profile as "pending deletion" (a flag in the DB) so it doesn't appear in active lists but isn't hard-deleted yet.
   - In M6 (UI): shows a prompt "Profile X was deleted by peer — also delete locally?"

For M5, we implement the flag + the RPC. The auto-prompting is UI work.

### 5. Queued notifications (tombstones)

If the peer is offline when a profile is deleted, the tombstone must be delivered later. We add a small `profile_tombstones` table:
```sql
profile_id TEXT PRIMARY KEY,
deleted_at TEXT NOT NULL,
delivered INTEGER NOT NULL DEFAULT 0
```

On each connection to the peer, undelivered tombstones are sent. Once acknowledged, `delivered = 1`.

---

## Protocol changes

### New RPC variants

```rust
RpcRequest::ListProfiles
// → Returns all profiles that target the requesting peer

RpcRequest::GetProfile { profile_id: Uuid }
// → Returns full profile config for one profile

RpcRequest::ReplicateProfile { profile: WireProfile }
// → Sent during StartSession (or standalone); peer upserts if version is higher

RpcRequest::ProfileDeleted { profile_id: Uuid, deleted_at: String }
// → Tombstone notification
```

```rust
RpcResponse::ProfileList { profiles: Vec<WireProfileSummary> }
// → Response to ListProfiles

RpcResponse::ProfileData { profile: WireProfile }
// → Response to GetProfile

RpcResponse::ProfileConflict { local_version: WireProfile }
// → Response to ReplicateProfile when local version is newer (initiator should update)

RpcResponse::ProfileAccepted
// → Response to ReplicateProfile when accepted
```

### Wire profile format

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireProfile {
    pub id: Uuid,
    pub name: String,
    pub mode: SyncMode,
    pub delete_propagation: bool,
    pub conflict_policy: ConflictPolicy,
    pub version: u64,
    pub updated_at: String,
    pub origin_instance_id: Uuid,
    pub anchors: Vec<WireAnchor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireAnchor {
    pub side_a_path: String,     // path on origin instance
    pub side_b_path: String,     // path on peer
    pub max_depth: i32,
    pub include_hidden: bool,
    pub ignore_patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireProfileSummary {
    pub id: Uuid,
    pub name: String,
    pub mode: SyncMode,
    pub version: u64,
    pub updated_at: String,
}
```

---

## Schema changes (syncstore)

### Migration 7: profile versioning + tombstones + peer binding

```sql
ALTER TABLE profiles ADD COLUMN version INTEGER NOT NULL DEFAULT 1;
ALTER TABLE profiles ADD COLUMN peer_id TEXT NOT NULL DEFAULT '';
ALTER TABLE profiles ADD COLUMN origin_instance_id TEXT NOT NULL DEFAULT '';
ALTER TABLE profiles ADD COLUMN pending_deletion INTEGER NOT NULL DEFAULT 0;

CREATE TABLE profile_tombstones (
    profile_id TEXT PRIMARY KEY NOT NULL,
    deleted_at TEXT NOT NULL,
    delivered INTEGER NOT NULL DEFAULT 0
);
```

- `version`: monotonic counter, bumped on every update.
- `peer_id`: the UUID of the paired peer this profile targets (already stored as `peer_name` textually; this adds the stable UUID).
- `origin_instance_id`: which instance originally created this profile (used for path mapping).
- `pending_deletion`: set to 1 when the peer reports deletion; the profile is hidden from active lists.

---

## Implementation steps

### Step 1: Schema migration + store layer

- Add migration 7 to `syncstore/src/migrations.rs`
- Add `version`, `peer_id`, `origin_instance_id`, `pending_deletion` to `ProfileRow`
- Add `insert_profile_tombstone`, `list_undelivered_tombstones`, `mark_tombstone_delivered` to `Db`
- Add `increment_profile_version` helper that bumps version + updated_at on update
- Add `list_profiles_for_peer(peer_id: Uuid)` query

### Step 2: Wire types + RPC extension

- Add `WireProfile`, `WireAnchor`, `WireProfileSummary` to `rpc.rs`
- Add new `RpcRequest` and `RpcResponse` variants
- Add conversion functions: `ProfileRow + Vec<AnchorRow>` → `WireProfile` (with instance_id for path mapping) and `WireProfile` → `(ProfileRow, Vec<AnchorRow>)` (flipping paths based on local instance_id)

### Step 3: Handler implementation

- `handle_list_profiles`: query profiles where `peer_id` matches the session's peer identity; return summaries
- `handle_get_profile`: look up by id, convert to wire format, return
- `handle_replicate_profile`: compare versions, upsert or return conflict
- `handle_profile_deleted`: mark as pending_deletion

### Step 4: Session integration

- In `run_remote_push`/`run_remote_pull`/`run_remote_bidi`: after `StartSession` succeeds, send `ReplicateProfile` with the current profile config. Handle `ProfileAccepted` or `ProfileConflict` (if conflict, update local profile with the newer version from the peer).
- After session setup, check and send any undelivered tombstones for this peer.

### Step 5: Standalone profile discovery

- Add a `list_peer_profiles` function in session.rs that connects to a peer, sends `ListProfiles`, and returns the summaries. This is used by the UI to show "profiles on peer that target me" without running a sync.

### Step 6: Integration tests

- `e2e_profile_replicated_on_push`: create profile on A, run push → profile appears on B with paths flipped
- `e2e_profile_version_conflict`: both sides edit profile → higher version wins; loser updates
- `e2e_profile_deleted_notification`: delete on A → B marks as pending_deletion
- `e2e_profile_tombstone_delivered_on_reconnect`: delete while offline → tombstone sent on next sync
- `e2e_list_peer_profiles`: query peer for profiles targeting this instance

---

## Files touched

| File | Change |
|---|---|
| `crates/syncstore/src/migrations.rs` | Add migration 7 |
| `crates/syncstore/src/profiles.rs` | Add version, peer_id, origin_instance_id, pending_deletion to ProfileRow; add tombstone operations; add list_profiles_for_peer |
| `crates/syncnet/src/rpc.rs` | Add WireProfile, WireAnchor, WireProfileSummary; new RPC variants |
| `crates/syncnet/src/handler.rs` | Add profile replication handlers |
| `crates/syncnet/src/session.rs` | Integrate ReplicateProfile into sync runs; add list_peer_profiles; send tombstones |
| `crates/syncnet/tests/e2e_sync.rs` | New profile-replication tests |

---

## ADRs to write during execution

- **0014 — Profile replication path mapping**: Why side_a/side_b neutral naming over local/remote on wire; round-trip safety.
- **0015 — Profile version reconciliation**: Why monotonic counter + timestamp, not CRDTs or vector clocks (two-node constraint simplifies this).

---

## Done when

- Running any sync (push/pull/bidi) replicates the profile config to the peer with correct path mapping.
- Editing a profile on either side and re-syncing converges both sides to the newer version.
- Deleting a profile sends a tombstone; peer marks it as pending_deletion.
- Offline tombstones are delivered on next connection.
- `list_peer_profiles` returns profiles the peer has that target this instance.
- `cargo test --workspace` passes.
- `cargo clippy --workspace --all-targets` clean.
