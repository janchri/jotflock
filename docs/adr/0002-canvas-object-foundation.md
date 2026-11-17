# ADR 0002: Canvas objects, frame membership and attached connectors

Status: Accepted and implemented.

## Context

Templates were only lists of colored cards. Useful diagramming and structured
workshops first require reusable geometry, actual containers and relationships.
These concepts must also survive persistence and concurrent document merges.

## Decision

Keep schema 2 objects in independent `card:<uuid>` Automerge maps. Persist world
coordinates, width/height, kind, shape, optional parent-frame ID and collaborative
text. Diagram components support rectangles, ellipses and diamonds; sticky notes
and plain text retain their existing roles. Frames are containers drawn behind
other content, with an interactive title bar rather than a draggable interior.

A non-frame object dropped fully inside a frame's content area adopts that frame.
If several frames contain it, choose the smallest, then stable ID for ties. Moving
a frame translates its existing members. Resizing changes its bounds without
scaling or clipping members. Moving a child out detaches it. Frame deletion keeps
its children, and a deleted/missing parent resolves as no parent. Frames do not
nest in this version. Resizing uses a bottom-right handle and bounded dimensions.

Persist directed relationships as `edge:<uuid>` maps containing source/target IDs,
collaborative label text and deletion tombstones. Derive endpoints from the live
shape outlines every render; do not persist screen coordinates or detached line
endpoints. Straight arrows are the first routing mode. Edges with missing/deleted
endpoints are absent from the read model, including after concurrent edits.
Frames cannot be connected; self-loops are excluded from the read model. Selection, connecting previews and text focus are
local scene state. Draw frames, then edges, then nodes; selected nodes appear on
top. Connect by selecting a source, choosing Connect, then clicking a target.

Load schema 1 additively: keep IDs, positions, text and 232×158 legacy dimensions;
missing shape/membership fields default to rectangle/no parent. Do not infer
membership from old placement. New frames start at 720×480. Saving an edited board
writes schema 2 while storage retains the previous file as `.bak`. Earlier app
versions reject schema 2 rather than silently discarding newer fields.

## Consequences and limits

Templates can later compose the same objects and relationships as manual tools.
No separate template-specific rendering engine is needed. Membership is explicit,
not continuously inferred from overlap; moving a frame over a note does not adopt
it. Frame movement writes positions for members individually. These writes merge
deterministically, but frame movement is not a distributed atomic group operation:
concurrent reparenting/child moves can produce surprising layouts. Coordinates and
sizes are scalar CRDT fields; same-field conflicts use Automerge resolution.
Local frame movement preflights every member before changing the document, rejecting
translations whose final stored coordinates would be non-finite. Geometry uses f64
intermediates while retaining f32 world coordinates; these numeric checks are tested
in ADR 0003's generated suite and are not formal proofs.
Nested frames, multi-selection, snap guides, automatic layout, orthogonal routing,
connector endpoint reattachment and structured story-map ordering require later
work. Do not claim those behaviors from this foundation.
