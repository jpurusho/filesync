# M0 — Scaffolding

**Status:** ready to execute
**Owner model:** mechanical work — execute on Sonnet

## Goal

Stand up an empty-but-complete project skeleton so M1 can land engine code without yak-shaving build infra. After M0, `cargo test`, `cargo clippy`, `cargo fmt --check`, and `pnpm build` all pass on a fresh checkout, and CI runs them on every push.

## Non-goals

- No engine logic. `synccore` exposes only placeholder types.
- No networking. `syncnet` is an empty crate with `pub fn _placeholder()`.
- No real Tauri commands. The shell launches an empty React page; one stub command proves the bridge works.
- No real migrations beyond a baseline `0001_init.sql` that creates the meta table.
- No notarization, signing, or release tooling — that lives in M7.

## Deliverables

### 1. Cargo workspace

Top-level `Cargo.toml` declares a workspace with members:

```
crates/synccore
crates/syncnet
crates/syncstore
crates/syncplatform
src-tauri
```

Each crate:
- Rust 2024 edition.
- `[lints]` table inheriting from workspace; workspace `Cargo.toml` enables `clippy::pedantic` with these allows: `module_name_repetitions`, `missing_errors_doc`, `missing_panics_doc`.
- `thiserror` for the four library crates; `anyhow` only in `src-tauri`.
- `tokio` is a workspace dependency (`features = ["full"]`) but only `syncnet` and `src-tauri` enable it for now.

Workspace dependencies pinned in the root `Cargo.toml` (so individual crates use `dep.workspace = true`):

| Crate | Why |
|---|---|
| `tokio` 1.x | shared runtime |
| `serde` + `serde_json` | profile + index serialization |
| `thiserror` 1.x | library errors |
| `anyhow` 1.x | binary boundary |
| `tracing` + `tracing-subscriber` | structured logs (NFR-OBS-1) |
| `uuid` 1.x (with `v4`, `serde`) | profile/instance IDs |
| `blake3` 1.x | hashing (decision in §10) — declared now, used in M1 |
| `rusqlite` 0.31+ with `bundled` | SQLite driver — chose `rusqlite` over `sqlx` for simpler sync API in scan/apply paths and no async pool overhead for a single-writer DB. Note this in an ADR. |
| `rusqlite_migration` | migrations |

### 2. Crate skeletons

- **`synccore`** — `lib.rs` with empty `pub mod scan; pub mod diff; pub mod plan; pub mod apply; pub mod reconcile;` (each module just a `// TODO M1` placeholder). One trivial unit test per module to prove the test harness wires up.
- **`syncnet`** — `lib.rs` with `pub mod transport; pub mod discovery;` placeholders.
- **`syncstore`** — `lib.rs` with `pub mod migrations;` and a `Db` struct that opens a SQLite connection in WAL mode and runs migrations. Embed `migrations/0001_init.sql` via `include_str!`. Schema for `0001_init.sql`: a single `meta` table with `(key TEXT PRIMARY KEY, value TEXT NOT NULL)` plus an inserted row `('schema_version', '1')`. Actual profile/index/run tables come in later milestones.
- **`syncplatform`** — `lib.rs` with `#[cfg(target_os = "macos")] pub mod macos;` exposing one stub: `pub fn nfd_normalize(s: &str) -> String`. Unit test asserts it round-trips ASCII unchanged.

### 3. Tauri 2 shell (`src-tauri`)

- Initialize via `cargo tauri init` equivalent (hand-written so we don't pull `create-tauri-app`).
- `tauri.conf.json` minimal: app name "FileSync", window 1100×720, dev URL `http://localhost:5173`, frontendDist `../ui/dist`.
- One command: `#[tauri::command] fn ping() -> &'static str { "pong" }` registered in `lib.rs`. Smoke proof for the IPC bridge.
- Depends on `synccore`, `syncstore`, `syncplatform` via path. Does NOT depend on `syncnet` yet.

### 4. UI (`ui/`)

- Vite + React 18 + TypeScript template.
- Tailwind v3 configured (`tailwind.config.js`, `postcss.config.js`, `index.css` with the three `@tailwind` directives).
- `package.json` scripts: `dev`, `build`, `lint` (eslint), `typecheck` (`tsc --noEmit`).
- One page: a button that calls `invoke('ping')` and displays the response. That's the entire UI for M0.
- Pin Node tooling: `.nvmrc` with `20`. Use `pnpm` (not npm) — add `packageManager` field to `package.json`.

### 5. CI

GitHub Actions workflow at `.github/workflows/ci.yml`. Single job, macOS runner (`macos-14`):

```
steps:
  - checkout
  - setup-rust (stable, with rustfmt + clippy components)
  - setup-node (20) + pnpm
  - cargo fmt --all -- --check
  - cargo clippy --workspace --all-targets -- -D warnings
  - cargo test --workspace
  - pnpm install --frozen-lockfile (in ui/)
  - pnpm -C ui typecheck
  - pnpm -C ui lint
  - pnpm -C ui build
```

Caching: `Swatinem/rust-cache@v2` for cargo, `actions/cache` for pnpm store.

No Tauri build in CI for M0 — `tauri build` is heavy and not needed until M6/M7. We only verify the Rust + UI halves compile.

### 6. Tooling files

- `rustfmt.toml` — `edition = "2024"`, `imports_granularity = "Crate"`, `group_imports = "StdExternalCrate"`.
- `clippy.toml` — empty for now; lints driven by workspace `[lints]`.
- `.editorconfig` — 2-space indent for TS/JSON/YAML, 4-space for Rust, LF line endings, trim trailing whitespace.
- `.gitignore` — already exists; verify it covers `target/`, `node_modules/`, `ui/dist/`, `src-tauri/target/`, and reaffirm the existing `.claude/settings.local.json` + `.claude/log_token_usage.py` ignores.

## Execution order

1. Workspace `Cargo.toml` + four library crates with placeholders + first `cargo test` green.
2. `syncstore` migration baseline + `Db::open` + a unit test that opens an in-memory DB and confirms `schema_version = 1`.
3. `src-tauri` skeleton + `ping` command + `cargo check` green.
4. `ui/` Vite scaffold + Tailwind + `ping` button.
5. Manual smoke: `pnpm -C ui dev` in one terminal, `cargo tauri dev` in another, click button, see "pong".
6. Tooling files + `.github/workflows/ci.yml`.
7. Single commit per logical step (workspace, store, shell, UI, CI). All Co-authored-by Claude.

## Decisions to capture as ADRs during execution

Write each as its own `docs/decisions/NNNN-*.md` when the choice is made — don't batch:

- **0002 — `rusqlite` over `sqlx`.** Sync API fits scan/apply hot paths; no async pool needed for single-writer per-profile DB.
- **0003 — Workspace + crate boundaries.** Why four crates not one; what `synccore` may not depend on (Tauri, tokio runtime entry, anything OS-specific).
- **0004 — Tauri 2 + Vite + pnpm + Tailwind.** Lock the front-end toolchain early so M6 isn't a re-litigation.

Transport choice (QUIC vs TLS/TCP) is **not** an M0 decision — defer to the M3 plan.

## Done when

- `cargo test --workspace` passes locally and in CI.
- `cargo clippy --workspace --all-targets -- -D warnings` passes.
- `pnpm -C ui build` succeeds.
- Running the Tauri dev shell shows a window with a working `ping` button.
- ADRs 0002–0004 exist on disk.
- A commit history exists that a reviewer can step through to understand the skeleton.
