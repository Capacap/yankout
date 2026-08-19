# clipdrag

A Wayland clipboard picker in the dmenu tradition whose defining
feature is that entries can be *dragged* out — with file paths
converted to real file drops at drag time. Browser upload zones, chat
attachment areas and file managers accept dragged files but not pasted
paths; clipdrag turns the path already sitting in your clipboard
history into the drop those targets want.

Background and rationale live in [DESIGN.md](DESIGN.md); the milestone
plan in [ROADMAP.md](ROADMAP.md).

## Modes

**List mode** (`clipdrag`, no arguments) shows recent history entries,
newest first. Type to filter, arrows or Ctrl+j/k (or Ctrl+n/p) to move,
Enter or double-click to recall an entry back to the active clipboard.
The whole window is the drag handle for the selected entry: filter to
it with hands on the keyboard, then grab anywhere on the window and
pull. Closes on Esc, focus loss, or a completed drop.

**Puck mode** (`clipdrag --current`) shows no list: a postage-stamp
window whose entire surface drags the *live clipboard*. This is what
makes any terminal program a drag source:

```sh
echo ~/report.pdf | wl-copy && clipdrag --current
```

The puck survives focus loss — spawn it, click into the target to
scroll its drop zone into view, come back and drag. Only Esc or a
completed drop end it.

## What a drag delivers

Decided at drag time from the full entry content:

- An absolute path to an existing file or directory (one per line):
  `text/uri-list` and `text/plain` as a union — file managers and
  browsers take the file, text fields take the path string. Relative
  and stale paths degrade to plain text.
- Binary content: the sniffed actual type (`image/png`, …), or
  `application/octet-stream` if unrecognized.
- Everything else: plain text.

## Requirements

- Wayland with a GTK4-capable session (any compositor; no
  compositor-specific protocols are used)
- [cliphist](https://github.com/sentriz/cliphist) with its watchers
  running, as the history backend
- [wl-clipboard](https://github.com/bugaevc/wl-clipboard) (`wl-paste`
  for the puck, `wl-copy` for recall)

## Install

```sh
cargo install --path .
```

Then bind it. niri example (use a full path if `~/.cargo/bin` is not
on the session's PATH):

```kdl
Mod+Shift+C { spawn "clipdrag"; }
```

## Window rules

A Wayland xdg toplevel cannot position itself, so floating and placing
the windows is the compositor's job — a placement rule is part of
installing the tool on a tiling compositor. Ready-made niri rules are
in [contrib/niri.kdl](contrib/niri.kdl); the two windows match as
app-ids `dev.clipdrag.list` and `dev.clipdrag.puck`. The list window is
resizable and tiles without a rule; the fixed-size puck floats on its
own under niri's heuristics.

## Theming

A neutral default theme ships built in (monospace, no colors of its
own, follows the GTK light/dark palette). Layer your own on top with:

```sh
clipdrag --css ~/.config/clipdrag/theme.css
```

[contrib/warm.css](contrib/warm.css) is a starting point. Stylable
nodes:

| selector       | what it is                        |
|----------------|-----------------------------------|
| `window`       | both mode windows                 |
| `.history`     | the list view; rows are `.history row` |
| `searchentry`  | the filter field                  |
| `.empty`       | the "history empty" label         |
| `.puck-kind`   | the puck's type line (`file`, `image/png`, …) |
| `.puck-detail` | the puck's preview/size line      |
