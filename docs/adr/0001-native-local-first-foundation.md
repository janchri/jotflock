# ADR 0001: Native local-first desktop foundation

Status: Accepted and implemented; networking added by ADR 0006.

## Context

The repository includes a website clone and an older Rust prototype with
unauthenticated LAN multicast. Jotflock needs a native desktop foundation with
local ownership of board data and an eventual encrypted peer-to-peer transport.

## Decision

Use Rust with eframe/egui for the desktop UI and Automerge for board state and
collaborative text. Keep document editing independent of GUI rendering and disk
storage. Use stable IDs and deletion tombstones rather than replacing whole board
snapshots. The existing UDP transport is not carried into this application.

For the initial implementation, store full Automerge files using temporary-file
replacement, filesystem sync and previous-version backups. Persist open/closed
board state separately. Closing must save successfully before releasing a board;
board deletion moves the file to local trash. Ship Linux development builds as
AppImages, retaining the beige, violet and blue visual design.

## Consequences

Document merge and synchronization can be tested without a GUI or network. Files
remain portable at the document level, but they are not yet a user-facing archive
format. Saving/materialization currently run synchronously and full history grows
with edits. SQLite, background work and history compaction need later decisions.
Invitations, iroh transport, relay fallback and authenticated membership were added
in ADR 0006, with their verification boundary recorded there. The prototype's JSON file
is preserved but not imported automatically.
