# Jotflock architecture decisions

For example, a board can be edited offline on Windows or Linux, then shared with
an invited teammate in a live encrypted session. Each editor retains a local copy.

These ADRs describe the current implementation. Update the relevant decision when
behavior changes. Keep setup instructions and validation commands in the
[app README](../../README.md).

## Decisions

| ADR | Topic |
| --- | --- |
| [0001](0001-native-local-first-foundation.md) | Native desktop and local storage |
| [0002](0002-canvas-object-foundation.md) | Canvas objects, frames and connectors |
| [0003](0003-core-invariants-and-generated-testing.md) | Document invariants and generated tests |
| [0004](0004-live-session-availability.md) | Live-session availability |
| [0005](0005-structured-templates.md) | Templates and story maps |
| [0006](0006-authenticated-iroh-sessions.md) | Authentication, replication and transport |
| [0007](0007-home-and-canvas.md) | Home and canvas interaction |
| [0008](0008-jotflock-identity.md) | Full wordmark and compact mark |
| [0009](0009-command-interface.md) | Commands and headless mode |

## Code map

| Area | Modules |
| --- | --- |
| Controller and desktop UI | `main.rs`, `app_ui.rs`, `interface.rs` |
| Commands | `terminal.rs`, `command.rs` |
| Canvas and layout | `scene.rs`, `card.rs`, `geometry.rs`, `templates.rs`, `story_map.rs` |
| Documents and storage | `document.rs`, `workspace.rs`, `storage.rs` |
| Sharing and local IPC | `sharing.rs`, `network.rs`, `desktop.rs` |

## Release boundaries

The build workflow packages Linux AppImages and portable Windows executables.
Build artifacts alone do not establish platform compatibility. Windows runtime,
Linux distribution compatibility and connections across separate physical networks
still need release validation.

Known limitations include synchronous board saving, growing CRDT history, linear
document scans and unmeasured large-board performance. Undo/redo, multi-selection,
nested frames, images, PNG export and portable board archives are pending.
Sharing assumes trusted editors. Member removal, key rotation and encrypted local
storage are pending. Unix provides command forwarding and persistent headless mode.

The static website lives in `html/`. Its header and footer use the full wordmark.
Desktop Home uses that mark too. Launchers and the favicon use the compact mark.
