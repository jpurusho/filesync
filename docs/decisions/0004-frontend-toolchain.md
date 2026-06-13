# 0004 — Frontend toolchain: Tauri 2, Vite, React, pnpm, Tailwind

**Status:** accepted
**Date:** 2026-06-12

## Context

The spec chose Tauri 2 + React/TypeScript for the UI (§10). Need to lock the specific frontend toolchain now so M6 (UI milestone) doesn't re-litigate build tools.

## Decision

| Layer | Choice | Why |
|---|---|---|
| Shell | **Tauri 2** | Native WKWebView on macOS, tiny binaries (~5 MB vs Electron's ~150 MB), first-class Rust backend, built-in code-signing/notarization support |
| Build tool | **Vite 6** | Fast HMR, native ESM, lightweight, standard for modern React |
| UI framework | **React 18 + TypeScript** | Mature ecosystem, matches user preference from spec |
| Package manager | **pnpm** | Fast, disk-efficient, strict dependency resolution |
| Styling | **Tailwind CSS 3** | Utility-first, fast iteration, consistent design tokens; avoids CSS-in-JS runtime cost |
| State | *(defer to M6)* | Likely Zustand (local UI state) + TanStack Query (Tauri command cache). Not deciding now since M0–M5 don't touch the UI. |

**Why Tauri over Electron:**
- Binary size: Tauri reuses the OS webview; Electron bundles Chromium.
- Rust integration: Tauri commands are Rust functions; Electron needs IPC boilerplate.
- Notarization: Tauri's macOS tooling is first-class; Electron's is workable but heavier.

**Why pnpm:**
- Faster installs than npm/yarn (shared content-addressable store).
- Strict mode catches phantom dependencies (importing something not in `package.json`).
- Lockfile is human-readable YAML.

**Trade-offs:**
- Tauri's webview quirks: occasionally Safari/WebKit differences bite. Acceptable for a macOS-only MVP; we're not fighting cross-browser bugs.
- React vs Svelte/Solid: React is heavier, but the team knows it and the bundle size difference (~40 KB gzipped) is negligible for a desktop app.

## Consequences

- `ui/` directory: Vite config, `package.json` with `pnpm` as `packageManager` field, `.nvmrc` pinning Node 20.
- Tailwind configured at M0 (even though real UI lands in M6) so the skeleton compiles.
- CI runs `pnpm typecheck`, `pnpm lint`, `pnpm build` in the `ui/` subdirectory.
- No state management library installed yet — that's an M6 decision once we see the data-flow patterns.
