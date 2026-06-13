# 0010 — Framing Protocol (Length-Prefixed MessagePack over TLS-TCP)

## Context

M3 needs a wire protocol for file transfer RPCs between paired peers over the TLS connections established in M2. Options considered:

1. **QUIC via `quinn`** — multiplexed streams, built-in TLS, resumable. Adds ~20 crate dependencies and requires UDP, which some corporate LANs restrict.
2. **gRPC/tonic** — mature RPC framework. Heavy dependency tree, code generation step, overkill for our simple request-response pattern.
3. **TLS-over-TCP with length-prefixed framing** — minimal, builds on existing M2 TLS, simple to implement and debug.

## Decision

Use option 3: a custom length-prefixed binary framing protocol over the existing TLS-over-TCP transport.

Frame format: `[u32 len (big-endian)][u8 message_type][payload bytes]`. Max frame 16 MiB. Payload encoded as MessagePack (via `rmp-serde`).

MessagePack over JSON because:
- 2-5x smaller for binary-heavy payloads (file metadata, snapshot trees)
- No base64 needed for binary data
- Schema-flexible like JSON (rmp-serde uses serde, so same derive story)

16 MiB max because files stream as 256 KiB chunks — no single frame needs to hold an entire file.

## Consequences

- **Simple.** ~100 lines for the codec (using `tokio_util::codec`). Easy to debug with a hex dump.
- **Single-stream.** No multiplexing within a connection. Adequate for our serial push/pull flow.
- **Migration path to QUIC** remains open for Phase 2 if we need parallel stream transfers or hole-punching for non-LAN scenarios. The RPC types (`RpcRequest`/`RpcResponse`) are transport-agnostic.
- We carry `rmp-serde`, `tokio-util`, `bytes`, `futures-util` as new dependencies.
