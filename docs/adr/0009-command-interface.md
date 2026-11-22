# ADR 0009 — Commands and headless mode

Status: Implemented. Running-instance commands and persistent headless mode require Unix.

## Decision

Use the same command dispatcher for the desktop command bar and shell CLI.
For example, `jotflock cli board list` reads the running profile's boards, or
opens the profile for one local command when it has no owner.

The command bar uses Ctrl+K. Dragging edits continuously and records the final
absolute move or resize when released. The recent command history stays in memory.

## Ownership and persistence

An exclusive profile lock prevents concurrent owners. On Unix, clients send JSON
argument arrays through the private `app.sock` and receive a JSON success/error
envelope. Invitation links use the same socket. Commands run against the owner's
document, selection, viewport and sharing session.

Without an owner, a CLI process locks the profile, runs one local command, saves
changes and exits. Persistent sharing requires a desktop or headless owner.
`jotflock --headless` runs the workspace, autosave and network loop without a window.

## Limits

Windows supports the desktop command bar and local CLI calls when the profile is
free. Forwarding commands to a running Windows instance and persistent headless
mode are unavailable. Linux desktop link registration is explicit.
The release executable uses the Windows GUI subsystem; CLI invocations attach to
their parent console for output.

CLI edits save before reporting success. A save failure can leave edits in memory
for autosave to retry. View state and history last for the owner's process.
Replaying a creation command allocates new IDs. Clipboard and widget focus belong
to the desktop.

Code — [command.rs](../../src/command.rs), [desktop.rs](../../src/desktop.rs),
[terminal.rs](../../src/terminal.rs).
