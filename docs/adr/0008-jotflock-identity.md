# ADR 0008 — Jotflock project identity

Jotflock uses a bold violet **jot** and regular flock wordmark. The executable,
desktop launcher, AppImage, configuration variables and invitation scheme use
the Jotflock name.

The invitation signature domains and transport identifier use the new name.
Owners issue fresh invitations using their existing device keys. New grants are
saved under `network/access/`. Previous records retain the local board's
authority binding so a returning editor can rejoin the same board with a new
grant. Previous invitation links and grants cannot be used for admission.
The board document format and device key file retain their existing formats.
