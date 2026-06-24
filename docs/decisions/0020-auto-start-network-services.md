# ADR-0020: Auto-start network services on app launch

## Context

The app was not starting a pairing listener or mDNS advertisement on launch.
This caused OS error 61 (connection refused) when trying to pair from another
machine, and mDNS discovery didn't find peers. The `pair_peer` command initiates
an *outgoing* connection, but nothing was accepting *incoming* connections.

## Decision

Start both services automatically during Tauri `setup()`:

1. **Pairing listener** on port 5300 (0.0.0.0). After each successful or failed
   pairing, the listener restarts to accept the next connection. Incoming peers
   are auto-confirmed and saved to the database.

2. **mDNS discovery** — advertises `_filesync._tcp.local.` and browses for
   other instances. Discovered peers are surfaced via a new
   `list_discovered_peers` command.

3. **Network info** — a `get_network_info` command returns the local LAN IP,
   port, hostname, and identity fingerprint so the UI can display it.

State is held in `SharedNetworkState` (Arc<RwLock<Option<...>>>) managed by
Tauri. This allows the async initialization to complete without blocking app
startup, while commands gracefully return "not yet initialized" if called too
early.

## Consequences

- Both machines see each other via mDNS within seconds of launch.
- Users can pair by clicking a discovered peer or entering the displayed address.
- Port 5300 must be available; if occupied, the app logs an error but still
  functions (pairing can be initiated outbound).
- `get_if_addrs` crate added for LAN IP detection.
