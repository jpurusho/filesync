# 0009 — Pairing protocol: mutual TLS + fingerprint confirmation

**Status:** accepted
**Date:** 2026-06-13

## Context

FR-DP-5 requires a pairing handshake before first sync. The pairing must establish mutual trust without a pre-existing shared secret. Options considered:

1. **Numeric code (like Bluetooth):** Both sides display a short number, user confirms match. Vulnerable to display spoofing if attacker controls one UI.
2. **QR code scan:** Good UX but requires camera/screen interaction. Overkill for desktop.
3. **Certificate fingerprint comparison:** Both sides show the other's cert fingerprint. User verifies out-of-band (visually or verbally). Standard TOFU pattern.

## Decision

Three-message protocol over a pairing-mode TLS connection:

```
Initiator                     Responder
    |                             |
    |--- Request {id, cert} ----->|
    |                             |
    |<-- Response {id, cert} -----|
    |                             |
    |--- Confirm / Reject ------->|
    |                             |
    |<-- Confirm / Reject --------|
```

Key properties:
- **Mutual confirmation required.** Both sides must independently confirm. If either rejects, no cert is pinned.
- **TLS protects the exchange.** Even in pairing mode (accept-any-cert), the channel is encrypted — an eavesdropper can't extract certificates.
- **Fingerprint-based, not numeric code.** 16 hex characters (8 bytes) gives 2^64 entropy. Short enough to read aloud, long enough to prevent brute-force.
- **Length-prefixed JSON wire format.** Simple, debuggable, extensible. Performance irrelevant (pairing happens once per peer pair).
- **Pairing mode vs authenticated mode.** The same TLS listener handles both. Pairing mode accepts any cert; authenticated mode uses a `PinnedCertVerifier` that rejects unknown certs.

## Consequences

- A MITM during pairing would present their own cert — the fingerprints would mismatch. The user must verify the fingerprint to detect this.
- If the user blindly confirms without checking, MITM is possible. This is inherent to TOFU; acceptable for a trusted-LAN tool.
- The protocol is stateless on the wire — no session tokens, no replay risk. Each pairing is a fresh TLS connection.
- Future: could add a "pairing PIN" mode (4-digit code derived from both fingerprints) for users who don't want to compare hex strings. Requires no protocol change — just a different UI representation of the same data.
