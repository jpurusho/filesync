# M6 — Tauri/React UI

**Status:** planning
**Owner model:** Sonnet (mechanical UI wiring) with Opus for state design review

## Goal

Expose the sync engine through a desktop UI. After M6, a user can:
- Create, edit, and delete sync profiles via a form
- Pair with a peer (enter address, confirm pairing)
- Trigger push/pull/bidi sync and see progress
- View profile conflict resolution results
- Confirm or reject profile deletions from a peer (tombstone prompt)
- See basic drift/status reporting per profile

## Priority order

1. **Tauri commands** — expose backend operations to the frontend
2. **Core UI** — profile list, profile editor, peer pairing, sync trigger
3. **Profile conflict display** — show when version reconciliation occurred
4. **Deletion prompt** — prompt user when peer deletes a profile
5. **Drift/status reporting** — show last sync time, file counts, pending changes

## Non-goals (M6)

- Auto-trigger / watch mode (Phase 2)
- Multi-peer management (only one peer per profile for now)
- Mobile / web target
- Theming / dark mode (use system default via Tailwind)

---

## Design decisions

### 1. State management: Zustand

Lightweight, no boilerplate, works well with Tauri's async invoke pattern. One store with slices for profiles, peers, and sync status.

### 2. Routing: single-page with tab navigation

No react-router needed yet. A simple tab bar (Profiles | Peers | Sync) with conditional rendering. Keeps bundle small and avoids complexity.

### 3. Tauri command surface

Each command maps to one backend operation. Commands return serializable types (serde JSON). Errors return as `Result<T, String>` (Tauri convention).

### 4. Event-driven sync progress

Tauri events (not polling) for sync progress updates. The backend emits events (`sync-progress`, `sync-complete`, `sync-error`) that the frontend subscribes to.

---

## Tauri commands to implement

| Command | Backend operation | Returns |
|---|---|---|
| `list_profiles` | `Db::list_profiles()` | `Vec<ProfileView>` |
| `get_profile` | `Db::get_profile(id)` + anchors | `ProfileDetail` |
| `create_profile` | `Db::insert_profile(...)` | `ProfileView` |
| `update_profile` | `Db::update_profile(...)` | `ProfileView` |
| `delete_profile` | `Db::delete_profile(id)` + queue tombstone | `()` |
| `list_peers` | `Db::list_peers()` | `Vec<PeerView>` |
| `pair_peer` | Pairing handshake | `PeerView` |
| `unpair_peer` | Remove peer | `()` |
| `start_sync` | `run_remote_push/pull/bidi` | `()` (progress via events) |
| `get_sync_status` | Last sync metadata per profile | `SyncStatus` |
| `list_pending_deletions` | Profiles with `pending_deletion = 1` | `Vec<ProfileView>` |
| `confirm_deletion` | Hard-delete the profile | `()` |
| `reject_deletion` | Clear `pending_deletion` flag | `()` |

---

## React component tree

```
App
├── TabBar (Profiles | Peers | Activity)
├── ProfilesPage
│   ├── ProfileList
│   │   └── ProfileCard (per profile)
│   └── ProfileEditor (create/edit form)
├── PeersPage
│   ├── PeerList
│   │   └── PeerCard (per peer)
│   └── PairForm (address input)
├── ActivityPage
│   ├── SyncControls (trigger sync per profile)
│   ├── SyncProgress (live progress)
│   └── SyncHistory (last N syncs)
└── Modals
    ├── DeletionPrompt (peer deleted a profile)
    └── ConflictNotice (version conflict resolved)
```

---

## Implementation steps

### Step 1: Tauri command layer

- Add `commands.rs` to `src-tauri/src/` with all commands above
- Wire commands into `tauri::generate_handler![]`
- Add managed state: `Db` handle (wrapped in `Mutex` or `Arc`)
- Add serializable view types (`ProfileView`, `PeerView`, `SyncStatus`)

### Step 2: Frontend foundation

- Install zustand
- Create store with profile/peer/sync slices
- Create `TabBar` component and page shells
- Set up Tauri `invoke` and `listen` wrappers in a `lib/tauri.ts` module

### Step 3: Profiles UI

- `ProfileList` — fetches and displays all profiles
- `ProfileCard` — shows name, mode, peer, last sync time
- `ProfileEditor` — form with fields: name, mode, conflict policy, delete propagation, anchors (local/remote path, max depth, include hidden, ignore patterns)
- Wire create/update/delete to Tauri commands

### Step 4: Peers UI

- `PeerList` — shows paired peers with status (online/offline via last-seen)
- `PairForm` — enter peer address, trigger pairing handshake
- Show pairing confirmation code exchange

### Step 5: Sync & Activity UI

- `SyncControls` — select profile, choose direction, click sync
- `SyncProgress` — subscribe to Tauri events, show file transfer progress
- `SyncHistory` — display last sync results per profile

### Step 6: Conflict & deletion UX

- `DeletionPrompt` modal — poll `list_pending_deletions` on app start and after sync; show prompt with confirm/reject
- `ConflictNotice` — after sync, if a profile version conflict was resolved, show a toast/notice with what changed

### Step 7: Drift reporting

- `ProfileCard` shows: last sync time, files tracked, pending local changes count
- Requires a lightweight `get_drift_summary(profile_id)` Tauri command that diffs the local index against filesystem

---

## Files touched

| File | Change |
|---|---|
| `src-tauri/src/lib.rs` | Wire new commands, add managed state |
| `src-tauri/src/commands.rs` | New: all Tauri commands |
| `src-tauri/src/views.rs` | New: serializable view types |
| `src-tauri/Cargo.toml` | Add serde derives if needed |
| `ui/src/App.tsx` | Replace scaffold with TabBar + pages |
| `ui/src/store.ts` | New: Zustand store |
| `ui/src/lib/tauri.ts` | New: invoke/listen wrappers |
| `ui/src/components/*.tsx` | New: all UI components |
| `ui/src/pages/*.tsx` | New: ProfilesPage, PeersPage, ActivityPage |
| `ui/package.json` | Add zustand dependency |

---

## Done when

- User can create a profile, pair a peer, and trigger a sync from the UI
- Sync progress is displayed in real-time via events
- Profile conflict resolution is surfaced post-sync
- Peer deletion prompts appear and can be confirmed/rejected
- `cargo build` and `npm run build` both succeed
- App launches and all flows work end-to-end in the Tauri window
