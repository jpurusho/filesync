# 0006 — Lazy hashing (size+mtime shortcut)

**Status:** accepted
**Date:** 2026-06-12

## Context

FR-SE-4 requires: "determine that a file is unchanged using a cheap check first (size + mtime match the index) and fall back to a content hash when size/mtime are ambiguous."

Hashing every file on every scan would be correct but slow for large trees (a 100K-file tree with BLAKE3 at ~1 GB/s still takes noticeable time for a "nothing changed" run). The sync index records the last-synced `(size, mtime, hash)` per file, giving us a cheap shortcut.

## Decision

During `diff_side()`:
1. If the file is not in the index → `Created` (hash computed later during apply/index-commit).
2. If `size != index.size` → `Modified` (size change is conclusive; no hash needed).
3. If size matches but `mtime != index.mtime` → hash the file, compare to index hash. If different → `Modified`. If same → unchanged (mtime changed due to touch/copy but content is identical).
4. If both size and mtime match → `Unchanged` (skip hashing entirely).

Hashes are NOT computed during `scan_tree()`. The scan captures `(size, mtime)` only. This makes the common "nothing changed" case O(stat) per file, not O(read).

**Trade-off with correctness:**
- Step 4 can miss a change where content changed but size and mtime are identical (extremely unlikely in practice — requires sub-second write at exact same size). This is an acceptable trade-off per FR-SE-4.
- The user can force a full hash comparison by resetting the profile (clearing the index), which triggers first-sync behavior.

## Consequences

- `Snapshot.entries[].hash` is `Option<String>` (None during scan, populated on demand).
- A "nothing changed" re-run (AC-2) does zero file reads beyond stat — just walks + compares metadata against index.
- `hash_file()` uses streaming I/O (not `fs::read` into memory) — safe for large files.
- Future: if we add `--verify` mode, it can unconditionally hash and compare. The architecture supports it without changes to the scan/diff interface.
