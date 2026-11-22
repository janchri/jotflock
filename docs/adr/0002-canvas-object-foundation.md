# ADR 0002 — Canvas objects, frames and connectors

Status: Implemented.

## Decision

Store objects as `card:<uuid>` maps and connectors as `edge:<uuid>` maps in
Automerge. Objects have stable IDs, world geometry and collaborative text.
Deletion uses tombstones. Component shapes are rectangle, ellipse and diamond.

For example, dropping a note fully inside a frame adopts it. Dragging that
frame's title bar moves its members. Deleting the frame keeps the notes.

## Interaction and compatibility

- Membership changes on object placement. Overlapping frames resolve by smallest
  area, then ID. Moving a frame over an object does not adopt it.
- Resizing a frame changes its bounds. Frames do not nest.
- Labeled straight arrows follow live, non-frame endpoints. Missing or deleted
  endpoints and self-loops are hidden by the read model.
- Selection, pan, zoom and editing focus stay local.
- Schema 1 loads as schema 2, retaining IDs, text, positions and legacy 232×158
  dimensions. Missing shape and membership fields default to rectangle and no
  parent. Unknown schemas fail. Structured layouts are covered by ADR 0005.

## Limits

Coordinates must be finite. Width is 120–20,000 and height is 80–20,000.
Frame moves validate all member translations before writing. Concurrent moves
and reparenting still merge per field and can produce unexpected layouts.
Multi-selection, nested frames and advanced connector routing are pending.

Code — [document.rs](../../src/document.rs), [scene.rs](../../src/scene.rs),
[geometry.rs](../../src/geometry.rs).
