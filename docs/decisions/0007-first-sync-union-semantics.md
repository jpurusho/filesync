# 0007 — First-sync reconciliation: union with conflict on differing content

**Status:** accepted
**Date:** 2026-06-12

## Context

FR-CR-7 specifies first-sync behavior: "paths existing on only one side are treated as creations (union), and paths existing on both with differing content are treated as conflicts resolved per policy."

Alternative considered: "source wins" for first sync (treat like a push even in bidirectional mode). Rejected because it would silently overwrite files that only exist on the remote side — violating NFR-REL-3 ("never silently lose user data").

## Decision

When the sync index is empty (first run or after profile reset):

1. **Paths only on one side** → Created on that side. The reconciler propagates them to the other side (union). Both sides end up with the same files.
2. **Paths on both sides with identical content** (same hash) → No conflict. Both sides already agree.
3. **Paths on both sides with different content** → Conflict, resolved per profile policy (default: newer-wins).

This means the first sync of a bidirectional profile is always a **merge** (union), never a one-directional overwrite. Files that exist only on B arrive on A, and vice versa.

**Why empty index → all "Created":**
- With no index entry, `diff_side()` classifies every path in the snapshot as `Created`.
- The reconciler sees `Created` on both sides for paths that exist on both → enters the "both changed" branch → checks content equality → conflict or no-action.

## Consequences

- AC-14 (reset profile): after clearing the index, the next run does a full union merge. No deletions are propagated (there's no "was in index but now missing" signal — everything is "new").
- If both sides have diverged significantly before first sync, the user may see many conflicts. The spec requires "clearly communicating" this to the user before it runs (FR-CR-7) — that's a UI concern for M6.
- For push/pull mode, first-sync is simpler: source is authoritative, everything on source is "Created" and copied to dest. No conflict possible.
