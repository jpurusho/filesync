# 0011 — Sync Session Lifecycle

## Context

When two peers exchange files, the protocol needs structure around:
- Who drives the interaction (initiator vs responder)
- When the sync index is updated (commit point)
- What happens if the connection drops mid-transfer

## Decision

**Initiator-driven sessions.** The peer that triggers a sync run (whether push or pull) drives the entire flow. The responder is a passive RPC server.

Session lifecycle:
1. `StartSession` — negotiate profile, validate anchors
2. `ScanRemote` — initiator requests remote snapshot
3. Initiator runs the full engine pipeline locally (scan, diff, reconcile, plan)
4. Execute plan: `PutFile`/`GetFiles`/`MkdirRemote`/`DeleteRemote`
5. `EndSession` — signals successful completion

**Index commit is at EndSession only.** The sync index (the record of "what's synced") is updated only after the initiator receives a successful `EndSession` acknowledgment. If the session is interrupted at any prior point, the next run starts fresh (idempotent).

**Per-file atomicity.** Each file write (push or pull) uses temp+fsync+rename. An interrupted file leaves no partial content at the destination — only a temp file that gets cleaned up.

**Per-file error tolerance.** A single file failing to transfer (IO error, permission denied) does NOT abort the session. The error is recorded, and the run continues with remaining files (NFR-REL-2).

## Consequences

- **Idempotent retries.** Since index is committed only on success, a crashed-and-restarted run will re-transfer files that already arrived (they get overwritten identically). This is safe but slightly redundant.
- **No server-side state between sessions.** The responder holds no durable state about in-progress syncs — only the TLS connection lifetime matters.
- **Initiator bottleneck.** The initiator holds the full plan in memory and drives serially. For M3 this is fine (simplicity). M4 can pipeline or parallelize actions if needed.
- **Clock-skew is not handled.** Both sides' mtimes are used as-is for conflict resolution. M4 will add clock-skew detection and compensation.
