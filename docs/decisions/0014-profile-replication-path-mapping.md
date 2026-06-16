# 14. Profile Replication Path Mapping

Date: 2026-06-16

## Context

When a profile is replicated from instance A to instance B, the anchor paths must be swapped: A's local_path becomes B's remote_path and vice versa. The wire format must be unambiguous about which path belongs to which side so that round-trips work correctly (A sends to B, B sends back to A without information loss).

## Decision

The wire format uses neutral field names — `side_a_path` and `side_b_path` — plus an `origin_instance_id` that identifies which instance is "side A." Each receiver maps paths based on whether it is the origin instance:

- If I am `origin_instance_id`: `local_path = side_a_path, remote_path = side_b_path`
- If I am the peer: `local_path = side_b_path, remote_path = side_a_path`

When re-serializing back to wire, a non-origin instance reconstructs the original orientation by reversing the mapping.

## Consequences

- Round-trips are lossless: A→B→A preserves the original paths.
- No "direction" enum or flags needed — the origin_instance_id is the sole disambiguation key.
- If a profile is created on B (B is origin), the same logic works symmetrically.
- Adding a third node (Phase 3) would require extending this to a per-pair path mapping, but the two-node constraint makes this simple model correct.
