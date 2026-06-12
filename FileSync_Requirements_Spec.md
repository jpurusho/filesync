# File Sync Application — Software Requirements Specification (SRS)

| | |
|---|---|
| **Document** | File Sync Application — Requirements Specification |
| **Version** | 0.2 (Draft) |
| **Date** | 2026-06-08 |
| **Status** | Draft — open decisions resolved (see §7 Decisions Log) |
| **Target platform (MVP)** | macOS only (source and target) |
| **Stack (MVP)** | Tauri 2 + React/TypeScript front end, Rust core (see §10) |

> **How to read this document.** Statements written with **shall** are firm requirements. Items marked **[REC]** are engineering recommendations. The nine MVP-blocking decisions have been made and are recorded in the **Decisions Log (§7)**; any items still marked **[OPEN-…]** are non-blocking and noted there.

---

## 1. Introduction

### 1.1 Purpose
This document specifies the functional and non-functional requirements for a desktop **File Sync** application that synchronizes files and folders between two computers on the same local network. It is intended to be the reference for design, implementation, and acceptance testing.

### 1.2 Scope

**In scope (MVP):**
- One-way and bidirectional sync of selected files/folders between **two** computers on a trusted LAN.
- Recursive sync to a configurable depth, with an option to include hidden files.
- Sync profiles (a named set of folders + settings) with locally persisted state.
- Automatic discovery of other app instances on the network, plus manual peer entry.
- Incremental sync (only changed files transferred).
- Replication of profile configuration to the peer.

**Deferred to later phases (explicitly out of MVP):**
- Block/chunk-level **delta sync** for large files (Phase 2).
- **Cloud / Google Drive–based** state management and cross-internet sync (Phase 3).
- Sync across **more than two** computers / many-to-many topologies (Phase 3).

### 1.3 Definitions & Glossary

| Term | Meaning |
|---|---|
| **Instance / Node** | A running copy of the File Sync app on one computer. |
| **Peer** | A remote instance an instance syncs with. |
| **Profile** | A named configuration: the set of folders to sync, the sync mode, filters, and settings. |
| **Anchor / Root** | The top-level folder a profile syncs; child paths are relative to it. |
| **Sync run** | A single execution of a sync for a profile. |
| **Sync index** | A per-profile local database recording the last-known-synced state of every path (path, size, mtime, content hash, version). Distinct from the live filesystem. |
| **Conflict** | The same path changed on **both** sides since the last successful sync. |
| **Push (A→B)** | One-way replication from source to destination. |
| **Pull (B→A)** | One-way replication in the reverse direction. |
| **Bidirectional (A+B)** | Two-way reconciliation; both sides converge to the same content. |
| **TOFU** | Trust On First Use — pin a peer's identity/key the first time it is paired. |

### 1.4 Phasing overview
- **Phase 1 (MVP):** modes A→B, B→A, A+B; profiles + local state; discovery + manual pairing; whole-file incremental transfer; conflict detection with a basic policy; profile replication.
- **Phase 2:** delta/chunk transfer; filesystem-watch–driven auto-sync; richer conflict UI.
- **Phase 3:** cloud/Drive state; >2 nodes; sync over the internet (relay/NAT traversal).

---

## 2. System Overview

The system is a **peer-to-peer** application: there is no central server. Each instance contains the same components and can act as source or destination.

**Logical components**
1. **UI layer** — profile management, drag/drop, status, conflict prompts.
2. **Profile manager** — CRUD on profiles; owns profile config and triggers runs.
3. **Discovery service** — advertises this instance and finds peers on the LAN.
4. **Transport layer** — authenticated, encrypted connection to a peer; moves file data and metadata.
5. **Sync engine** — scans folders, computes changes vs. the sync index, plans actions, resolves conflicts, applies changes atomically.
6. **State store** — local persistence of profiles, sync index, and run history.

```
  Computer A                         Computer B
+-----------------+   discovery    +-----------------+
| UI              |<-------------->| UI              |
| Profile Manager |                | Profile Manager |
| Sync Engine     |   transport    | Sync Engine     |
| State Store     |<==encrypted==> | State Store     |
| (index, profiles)|   sync data   | (index, profiles)|
+-----------------+                +-----------------+
```

---

## 3. Functional Requirements

### 3.1 Sync Modes (FR-SM)
- **FR-SM-1** The system **shall** support three sync modes per profile: **Push (A→B)**, **Pull (B→A)**, and **Bidirectional (A+B)**.
- **FR-SM-2** In Push/Pull, all selected files and folders from the source **shall** be replicated to the destination at the selected destination location, **retaining the exact hierarchical structure** beneath each anchor.
- **FR-SM-3** In Bidirectional mode, both sides **shall** converge so that each path holds the most-recent agreed version, reconciled using the sync index, content checksum, and modification time (see §3.6).
- **FR-SM-4** All sync directions **shall** be **incremental**: only files that differ from the destination (or have changed since the last run) are transferred (see §3.4).
- **FR-SM-5** Push/Pull **shall** offer two sub-behaviors, selectable per profile: **additive** (default — never delete on destination) and **mirror** (delete destination files that no longer exist on source). Delete propagation is **opt-in**; it is off unless the user explicitly enables it for the profile. The same opt-in governs deletion propagation in Bidirectional mode (see FR-CR-6).
- **FR-SM-6** The system **shall** support **quick-send**: a one-shot, profile-less transfer of a selected file or folder to a paired peer. Quick-send **shall not** create a profile, **shall not** write to any sync index, and **shall not** participate in drift tracking (FR-ST-7) or delete propagation. It **shall** reuse the authenticated, encrypted transport (NFR-SEC) and **shall** be recorded in the run history (FR-ST-4) as a distinct entry type. Quick-send writes to the destination **shall** still be atomic (FR-FH-7).

### 3.2 Sync Profiles (FR-PR)
- **FR-PR-1** A user **shall** be able to create, edit, rename, duplicate, and delete sync profiles.
- **FR-PR-2** A profile **shall** be creatable on **either** the source or the destination computer.
- **FR-PR-3** A profile **shall** contain: a name, the sync mode, a list of folders/files to sync (each with its anchor path on the local side and the corresponding target location on the peer), the target peer, recursion depth, include-hidden flag, ignore/filter rules, and conflict policy.
- **FR-PR-4** The user **shall** be able to add folders to a profile by **drag-and-drop** as well as via a file picker.
- **FR-PR-5** A profile **shall** support multiple anchor folders.
- **FR-PR-6 [REC]** Each profile **should** support an **ignore-pattern list** (e.g. glob patterns, default ignores for temp/system files such as `~$*`, `.DS_Store`, `Thumbs.db`, `desktop.ini`) to avoid syncing junk.
- **FR-PR-7** A profile **shall** be runnable on demand (manual trigger) in MVP.
- **FR-PR-8 [REC]** Profiles **should** later support automatic triggers: on filesystem change (watch) and/or on a schedule/interval (Phase 2).
- **FR-PR-9** Each profile **shall** have a stable UUID (per §6.1). Discarding a profile and recreating an equivalent one produces a **new** profile identity; the system **shall not** treat the recreated profile as a continuation of the old one on either side.
- **FR-PR-10** The system **shall** distinguish two destructive operations:
  - **Reset profile** — retain the profile configuration but clear its sync index/state; the next run behaves as a first sync (full rescan, no delete-awareness until a new baseline is established).
  - **Delete profile** — remove the profile configuration *and* its local state; per FR-PS-4 the user is prompted whether to also remove it (and its state) on the peer.
- **FR-PR-11** On save, the system **shall** detect **overlapping anchors** across profiles — the same folder, or a folder nested within another profile's anchor — and act based on the most permissive delete-propagation setting among the involved profiles:
  - If **all** overlapping profiles are **additive** (no delete propagation), the system **shall warn** the user and allow the save. The warning **shall** name the conflicting profiles and the overlapping path.
  - If **any** of the overlapping profiles has **mirror** semantics (delete propagation enabled, per FR-SM-5 — including Bidirectional with delete propagation on per FR-CR-6), the system **shall block** the save until the user removes the overlap or disables delete propagation, since a mirror profile can delete files that another profile is also managing.
  - Overlap detection **shall** be scoped to profiles targeting the **same peer**; profiles targeting different peers do not overlap for this rule.

### 3.3 Peer Discovery & Pairing (FR-DP)
- **FR-DP-1** The system **shall** automatically detect other File Sync instances running on the same local network and present them as selectable peers.
- **FR-DP-2** Discovery **shall** use **mDNS / DNS-SD (Zeroconf / Bonjour)** advertising a service such as `_filesync._tcp.local`, advertising at minimum: instance display name, host, port, app/protocol version, and a stable instance UUID. *(Native and well-supported on macOS.)*
- **FR-DP-3** The system **shall** allow **manual** peer entry by host/IP and port, for cases where discovery is blocked.
- **FR-DP-4** For MVP, a sync session **shall** involve exactly **two** instances. The discovery list may show more than one peer, but a given profile targets exactly one peer.
- **FR-DP-5** Before first sync, two instances **shall** complete a **pairing handshake**: each presents a stable identity (self-signed certificate / public key) confirmed via a short pairing code or fingerprint shown in both UIs, then pinned (**TOFU**). Subsequent connections trust the pinned identity. *(Adopted in MVP, not deferred.)*
- **FR-DP-6** The system **shall** display peer online/offline/last-seen status.

### 3.4 Sync Engine — Scanning & Change Detection (FR-SE)
- **FR-SE-1** The engine **shall** walk each anchor recursively, honoring the profile's **configurable depth** (e.g. depth 0 = anchor only, depth N = N levels deep, unlimited = full tree).
- **FR-SE-2** The engine **shall** include or exclude **hidden files/folders** per the profile flag (default: excluded). *(Platform note: "hidden" means dotfiles on Unix and the hidden attribute on Windows.)*
- **FR-SE-3** For each path the engine **shall** capture metadata: size, modification time, type (file/dir/symlink), and — when needed for comparison — a **content hash** using **BLAKE3** (fast, parallelizable, native Rust). *(Chosen over SHA-256 for change-detection speed; BLAKE3 is still cryptographically strong if integrity guarantees are later needed.)*
- **FR-SE-4** The engine **shall** determine that a file is unchanged using a cheap check first (size + mtime match the index) and **shall** fall back to a content hash when size/mtime are ambiguous or when integrity must be confirmed.
- **FR-SE-5** Change detection for Bidirectional mode **shall** compare current state against the **sync index** (last-synced state), classifying each path on each side as *unchanged / created / modified / deleted* (see §3.6 and §6.2). It **shall not** rely on a naive "compare A's current tree to B's current tree," because that cannot distinguish a new file on one side from a deletion on the other.
- **FR-SE-6 [REC]** The engine **should** optionally detect **renames/moves** by matching content hash + size when a path disappears and an identical one appears, to avoid re-transferring moved data (Phase 2).

### 3.5 Incremental & Delta Transfer (FR-DT)
- **FR-DT-1 (MVP)** Transfer **shall** be incremental at **whole-file** granularity: only files whose content differs are sent.
- **FR-DT-2 (Phase 2) [REC]** The system **should** add **block-level delta transfer**: split large files into chunks, exchange per-chunk hashes, and transmit only differing chunks, reassembling in correct sequence. *(This is the optimization the description defers; the index/protocol should be designed so it can be added without breaking compatibility.)*
- **FR-DT-3** Large-file and interrupted transfers **shall** be resumable: a sync interrupted mid-file **shall not** corrupt the destination and **shall** resume or safely restart on the next run (see FR-FH-7 on atomic writes).
- **FR-DT-4 [REC]** Data in transit **should** be compressed where beneficial (skipped for already-compressed types).

### 3.6 Conflict Detection & Resolution (FR-CR)
- **FR-CR-1** For Push/Pull modes, the source is authoritative; no conflict arises (the destination is overwritten per FR-SM-2 / FR-SM-5).
- **FR-CR-2** For Bidirectional mode, a **conflict** exists when the **same path has changed on both sides** since the last successful sync (as determined from the sync index). Differing content alone is not a conflict if only one side changed — that side simply wins.
- **FR-CR-3** Default conflict policy **shall** be **"most-recently-modified wins"** (newer-wins) based on modification time, consistent with the project description.
- **FR-CR-4** The system **shall** also support a **"keep both"** policy as a user-selectable option per profile: the losing copy is preserved by renaming it (e.g. `name (conflict from <peer> 2026-06-08).ext`) so no data is lost. *Optional additional policies (Prompt, Source-wins, Dest-wins) **[REC]** may be added later but are not required for MVP.*
- **FR-CR-5 [REC]** To make timestamp comparison reliable across machines, the system **should** handle **clock skew**: during the handshake, instances **should** exchange clock readings to estimate offset, apply a configurable skew tolerance, and prefer **content hash** to decide equality before using timestamps to decide ordering. *(Cross-machine mtimes are not directly comparable without this; see §6.)*
- **FR-CR-6** Deletions in Bidirectional mode **shall** be reconciled using the index: a path present in the index but now missing on one side, and unchanged on the other, **shall** be deleted on the other side; a *delete-vs-edit* situation **shall** be treated as a conflict and resolved per policy (default: keep the edited copy).
- **FR-CR-7** On the **first** sync of a profile (no prior index), the engine **shall** perform a reconciliation in which paths existing on only one side are treated as creations (union), and paths existing on both with differing content are treated as conflicts resolved per policy. This first-run behavior **shall** be clearly communicated to the user before it runs.
- **FR-CR-8** All conflict resolutions **shall** be recorded in the run log.

### 3.7 File & Filesystem Handling Rules (FR-FH)
- **FR-FH-1** The system **shall** support **all file types** (binary and text are treated identically as opaque byte streams).
- **FR-FH-2** The system **shall** preserve the relative directory hierarchy beneath each anchor.
- **FR-FH-3 [REC]** The system **should** preserve modification timestamps on the destination and **should** define behavior for file permissions/ownership across OSes (best-effort; document what is not portable, e.g. POSIX permissions on Windows).
- **FR-FH-4 [REC]** **Symbolic links / junctions** behavior **should** be configurable (skip, or copy as link, or follow) with **loop protection** when following, to avoid infinite recursion. Default: do not follow.
- **FR-FH-5 [REC]** **Special files** (devices, sockets, FIFOs) **should** be skipped and logged.
- **FR-FH-6** Because MVP is macOS-only, the engine **shall** handle macOS filesystem behavior correctly: APFS/HFS+ **case-insensitivity** (case-preserving but case-insensitive by default), and **Unicode NFD normalization** (macOS stores filenames decomposed). Path comparisons in the index **shall** normalize consistently so the same file is not seen as two. *(Cross-platform hazards — Windows reserved names, long-path limits — are deferred with the Windows/Linux ports.)*
- **FR-FH-9** Folder access on macOS **shall** account for the platform permission model (**TCC / Full Disk Access**). For folders added by drag-and-drop or picker, the app **shall** persist durable access using **security-scoped bookmarks** (if sandboxed) so a profile can re-sync across app launches without re-prompting; if distributed un-sandboxed (notarized DMG), it **shall** guide the user to grant Full Disk Access. **[OPEN-DIST: Mac App Store (sandboxed) vs notarized DMG — see §7.]**
- **FR-FH-7** Destination writes **shall** be **atomic**: write to a temp file in the same volume, fsync, then rename into place, so an interrupted write never leaves a partially-written file at the target path.
- **FR-FH-8 [REC]** The system **should** handle files that are **locked or change during a run** gracefully (retry/skip-and-report rather than fail the whole run).

### 3.8 State & Persistence (FR-ST)
- **FR-ST-1** Each instance **shall** persist, **locally**, all profiles and their sync state. No central server is required.
- **FR-ST-2** Per-profile state **shall** include: list of folders synced, **last sync time**, last result (success/partial/failed), and counts of folders/files (and bytes) synced.
- **FR-ST-3** The engine **shall** maintain a per-profile **sync index** recording, for each synced path, its last-synced size, mtime, content hash, and a sync version/generation (see §6.2). This index is what makes correct incremental and bidirectional sync possible.
- **FR-ST-4** The system **shall** keep a **run history** (timestamp, direction, files added/updated/deleted, conflicts, errors) viewable in the UI.
- **FR-ST-5** State **shall** survive app restarts and be stored in **SQLite** (via the Rust `rusqlite` or `sqlx` crate), used in a crash-safe, transactional manner (WAL mode). One database per profile or a single database with a profile key — **[REC]** single DB with per-profile tables/keys for simpler backup.
- **FR-ST-6 (Phase 3) [REC]** State **may** later be mirrored to the cloud / a shared Google Drive folder. The local store remains the source of truth; cloud is a convenience/backup. The data model **should** be designed so this can be added without redesign.
- **FR-ST-7** The system **shall** be able to compute and report **drift** for a profile: the difference between desired state (the profile definition) and observed state (current filesystem + last-synced index) — e.g. counts of files pending, in conflict, or failed, plus time since last clean sync. This desired-vs-observed reporting is a core advantage of the declarative profile model.

### 3.9 Profile Synchronization (FR-PS)
- **FR-PS-1** Profiles **shall** be **discoverable from either instance**: when paired, an instance can see profiles defined on its peer that target it.
- **FR-PS-2** On sync, the **profile configuration itself shall be replicated** to the peer, so both sides hold a consistent definition of what is being synced.
- **FR-PS-3 [REC]** Profile config replication **should** carry its own version/updated-at so that edits made on both sides can be reconciled; conflicting profile edits **should** be resolved by newer-wins or surfaced to the user. *(Path fields may need per-side mapping, since the same anchor lives at different local paths on A and B.)*
- **FR-PS-4** Deleting a profile on one side **shall** prompt whether to also remove it (and optionally its state) on the peer.

### 3.10 User Interface (FR-UI)
- **FR-UI-1** The UI **shall** let users create/edit profiles, choose mode, set depth, toggle hidden files, set filters, and pick a peer.
- **FR-UI-2** The UI **shall** support **drag-and-drop** of folders into a profile (FR-PR-4).
- **FR-UI-3** The UI **shall** display discovered peers with status, and allow manual peer entry and pairing confirmation.
- **FR-UI-4** During a run, the UI **shall** show progress (current file, files/bytes done vs. remaining) and allow cancel.
- **FR-UI-5** The UI **shall** present a **preview/dry-run** of planned actions (to copy / to update / to delete / conflicts) before applying, especially for mirror and bidirectional modes. **[REC]**
- **FR-UI-6** The UI **shall** display per-profile state: last sync time, counts, and run history; and surface conflicts and errors clearly.
- **FR-UI-7 [REC]** The UI **should** surface per-profile **compliance/health**: in-sync vs drifted status, pending/conflict/error counts, and last successful sync time, so profiles can be monitored at a glance. This view becomes the trigger surface for automatic reconciliation in Phase 2.

---

## 4. Non-Functional Requirements

### 4.1 Performance (NFR-PERF)
- **NFR-PERF-1** Incremental scans **shall** scale to large trees; an unchanged tree **should** be re-scanned quickly using size+mtime shortcuts before hashing.
- **NFR-PERF-2** Transfers **should** use the available LAN bandwidth efficiently (streaming, pipelining, optional parallelism), with optional throttling.

### 4.2 Reliability & Robustness (NFR-REL)
- **NFR-REL-1** A sync run **shall** be safely interruptible and resumable without data corruption (atomic writes, idempotent re-runs).
- **NFR-REL-2** A failure on one file **shall not** abort the entire run; failures are reported and the run continues where safe.
- **NFR-REL-3** The system **shall** never silently lose user data; destructive actions (delete propagation, conflict overwrite) **shall** be logged and, where configured, recoverable (e.g. "keep both" or a trash/versioned copy). **[REC]**

### 4.3 Security (NFR-SEC)
- **NFR-SEC-1** Peer connections **shall** be **encrypted in transit** using **TLS 1.3** (e.g. `rustls`), even on a trusted LAN.
- **NFR-SEC-2** Peers **shall** be **authenticated** via the pinned identity established at pairing (FR-DP-5); unpaired instances cannot initiate sync.
- **NFR-SEC-3** An instance **shall** only expose folders that are part of a profile targeting the requesting peer (no arbitrary filesystem access over the network).

### 4.4 Portability (NFR-PORT)
- **NFR-PORT-1** The application **shall** target **macOS only** for MVP (both source and target run macOS). The architecture **shall** keep OS-specific concerns isolated so Windows/Linux ports remain feasible later (see §10).
- **NFR-PORT-2** Filesystem differences **shall** be handled per FR-FH-6.

### 4.5 Usability (NFR-USE)
- **NFR-USE-1** Common tasks (create profile, add folders, run sync) **shall** be achievable without documentation; destructive options **shall** be clearly labeled.

### 4.6 Observability (NFR-OBS)
- **NFR-OBS-1** The system **shall** produce structured logs of runs, decisions, conflicts, and errors, exportable for troubleshooting.

---

## 5. Constraints & Assumptions

- **A-1** Both computers are on the **same local network** (MVP). Internet/relay sync is Phase 3.
- **A-2** The network is **trusted** (per the description). The recommended pairing/encryption still applies as defense-in-depth.
- **A-3** MVP supports exactly **two** instances per sync relationship.
- **A-4** Each computer's filesystem is locally writable by the app for the targeted folders.
- **C-1** No central server; the design is peer-to-peer.
- **C-2** Cloud/Drive state and delta sync are explicitly deferred.

---

## 6. Data Model (Indicative)

### 6.1 Profile (replicated; FR-PS)
```jsonc
{
  "id": "uuid",
  "name": "Photos backup",
  "mode": "PUSH | PULL | BIDIRECTIONAL",
  "peer": { "instanceId": "uuid", "displayName": "B-Laptop" },
  "anchors": [
    {
      "localPath": "/Users/me/Photos",     // differs per side
      "peerPath":  "D:\\Backup\\Photos",    // mapping on the peer
      "recursive": true,
      "maxDepth": -1,                        // -1 = unlimited
      "includeHidden": false
    }
  ],
  "ignorePatterns": ["*.tmp", ".DS_Store", "~$*"],
  "deletePropagation": false,                // mirror vs additive (FR-SM-5)
  "conflictPolicy": "NEWER_WINS | KEEP_BOTH | PROMPT | SOURCE_WINS | DEST_WINS",
  "version": 7,
  "updatedAt": "2026-06-08T12:00:00Z"
}
```

### 6.2 Sync Index (local only; per profile; FR-ST-3)
One record per known path, representing the **last successfully synced** state — the basis for detecting create/modify/delete on each side and for conflict logic.
```jsonc
{
  "path": "2024/trip/img001.jpg",   // relative to anchor
  "type": "file",
  "size": 482133,
  "mtime": "2026-06-01T09:22:11Z",
  "hash": "blake3:...",
  "syncVersion": 42                  // generation at last sync
}
```

### 6.3 Run Record (local; FR-ST-4)
```jsonc
{
  "runId": "uuid",
  "profileId": "uuid",
  "startedAt": "...", "finishedAt": "...",
  "direction": "A_TO_B | B_TO_A | BIDIRECTIONAL",
  "result": "SUCCESS | PARTIAL | FAILED",
  "counts": { "added": 12, "updated": 5, "deleted": 1, "bytes": 90211333 },
  "conflicts": [ { "path": "...", "resolution": "NEWER_WINS", "winner": "A" } ],
  "errors": [ { "path": "...", "reason": "locked" } ]
}
```

---

## 7. Decisions Log

All MVP-blocking questions are resolved as follows:

| # | Decision | Resolution | Affects |
|---|---|---|---|
| 1 | Target OS | **macOS only** (source + target); keep OS concerns isolated for later ports | NFR-PORT-1, §3.7, §10 |
| 2 | Conflict default | **Newer-wins** default; **keep-both** required as a selectable option | FR-CR-3, FR-CR-4 |
| 3 | Delete propagation | **Opt-in** per profile; default additive (no deletes) | FR-SM-5, FR-CR-6 |
| 4 | Hash algorithm | **BLAKE3** (fast, native Rust) | FR-SE-3 |
| 5 | Storage engine | **SQLite** (WAL, via rusqlite/sqlx) | FR-ST-5 |
| 6 | Discovery transport | **mDNS / DNS-SD (Bonjour)** | FR-DP-2 |
| 7 | Pairing & encryption | **TOFU pairing + TLS 1.3 now** (in MVP) | FR-DP-5, NFR-SEC |
| 8 | Auto-trigger | **Phase 2** (manual trigger in MVP) | FR-PR-7/8 |
| 9 | Tech stack | **Tauri 2 + React/TS + Rust core** | §10 |

**Remaining (non-blocking) decisions**
- **[OPEN-DIST]** Distribution channel: **Mac App Store** (sandboxed → security-scoped bookmarks mandatory, mDNS multicast entitlement needed) vs **notarized DMG** (un-sandboxed → simpler folder access, recommended for a sync tool). *Recommendation: notarized DMG for MVP.*
- Minimum supported macOS version (recommend macOS 13+).
- Whether the engine ships also as a headless CLI/daemon (the architecture supports it; see §10).

### 7.1 Interaction model — dual-pane browser (evaluated, rejected)

A dual-pane "browse local + remote filesystem, drag to transfer" model (FileZilla / Beyond Compare style) was evaluated and **deliberately not adopted**:
- A drag is **imperative and point-in-time**, whereas sync here is **declarative and ongoing**; bidirectional and delete-aware sync require the persistent index, which a drag gesture cannot express.
- Browsing the remote filesystem **widens the security scope** and is constrained by macOS TCC / Full-Disk-Access anyway, so the whole remote disk cannot reliably be shown.
- Profiles provide **auditable, manageable desired-state** with drift/compliance reporting (FR-ST-7, FR-UI-7); profile config is disposable metadata (FR-PR-9/10).
- A lightweight **quick-send** (FR-SM-6) covers the one-off-copy convenience the dual pane would have offered, without the persistence/security cost.

---

## 8. Representative Acceptance Scenarios

| # | Scenario | Expected result |
|---|---|---|
| AC-1 | Push A→B of a nested folder with hidden files excluded | B mirrors A's visible tree exactly; hidden files absent; counts logged |
| AC-2 | Re-run AC-1 with no changes | Zero files transferred; last sync time updated |
| AC-3 | Modify one file on A, re-run Push | Only that file is transferred (incremental) |
| AC-4 | Bidirectional: edit file X on A, edit file Y on B | X propagates to B, Y propagates to A, no conflict |
| AC-5 | Bidirectional: edit the **same** file on both since last sync | Conflict detected; resolved per policy; nothing silently lost |
| AC-6 | Bidirectional: delete a file on A (unchanged on B) | File deleted on B; index updated |
| AC-7 | Delete on A while edited on B (delete-vs-edit) | Treated as conflict; edited copy preserved by default |
| AC-8 | Interrupt a large-file transfer (kill app / drop network) | No partial/corrupt file at destination; resumes/retries cleanly on next run |
| AC-9 | Launch second instance on the LAN | First instance discovers it; pairing completes; peer shows online |
| AC-10 | Create profile on A, sync | Profile configuration appears on B (replicated) |
| AC-11 | Depth set to 1 | Only anchor + first level synced; deeper items ignored |
| AC-12 | Clock on B is 5 minutes ahead | Equality decided by hash; ordering not corrupted by skew |
| AC-13 | Delete a profile on A and recreate one with the same name and anchors | New profile has a fresh UUID; first run is treated as a first sync (FR-PR-9, FR-CR-7); no orphaned state on B is auto-relinked |
| AC-14 | Reset profile on A | Config retained; index cleared; next run rescans fully and establishes a new baseline without delete propagation |
| AC-15 | Save a new additive profile whose anchor overlaps an existing additive profile (same peer) | Save succeeds with a warning naming the conflicting profile and overlapping path |
| AC-16 | Save a profile with mirror semantics whose anchor overlaps any existing profile (same peer) | Save is blocked until the overlap is removed or delete propagation is disabled |
| AC-17 | Open the health view for a profile with local edits not yet synced | Drift report shows pending counts and last clean sync time without performing a sync |
| AC-18 | Quick-send a folder from A to B | Folder copied to B with atomic writes; no profile created on either side; run history records a quick-send entry; subsequent profile drift reports are unaffected |

---

## 9. Traceability — Your Requirements → This Spec

| From the project description | Covered by |
|---|---|
| Bidirectional sync between two computers | FR-SM-1, A-3 |
| Recursive to configurable depth | FR-SE-1 |
| Option to include hidden files | FR-SE-2 |
| A→B / B→A / A+B modes, retain hierarchy | FR-SM-1..3, FR-FH-2 |
| Mirror by timestamp + checksum, newer wins | FR-CR-2..5 |
| Incremental, only modified files | FR-SM-4, FR-DT-1 |
| Delta/chunk sync (later) | FR-DT-2 (Phase 2) |
| All file types | FR-FH-1 |
| Detect instances on network | FR-DP-1..3 |
| Two computers for now | FR-DP-4, A-3 |
| Create profiles on source or destination | FR-PR-2 |
| Profile = list of folders | FR-PR-3, FR-PR-5 |
| Maintain local state (folders, last sync, counts) | FR-ST-1..4 |
| Drag/drop folders to profile | FR-PR-4, FR-UI-2 |
| Cloud / Google Drive state (later) | FR-ST-6 (Phase 3) |
| Trusted network assumption | A-2, NFR-SEC |
| Profiles discoverable from A or B; config synced on sync | FR-PS-1..3 |
| Interaction model (profiles vs dual-pane) | §7.1, FR-ST-7, FR-UI-7 |
| One-off transfers (quick-send) | FR-SM-6 |

---

## 10. Technology Stack & Architecture

**Decision:** Tauri 2 + React/TypeScript front end over a Rust core. This is well-matched to the workload — the hard part of this app is a high-throughput, correctness-critical sync engine, and Rust is an excellent fit for that, while Tauri keeps the binary small and gives the UI a native macOS webview without bundling a browser (as Electron would).

### 10.1 Recommended stack

| Layer | Choice | Why |
|---|---|---|
| Shell / packaging | **Tauri 2** | Native WKWebView on macOS, tiny binaries, first-class Rust backend, built-in file-drop events, code-signing/notarization tooling |
| UI | **React + TypeScript**, **Vite** | Mature, fast iteration; matches your preference |
| UI state / data | **Zustand** (local UI state) + **TanStack Query** (calls into Rust) | Lightweight; good fit for command/event model |
| Styling | **Tailwind CSS** (+ optionally shadcn/ui) | Fast, consistent UI |
| UI↔core bridge | **Tauri commands** (request/response) + **Tauri events** (push progress) | No separate IPC server needed |
| Core language | **Rust** | Performance + memory safety for the sync engine |
| Async runtime | **tokio** | Concurrent scanning + transfers |
| Hashing | **blake3** crate | Decision #4 |
| State store | **SQLite** via **sqlx** or **rusqlite** (WAL) | Decision #5 |
| Discovery | **mdns-sd** crate (DNS-SD/Bonjour) | Decision #6 |
| Transport | **QUIC via `quinn`** (TLS 1.3 built in, multiplexed, resumable) — or TLS-over-TCP via `rustls` + `tokio` for a simpler MVP | NFR-SEC, FR-DT-3 |
| TLS / identity | **rustls** + self-signed cert per instance, pinned at pairing (TOFU) | FR-DP-5, NFR-SEC |
| FS walk / temp / atomic write | `walkdir` (or `jwalk` for parallel), `tempfile`, atomic rename | FR-FH-7 |
| File watching (Phase 2) | **notify** crate (FSEvents on macOS) | FR-PR-8 |
| macOS durable folder access | security-scoped bookmarks via `objc2`/`core-foundation` bindings | FR-FH-9 |

### 10.2 Architecture notes
- **Workspace split.** Put the engine in a standalone Rust **library crate** (e.g. `synccore`) that has *no* Tauri dependency, with the Tauri app as a thin shell calling into it. Benefits: the engine is unit-testable in isolation, reusable as a future headless **CLI/daemon**, and the eventual Windows/Linux ports reuse it unchanged (only OS-specific bits — bookmarks, hidden-file detection — sit behind a small platform trait). This directly satisfies NFR-PORT-1's "keep OS concerns isolated."
- **Transport recommendation.** Prefer **QUIC (`quinn`)**: TLS 1.3 is integral, and multiplexed streams make parallel + resumable transfers natural (helps FR-DT-3 now and delta sync in Phase 2). If you want the simplest possible MVP, TLS-over-TCP with a length-prefixed framed protocol is fine and easy to evolve.
- **Concurrency model.** A scan stage produces a change set (diff vs. the SQLite index); a planner turns it into ordered actions (mkdir → transfer → rename → delete); a bounded pool of transfer workers executes with back-pressure; the UI subscribes to progress via Tauri events.
- **macOS gotchas already designed for:** NFD filename normalization and case-insensitivity (FR-FH-6), TCC/Full-Disk-Access and security-scoped bookmarks (FR-FH-9), and notarization for distribution (§7 [OPEN-DIST]).

### 10.3 Honest trade-offs
- **Tauri vs Electron:** Tauri wins on size/performance and pairs naturally with a Rust core; the cost is a less mature ecosystem and occasional webview quirks. For a macOS-only, Rust-heavy app this trade-off favors Tauri clearly.
- **QUIC vs TLS/TCP:** QUIC is the better long-term fit but adds a little complexity; either is acceptable for MVP.
- **One process vs daemon:** shipping as a single Tauri app is simplest now; the library-crate split keeps the daemon option open without rework.
