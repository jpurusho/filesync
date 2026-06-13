# 0005 — RelPath normalization model

**Status:** accepted
**Date:** 2026-06-12

## Context

macOS APFS stores filenames in NFD (decomposed Unicode) and is case-insensitive by default (case-preserving). If the sync engine compares raw path strings, it will see "café.txt" (NFC) and "café.txt" (NFD) as different files — causing phantom creates/conflicts on every run. Similarly, "Photo.JPG" and "photo.jpg" would appear as two distinct paths when only one file exists on disk.

## Decision

`RelPath` normalizes for comparison but preserves the original form for display:

- **Storage:** Two fields — `display` (original casing/encoding) and `normalized` (NFD + lowercase).
- **Comparison (`Eq`, `Ord`, `Hash`):** Uses the normalized form. Two paths that refer to the same file on APFS always compare equal.
- **Filesystem operations:** Uses the `display` form. We write files with their original casing.
- **NFD, not NFC:** macOS stores NFD natively. Converting to NFC would mean every scan reconverts back — pointless overhead. We normalize *to* what the OS stores.
- **Case-insensitive always:** The MVP is macOS-only and APFS default is case-insensitive. We don't offer a case-sensitive mode because that would require detecting the volume's format (which is possible but adds complexity with no MVP benefit).

## Consequences

- A file renamed only in casing (e.g., `readme.md` → `README.md`) is invisible to the diff engine — both names map to the same index entry. Rename detection (Phase 2, FR-SE-6) would handle this if we wanted.
- `BTreeMap<RelPath, _>` naturally groups paths that differ only in casing/encoding together — no duplicate entries.
- When porting to Windows: same model works (NTFS is also case-insensitive by default). For Linux (case-sensitive ext4), we'd need a volume-aware switch — but that's explicitly out of scope for MVP.
- The `unicode-normalization` crate dependency is lightweight (~50 KB) and widely used.
