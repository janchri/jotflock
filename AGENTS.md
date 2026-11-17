# Jotflock working conventions

For every architecture-affecting change, update `docs/adr/README.md` with the current implemented architecture and add or update the relevant numbered ADR in that folder. Describe implemented behavior separately from planned work. Keep schema compatibility, interaction rules and material limitations documented alongside their decisions.

## Commits, documentation, and comments

- Use Conventional Commits, for example `fix(canvas): preserve frame members`.
- Keep documentation simple and concise (KISS).
- Avoid inline code comments. Add one only when absolutely necessary to explain something the code cannot make clear.

# Project identity

Do not include name, URLs, branding, or other direct references to any other whiteboard/collaborative product anywhere under `jotflock/`. This applies to source code, identifiers, comments, documentation, assets and metadata. Use Jotflock terminology and describe features independently. Audit new and changed files for this restriction before finishing work.
