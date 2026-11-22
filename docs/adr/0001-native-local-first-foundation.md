# ADR 0001 — Native desktop and local storage

Status: Implemented.

## Decision

Use Rust and eframe/egui for the desktop app, Automerge for collaborative board
state, and iroh for live sharing. Keep document operations independent of rendering.
The build workflow packages a Linux AppImage and a portable Windows executable.

For example, closing a board saves it and keeps it in the library. Deleting it
saves its latest content, then moves the board file into local `trash/`.

## Storage

- Each board is a full `<id>.automerge` file in the platform app data directory.
  `JOTFLOCK_DATA_DIR` overrides that directory.
- Saves validate the board and copy the previous file to `.bak`. A shared writer
  uses a unique temporary file, syncs it and atomically replaces the destination.
  Unix files use mode 0600 and the writer also syncs the directory.
- `session.json` stores open board IDs and the active board. Editor preferences
  and network authorization are separate files.
- Invalid files and board IDs that disagree with their filenames report an error.
  Legacy prototype JSON import is pending.

## Limits

Saving and materializing boards run synchronously. Full CRDT history grows with
edits. Background storage, compaction and a portable archive format are pending.
Board, session and trash updates span separate filesystem operations. A failure
after a rename can leave an uncertain result on disk.

Code — [storage.rs](../../src/storage.rs), [workspace.rs](../../src/workspace.rs),
[build workflow](../../.github/workflows/build.yml).
