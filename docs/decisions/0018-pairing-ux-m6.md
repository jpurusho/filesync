# 0018 — M6 pairing UX: auto-confirm with post-hoc fingerprint display

**Status:** accepted (M6 only, may refine later)
**Date:** 2026-06-17

## Context

The pairing protocol (ADR-0009) uses a three-message handshake where both peers must confirm before certificates are pinned. Ideally, the UI would pause mid-handshake, show the peer's fingerprint, and wait for user confirmation before sending the Confirm message.

However, implementing a **true async confirmation flow in Tauri requires**:
- Holding the TLS stream state between two separate command invocations
- Complex state management (start pairing → return fingerprint → user confirms → resume handshake)
- Risk of connection timeout if user takes too long

For M6 MVP, this complexity outweighs the security benefit (trusted LAN assumption).

## Decision

**M6 implementation: auto-confirm the pairing** and display the peer fingerprint **after** the handshake completes.

Flow:
1. User enters peer address and clicks "Pair"
2. Frontend calls `pair_peer(address)` command
3. Backend performs full handshake with `confirm_fn = |_| true` (auto-accept)
4. Backend stores peer in database and returns `PairingConfirmation` with fingerprint
5. Frontend shows success modal with peer name, ID, and fingerprint
6. User can verify fingerprint out-of-band (verbally or visually with the peer)

If fingerprint doesn't match (MITM detected), user manually removes the peer via "Remove" button.

## Consequences

**Security trade-off:**
- A MITM during pairing would be accepted automatically, relying on post-hoc verification.
- Acceptable for M6's trusted-LAN threat model.
- User must actually check the fingerprint (documentation/warning in UI will emphasize this).

**UX benefits:**
- Simple implementation, no stream state management
- No risk of timeout during user confirmation
- Immediate feedback (pairing completes in <1s)

**Future enhancement path:**
- Add a "Verify before accepting" toggle in settings
- Implement two-step pairing with TLS stream held in managed state (e.g., using channels or a pairing session manager)
- Show fingerprint comparison **before** sending Confirm message

## Why this is acceptable for M6

M6 targets desktop users on trusted LANs where:
- MITM attacks are unlikely (home/office networks, not public Wi-Fi)
- Post-hoc verification (verbal confirmation or visual check) is feasible
- The alternative (complex async state management) would delay M6 without proportional security gain

For untrusted networks or production Phase 2, revisit this decision.

## How to apply

- Document the fingerprint verification step in onboarding/help UI
- Add a warning in the pairing success modal: "Verify fingerprint with peer to detect MITM"
- Track as technical debt: "P2: Pre-confirm pairing UX" for later refinement
