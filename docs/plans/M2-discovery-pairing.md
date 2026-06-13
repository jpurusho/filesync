# M2 — mDNS Discovery + TOFU Pairing

**Status:** complete
**Owner model:** Opus (crypto/identity design, TOFU correctness)

## Goal

Implement peer discovery via mDNS/DNS-SD and TOFU pairing with self-signed TLS certificates. After M2, two instances on the same LAN can find each other, verify fingerprints, and establish a pinned trust relationship. No file transfer yet (that's M3).

## Requirements covered

- FR-DP-1: Auto-detect peers on LAN
- FR-DP-2: mDNS/DNS-SD `_filesync._tcp.local` with instance UUID, display name, host, port, protocol version
- FR-DP-3: Manual peer entry by host/IP + port
- FR-DP-5: TOFU pairing — self-signed cert, fingerprint verification, pin identity
- FR-DP-6: Online/offline/last-seen status (data model; UI is M6)
- NFR-SEC-1: TLS 1.3 via rustls
- NFR-SEC-2: Authentication via pinned identity

## Non-goals (M2)

- No file transfer over the wire (M3).
- No profile-scoped folder access control enforcement (M3/M4 — needs transfer protocol).
- No UI for peer list or pairing dialogs (M6) — exercise via integration tests.
- No quick-send (M3.5).

## Architecture

### Identity

Each instance generates a persistent Ed25519 keypair + self-signed X.509 certificate on first launch. Stored in the app's data directory (platform-specific). The certificate's Subject Common Name is the instance UUID.

```
~/.filesync/
├── identity.key    # Ed25519 private key (PEM)
├── identity.cert   # Self-signed X.509 cert (PEM)
└── peers/
    └── <uuid>.cert # Pinned peer certificates
```

The **fingerprint** is SHA-256 of the DER-encoded certificate, displayed as `XX:XX:XX:XX` (first 8 bytes = 16 hex chars, human-readable).

### Discovery (syncnet::discovery)

Uses the `mdns-sd` crate (pure-Rust, async-compatible, macOS/Linux/Windows):

1. **Advertise:** Register `_filesync._tcp.local` with TXT records:
   - `id=<instance-uuid>`
   - `name=<display-name>` (machine hostname by default)
   - `ver=1` (protocol version)
   - `fp=<fingerprint-short>` (first 8 bytes of cert fingerprint, for quick matching)

2. **Browse:** Subscribe to `_filesync._tcp.local`, maintain a `PeerMap`:
   ```rust
   pub struct DiscoveredPeer {
       pub id: Uuid,
       pub name: String,
       pub addrs: Vec<SocketAddr>,
       pub protocol_version: u32,
       pub fingerprint_short: String,
       pub last_seen: Instant,
   }
   ```

3. **Events:** Emit `PeerDiscovered`, `PeerUpdated`, `PeerLost` events via a tokio broadcast channel.

4. **Manual entry:** `add_manual_peer(addr: SocketAddr)` connects directly, skipping mDNS.

### TOFU Pairing (syncnet::pairing)

Pairing is an explicit user-initiated flow:

1. Initiator opens a TLS connection to the target peer (using rustls, accepting any cert since it's the first contact).
2. Both sides exchange their full certificate fingerprints over the encrypted channel.
3. Both UIs display the fingerprint of the OTHER side's certificate and ask the user to confirm it matches (short code, 16 hex chars).
4. On confirmation, each side pins the other's certificate to `~/.filesync/peers/<uuid>.cert`.
5. Subsequent connections use `rustls` with a custom `ServerCertVerifier`/`ClientCertVerifier` that only accepts pinned certificates.

```rust
pub enum PairingState {
    Idle,
    AwaitingConfirmation { peer_fingerprint: String },
    Confirmed { peer_id: Uuid },
    Rejected,
}
```

The pairing protocol is a simple 3-message exchange:
```
Initiator                        Responder
    |                                |
    |--- PairRequest {id, cert} ---->|
    |                                |
    |<-- PairResponse {id, cert} ----|
    |                                |
    |--- PairConfirm / PairReject -->|
    |                                |
    |<-- PairConfirm / PairReject ---|
```

Both sides must confirm independently. If either rejects, pairing fails and no cert is pinned.

### TLS Configuration (syncnet::tls)

- `rustls` with ring backend (Ed25519 certs).
- Server: presents self-signed cert, requires client cert (mutual TLS).
- Client: presents self-signed cert, verifies server cert against pinned store OR accepts-any during pairing flow.
- Protocol: TLS 1.3 only (no fallback).

### Peer Store (syncstore)

New migration 0005:
```sql
CREATE TABLE peers (
    id TEXT PRIMARY KEY NOT NULL,        -- peer's UUID
    name TEXT NOT NULL,
    fingerprint TEXT NOT NULL,           -- full SHA-256 fingerprint
    cert_pem TEXT NOT NULL,             -- pinned certificate (PEM)
    paired_at TEXT NOT NULL,
    last_seen TEXT,
    is_online INTEGER NOT NULL DEFAULT 0
);
```

### Listener

The instance runs a TLS listener on a random high port (or configured port). The port is advertised in the mDNS service record. The listener handles:
- Pairing requests (from unpaired peers)
- Authenticated connections (from paired peers — M3+ will add RPC here)

## New dependencies (workspace)

| Crate | Why |
|---|---|
| `mdns-sd` | mDNS/DNS-SD service discovery (pure Rust) |
| `rustls` | TLS 1.3 implementation |
| `tokio-rustls` | Async TLS wrapper for tokio |
| `rcgen` | Self-signed certificate generation |
| `x509-parser` | Certificate parsing for fingerprint extraction |
| `ring` | Crypto backend for rustls (Ed25519) |
| `pem` | PEM encoding/decoding |
| `hostname` | Get local machine name for display name |

## Execution order

1. **Identity generation** — `syncnet/src/identity.rs`: Generate Ed25519 key + self-signed cert. Load from disk or create on first run. Fingerprint computation.
2. **TLS config** — `syncnet/src/tls.rs`: Build rustls `ServerConfig`/`ClientConfig`. Custom cert verifier that checks pinned store. Pairing-mode verifier that accepts any cert.
3. **Peer store** — `syncstore`: Migration 0005, `peers.rs` repo module (CRUD + pin/unpin).
4. **mDNS discovery** — `syncnet/src/discovery.rs`: Advertise + browse. `PeerMap` + event channel.
5. **Pairing protocol** — `syncnet/src/pairing.rs`: Implement the 3-message exchange. Fingerprint comparison. Cert pinning on success.
6. **Listener** — `syncnet/src/listener.rs`: TLS acceptor that routes pairing vs authenticated connections.
7. **Integration tests** — Two instances in-process, discover each other, complete pairing, verify pinned state.

## ADRs to write during execution

- **0008 — Identity model (Ed25519 + self-signed X.509)**: Why self-signed, why Ed25519, why not pre-shared key or password-based.
- **0009 — Pairing protocol design**: Why 3-message, why mutual confirmation, why fingerprint-based (not numeric code).

## Done when

- Two in-process instances discover each other via mDNS within 3 seconds in tests.
- Pairing completes successfully and both sides pin the other's cert.
- A paired instance rejects connections from an unpaired instance.
- `cargo test --workspace` passes.
- `cargo clippy --workspace --all-targets` clean.
- The discovery/pairing code has zero dependencies on synccore or the UI.
