# ADR 0005: Useful templates and structured story maps

Status: Implemented and covered by template/document tests.

## Decision

Templates build ordinary editable document objects in a candidate document, validate
it, then replace the original only on success. Insertions append to the right of
existing content; the viewport fits only the newly inserted content, preserving existing content
and making repeated insertions discoverable. Brainstorming creates four working frames with prompts. Architecture
creates a component diagram with shape types and labeled directed relationships.

A story map reuses one frame, text headers and sticky-note stories. The frame owns
CRDT lists of activity-header and release-header IDs. Stories reference stable
column/release IDs, not numeric indexes. Derive header/story positions from those
lists at read time, with deterministic ID ordering within each cell. Axis reorder
therefore moves stories visually while retaining their semantic slot, including
after reload and replica merge. Duplicate axis IDs caused by concurrent list moves
are suppressed by the read model. Headers can be renamed but cannot be independently
dragged or deleted; add/reorder axes through the map's Selection controls.

Dragging a story clears its slot while moving. Dropping it fully inside a map
assigns the cell under its center; dropping it outside restores free placement.
Story-card dimensions follow the layout. Moving the parent frame moves its members.
Deleting a map persists the current projected child positions and detaches the
children so their content and placement survive. Ordinary frames still do not nest.

## Limits

The initial tool supports up to 12 activities and 12 releases per map. Very crowded
cells use compact, overlapping stacks to keep frame dimensions bounded. Concurrent
additions at the axis limit can leave excess headers as ordinary text objects;
objects are preserved. Removing populated axes, arbitrary grid styling, filtering,
priorities, voting and task integrations are future work. This does not claim a
complete workshop or story-management system. Additive fields retain app schema 2;
old app versions do not render structured layouts and should not edit these boards.
