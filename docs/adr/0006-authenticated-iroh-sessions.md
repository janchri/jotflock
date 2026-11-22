# ADR 0006 — Authenticated iroh sessions

Status: Implemented.

## Decision

Use iroh QUIC/TLS connections authenticated by persistent endpoint keys.
Signed, device-bound delegation chains authorize board access separately from
canvas content. Verify board ID, authority root and the remote device before
sending board state.

For example, an admitted editor can authorize another device after the founder
leaves. A copied board file alone does not grant membership.

## Replication and connectivity

A background Tokio worker owns a session replica. The UI and worker exchange
snapshots and merge concurrent changes. Peers exchange incremental Automerge sync
messages, validated in a candidate document before publication. Received changes
are saved locally, with retry handling for persistence failures.

Default discovery and relay services help establish direct connections or an
encrypted relay path. `JOTFLOCK_RELAY_URL` selects a relay.
`JOTFLOCK_RELAY_ONLY=1` disables direct IP transport. A custom relay alone
does not disable public discovery. Presence expires after three seconds.

Only one board is offered per instance. Switching or closing it stops that
instance's session. Invitation availability follows ADR 0004.

## Trust and resource limits

- Invitations last 24 hours. Forwarded links admit editors with equal invitation
  authority. Revocation, member removal and roles are pending.
- Keys and membership live under `network/`, outside board documents.
  Unix restricts the network directory and private files. Local files are
  unencrypted. Back up the whole data directory to retain membership.
- Sharing caps are 16 participants, 32 incoming handlers, 32 grants per chain,
  128 KiB links, 8 MiB wire frames, 4 MiB serialized board history, 10,000 live
  cards and world coordinates within ±1,000,000.
- Compressed CRDT data can allocate before post-apply limits run. Direct peers
  can learn network addresses. Discovery and relay services see metadata.
- A local save or transport acknowledgement does not guarantee every peer has
  saved the latest edits.

Endpoint tests cover admission, delegation, reconnect, presence and rejection.
An optional relay test uses public infrastructure. Separate physical networks,
platform behavior and hostile-input resistance still need release validation.

Code — [network.rs](../../src/network.rs), [sharing.rs](../../src/sharing.rs).
