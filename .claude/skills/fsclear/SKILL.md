---
name: fsclear
description: Capture any unpersisted decisions to disk, show cost summary, then prompt the user to /clear
trigger: manual
---

# Instructions

When this skill is invoked:

1. Review the current conversation for any architectural decisions, design choices, or important context that was discussed but not yet persisted
2. For each unpersisted decision:
   - Create or update an ADR (Architecture Decision Record) in `docs/decisions/`
   - Follow the project's ADR format (check `docs/decisions/0001-record-architecture-decisions.md`)
3. Show token cost summary:
   - Run: `~/bin/claude-token-cost.py --summary`
   - This shows:
     * **CURRENT SESSION** (since last /clear): turns, tokens, cache, cost
     * **CUMULATIVE** (all sessions to date): total turns, tokens, cost
     * Top models used
   - If the script doesn't exist at ~/bin/claude-token-cost.py, skip this step
4. After showing the cost summary, prompt the user: "Ready to /clear?"

## What to capture
- Design decisions ("we chose X over Y because...")
- Architecture changes
- Important constraints or requirements discovered during implementation
- Patterns or conventions established in conversation

## What NOT to capture
- Completed tasks (already in git history)
- Temporary debugging context
- Implementation details already in code