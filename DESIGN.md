# clipdrag

A general-purpose Wayland clipboard manager whose defining feature is that
history entries can be dragged out, with file paths converted to real file
drops at drag time. Working name; see open questions.

Idea from August 2026. Parked here until the project gets picked up.

Assumes Wayland. Assumes nothing about the compositor.

## The gap

Every clipboard manager offers recall-and-paste. None of the common Wayland
ones (cliphist, CopyQ aside, clipman) let you *drag* a history entry, and
paste is the wrong verb for a growing class of targets. Browser upload
zones, chat attachment areas and similar drop targets accept dragged files
but not pasted paths; a file path copied earlier sits in history as text
that these targets cannot use.

A daemon cannot fix this. A Wayland drag must start from a pointer gesture
on a surface owned by the initiating client, so something visible has to
exist under the cursor when the drag begins. The history window is that
surface. That constraint is also why the feature belongs in the clipboard
manager itself rather than in a helper bolted onto one.

## Behaviour

Launch shows a small window listing recent history entries, newest first.
Two verbs per row:

- Press-and-hold and drag: the entry leaves as a drag, interpreted as
  described below, and the window closes after a successful drop.
- Click (or Enter): the entry is promoted back to the active clipboard,
  the standard clipboard-manager recall action.

Escape closes without action. Deployments that already have a paste picker
can treat the window as drag-only and keep using their picker; nothing in
the design requires this program to be the only recall path.

## Drag-time interpretation

The core design point. History stores content as text or binary; what a
drop target needs depends on what the entry *is*, and that is decided at
drag time, not at store time:

- Entry is an existing file path (after trimming): offer `text/uri-list`
  and `text/plain` as a union, receiver picks. File managers and browsers
  take the file; text fields take the path string.
- Multi-line entry where every line is an existing path: multi-file
  `text/uri-list`.
- Path that no longer exists: plain text. Stale history degrades to what
  it literally is instead of erroring.
- Binary image entry: offer `image/png`.
- Everything else: plain text.

## History backend

Open in the details, settled in the shape: the UI should not care where
history comes from.

- External store, cliphist being the obvious first backend (`cliphist
  list` for rows, `cliphist decode` on demand). Cheapest path to working
  software and matches setups that already run cliphist watchers.
- Own watcher via the data-control protocol family (`zwlr-data-control`,
  and its successor `ext-data-control-v1`), making the program
  self-contained. Not available on every compositor, which is an argument
  for keeping the external-store path alive rather than replacing it.

Start with the cliphist backend behind a thin trait; add the native
watcher when the UI is proven.

## Portability

The window is a plain xdg toplevel. No layer-shell, no compositor-specific
protocols in the UI path; tiling compositors float it by user rule, stacking
desktops treat it as the small utility window it is. Styling ships with a
neutral default theme plus a `--css <file>` flag so any deployment can
match its desktop without wrapper hacks (the flag exists because retrofitting
exactly this onto ripdrag required a scoped `XDG_CONFIG_HOME` workaround).

## Relation to ripdrag

With paths accepted on argv and stdin, the program is a superset of ripdrag
as used from file managers, and this machine's yazi binding could point here
instead. One drag program, two feeders (file manager for files in view,
history for everything else). Not a launch requirement; build the history
mode first and decide with both in hand.

## First deployment (this machine)

niri, cliphist watchers already running, wofi as the existing paste picker.
Here the window would run drag-only beside wofi, float via a niri
window-rule like the ripdrag and askpass panels, and take the warm palette
through `--css`. None of this is assumed anywhere above.

## Stack

Rust with gtk4-rs. GTK4 targets Wayland cleanly, and the stack is proven on
the first-deployment machine by ripdrag. GtkListView rows each carry a
GtkDragSource controller; the content provider is built lazily in the
drag-begin handler so decoding only runs for the entry actually dragged.

Estimated size a few hundred lines for the cliphist-backed drag-only core;
the native watcher and richer management grow it from there. The hard parts
(drag sources, content providers, list rows) are well-trodden gtk4-rs
ground.

## Open questions

- Name. clipdrag is descriptive but flat. draghist? yankout?
- How much manager beyond recall belongs in scope? Delete entry, pin
  entry, clear history, search-as-you-type?
- Entry count and window height. Ten entries? Scroll or hard cap?
- Should rows show a type marker (file / files / image / text) so you know
  what a drop will produce before dragging?
- Filter mode that lists only file-path entries, since drop targets mostly
  want files?
- Image thumbnails in rows, or text labels only?
