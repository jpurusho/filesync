# 17. Tauri UI Architecture

Date: 2026-06-16

## Status

Accepted

## Context

M6 requires a desktop UI for profile management, peer pairing, and sync operations. We need to choose:
- State management approach
- Routing strategy
- Command/event communication pattern

## Decision

### State management: Zustand

Lightweight state library with minimal boilerplate. No context providers or complex wiring. Works well with Tauri's async invoke pattern.

Alternative considered: React Context. Rejected: too verbose for this scale.

### Routing: single-page tabs

No react-router needed. A simple TabBar component with conditional rendering keeps the bundle small and avoids routing complexity.

Why not react-router: The app has three top-level views (Profiles, Peers, Activity) with no deep nesting or URL sharing requirements. Tab-based navigation is simpler.

### Tauri command surface

Each backend operation exposed as one command. Commands return `Result<T, String>` for errors. Serialization via serde.

For sync progress: event-driven updates via Tauri events (`sync-progress`, `sync-complete`) rather than polling. Deferred to later implementation.

## Consequences

### Positive
- Small bundle size (no router dependency)
- Minimal state management boilerplate
- Clear 1:1 mapping between UI actions and backend commands

### Negative
- No URL-based navigation (can't bookmark specific profiles)
- Tab state resets on refresh (acceptable for desktop app)

## How to apply

When adding new UI features:
- Add view types to `src-tauri/src/views.rs`
- Add command to `src-tauri/src/commands.rs`
- Wire command in `lib.rs` handler
- Add TypeScript types and wrapper to `ui/src/lib/tauri.ts`
- Add store slice to `ui/src/store.ts` if persistent state is needed
