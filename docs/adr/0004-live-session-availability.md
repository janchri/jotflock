# ADR 0004: Live-session availability

Status: Implemented for the first collaborative release; see ADR 0006.

## Context

Participants will run native applications on their own computers and share board
invitations. Guaranteed board availability while all participant computers are off
would require additional board-storage infrastructure. Live sessions are sufficient
for the first collaborative release.

## Decision

A newcomer needs at least one authorized, reachable participant with the board
available and sharing enabled for admission and initial synchronization. Merely
running the app is insufficient if that board is closed or no participant is
currently offering it. Existing members retain local replicas and can open and edit
them offline. Changes synchronize when authorized replicas connect again.

When the last participant goes offline, live collaboration stops. Saved copies
remain on the participants' devices. Invitation links do not promise that a board
can be downloaded while everyone is offline. The UI must distinguish waiting for
an online participant from invalid invitations, authentication failures and
network-connectivity failures.

Do not introduce a central board store or always-on board replica for the first
release. Discovery and relay infrastructure may help establish or forward encrypted
connections, but must not be presented as offline board storage. Relay hosting is a
separate decision from board availability.

## Admission authority

Every admitted editor may issue invitations and admit new editors. The board creator
has no exclusive admission role. Admitted editors can continue collaborating and
inviting others after the creator leaves, provided an authorized peer offering the
board remains reachable. This is a trusted-team model: any editor can extend board
access to another person.

Membership must be authenticated independently of board content; possession of a
copied board file or an arbitrary membership field is not sufficient evidence of
admission. The protocol must let a peer verify that an admission was authorized by
an already-admitted editor. ADR 0006 implements signed, device-bound delegation
chains independently of canvas content.

## Invitation lifetime

Join invitations expire 24 hours after issuance. An admitted editor may generate a
new invitation at any time, with its own issuance time and 24-hour lifetime. Expiry
prevents new admission using that invitation; it does not remove access from
already-admitted devices or delete their local replicas. Invite expiry is not a
membership-revocation mechanism.

The signed expiration timestamp is checked at admission against the device's UTC
clock, with no expiry grace. Device clocks must be correct. Early cancellation and
rotation remain future work.

## Protocol resolution

ADR 0006 implements verified member rosters and cached endpoints. New invitations
carry the peers known at issuance; every admitted editor can generate a fresh link.
No global board-directory service is implemented, so an old link whose listed peers
are all unreachable needs replacement by an online editor. Signed membership chains
let the creator leave without interrupting admitted participants. Early invitation
cancellation, member removal and key rotation remain deliberately unimplemented.

## Consequences

There is no guaranteed delivery between participants whose online periods never
overlap, and one online replica may not yet contain another offline member's latest
changes. Do not promise the latest global version merely because a peer is online.
An always-on service can be considered separately later without changing the
requirement that existing local boards remain usable offline.
