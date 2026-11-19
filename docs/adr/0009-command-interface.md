# ADR 0009 — Application commands and headless mode

The desktop command bar and shell CLI use the same command dispatcher. Desktop
controls use the same application and document operations. A drag updates the
document while the pointer moves and records an absolute move or resize command
when released. Scrolling and pinching use the canvas zoom operation with a local
anchor. The GUI keeps a bounded in-memory transcript with the latest commands.

A profile has one owner at a time. A desktop or headless owner accepts argument
arrays on the existing private Unix socket and serializes them with the other
application events. Command clients use that owner's live document, selection,
viewport and sharing session. The socket still accepts forwarded desktop
invitation links. If no owner is running, a CLI process takes the profile lock,
executes one local command, and saves its edits before exiting. This mode skips
network startup. A headless owner runs the same workspace, autosave and network
loop as the desktop without creating a window or rendering previews.

Local clients can also send a UTF-8 JSON array of argument strings directly
to `app.sock` and close their write half to mark the end of the request. Each
connection carries one command and one response. The executable handles profile
locking and the offline fallback for shell clients.

Commands identify boards, cards, connectors and story map frames by their
existing IDs. Output has a JSON success or failure envelope. Board and network
schemas are unchanged. Store and network limits still apply. Document writes
from the CLI are saved locally before success, and synchronization runs in
the background. Read commands can observe unsaved changes from the GUI because
they read the running workspace. If an edit applies in memory but saving fails,
the error says so and autosave can retry the local save.

The command set covers board management, canvas edits, templates, story map
axes, view and selection changes, editor settings and sharing. OS clipboard and
widget focus stay in the desktop. Headless sharing, view state and command
history last for the lifetime of the process. Command history is a recent
activity log. Commands that create objects allocate new IDs when run again.
