# yankout

A Wayland clipboard picker in the dmenu tradition, except entries
leave by drag, and a copied file path leaves as a real file drop.
Browser upload zones and chat attachment areas accept dragged files
but not pasted paths; yankout turns the path already sitting in your
clipboard history into the drop those targets want.

The window exists because Wayland demands one. A drag must start from
a pointer gesture on a surface owned by the dragging client, so no
daemon or terminal escape sequence can do this. yankout keeps that
window in the launcher shape terminal users already accept: spawned by
keybind, keyboard-driven, gone on Esc. Filter-as-you-type is in;
browsing and curating history stays in the terminal, where `yankout
list` pipes into fzf, fuzzel, or whatever picker you already use.

## Usage

**`yankout`** shows recent history, newest first. Type to filter,
arrows or Ctrl+j/k to move, Enter to recall an entry to the active
clipboard. The whole window is the drag handle for the selected entry,
so filter with hands on the keyboard, then grab anywhere and pull.
Closes on Esc, focus loss, or a completed drop.

**`yankout --current`** skips the list and shows a postage-stamp
window whose entire surface drags the live clipboard, which makes any
terminal program a drag source:

```sh
echo ~/report.pdf | wl-copy && yankout --current
```

The puck survives focus loss. Spawn it, click into the target to bring
its drop zone into view, then come back and drag; only Esc or a
completed drop ends it.

What a drag delivers is decided at drag time. Absolute paths to
existing files (one per line) are offered as `text/uri-list` plus
`text/plain`, so file managers and browsers take the file while text
fields take the path string; relative and stale paths degrade to plain
text. Binary content is offered as the type sniffed from its magic
bytes, and everything else as plain text.

## History

`yankout watch` maintains the history. Start it once per session from
the compositor config (`spawn-at-startup "yankout" "watch"` on niri);
it listens on ext-data-control-v1 (or the older zwlr variant) and
keeps entries under `$XDG_DATA_HOME/yankout/history`, newest first,
deduplicated, capped at 750. Offers marked `x-kde-passwordManagerHint`
are never stored. On compositors with no data-control protocol,
[cliphist](https://github.com/sentriz/cliphist) fills in if installed;
`--backend cliphist|native` forces the choice.

The history is also readable from the terminal, in cliphist's shape so
existing pickers switch by renaming the command:

```sh
yankout list        # <id>TAB<preview> per line, newest first
yankout decode 42   # raw content of one entry
yankout list | fuzzel --dmenu | yankout decode | wl-copy
```

## Installation

Requires a GTK4-capable Wayland session (the UI path uses no
compositor-specific protocols) and
[wl-clipboard](https://github.com/bugaevc/wl-clipboard).

```sh
cargo install --path .
```

Bind it to a key, and on a tiling compositor add a placement rule: a
Wayland window cannot position itself, so floating the picker is the
compositor's job. Ready-made niri rules and binds are in
[contrib/niri.kdl](contrib/niri.kdl); the windows match as app-ids
`dev.yankout.list` and `dev.yankout.puck`.

## Theming

The built-in theme is neutral monospace and follows the GTK light and
dark preference. Put GTK CSS in `~/.config/yankout/style.css` to layer
your own on top; `--css <file>` tries a theme without installing it.
There is no other configuration file, everything customizable is CSS.
[contrib/warm.css](contrib/warm.css) is a starting point. Stylable
nodes:

| selector              | what it is                                  |
|-----------------------|---------------------------------------------|
| `window`              | both mode windows                           |
| `.bar`                | filter line: `.prompt` label, `.filter` entry |
| `.filter placeholder` | the dim "filter" hint                       |
| `.history`            | the list; rows are `.history row`, `.history row:selected` |
| `.age`                | relative-age label on each row (prefer a muted `color` over `opacity`) |
| `.empty`              | the "history empty" label                   |
| `.puck`               | the puck row: `.puck-kind` marker, `.puck-detail` text |

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE)
or [MIT license](LICENSE-MIT) at your option. Unless you explicitly
state otherwise, any contribution intentionally submitted for
inclusion in the work by you shall be dual licensed as above, without
any additional terms or conditions.
