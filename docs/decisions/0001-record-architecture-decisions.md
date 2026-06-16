# 0001 — Record architecture decisions in this directory

**Status:** accepted
**Date:** 2026-06-16

## Context

Conversation context is ephemeral and expensive to replay every session. Re-deriving architectural choices each time is the largest avoidable token cost on a project, and it risks drift if the user and Claude reach different conclusions in different sessions.

## Decision

All non-obvious design decisions, schema choices, protocol changes, trade-offs, and explicitly-rejected alternatives are captured here as short ADRs. CLAUDE.md instructs Claude to write them automatically as decisions are made, without being asked each time.

Format: numbered sequentially, kebab-case slug, three sections (Context, Decision, Consequences). Aim for ~30 lines or fewer.

Spec-level changes (requirements, behavior, acceptance criteria) go in the spec file. ADRs are for the *how*, the spec is for the *what*.

## Consequences

- A future session can reconstruct the design by reading the spec + this directory; conversation history becomes optional.
- Contradictions between an ADR and the spec must be flagged and resolved by amending one or the other — not glossed over.
- ADRs are append-only by convention. To revise a decision, write a new ADR that supersedes it and reference both numbers.
