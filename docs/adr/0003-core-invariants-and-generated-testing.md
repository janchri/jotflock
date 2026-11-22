# ADR 0003 — Document invariants and generated tests

Status: Implemented.

## Decision

Use focused tests and Proptest sequences to check document, merge and workspace
behavior. Generated tests run 64 cases per property by default, shrink failures
and retain seeds in `proptest-regressions/`.

For a deeper run, use `PROPTEST_CASES=128 PROPTEST_RNG_SEED=20261117 cargo test`.

## Invariants

| ID | Contract |
| --- | --- |
| I01 | Visible IDs equal created IDs minus explicit deletions. Frame deletion keeps members. |
| I02 | Geometry is finite and dimensions stay within document limits. |
| I03 | Visible parents are live frames. Frames have no parents. |
| I04 | Visible connectors join distinct, live, non-frame endpoints. |
| I05 | Successful save/load preserves supported document content. |
| I06 | Replicas receiving the same changes converge. Repeated merges are idempotent. |
| I07 | Invalid editing inputs reject before document mutation. Connector labeling cannot edit nodes. |
| I08 | Open/close saves current edits first and restores in-memory session state on session-write failure. |
| I09 | Successful deletion archives the latest board. Restart restores saved boards and session state. |

## Coverage and limits

Tests cover generated edits, Unicode, reloads, three-replica merges, sync messages,
workspace restarts, concurrent atomic writes and injected filesystem failures.
Validation checks stored geometry before applying story-map layout. Keep focused regressions and
the schema-2 fixture written by Automerge 0.7.4 when upgrading dependencies.

Coverage applies to the tested operations and failure paths. These finite tests
cannot establish crash durability, distributed atomicity or hostile-input safety.
UUIDs and actor IDs can vary during seed replay. Authentication and network limits
are described in ADR 0006. Current validation commands are in the
[app README](../../README.md#validation).

Code — [property_tests.rs](../../src/property_tests.rs),
[fixtures](../../tests/fixtures/README.md).
