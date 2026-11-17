# ADR 0006: Authenticated iroh sessions

Status: Implemented and verified within the limits below.

## Context and accepted policy

Implement ADR 0004: live sessions, every admitted editor may invite, 24-hour join
invitations, and persistent access after admission. There is no central board store.
Use maintained transport and signature primitives rather than inventing encryption.

## Identity, authorization and invitations

Use iroh 1.4 QUIC connections, with TLS authenticated by persistent endpoint keys.
The device key is stored separately from canvas documents in `network/device.key`;
Unix permissions are 0600, and the containing directory is 0700. Files are not
encrypted at rest and are not currently integrated with a platform credential store.

The first sharing device establishes a board's authority root. Delegation is an
Ed25519-signed chain binding board ID, issuer and admitted device ID. Each grant
is verified against the preceding admitted device, starting from the pinned root.
A member may sign the next grant; the founder need not be online. Chains are bounded
to 32 grants and reject cycles. Membership is not inferred from the board document.
A copied board file or another device's membership proof does not authorize access.

Invitations are domain-separated signed payloads containing version, board/root IDs,
issuer proof, random nonce, issuance/expiry timestamps and authenticated peer seeds.
A custom-scheme link base64url-encodes the payload and signature. Possession grants
admission; anyone receiving a forwarded link can join. TTL is exactly 86,400 seconds;
check the signed timestamps against the admitting device's UTC clock, with no expiry
grace. Correct device clocks are required. Expiration prevents new joins; saved
membership proofs have no invitation expiry. Early cancellation, membership removal,
key rotation and roles are not implemented and must not be implied by Stop sharing.

A receiver pins root and board identity before importing data. An invitation cannot
merge into a preexisting local board without matching saved authorization. Hello
binds credentials and advertised endpoint ID to the TLS-authenticated remote device.
Validate admission before sending any board state. An admission already in progress
is rejected if the local session is stopped before authorization completes. Authenticated peer rosters carry
verifiable membership and endpoint-address information, and are cached for reconnect.

## Replication and lifecycle

A background Tokio actor owns a per-session replica and iroh endpoint. The UI owns
its local document. Bounded channels exchange document snapshots; both sides merge
rather than overwrite, preserving concurrent local edits. Apply remote sync messages
to a candidate, then validate board identity, geometry and limits before publishing.
The UI saves received changes and retries notification when disk persistence fails.
Membership/address metadata persists separately. Presence uses a latest-value
channel and encrypted session packets: names, world-space cursors and selection IDs
are transient, validated and expire after three seconds without updates. The UI
renders stable peer colors; display names are labels, not proof of human identity.

Use framed, versioned messages over reliable ordered bidirectional streams. A dialer
polls incremental Automerge sync at about 120 ms and exchanges peer rosters every
ten polls. Either side can reconnect. If connections race, keep a deterministic
preferred direction; connection cleanup compares stable IDs so an obsolete task
cannot remove a newer connection. Peers form a small mesh and continue after the
founder leaves. Invitation admission can be performed by any reachable authorized
seed, even when the original issuer is offline.

Only one board is offered per app instance. Switching/closing it stops offering it;
saved edits remain available for a later reconnect. Local save status and live
connection status are separate. Immediate shutdown does not guarantee that the last
local edit has reached another participant. A transport acknowledgement is not a
claim that every peer has durably saved a change.

## Connectivity and desktop integration

Default endpoints use iroh's discovery and public relay infrastructure, prefer
direct connections and retain encrypted relay fallback. `JOTFLOCK_RELAY_URL`
selects a custom relay; `JOTFLOCK_RELAY_ONLY` disables direct IP transports.
Display the selected path as direct, relay or mixed. Relays do not store boards.
A link contains peers known at issuance, not a global board directory. If all listed
seeds are offline or obsolete, obtain a fresh link from an online editor. The fact
that some member is online is necessary but does not make every old link routable.

Linux desktop registration is explicit. Invocation with a link forwards it to the
running instance over a private local socket. An exclusive file lock prevents two
UI processes from writing the same data directory. Paste invitation remains the
fallback. Other desktop platforms are not currently release-tested.

## Resource and trust limits

Initial caps: 16 participants, 32 concurrent incoming connection handlers, 32 grants
per chain, 128 KiB invitation links, 8 MiB wire frames, 4 MiB saved session documents,
10,000 live objects and world coordinates within ±1,000,000 for network sharing.
Handshake/read/write timeouts, retry delay and bounded queues limit ordinary load.
These are trusted editor sessions, not a proven hostile-peer sandbox. Compressed
CRDT inputs can allocate before post-apply quotas run; all-party denial-of-service
resistance, metadata privacy, traffic analysis and global durable delivery are not
proved. Direct peers can learn network addresses. No formal verification is claimed.

## Validation record

2026-11-17: the normal suite passed 51 tests with 64 cases per generated property
and retained seed replay; the one public-infrastructure test was excluded there.
The relay-only test was run separately and passed, with direct IP transport disabled
and the selected path asserted to be a relay. Real endpoint tests also cover signed
admission/rejection, delegation after founder departure, presence exchange/expiry,
identity-preserving restart, reconnection and stopping during an incomplete greeting.
Formatting and Clippy with warnings denied passed. The Linux x86_64 AppImage was
rebuilt. GUI behavior, desktop registration and two physical machines behind separate
NAT/firewall devices still require manual verification; no production security audit
or formal proof is claimed.
