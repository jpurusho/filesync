# filesync — Project Guide for Claude

## What this is

<!-- One paragraph: what this project does and the stack. Fill in turn 1 with Claude. -->
<TODO: project description>

## Architecture (high level)

<!-- Sketch the major components and how they fit. Fill in once design exists. -->
<TODO: architecture sketch>

## Phasing / milestones

<!-- Numbered slices, each ending with something runnable. Fill in once planned. -->
<TODO: milestones>

## Working style — please follow

### Persistence: write decisions to the repo, not to conversation

When we make any non-obvious design decision, schema choice, protocol change, trade-off, or rejected alternative — **write it to disk, don't just discuss it**. Conversation context is ephemeral and expensive to replay; the repo is durable and free.

- **Spec amendments** (changes to behavior, requirements, or acceptance criteria) → edit the spec file directly, even mid-conversation.
- **Architectural / design decisions** (choices with trade-offs, not requirements) → append a short ADR-style note to `docs/decisions/NNNN-short-slug.md`. Number sequentially. Keep each one to: Context, Decision, Consequences. No more than ~30 lines.
- **Implementation plans** for a milestone → `docs/plans/MX-short-slug.md` so the next session doesn't have to re-derive them.
- **Don't ask before writing these.** If a decision is being made, capture it. Mention briefly that you wrote it (one line, with the path) so the user can review.

This is mandatory, not optional. Re-deriving the same architecture every session is the single biggest waste of tokens on this project.

### Cost efficiency: reduce token usage without sacrificing quality

Token cost scales with conversation length and file re-reads. Follow these practices:

- **Batch work.** For multi-part questions, answer all parts in one turn instead of back-and-forth.
- **Read once, act once.** Don't re-read a file you just edited — Edit/Write fails loudly if the change didn't work.
- **Use memory and docs over conversation.** Check CLAUDE.md, ADRs, and memory before asking "what did we decide about X?"
- **Delegate broad searches.** Use `Explore` subagent for codebase-wide searches instead of multiple greps in the main loop.
- **Be decisive.** When the path forward is clear, act. Don't enumerate options you won't pursue or re-litigate settled decisions.
- **/clear early, /clear often.** After each milestone or substantial change, run `/clear` to reset context. The spec, ADRs, and plans on disk are the durable state.

**Why this matters:** A 50-turn session with repeated file reads can cost $2-5 for work that could fit in 5-10 turns. Discipline saves tokens; tokens save dollars.

### Model selection

- **Opus** for: correctness-critical reasoning, architecture, protocol design, code review of subtle paths.
- For mechanical work (scaffolding, file edits, dependency wiring, CI, README/docs, simple test stubs), suggest the user switch to **Sonnet** with `/model sonnet` before starting that block. Don't switch automatically — surface the suggestion and let the user decide.

### Turn discipline

- **Batch independent asks** when answering a multi-part question.
- For broad codebase searches ("find all places that touch X"), use the `Explore` subagent — keeps the search transcript out of our context window.
- After a milestone is done, suggest the user run `/clear` to reset conversation. The spec + ADRs + plans on disk are the durable context.

### Spec is the source of truth

If conversation memory and the spec disagree, the spec wins. If the user asks for something that contradicts the spec, point it out and ask whether to amend the spec or follow conversation intent.

## Where things live

| What | Where |
|---|---|
| Authoritative requirements | <TODO: path to spec, e.g. `SPEC.md`> |
| Architecture decisions (ADRs) | `docs/decisions/NNNN-*.md` |
| Per-milestone plans | `docs/plans/MX-*.md` |
| Token usage ledger (auto-logged) | `~/.claude/projects/<sanitized-cwd>/memory/token_usage_log.md` |

## Reading order for a fresh session

1. This file (loaded automatically).
2. The spec, if one exists.
3. `docs/decisions/` — newest first; these capture what *isn't* in the spec.
4. `docs/plans/` — only the active milestone's plan.
