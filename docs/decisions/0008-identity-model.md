# 0008 — Identity model: Ed25519 + self-signed X.509

**Status:** accepted
**Date:** 2026-06-13

## Context

Each FileSync instance needs a stable identity for peer authentication (NFR-SEC-2). Options considered:

1. **Pre-shared key / password-based:** Requires out-of-band key exchange for every new peer. Doesn't scale.
2. **CA-signed certificates:** Requires running or trusting a CA. Overkill for a peer-to-peer app.
3. **Self-signed X.509 + TOFU:** Each instance generates its own cert. Identity is established on first contact and pinned. No CA needed, no passwords, no pre-shared secrets.

Ed25519 was chosen over RSA/ECDSA for key generation because: (a) fast key generation and signing, (b) small keys (32 bytes), (c) deterministic signatures (no nonce issues), (d) widely supported in rustls/ring.

## Decision

- Each instance generates an Ed25519 keypair + self-signed X.509 certificate on first launch.
- The certificate's SAN contains the instance UUID.
- Identity persists across launches (`~/.filesync/identity.key` + `identity.cert`).
- Fingerprint = SHA-256 of DER-encoded certificate, displayed as hex with colons.
- Short fingerprint (first 8 bytes) used in mDNS TXT records and quick-match UI.
- Peer certificates are pinned to `~/.filesync/peers/<uuid>.cert` after successful pairing.

## Consequences

- No PKI infrastructure needed. Works offline, works on any LAN.
- If an instance loses its key, it must re-pair with all peers (new identity = new fingerprint).
- A compromised key allows impersonation until the victim unpins the old cert. Acceptable for a trusted-LAN tool.
- The X.509 format allows future integration with standard TLS tooling if needed.
