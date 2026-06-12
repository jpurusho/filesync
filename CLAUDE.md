# FileSync — Project Guide for Claude

## What this is

Peer-to-peer file sync app for macOS, syncing two computers on a trusted LAN. Tauri 2 + React/TypeScript front end over a Rust core. The authoritative spec is `FileSync_Requirements_Spec.md` at the repo root — read that first when you need ground truth on behavior.

## Architecture (high level)

Cargo workspace, four crates plus the shell:

```
filesync/
├─ crates/
│   ├─ synccore/      # pure engine: scan → diff → plan → apply, conflict resolver
│   ├─ syncnet/       # transport: TLS 1.3 + framed RPC, mDNS discovery
│   ├─ syncstore/     # SQLite schema + migrations, repos for profiles/index/runs
│   └─ syncplatform/  # macOS specifics: NFD normalization, security-scoped bookmarks, FSEvents
├─ src-tauri/         # thin Tauri 2 shell, command handlers, event bus
└─ ui/                # React + TS + Vite + Tailwind
```

Engine pipeline (per profile run):
`Scan(local) + Scan(peer) → Diff(vs index) → Reconcile → Plan → Apply → Commit(index)`

`Diff` is a first-class output, not just an internal step. `DriftReport` (read-only, no peer write) is the same code path stopped at `Diff` — it powers the health view (FR-UI-7) and dry-run preview (FR-UI-5).

`synccore` has **zero Tauri/UI/network deps**. It's a pure state machine over filesystem snapshots + sync index. This makes it unit-testable in isolation and reusable for a future CLI/daemon.

## Phasing (MVP, 7 slices)

- **M0** — workspace scaffolding, Tauri shell, SQLite migrations baseline, CI lint+test
- **M1** — local engine kernel (no networking; sync `/tmp/A` ↔ `/tmp/B` in-process). Conflict logic, NFD normalization, atomic writes, run records all land here.
- **M2** — mDNS discovery + TOFU pairing (no transfer yet)
- **M3** — networked Push/Pull over TLS-over-TCP framed protocol
- **M3.5** — quick-send (FR-SM-6): profile-less one-shot transfer, reuses M3 transport
- **M4** — bidirectional reconciliation over the wire, clock-skew handling
- **M5** — profile replication (UUID-strict; no name-fallback matching)
- **M6** — UI: profile CRUD, drag-drop, peer list, health view, conflict surfacing, security-scoped bookmarks
- **M7** — hardening, notarization, dogfooding, acceptance pass against §8

## Working style — please follow

### Persistence: write decisions to the repo, not to conversation

When we make any non-obvious design decision, schema choice, protocol change, trade-off, or rejected alternative — **write it to disk, don't just discuss it**. Conversation context is ephemeral and expensive to replay; the repo is durable and free.

- **Spec amendments** (changes to behavior, requirements, or acceptance criteria) → edit `FileSync_Requirements_Spec.md` directly, even mid-conversation.
- **Architectural / design decisions** (choices with trade-offs, not requirements) → append a short ADR-style note to `docs/decisions/NNNN-short-slug.md`. Number sequentially. Keep each one to: Context, Decision, Consequences. No more than ~30 lines.
- **Implementation plans** for a milestone → `docs/plans/MX-short-slug.md` so the next session doesn't have to re-derive them.
- **Don't ask before writing these.** If a decision is being made, capture it. Mention briefly that you wrote it (one line, with the path) so the user can review.

This is mandatory, not optional. Re-deriving the same architecture every session is the single biggest waste of tokens on this project.

### Model selection

- **Opus** stays selected for: reconciler logic, conflict-policy reasoning, protocol design, code review of correctness-critical paths.
- For mechanical work (scaffolding, file edits, dependency wiring, CI, README/docs, simple test stubs), suggest the user switch to **Sonnet** with `/model sonnet` before starting that block. Don't switch automatically — surface the suggestion and let the user decide.

### Turn discipline

- **Batch independent asks** when answering a multi-part question. Don't split into multiple turns unless one genuinely depends on the other.
- For broad codebase searches ("find all places that touch X"), use the `Explore` subagent — keeps the search transcript out of our context window.
- After a milestone is done, suggest the user run `/clear` to reset conversation. The spec + ADRs + plans on disk are the durable context.

### Spec is the source of truth

If conversation memory and the spec disagree, the spec wins. If the user asks for something that contradicts the spec, point it out and ask whether to amend the spec or follow conversation intent.

### Code style (when implementation begins)

- Rust 2024 edition, `rustfmt` + `clippy::pedantic` (selectively allow noisy lints).
- Errors: `thiserror` for library crates, `anyhow` only at the binary boundary.
- Async: `tokio` runtime, single shared runtime across crates.
- Tests: `cargo test` for unit + integration; `proptest` for the reconciler (property tests are mandatory there — see `docs/decisions/` once that ADR exists).
- No comments explaining *what* code does. Only *why* when it's non-obvious.
- No backwards-compat shims, no premature abstractions, no half-finished implementations.

### Commits

- Conventional, short subjects. Co-authored trailer on Claude-authored commits.
- Don't commit unless asked.
- Never commit `.claude/settings.local.json` or `.claude/log_token_usage.py` (already gitignored — token-logging hook).

## Where things live

| What | Where |
|---|---|
| Authoritative requirements | `FileSync_Requirements_Spec.md` |
| Architecture decisions (ADRs) | `docs/decisions/NNNN-*.md` |
| Per-milestone plans | `docs/plans/MX-*.md` |
| Token usage ledger (auto-logged) | `~/.claude/projects/-Users-jpurshot-experimental-filesync/memory/token_usage_log.md` |

## Reading order for a fresh session

1. This file (loaded automatically).
2. `FileSync_Requirements_Spec.md` — skim §3 (functional reqs), §6 (data model), §7 (decisions), §10 (stack).
3. `docs/decisions/` — newest first; these capture what *isn't* in the spec.
4. `docs/plans/` — only the active milestone's plan.

That's enough to start a turn productively without re-deriving anything.
