# ADR 0004 — Live-session availability

Status: Implemented.

## Decision

Use live peer sessions with local board copies. A new editor needs a reachable,
authorized participant listed in the invitation and offering the board.

For example, admitted editors can keep working after the creator leaves.
When everyone disconnects, they can edit their saved copies offline. Changes
merge when authorized replicas reconnect.

## Invitations

Every admitted editor may invite and admit others. Anyone receiving a forwarded
invitation can join. Invitations expire after 24 hours, using the device's UTC
clock. Existing membership survives invitation expiry.

An old invitation can become unusable when its listed peers are unreachable.
An online editor can issue a fresh link. Authentication is covered by ADR 0006.

## Limits

Discovery and relays establish connections and forward encrypted traffic.
Boards are stored on participants' devices. There is no always-on board store.
An online participant may still lack another editor's offline changes.
Early invitation cancellation, member removal and key rotation are pending.

Code — [sharing.rs](../../src/sharing.rs), [network.rs](../../src/network.rs).
