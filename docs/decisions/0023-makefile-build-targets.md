# 0023 — Makefile for standardized build workflow

**Status:** accepted  
**Date:** 2026-06-23

## Context

User confusion about whether to run commands from `ui/` or project root. The project uses Tauri, which requires coordination between UI (pnpm/Vite in `ui/`) and Rust backend (cargo in root). Commands like `pnpm tauri dev` fail from `ui/`, `npm` vs `pnpm` mismatch causes cryptic errors, and GitHub Actions workflows duplicated build logic.

No single source of truth for how to build, test, or run the application — different steps in README, CI workflows, and mental model.

## Decision

Introduce a Makefile at project root with standardized targets:
- `make dev` — Run development mode with hot reload
- `make build` — Build optimized release bundle
- `make test` — Run all Rust tests
- `make check` — CI-compatible linting and format checks
- `make install` — Install to /Applications
- `make clean`, `make fmt`, `make lint` — Utilities

GitHub Actions workflows (`ci.yml`, `release.yml`) updated to use `make check`, `make test`, and `make build` instead of direct cargo/pnpm commands.

**All commands now run from project root**, regardless of whether they touch UI or Rust code.

## Consequences

**Positive:**
- Single source of truth for build commands (Makefile)
- No more confusion about working directory
- Same commands work locally and in CI
- Easier onboarding (just run `make dev` or `make help`)
- Consistent build process across environments

**Neutral:**
- Adds Make as a dependency (already present on macOS and CI runners)
- One more file to maintain, but replaces duplicated logic in CI workflows

**Negative:**
- None identified. Make is universally available and the abstraction is thin (targets just call underlying tools).
