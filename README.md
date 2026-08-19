# yankout

A Wayland clipboard picker in the dmenu tradition. What sets it apart
is the exit: entries leave by drag, and a copied file path leaves as a
real file drop. Browser upload zones and chat attachment areas accept
dragged files but not pasted paths; yankout turns the path already
sitting in your clipboard history into the drop those targets want.

Background and rationale live in [DESIGN.md](DESIGN.md); the milestone
plan in [ROADMAP.md](ROADMAP.md).

## Modes

**List mode** (`yankout`, no arguments) shows recent history entries,
newest first. Type to filter, arrows or Ctrl+j/k (or Ctrl+n/p) to move,
Enter or double-click to recall an entry back to the active clipboard.
The whole window is the drag handle for the selected entry, so there is
no precise pointing at a narrow row. Filter to the entry with hands on
the keyboard, then grab anywhere on the window and pull. Closes on Esc,
focus loss, or a completed drop.

**Puck mode** (`yankout --current`) shows no list, just a postage-stamp
window whose entire surface drags the *live clipboard*. This is what
makes any terminal program a drag source:

```sh
echo ~/report.pdf | wl-copy && yankout --current
```

The puck survives focus loss. Spawn it, click into the target to scroll
its drop zone into view, then come back and drag; only Esc or a
completed drop ends it.

## What a drag delivers

Decided at drag time from the full entry content:

- An absolute path to an existing file or directory (one per line):
  offered as both `text/uri-list` and `text/plain`. File managers and
  browsers take the file; text fields take the path string. Relative
  and stale paths degrade to plain text.
- Binary content: the actual type sniffed from magic bytes
  (`image/png`, …), or `application/octet-stream` when nothing matches.
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
Mod+Shift+C { spawn "yankout"; }
```

## Window rules

A Wayland xdg toplevel cannot position itself, so floating and placing
the windows is the compositor's job. On a tiling compositor a placement
rule is part of installing the tool; ready-made niri rules are in
[contrib/niri.kdl](contrib/niri.kdl). The windows match as app-ids
`dev.yankout.list` and `dev.yankout.puck`. The list window is resizable
and would tile without a rule, while the fixed-size puck floats on its
own under niri's heuristics.

## Theming

The built-in default theme is neutral (monospace, no colors of its own)
and follows the GTK theme in light and dark. Layer your own on top
with:

```sh
yankout --css ~/.config/yankout/theme.css
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
