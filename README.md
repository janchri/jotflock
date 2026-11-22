# Jotflock

A collaborative whiteboard for Windows and Linux. Use sticky notes to brainstorm,
draw architecture diagrams, or plan releases with user story maps. Boards stay on
your device and work offline. Invite teammates to edit together in a live session.

[Architecture decisions](docs/adr/README.md) · [Product plan](docs/jotflock.md)

## Run

Get the package for your platform from a successful
[Build workflow run](https://github.com/janchri/jotflock/actions/workflows/build.yml),
under **Artifacts**. Sign in to GitHub with repository access to download them.

- **Windows** — extract the package and run the portable `jotflock.exe`.
- **Linux** — extract the package, make the AppImage executable, and launch it.

```sh
chmod +x jotflock-x86_64.AppImage
./jotflock-x86_64.AppImage
```

Use the same current version on collaborating computers. Close Jotflock before
replacing the executable. See [Build from source](#build-from-source) for development.

## Home and canvas

Create a board with **New board** or a template. Click a board preview to open it
and its title to rename it. The **⋯** menu contains board settings, invitations,
closing and deletion. Closing keeps the board in the library. Deletion moves its
saved content into local `trash/`.

| Action | Control |
| --- | --- |
| Edit text | Click card text or an arrow label. Escape finishes editing. |
| Move | Drag a card. Drag a frame's title bar to move its members. |
| Resize | Select a free object and drag its bottom-right handle. |
| Pan | Drag with the middle mouse button. |
| Zoom | Pinch or scroll around the pointer. |
| Connect | Select a source, choose the connector tool, then click a target. |
| Properties | Select an object and open **Object properties**. |

Components support rectangles, ellipses and diamonds. Arrows follow their endpoints.
Drop an object fully inside a frame to adopt it. Deleting a frame keeps its contents.

Templates cover **Brainstorming**, **Architecture** and **User story map**.
Story maps have activity columns and release lanes. Rename headers in place,
add or reorder axes in properties, and drag stories between cells. Each map
supports up to 12 activities and 12 releases.

## Encrypted live collaboration

1. Choose **Copy invitation** from the board's share button or **⋯** menu.
   Wait for confirmation, then send the link to a teammate.
2. They choose **Join a board**, paste the link and join. A participant listed in
   the invitation must be reachable and sharing that board.
3. Edit together. Set an editor name in board settings to identify your cursor.
   Offline edits merge when participants reconnect.

Invitations expire after 24 hours, so device clocks must be correct. Admitted
editors retain membership and can invite others after the creator leaves.
After restarting, use **Copy invitation** to resume sharing with cached peers.
Ask an online editor for a fresh link if an old link cannot reach anyone.

One board is shared per instance. Returning Home keeps the session active.
Switching boards, closing the offered board or stopping sharing ends this device's
session. Other editors can continue. Newcomers need an online participant to
download the board.

Share only with trusted editors. Anyone with a forwarded invitation may join and
invite others. Member removal, early link cancellation and key rotation are pending.
Local files are unencrypted. Public discovery and relays help establish encrypted
connections and see connection metadata. Direct peers can learn network addresses.

### Linux invitation links

Choose **Enable invitation links on this PC**, or run the following with xdg-utils
installed. Register again after moving the AppImage.

```sh
./jotflock-x86_64.AppImage --install-desktop
```

Paste invitation works on both platforms. Relay configuration and sharing quotas
are documented in [ADR 0006](docs/adr/0006-authenticated-iroh-sessions.md).

## Command bar and CLI

Press **Ctrl+K**, type `help`, and press **Enter**. Use **↑** and **↓** to recall
commands and the arrow button to view recent results.

Use the same commands in a shell with the executable's path.

```sh
jotflock cli board list
jotflock cli board open BOARD_ID
jotflock cli object add sticky 80 120 "Plan the release"
jotflock cli view fit
```

Use `jotflock.exe` on Windows or the AppImage path on Linux if the executable is
outside PATH. Commands return a JSON success/error result. Use IDs from command
results. Object coordinates are canvas coordinates. History lasts for the process.

On Unix, the CLI sends commands to the running instance for the same data
directory. Without a running instance, local commands load and save the profile
directly. On Windows, close the app before using the shell CLI.

### Run without a window

On Unix, keep a headless instance running for live sharing and state across calls.

```sh
JOTFLOCK_DATA_DIR=/path/to/profile jotflock --headless
```

Run CLI commands in another terminal using the same `JOTFLOCK_DATA_DIR`.
Use `share invite`, `share join LINK` and `share status` for collaboration.
`jotflock cli quit` saves and exits. See [ADR 0009](docs/adr/0009-command-interface.md)
for command behavior and platform limits.

## Storage and compatibility

Back up the entire app data directory, including `network/`, to preserve boards,
preferences, device identity and membership. Set `JOTFLOCK_DATA_DIR` to use an
existing profile or an isolated directory. Move a profile while the app is closed.

Boards are Automerge files with a previous-version `.bak`. Schema 1 loads into
schema 2. Earlier builds without structured templates should not edit story-map
boards. Legacy prototype JSON import is pending. Upgrades from the previous
identity require fresh invitations. See [ADR 0008](docs/adr/0008-jotflock-identity.md).

## Build from source

Install stable Rust and a C toolchain. On Linux, install pkg-config and the
Wayland/X11/OpenGL development libraries. The
[build workflow](.github/workflows/build.yml) lists the Linux packages.

From this repository, run the following.

```sh
cargo run --release
```

Windows release builds produce `target/release/jotflock.exe`.

### Build an AppImage

Install linuxdeploy with its AppImage output plugin, then run the following.

```sh
./package-appimage.sh
```

Set `LINUXDEPLOY=/path/to/linuxdeploy.AppImage` if needed. Build on the oldest
supported Linux baseline and check AppImage/FUSE compatibility on target systems.

## Validation

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

Generated tests default to 64 cases. Keep `proptest-regressions/`. For a deeper run,
use `PROPTEST_CASES=128 PROPTEST_RNG_SEED=20261117 cargo test`.

The public relay test requires internet access and runs separately.

```sh
cargo test public_relay_only_synchronizes_encrypted_board -- --ignored --nocapture
```

Platform compatibility, separate physical networks and large-board performance
still need release validation. Undo/redo, multi-selection, nested frames, images,
PNG export and portable board archives are pending. See the
[architecture overview](docs/adr/README.md) for implementation limits.
