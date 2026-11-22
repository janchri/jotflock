# ADR 0008 — Jotflock identity

Status: Implemented.

## Decision

Use two marks.

- **Full wordmark** — bold violet **jot**, regular dark flock, then three dots
  matching flock's color. Used in desktop Home and the website header and footer.
- **Compact mark** — a violet lowercase **j** with three blue flock dots.
  Used for the desktop window, Linux launcher and website favicon.

The three dots represent the flock and belong with the wordmark.
Beige surfaces, violet primary accents and blue secondary accents form the palette.

## Names and compatibility

Executables, desktop registration, environment variables and `jotflock://join/`
links use the Jotflock name. Invitation signatures and the transport identifier
also use this identity.

After the identity change, existing device keys and board files remain usable.
Fresh invitations are required. Current membership records live in
`network/access/`. Previous records retain the board's authority root so a
returning device can rejoin with a fresh grant. Previous links and grants cannot
authorize admission.

Code — [wordmark](../../src/app_ui.rs), [compact SVG](../../assets/jotflock.svg),
[sharing.rs](../../src/sharing.rs).
