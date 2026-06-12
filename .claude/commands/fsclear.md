---
description: Capture any unpersisted decisions to disk (ADRs / plans / spec amendments), then prompt the user to /clear.
allowed-tools: Read, Write, Edit, Bash, Glob, Grep
---

You are at a session-clearing checkpoint for the FileSync project. Per `CLAUDE.md`, durable context lives in `FileSync_Requirements_Spec.md`, `docs/decisions/NNNN-*.md`, and `docs/plans/MX-*.md` — not in the conversation. Before the user runs `/clear`, make sure anything decided this session is written to disk.

Do this, in order:

1. **Scan this conversation** for anything that should persist:
   - **Spec amendments** — changes to behavior, requirements, or acceptance criteria → edit `FileSync_Requirements_Spec.md` directly.
   - **Architectural / design decisions** with trade-offs → new file at `docs/decisions/NNNN-short-slug.md` (next sequential number; format: Context / Decision / Consequences; ~30 lines max).
   - **Implementation plans** for a milestone → `docs/plans/MX-short-slug.md`.
   - Skip anything already captured (re-read the existing files first to check). Skip ephemeral chatter — code we wrote, bugs we fixed, files we read.

2. **Write the files.** Don't ask permission — `CLAUDE.md` already mandates capture-without-asking. If nothing qualifies, that's a valid outcome; say so.

3. **Report concisely** — one line per file written (path + one-sentence summary), or "Nothing new to persist" if the session was just exploration / mechanical edits.

4. **Tell the user to run `/clear`** as the final line of your response. (You can't trigger it from here — it's a CLI built-in.)

Be terse. This is a checkpoint, not a retrospective.
