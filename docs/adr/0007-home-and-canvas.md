# ADR 0007 — Home and canvas

Status: Implemented.

## Decision

Use Home as the board library and a separate canvas editor.

For example, click a board preview to open it and click its title to rename it.
Each tile offers an invitation button and a settings menu. Previews are cached
locally and invalidated by document changes.

## Interaction

- Create a blank board or choose a template on Home. In the editor, insert a
  template beside existing content.
- Use the icon rail for canvas tools and object properties for the selection.
- Click object text or arrow labels to edit. Drag cards to move them, use the
  resize handle for free objects, and pinch or scroll to zoom around the pointer.
- Dialogs adapt to the window and scroll on short displays. They block background
  controls while open.
- The editor name is a local preference shared across boards.
- Window close saves pending edits first. A failed save keeps the window open
  and reports the error.
- Linux disables swap interval pacing so presentation to a hidden workspace
  cannot block the window event loop.
- Returning Home keeps sharing active. Switching boards ends the previous session
  on this device.

Code — [app_ui.rs](../../src/app_ui.rs), [interface.rs](../../src/interface.rs),
[card.rs](../../src/card.rs), [scene.rs](../../src/scene.rs).
