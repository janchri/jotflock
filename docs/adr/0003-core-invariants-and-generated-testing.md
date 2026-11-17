# ADR 0003: Core invariants and generated testing

Status: Accepted and implemented.

## Context

Example-based tests cover known interactions but leave many edit orderings and
failure paths unexplored. Before adding transport, specify the document contract
and exercise long sequences against independently tracked object/relationship IDs.

## Decision

Keep example and headless interaction tests. Add test-only Proptest generators in
`src/property_tests.rs`; do not add a proof system or change the implementation
language. Run 64 generated cases per property by default, with environment overrides
for deeper runs. Shrink failures and retain generated seeds in
`proptest-regressions/`; add focused regression tests for each corrected bug.

The required invariants are:

| ID | Rule | Scope |
|---|---|---|
| I01 | IDs of visible objects equal created IDs minus explicitly deleted IDs. Deleting a frame never deletes its members. | Valid document commands, reload and merges |
| I02 | Coordinates are finite; width is 120–20,000 and height is 80–20,000. A frame move cannot publish invalid member positions. | Commands and loaded documents |
| I03 | A visible parent is a live frame; frames have no parents. Missing/deleted parents resolve as no parent, so membership is acyclic. | Read model, including concurrent deletes |
| I04 | A visible connector has two distinct, live, non-frame endpoints. Its geometry follows those endpoints. | Read model and world geometry |
| I05 | Saving and loading preserves IDs, title, text, geometry, shapes, membership and relationships. | Supported schema and successful I/O |
| I06 | Replicas receiving the same changes converge, independent of merge order; repeated merges do not change content or heads. | Generated direct merges and reliable-stream sync/reconnect |
| I07 | Invalid local commands reject before modifying document content/history; connector commands cannot mutate node text. | Public editing operations |
| I08 | Closing/opening saves the current board first. A pre-commit session-write failure leaves open/active state intact. Closing all boards preserves saved content. | Workspace operations and injected filesystem failures |
| I09 | Deleting a board preserves its latest content in the recovery archive and keeps selection valid. Restart preserves successfully saved boards and session state. | Successful local filesystem operations |

The generated model tracks created/deleted IDs and relationship endpoints separately
from the implementation. Check invariants after every generated edit. Generate
Unicode text, node/frame creation, moves, resizes, membership changes, connections,
labels, deletions and reloads. Generate partitioned edits on three replicas and
compare direct merges with encoded sync messages after reconnect. Generate board
library operations with temporary storage and restarts. Test exceptional numeric
inputs and failed writes separately rather than relying on their random discovery.

## Bugs exposed and corrections

The first generated runs exposed non-finite coordinates accepted by creation,
connector labeling applied to node text, failed session writes closing boards in
memory, overflowing member positions during frame moves, non-finite saved geometry
accepted at load, and non-finite connector anchors at extreme coordinates.

Creation and edit validation now precede writes; frame movement preflights all
members. Load and filesystem save validate live geometry and text-object types.
Connector labeling requires a live connector, and relationship projection excludes
self-loops, frame endpoints and missing/deleted endpoints. Open/close restore their
previous in-memory state if session persistence fails. Geometry uses f64
intermediates to avoid overflowing finite f32 inputs.

Generated reconnection sequences also exposed `PatchLogMismatch` inside Automerge
0.7.4. A short, focused reproducer (reload, no-op reparent, then merge) failed with
that version and passes with the upgraded 0.12.0 dependency. Keep that regression
and the schema-2 fixture written by 0.7.4; do not replace the fixture with bytes from
the newer dependency. This tests our upgrade path, not every historical file.

## Verification boundary

These are executable, finite tests, not formal proofs, exhaustive model checking
or a verification of Automerge itself. Stored seeds replay generated parameters;
production UUID allocation and actor IDs may still vary. Focused deterministic
reproducers preserve the known bugs independently of generated target ordering. No proof-backed guarantee is claimed. They
do not establish UI correctness, authorization, real-network delivery, crash/power
loss durability, multi-process access safety or atomicity across document/session/
trash files. State failures involving filesystem sync after a successful rename
can have uncertain on-disk outcomes. Transport resource limits and authentication
need their own decisions when networking is introduced. Low-level CRDT mutation is
not a guarded import API; the future transport must validate incoming state and
board identity before exposing it. Text limits apply to local command input, not
to a proven global quota over every possible concurrent text merge.

Later, consider a Rust verification tool for an isolated, stable transition with a
precise contract and a direct connection to production code. A proof of an unrelated
model would not verify the implementation. Record any future proven properties,
assumptions and tool versions separately from tested ones.

## Validation record

2026-11-17: `PROPTEST_CASES=128 PROPTEST_RNG_SEED=20261117 cargo test` passed
all 37 tests (10 generated properties, 128 cases each plus retained seed replay,
and 27 example/interaction/regression tests). Formatting and Clippy with warnings
denied passed. The Linux x86_64 release AppImage was rebuilt and its runtime-version
check passed. This records a finite test run, not a proof or live desktop test.
