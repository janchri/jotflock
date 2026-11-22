# ADR 0005 — Templates and story maps

Status: Implemented.

## Decision

Templates create ordinary editable objects in a candidate document. Validate the
candidate before replacing the original. Insert beside existing content and fit
the viewport to the inserted objects.

- Brainstorming creates frames for the challenge, ideas, themes and next steps.
- Architecture creates components with shapes and labeled arrows.
- User story map creates activity columns, release lanes and sticky-note stories.

## Story map behavior

For example, reordering a release lane moves its stories with it. Stories reference
stable activity/release header IDs. The read model derives positions from CRDT
lists, removes duplicate axis IDs and orders stories deterministically within cells.

Headers support renaming. Add and reorder activities and releases through object
properties or commands. Dropping a story into a cell assigns it there. Moving it
outside the map restores free placement. Deleting the map preserves its children
at their projected positions.

## Limits and compatibility

Each map displays up to 12 activities and 12 releases. Crowded cells overlap.
Concurrent additions beyond the axis limit can leave extra headers as ordinary
text objects. Axis deletion and custom grid styling are pending.

Structured fields retain schema 2. Earlier builds that lack this feature cannot
render the layout and should not edit these boards.

Code — [templates.rs](../../src/templates.rs), [story_map.rs](../../src/story_map.rs).
