# 15. Profile Version Reconciliation

Date: 2026-06-16

## Context

When both instances can edit a profile, we need a deterministic rule for which edit wins on replication. Options considered: CRDTs, vector clocks, timestamp-only, or a monotonic counter.

## Decision

Each profile carries a monotonic `version: u64` bumped on every local save, plus an `updated_at` timestamp. On replication:

- `incoming.version > local.version` → accept incoming (it has more edits).
- `incoming.version < local.version` → reject; respond with local copy so initiator updates.
- `incoming.version == local.version` → no-op (already in sync).

The responder's `ProfileConflict` response carries its full profile so the initiator can self-correct in a single round-trip.

## Consequences

- Deterministic: no tie-breaking needed because versions are integers. True version equality means content is identical (since every edit bumps the counter).
- Simpler than vector clocks (justified by the two-node constraint — only two actors can edit).
- A user who edits on both sides between syncs will lose one side's edits (last-writer-wins). This is acceptable for profile config; for file data we have proper conflict detection.
- The `updated_at` timestamp exists for user visibility (UI can show "last edited") but is not used for ordering decisions.
