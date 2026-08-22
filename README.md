# yankout

A Wayland clipboard picker in the dmenu tradition. What sets it apart
is the exit: entries leave by drag, and a copied file path leaves as a
real file drop. Browser upload zones and chat attachment areas accept
dragged files but not pasted paths; yankout turns the path already
sitting in your clipboard history into the drop those targets want.

## Why a window

A Wayland drag must start from a pointer gesture on a surface owned by
the initiating client, so a daemon cannot add drag to an existing
clipboard manager: something visible has to exist under the cursor.
Terminals cannot be that surface either — there is no escape-sequence
protocol for initiating a drag the way OSC 52 writes the clipboard —
so a toolkit window is unavoidable. yankout makes it the smallest one
possible, in the launcher genre terminal users already accept (dmenu,
fuzzel): spawned by keybind, keyboard-driven, gone on Esc, styled to
match the terminal. The mouse does exactly the one thing Wayland
demands and nothing else.

Browsing and curating history is the terminal's job (fzf over
`cliphist list`, or whatever picker you already use); filter-as-you-type
is part of the launcher contract and stays, pin/delete and the like are
out of scope.

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

## History

`yankout watch` maintains yankout's own clipboard history, one
instance per session. It talks to the compositor directly over
ext-data-control-v1, or the older zwlr-data-control where that is all
there is, and keeps entries under `$XDG_DATA_HOME/yankout/history`
(newest first, deduplicated, capped at 750). Offers marked
`x-kde-passwordManagerHint` are never stored, so password managers
stay out of history. Start it from the compositor config; on niri:

```kdl
spawn-at-startup "yankout" "watch"
```

While a watcher is running, list mode reads this history. Otherwise it
falls back to [cliphist](https://github.com/sentriz/cliphist), which
also covers compositors that offer no data-control protocol at all.
`--backend cliphist|native` forces the choice.

## Requirements

- Wayland with a GTK4-capable session (any compositor; the UI path
  uses no compositor-specific protocols, and the watcher needs
  data-control or a cliphist fallback as described above)
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

The built-in default theme is neutral (monospace, flat, no colors of
its own) and follows the GTK theme in light and dark. To layer your
own on top, put GTK CSS in `~/.config/yankout/style.css`
(`$XDG_CONFIG_HOME` respected); it is picked up automatically. `--css
<file>` loads a different file instead, handy for trying a theme out.
There is no other config file: everything customizable is CSS.

[contrib/warm.css](contrib/warm.css) is a starting point. Stylable
nodes:

| selector       | what it is                        |
|----------------|-----------------------------------|
| `window`       | both mode windows                 |
| `.bar`         | the filter line: `.prompt` label (`>`) and `.filter` entry |
| `.filter placeholder` | the dim "filter" hint       |
| `.history`     | the list view; rows are `.history row`, `.history row:selected` |
| `.empty`       | the "history empty" label         |
| `.puck`        | the puck's single row: `.puck-kind` marker (`file`, `image/png`, …) and `.puck-detail` text |
