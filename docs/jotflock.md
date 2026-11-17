# Jotflock — collaborative Rust whiteboard proposal

**Status:** See the [README](../README.md) for the current implementation. This plan also covers future work.

Implementation checkpoint: the current build provides the native canvas, multiple local boards with persistent open/closed state and recoverable deletion, whole-card dragging/resizing, in-place text editing, component shapes, member-carrying frames, labeled attached arrows, Automerge documents and offline-merge/interaction tests. Core invariants are documented in [ADR 0003](adr/0003-core-invariants-and-generated-testing.md) and exercised with generated tests and focused regressions; no formal proofs are claimed. Durable board files are an initial storage implementation; Useful frame/diagram templates, structured story-map slots and encrypted iroh live sessions are now implemented, with signed delegated membership and 24-hour invitation links. SQLite, legacy import and the remaining release features are pending. See [README](../README.md) for the current runnable scope and [architecture overview](adr/README.md) for the implemented architecture and decisions.

## Project conventions

- Use Conventional Commits, for example `fix(canvas): preserve frame members`.
- Keep documentation simple and concise (KISS).
- Avoid inline code comments. Add one only when absolutely necessary to explain something the code cannot make clear.

## Goal and starting point

A native Rust desktop app where a small team can open a shared canvas on their own computers, brainstorm, design software architectures, and map user stories. Boards work offline and synchronize directly between authorized peers when connected. No mandatory account or central board-storage service.

Jotflock started from a Rust/eframe/egui prototype with notes, templates, pan/zoom, JSON storage, undo and LAN multicast. The current app uses Automerge board files and authenticated iroh sessions.

## Home and editor

Home shows board previews and offers creation and joining. Click a preview to open the board or its title to rename it. Each board’s
share icon copies its invitation. The three-dot menu holds its settings. The editor focuses
on drawing, with an icon tool rail and on-demand object properties. Click the board title, object text or an arrow label to edit in place. Pinch or scroll on the canvas to zoom.

## Name and visual design

The app is named Jotflock. The wordmark uses bold violet **jot** followed by regular dark flock. Preserve the existing beige, violet, and blue color scheme: beige surfaces, violet primary accents, and a range of blues for secondary accents and canvas elements. Reuse the established visual direction from the current app, with readable text and clear contrast for controls, selection and collaboration indicators.

## First usable release

- Board library: create, rename, open, duplicate, delete, autosave, import/export.
- Canvas: pan/zoom, selection and multi-selection, drag/resize, clipboard, keyboard shortcuts, grouping, frames, colors, text and sticky notes, local-user undo/redo.
- Architecture: rectangles and other basic shapes, labeled directional connectors anchored to objects; a simple component-diagram template.
- Brainstorming: sticky notes, topic frames, colors, brainstorming template.
- Story mapping: activity columns, story cards, release lanes, reorder/move cards, story-map template.
- Collaboration: invitation copy/paste, optional join approval, participant names/colors, live cursors/selections, connection/sync status, concurrent editing, reconnect and offline merge.
- Portable board archive with schema version and embedded assets; PNG export. Images and image sharing follow the core collaboration milestone.

Linux distribution: ship a runnable **AppImage** so users can download the app, make it executable, and launch it without installing Rust or building from source. Use [package-appimage.sh](../package-appimage.sh) to bundle the runtime libraries, desktop entry and icon. Build on the oldest supported Linux baseline and test the resulting artifact on the target distributions, including any FUSE requirements.

Initial scale target: 2–10 trusted collaborators and 5,000 simple objects per board. Linux first (existing prototype), then Windows and macOS; validate builds and input behavior on all three before calling them supported. Voice/video, AI, plugins, Jira integrations, full Miro import, and enterprise administration are outside the first release.

## Architecture

**Native UI:** retain `eframe/egui`, keeping rendering and interaction separate from document state. Use viewport culling and a spatial index; keep disk/network work off the UI thread. Confirm text editing, high-DPI behavior and accessibility in the first spike.

**Shared document:** use Rust Automerge CRDTs for objects keyed by stable IDs, text, geometry, styles, connectors, frame membership, and story-map ordering. Text uses collaborative text operations, not replacement strings. Define deterministic concurrent move/resize and delete-versus-edit behavior. Undo emits compensating operations for the local user's changes; it must never restore an entire old board over peers' work. Prototype these semantics before committing to the schema.

**Networking:** Tokio plus iroh for encrypted peer connections and NAT traversal. Run Automerge's per-peer synchronization over reliable ordered streams. Exchange authorized peer introductions to build a small full mesh; the creator can leave after other peers have connected. Presence is separate, rate-limited, and expires on disconnect; only document changes persist.

**Storage:** local SQLite transactions for incremental document changes, snapshots, board metadata and identities; content-addressed files for later image assets. Commit local changes before reporting them saved, retain recovery backups, and version both disk format and wire protocol. Keep separate modules/crates for UI, document/commands, storage and networking so synchronization can be tested without a GUI.

## What P2P means in practice

Boards have stable random IDs; a content hash changes with edits and is not the permanent board address. Sharing uses `jotflock://join/<invitation-ticket>`. The ticket carries a protocol version, board ID, an inviter’s cryptographic endpoint identity, connection/discovery information and a high-entropy admission secret. Anyone with the link may join as an editor by default. Every admitted editor may invite and admit new editors; the creator has no exclusive admission role. Optional approval modes are later work. Join invitations expire 24 hours after issuance; any admitted editor can generate a fresh invitation. Expiry blocks new joins using that link and does not remove existing members. Treat the invitation as a password. Register the desktop URL handler and provide a Paste invitation fallback, including for AppImage launches where desktop integration is unavailable.

Create a board → copy its link → another user opens/pastes it → authenticate the ticket → download the board from an online participant → merge changes continuously. Persist the device identity and board replicas across restarts. Invitation cancellation prevents new admissions; it cannot erase copies already downloaded. Membership and invitation rotation need explicit protocol semantics before shipping sharing.

Use iroh discovery and relay infrastructure to find the inviter and establish an encrypted connection, then attempt direct UDP NAT traversal/hole punching. If the direct path fails, continue over the encrypted relay. Normally users need no port forwarding; restrictive corporate firewalls can still block traffic. Show whether the connection is direct, relayed, reconnecting or blocked. Test separate NATs, carrier-grade NAT, blocked UDP and relay-only connections. Do not claim universal firewall traversal.

Direct connections are preferred. Some routers/firewalls require a relay: use configurable public infrastructure during development and offer a self-hosted relay for controlled deployment. Relays forward encrypted traffic; they do not store boards. Relay operation, availability and bandwidth still have costs. Provide a direct/LAN-only option with clear connection failures.

**Accepted availability model: live sessions.** A newcomer needs an authorized, reachable peer offering the board for admission and initial synchronization. Existing members can open and edit their local copies offline; changes merge when authorized replicas connect again. If everyone is offline, the invitation cannot download the board. No central board store or always-on replica is included in the first release. Every admitted editor can invite and admit new editors, so the creator need not stay online. ADR 0006 implements signed membership, cached peer discovery and strict UTC expiry checks. Early invitation cancellation, key rotation and member removal remain future work. See [ADR 0004](adr/0004-live-session-availability.md).

## Trust, privacy and resilience

- Persist device keys using platform credential storage where available; authenticate membership as well as encrypt transport. A board ID alone grants no access.
- First release assumes trusted, equal editors. Invitations are admission credentials, approved members are replicated consistently, and strangers are rejected. Fine-grained roles and robust revocation are later work; removing a peer cannot erase copies they already hold.
- Validate protocol versions, object sizes and asset sizes; limit connections/queues and reject malformed traffic. Reconnect with backoff and show actionable errors.
- Local boards use OS file permissions; transport encryption does not imply encrypted files at rest. Document that direct peers can learn each other's network addresses.
- Preserve existing prototype boards through a one-time importer with backup; never silently replace their saved JSON.

## Delivery and acceptance gates

1. **Foundation spike:** split prototype modules; prove two Rust replicas merging concurrent text, moves, deletes and offline edits over iroh. Check connectivity across separate home networks and forced relay use. Finalize schema, undo and invitation design.
2. **Local board:** implement canvas tools, three templates, board library, durable storage, recovery and archive import/export. Verify restart/crash recovery and existing-board migration.
3. **Collaboration MVP:** invitations, authenticated mesh, initial/incremental sync, presence and reconnect. Verify three peers converge after partitions, creator departure and restarts; test same-object edits and local undo without losing peers' changes.
4. **Release hardening:** images/PNG export, performance profiling, bounded-input tests, protocol compatibility, platform CI, a tested Linux AppImage, Windows/macOS installers, dependency/license review and signed release artifacts where applicable. Target smooth interaction at the stated board/team size; measure CPU, memory and bandwidth before expanding limits.

**Review assumptions:** native Rust UI; Linux first with Windows/macOS planned; trusted small teams; relay fallback allowed; no central board storage. These guide the implementation; revise them as the product evolves. Main risks are collaborative undo, text/input quality, large-board rendering and real-world NAT/firewall behavior.

Technical references: [Automerge Rust sync](https://docs.rs/automerge/latest/automerge/sync/), [Automerge concepts](https://automerge.org/docs/reference/concepts/), [iroh](https://www.iroh.computer/), [iroh security and privacy](https://docs.iroh.computer/deployment/security-privacy).
