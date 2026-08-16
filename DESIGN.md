# clipdrag

A Wayland clipboard picker in the dmenu tradition whose defining feature
is that entries can be dragged out, with file paths converted to real
file drops at drag time. Working name; see open questions.

Idea from August 2026. Assumes Wayland. Assumes nothing about the
compositor.

## The gap

Every clipboard manager offers recall-and-paste. None of the common
Wayland ones (cliphist, CopyQ aside, clipman) let you *drag* a history
entry, and paste is the wrong verb for a growing class of targets.
Browser upload zones, chat attachment areas and similar drop targets
accept dragged files but not pasted paths; a file path copied earlier
sits in history as text that these targets cannot use.

A daemon cannot fix this. A Wayland drag must start from a pointer
gesture on a surface owned by the initiating client, so something
visible has to exist under the cursor when the drag begins. The history
window is that surface. That constraint is also why the feature belongs
in the clipboard manager itself rather than in a helper bolted onto one.

## Why a GUI at all

The motivating workflow is terminal-oriented: file exploration in yazi,
selection in fzf, plain bash. Terminals forward mouse events to TUI
apps, but no terminal implements an escape-sequence protocol for
*initiating* a drag on the app's behalf — OSC 52 covers "write to the
clipboard" and has no DnD analogue. A TUI can therefore never be the
drag source, and patching individual terminals is a per-emulator rabbit
hole with no standard behind it.

So a toolkit window must exist for the gesture. The design goal is to
make that window as small and as keyboard-shaped as possible: the
clipboard is the bridge that lets any terminal program — yazi, fzf, raw
bash — produce draggable content without itself being a Wayland client.

## The genre

Terminal users already accept exactly one kind of GUI without
complaint: dmenu, rofi, wofi, fuzzel. Toolkit programs all, but they
follow the launcher contract and so read as extensions of the keyboard
rather than as apps. clipdrag adopts that contract wholesale:

- Spawned by keybind, ephemeral, gone on Esc or focus loss.
- Keyboard-driven end to end: arrows or j/k to move, type to filter,
  Enter to act.
- Flat, monospace-friendly styling that themes to match the terminal
  palette.
- Instant startup; no persistent state, no tray, no daemon of its own.

The mouse is required for exactly one thing — the gesture Wayland
demands — and for nothing else.

## Two modes, one binary

### List mode (default)

Launch shows a small window listing recent history entries, newest
first. Two verbs per row:

- Drag: the entry leaves as a drag, interpreted as described below, and
  the window closes after a successful drop.
- Enter (or click): the entry is promoted back to the active clipboard,
  the standard clipboard-manager recall action.

The drag target is not the row. Keyboard owns selection, and the whole
window is the drag handle for whatever entry is selected: filter to the
entry with hands on the keyboard, then grab anywhere on the window from
wherever the pointer happens to be and pull. No precise pointing at a
20-pixel row. Dragging a specific row directly still works for
mouse-first users.

Deployments that already have a paste picker can treat the window as
drag-only and keep using their picker; nothing in the design requires
this program to be the only recall path.

### Puck mode

`clipdrag --current` (flag name open) shows no list at all: a
postage-stamp window whose entire surface drags the *current clipboard
head*, interpreted by the same drag-time rules, exiting on drop or Esc.
Selection happens wherever it already happened — the terminal:

    # raw bash grows drag support
    echo ~/report.pdf | wl-copy && clipdrag --current

    # yazi: yank already writes the clipboard; one key spawns the puck

The terminal keeps everything terminal users care about — browsing,
searching, scripting, composability — and the GUI degenerates to the
irreducible minimum, a gesture surface.

Architecturally the puck is nearly free: both modes share the backend,
the drag-time interpretation and the content-provider code. The puck is
the list window with one implicit entry and no list.

## Drag-time interpretation

The core design point. History stores content as text or binary; what a
drop target needs depends on what the entry *is*, and that is decided at
drag time, not at store time:

- Entry is an existing file path (after trimming): offer `text/uri-list`
  and `text/plain` as a union, receiver picks. File managers and
  browsers take the file; text fields take the path string.
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
  self-contained. Not available on every compositor, which is an
  argument for keeping the external-store path alive rather than
  replacing it.

Start with the cliphist backend behind a thin trait; add the native
watcher when the UI is proven. Puck mode needs only the head of the
clipboard, which the same trait can serve.

## Scope

The dmenu framing settles how much manager belongs in scope: little. In
the TUI-centric deployment that motivates this program, browsing and
searching history is the terminal's job (fzf over `cliphist list`, or
an existing picker like wofi), and clipdrag's identity is the thing
that turns clipboard content into a drag. Filter-as-you-type in list
mode is part of the launcher contract and stays; pin, delete and other
curation verbs are not launch scope and may never be.

## Portability

The window is a plain xdg toplevel. No layer-shell, no
compositor-specific protocols in the UI path; tiling compositors float
it by user rule, stacking desktops treat it as the small utility window
it is. Styling ships with a neutral default theme plus a `--css <file>`
flag so any deployment can match its desktop without wrapper hacks (the
flag exists because retrofitting exactly this onto ripdrag required a
scoped `XDG_CONFIG_HOME` workaround).

## Relation to ripdrag

Puck mode replaces ripdrag outright, and more cleanly: ripdrag needed
paths on argv, the puck needs nothing because the clipboard already
carries the selection. This machine's yazi binding becomes yank plus
spawn-puck. An argv/stdin path mode for strict drop-in compatibility is
possible but probably unnecessary once the puck exists.

## First deployment (this machine)

niri, cliphist watchers already running, wofi as the existing paste
picker. List mode runs drag-only beside wofi; puck mode takes over the
yazi binding from ripdrag. Both float via a niri window-rule like the
ripdrag and askpass panels, and take the warm palette through `--css`.
None of this is assumed anywhere above.

## Stack

Rust with gtk4-rs. GTK4 targets Wayland cleanly, and the stack is
proven on the first-deployment machine by ripdrag. GtkListView for the
rows; a GtkDragSource on the window itself implements the whole-window
drag handle, building its content provider lazily in the drag-begin
handler from the currently selected entry, so decoding only runs for
the entry actually dragged. Rows may carry their own DragSource too for
direct mouse-first drags.

Estimated size a few hundred lines for the cliphist-backed core with
both modes; the native watcher grows it from there. The hard parts
(drag sources, content providers, list rows) are well-trodden gtk4-rs
ground.

## Open questions

- Name. clipdrag is descriptive but flat. draghist? yankout?
- Puck flag name and appearance: `--current`? What does the puck show —
  a type icon, a truncated preview of the entry, both?
- Entry count and window height in list mode. Ten entries? Scroll or
  hard cap?
- Should rows show a type marker (file / files / image / text) so you
  know what a drop will produce before dragging? Cheap, since the
  drag-time classification logic exists anyway.
- Image thumbnails in rows, or text labels only?
- Close on focus loss: part of the launcher contract, but does it fight
  with drag interaction on any compositor?
