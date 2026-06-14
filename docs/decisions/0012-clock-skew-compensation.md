# 0012 — Clock Skew Compensation Strategy

## Context

In bidirectional sync, conflict resolution uses "newer-wins" by comparing modification times from two different machines. Cross-machine mtimes are not directly comparable without knowing the clock offset between them. A 60-second skew is common on a LAN; without compensation, the wrong file wins or identical files generate spurious conflicts.

## Decision

Exchange clock offset during session startup. `StartSession` carries `initiator_unix_secs`; the responder replies with `SessionStarted { clock_offset_secs }` (responder_time − initiator_time). The initiator adjusts remote mtimes by subtracting the offset before comparing: `adjusted_remote = remote_mtime − clock_offset`. A 5-second skew tolerance is applied: if `|local − adjusted_remote| ≤ 5s`, the timestamps are considered too close to order reliably, and we tie-break to local. Hash equality is checked *before* timestamp comparison (`content_is_same`), so genuinely-identical files are never copied regardless of skew.

Why offset-on-start-session and not NTP: NTP requires an external server and adds a round-trip; piggybacking on the already-required session handshake is simpler and good enough for a LAN where latency is negligible.

## Consequences

- Clock skew up to ~5 seconds has no effect on sync correctness.
- Skew > 5s is handled by the offset, so practically unlimited skew is tolerated.
- The offset is computed once per session and reused for all paths in that session.
- For push/pull (unidirectional), `clock_offset_secs = 0` is passed — skew compensation is a no-op and existing behavior is unchanged.
