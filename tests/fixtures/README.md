# Compatibility fixtures

`schema2-automerge-0.7.4.automerge` was generated using Automerge 0.7.4 before the
0.12 dependency upgrade. It contains an app-schema-2 board with a frame, a Unicode
sticky note assigned to that frame, an ellipse component and a labeled connector.
It uses synthetic content and fixed IDs. Do not regenerate it with a newer library:
its purpose is to check backward loading and a subsequent save/load round trip.
