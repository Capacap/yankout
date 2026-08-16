# clipdrag

A drag source for clipboard history. Working name; see open questions.

Idea from August 2026, while unifying drag and clipboard flows on the niri
desktop. Parked here until the project gets picked up.

## The gap

The desktop already covers most data-movement flows with three pieces.
cliphist records everything that passes through the clipboard (text and image
watchers in niri's config.kdl). wofi on Mod+C recalls an entry and pastes it.
ripdrag, bound to Ctrl+n in yazi, turns the current file selection into a
Wayland drag for browser upload zones and other drop-only targets.

The uncovered flow is dragging something that is no longer in front of you. A
path yanked with yazi's `cc` ten minutes ago sits in cliphist as text, and
recalling it gives a paste, never a drop. Drop-only targets (a browser upload
zone, most chat attachment areas) are unreachable from history. That is the
whole reason this program should exist.

A daemon cannot fix this. A Wayland drag must start from a pointer gesture on
a surface owned by the initiating client, so something visible has to exist
under the cursor when the drag begins. The history window is that surface.

## Behaviour

Launch shows a small floating window listing recent cliphist entries, newest
first. Press-and-hold on a row starts a drag; drop lands it in the target and
the window closes itself (ripdrag's `--and-exit` behaviour). Escape closes
without action.

Drag-only, deliberately. Recall-and-paste already belongs to the wofi menu,
and duplicating it here would make two half-clipboard-managers. If later use
shows the split is annoying in practice, revisit.

## Drag-time interpretation

The core design point. cliphist stores content as text or binary; what a drop
target needs depends on what the entry *is*, and that is decided at drag
time, not at store time:

- Entry is an existing file path (after trimming): offer `text/uri-list`
  and `text/plain` as a union, receiver picks. File managers and browsers
  take the file; text fields take the path string.
- Multi-line entry where every line is an existing path: multi-file
  `text/uri-list`.
- Path that no longer exists: plain text. Stale history degrades to what it
  literally is instead of erroring.
- Binary image entry (cliphist shows these as `[[ binary data ... ]]`):
  decode via `cliphist decode` and offer `image/png`.
- Everything else: plain text.

## Fit with the desktop

Same family as the ripdrag window and the kitty askpass prompt: a small
floating panel, app-id matched by a niri window-rule, closed the moment its
job is done. Because the app is ours, the palette ships in its own CSS.
No scoped-XDG_CONFIG_HOME workaround, which exists only because ripdrag
cannot be told where to find a stylesheet.

Warm palette, Source Code Pro at 9pt, brightness hierarchy, no icons. Image
entries may show thumbnails; under the two-tier palette rule thumbnails are
content, not chrome, so their colour is not a violation.

## Possible ripdrag unification

If the program also accepts paths on argv and stdin, it is a superset of how
yazi uses ripdrag today, and the Ctrl+n binding could point here instead.
One drag program, two feeders (yazi for files in view, cliphist for history),
and ripdrag plus its cargo install and scoped-CSS machinery retire. Not a
launch requirement. Build the history mode first, decide with both in hand.

## Stack

Rust with gtk4-rs, the stack ripdrag proved out on this machine
(`gtk4-devel` is installed, cargo is the sanctioned toolchain here).
GtkListView rows each carry a GtkDragSource controller; the content provider
is built lazily in the drag-begin handler so `cliphist decode` only runs for
the entry actually dragged. Data source is `cliphist list` (TSV, id and
preview). No daemon, no state of its own.

Estimated size a few hundred lines. The hard parts (drag sources, content
providers, list rows) are all well-trodden gtk4-rs ground.

## Open questions

- Name. clipdrag is descriptive but flat. draghist? yankout?
- Keybinding and launcher. Mod+Shift+C beside the Mod+C menu?
- Entry count and window height. Ten entries? Scroll or hard cap?
- Should rows show a type marker (file / files / image / text) so you know
  what a drop will produce before dragging?
- Filter mode that lists only file-path entries, since drop targets mostly
  want files?
