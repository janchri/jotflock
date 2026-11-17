# Jotflock

Native Rust collaborative whiteboard with a bold violet **jot** and regular flock wordmark.
[Current architecture and ADRs](docs/adr/README.md) · [Product plan](docs/jotflock.md)

## Run

Install current stable Rust, a C compiler, pkg-config and the Linux Wayland/X11/OpenGL
runtime/development libraries for your distribution, then:

```sh
cd jotflock
cargo run --release
```

Or launch `jotflock-x86_64.AppImage`. Close older app instances before upgrading;
use the same current version on collaborating computers.

When upgrading an existing installation, use its existing data directory with
`JOTFLOCK_DATA_DIR` or move it to the Jotflock data directory while the app is
closed. This preserves board files, preferences and the device key. Board owners
must issue fresh `jotflock://join/` links, and previously invited editors can use
those links to reconnect their local copies.

## Home and canvas

Click a board preview on Home to open it. Choose **New board** or a template
preview to create one. Click the title on Home or in the editor to rename it. Use its **⋯** menu
for its name, your editor name, invitations, closing or deletion. **Home** in the
editor returns to the board library. Pinch or scroll over the canvas to zoom around
the pointer.

- Click card text to edit. Drag a card to move it. Escape finishes editing. Middle-drag
  pans the canvas. Select an object and drag its bottom-right corner to resize.
- Components offer rectangle, ellipse and diamond shapes. Select a source and open
  the sliders icon for **Object properties**. Click the connector icon and then
  the target. Click an arrow label to edit it. Use **Object properties** to add
  a label to an unlabeled arrow. Arrows follow moves and resizes.
- Frames collect objects dropped fully inside. Drag the title bar to move members
  together. Deleting a frame keeps its contents. Frames do not nest.
- **Brainstorming** creates four working frames: challenge, ideas, themes and next
  steps, each with prompts.
- **Architecture** creates a component diagram with database/authentication shapes
  and labeled application/API/storage/queue relationships.
- **User story map** creates activities, release lanes and assigned stories. Select
  the map frame and open **Properties** to add or reorder activities and releases.
  Click header text to rename it.
  Stories retain their activity/release when headers move. Drag stories between
  cells or outside the map. Header placement and story sizes follow the layout.
  Up to 12 activities and 12 releases are supported; crowded cells compact stacks.
- Template insertion appends beside existing content and fits the viewport.

Close boards with **Close board** in board settings. They remain in Saved
boards and can be reopened; open/closed state survives restart. Delete moves a board
into recoverable local `trash/` after confirmation.

## Encrypted live collaboration

1. On Home, click the share icon beside the board title, or choose **⋯ → Copy invitation**.
   Wait for the checkmark or invitation-copied message, then send the link to your collaborators.
2. On another computer, choose **Join a board** on Home, paste the link and click **Join board**.
   The inviter or another participant listed in that link must be reachable and
   offering the board. Device clocks must be correct; links expire after 24 hours.
3. Edit together. Board settings show connected editors and the direct/relay path.
   Enter a display name to identify your live cursor and selection to participants.
4. Every admitted editor can copy a fresh invitation, including after the creator
   leaves. Admitted devices retain membership after the original link expires.
5. After restart, choose **Copy invitation** in the saved board’s settings to reconnect using saved
   membership and cached peers. Offline edits merge when participants reconnect.

Stop sharing, switching boards or closing the offered board stops this instance's
session. Other participants can continue. Only one board is offered per instance.
If everyone is offline, newcomers cannot download the board. If all peers listed
in an old invitation are offline or have obsolete addresses, ask an online editor
for a fresh link. There is no central board store.

On Linux, **Enable invitation links on this PC** installs a desktop handler using
xdg-utils; a link can then launch the app or be forwarded to its running instance.
The AppImage must stay at the registered path; register again after moving it.
Paste invitation works without desktop integration. CLI equivalent:

```sh
./jotflock-x86_64.AppImage --install-desktop
```

Transport uses authenticated iroh TLS connections, direct NAT traversal and encrypted
relay fallback. By default it uses public discovery/relay infrastructure.
`JOTFLOCK_RELAY_URL=https://your-relay.example` selects a custom relay.
`JOTFLOCK_RELAY_ONLY=1` disables direct IP transport. A custom relay alone does not
disable public endpoint discovery. Infrastructure sees connection metadata; direct
participants can learn each other's network addresses.

Anyone receiving a forwarded invitation may join. This is a trusted-editor model;
early link cancellation, member removal and key rotation are not implemented. Stop
sharing is not access revocation. Local files are not encrypted at rest. Back up the
whole app data directory, including `network/`, to retain device identity/membership.
Limits: 16 participants, 4 MiB saved document history, 10,000 live objects and shared
world coordinates within ±1,000,000. These are not performance guarantees. A local
save does not guarantee every peer has saved an edit, especially on immediate exit.

## Storage and compatibility

Boards are Automerge files in the platform's local app data directory; set
`JOTFLOCK_DATA_DIR` for an isolated directory. Writes preserve a previous-version
`.bak`. Session state and network authorization are separate files. Unix device
keys/authorization files are private to the user. Invalid files fail explicitly
rather than being silently overwritten. Schema 1 loads with original content and
geometry; structured templates add fields to schema 2. Older versions cannot render
those layouts. Legacy prototype JSON import is not implemented.

## Validation

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
PROPTEST_CASES=128 PROPTEST_RNG_SEED=20261117 cargo test
```

The generated properties default to 64 cases plus replayed seeds. Keep
`proptest-regressions/`. Tests cover document invariants, Unicode edits, persistence,
concurrent merges, template slots/reordering, headless input, signed admission,
creator departure, device restart/reconnect and unauthorized access rejection.
These are tests, not formal proofs or an independent security audit.

The external relay test is excluded from ordinary offline runs. Run it explicitly:

```sh
cargo test public_relay_only_synchronizes_encrypted_board -- --ignored --nocapture
```

Current verification: 51 tests passed in the normal suite (64 cases per generated
property); the explicit public-relay test passed separately. Formatting, Clippy with
warnings denied and the Linux x86_64 AppImage build passed.

It disables direct IP transport and asserts an encrypted connection actually uses
the public relay. Live desktop interaction and testing across separate physical
NAT/firewall setups and Linux distributions remain pending.

## Build an AppImage

Install linuxdeploy with its AppImage output plugin, then:

```sh
./package-appimage.sh
```

Set `LINUXDEPLOY=/path/to/linuxdeploy.AppImage` if needed. The script builds the release
binary, linked libraries, desktop entry and icon. Build on the oldest supported Linux
baseline and test AppImage/FUSE compatibility on each target distribution.

## Remaining work

Undo/redo, multi-selection, nested frames, advanced connector routing/reattachment,
axis deletion, archives, images, PNG export, prototype import, SQLite/background
storage, member removal and platform credential-store integration are still pending.
