# yankout

A Wayland clipboard picker in the dmenu tradition whose defining feature
is that entries can be dragged out, with file paths converted to real
file drops at drag time. Named for the workflow: yank in the terminal,
drag it out into GUI-land (settled 19 August 2026 — clipdrag, the
original working name, collides with an ICLR 2025 drag-based diffusion
editing project, and the yank-* namespace was empty).

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
rather than as apps. yankout adopts that contract wholesale:

- Spawned by keybind, ephemeral, gone on Esc; list mode also closes on
  focus loss. The puck is exempt from focus-loss close — see below.
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
  the window closes after a successful drop (drop success is observable
  from the source side: `drag-end` fires on success, `drag-cancel`
  first on failure).
- Enter (or double-click): the entry is promoted back to the active
  clipboard, the standard clipboard-manager recall action. A single
  click only selects.

The drag target is not the row. Keyboard owns selection, and the whole
window is the drag handle for whatever entry is selected: filter to the
entry with hands on the keyboard, then grab anywhere on the window from
wherever the pointer happens to be and pull. No precise pointing at a
20-pixel row. Selection moves only by keyboard or an explicit click on
a row — the list deliberately avoids GTK's single-click-activate mode,
whose select-on-hover (an M0 finding) would let the pointer steal the
keyboard's selection on its way to grabbing the window. A press on a
row selects that row before the drag threshold is crossed, so a drag
that starts on a row drags that row. There are no per-row drag
sources; the window-level handle (capture phase, see Stack) is the
only one.

Empty history shows the window with a "history empty" row rather than
failing; a missing or broken backend is a clear error on stderr and a
nonzero exit.

Deployments that already have a paste picker can treat the window as
drag-only and keep using their picker; nothing in the design requires
this program to be the only recall path.

### Puck mode

`yankout --current` (flag name open) shows no list at all: a
postage-stamp window whose entire surface drags the *live clipboard* —
read directly (`wl-paste` or equivalent), not through the history
backend, because history is fed by a watcher and `wl-copy && yankout
--current` would race its ingest. Same drag-time rules, exit on drop or
Esc. Selection happens wherever it already happened — the terminal:

    # raw bash grows drag support
    echo ~/report.pdf | wl-copy && yankout --current

    # yazi: yank already writes the clipboard; one key spawns the puck

The puck must survive focus loss: the working rhythm is spawn puck,
click into the target application to scroll its drop zone into view,
then come back and drag. Focus-loss close would kill the puck
mid-workflow, so only Esc and a completed drop end it. An empty
clipboard is an error and a nonzero exit, not an empty puck.

A Wayland xdg toplevel cannot position itself, so where the puck
appears is entirely the compositor's decision. That is most of the
puck's UX; the first deployment ships the niri window-rule that floats
and places it, and the README should say plainly that a placement rule
is part of installing the tool.

The terminal keeps everything terminal users care about — browsing,
searching, scripting, composability — and the GUI degenerates to the
irreducible minimum, a gesture surface.

Architecturally the puck is nearly free: both modes share the
drag-time interpretation and the content-provider code. The puck is
the list window with one implicit entry, no list, and a different
close policy.

## Drag-time interpretation

The core design point. History stores content as text or binary; what a
drop target needs depends on what the entry *is*, and that is decided
at drag time, not at store time. Classification always runs on the
full decoded content — list previews are truncated and
newline-collapsed, fine for display, useless as classifier input.

- Entry is a single line that, after trimming and expanding a leading
  `~`, is an *absolute* path to an existing file or directory: offer
  `text/uri-list` and `text/plain` as a union, receiver picks. File
  managers and browsers take the file; text fields take the path
  string. Relative paths never classify as files — spawned from a
  keybind the cwd is meaningless, and a copied word like `Documents`
  must not silently become a file drop.
- Multi-line entry where every line passes the same test: multi-file
  `text/uri-list`. (A filename that itself contains a newline is
  indistinguishable from two lines; it degrades to text, accepted.)
- Path that no longer exists: plain text. Stale history degrades to
  what it literally is instead of erroring.
- Binary entry: sniff the magic bytes and offer the *actual* type
  (`image/png`, `image/jpeg`, …) — the store keeps whatever bytes the
  source offered, so png must be detected, not assumed. Unrecognized
  binary is offered as `application/octet-stream`.
- Everything else: plain text.

## History backend

Open in the details, settled in the shape: the UI should not care where
history comes from.

- External store, cliphist being the obvious first backend (`cliphist
  list` for display rows, `cliphist decode <id>` on demand — id passed
  as argv, since decode via stdin is fragile about trailing newlines).
- Own watcher via the data-control protocol family (`zwlr-data-control`,
  and its successor `ext-data-control-v1`), making the program
  self-contained. Not available on every compositor, which is an
  argument for keeping the external-store path alive rather than
  replacing it.

Start with the cliphist backend behind a thin trait — list and decode,
nothing more; add the native watcher when the UI is proven. The puck's
live-clipboard read is deliberately not on this trait: it is a
different data source with different freshness semantics.

## Scope

The dmenu framing settles how much manager belongs in scope: little. In
the TUI-centric deployment that motivates this program, browsing and
searching history is the terminal's job (fzf over `cliphist list`, or
an existing picker like fuzzel), and yankout's identity is the thing
that turns clipboard content into a drag. Filter-as-you-type in list
mode is part of the launcher contract and stays; pin, delete and other
curation verbs are not launch scope and may never be.

## Portability

The window is a plain xdg toplevel. No layer-shell, no
compositor-specific protocols in the UI path; tiling compositors float
and place it by user rule (placement rules ship as documented examples,
starting with niri), stacking desktops treat it as the small utility
window it is. Styling ships with a neutral default theme plus a `--css
<file>` flag so any deployment can match its desktop without wrapper
hacks (the flag exists because retrofitting exactly this onto ripdrag
required a scoped `XDG_CONFIG_HOME` workaround).

## Relation to ripdrag

Puck mode replaces ripdrag outright, and more cleanly: ripdrag needed
paths on argv, the puck needs nothing because the clipboard already
carries the selection. This machine's yazi binding becomes yank plus
spawn-puck. An argv/stdin path mode for strict drop-in compatibility is
possible but probably unnecessary once the puck exists.

## First deployment (this machine)

niri, cliphist watchers already running, fuzzel as the existing paste
picker. List mode runs drag-only beside fuzzel; puck mode takes over the
yazi binding from ripdrag. Both float via a niri window-rule like the
ripdrag and askpass panels — the puck's rule also places it — and take
the warm palette through `--css`. None of this is assumed anywhere
above.

## Stack

Rust with gtk4-rs. GTK4 targets Wayland cleanly, and the stack is
proven on the first-deployment machine by ripdrag. GtkListView for the
rows. The whole-window drag handle is a single GtkDragSource on the
window at *capture* propagation phase — at the default bubble phase the
ListView claims the press and a window-level source never fires. Its
content provider is returned from the `prepare` signal (by `drag-begin`
the offered formats are already locked), built lazily from the
currently selected entry so decoding only runs for the entry actually
dragged. The capture-phase claim was the design's riskiest mechanism;
the M0 spike validated it — clean clicks reach the list, press-and-pull
starts a drag. The same spike showed that GTK's single-click-activate
implies select-on-hover, which is why recall is Enter/double-click
rather than single click and the list does not use that mode.

Classification and payload assembly stay plain Rust producing MIME-type
to bytes mappings (`text/uri-list` built by hand); the GTK content
provider is a thin byte-backed adapter over that, which keeps the core
testable without a display.

Estimated size a few hundred lines for the cliphist-backed core with
both modes; the native watcher grows it from there. The remaining parts
(content providers, list rows) are well-trodden gtk4-rs ground.

## Open questions

- Puck flag name and appearance: `--current`? What does the puck show —
  a type icon, a truncated preview of the entry, both?
- Entry count and window height in list mode. Ten entries? Scroll or
  hard cap?
- Per-row type markers (file / files / image / text) would tell you
  what a drop will produce before dragging, but classification needs
  full content, so markers mean decoding every visible row rather than
  reusing the list preview. Lazy-decode visible rows only, or skip
  markers?
- Image thumbnails in rows, or text labels only?
- Binary entries drag as raw typed bytes (`image/png`, …), which only
  targets that accept image MIME on drop can take; upload zones and
  file managers want `text/uri-list`, i.e. a real file. Materialize
  binary entries to a temp file at drag time and offer a uri-list
  alongside the bytes? Opens temp-file lifetime and naming questions.
