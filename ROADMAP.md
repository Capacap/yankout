# Roadmap

Milestones toward the design in DESIGN.md. Every milestone ends with
something runnable on the first-deployment machine. Puck mode comes
before list mode deliberately: once the classification core and backend
exist the puck is a day of work and immediately replaces ripdrag, so
real value ships before any list UI exists — and the list mode's
whole-window drag handle then reuses the puck's drag code.

## M0 — Spike

Cargo project, GTK4 window, two throwaway experiments:

- Drag one hardcoded string and one hardcoded file path out to a
  browser and a file manager. Proves the toolchain and the drag path.
- A capture-phase window-level GtkDragSource over a
  `single_click_activate` ListView — the click/drag disambiguation
  that the whole-window handle depends on is the least certain
  mechanism in the design, so it gets prototyped before anything is
  built on it.

Outcome (16 August 2026): done, both spikes pass by hand. String and
union file-path drags land in real targets; the capture-phase handle
coexists with clicks and drags the pressed row. Finding:
`single_click_activate` implies select-on-hover, which would let the
pointer steal the keyboard's selection — so list mode drops that mode
and recall becomes Enter/double-click (folded into DESIGN.md and M4).

## M1 — Classification core

Pure `interpret(content) -> payload` module implementing the drag-time
rules from DESIGN.md, where a payload is a plain MIME-type-to-bytes
mapping (`text/uri-list` assembled by hand, image types sniffed from
magic bytes). No GTK types here — the GdkContentProvider is a thin
byte-backed adapter built later, which keeps this module fully
unit-testable without a display: tempfiles for path-existence cases,
byte-level assertions on the produced payloads.

## M2 — Data sources

The history backend trait — list and decode only — with the cliphist
implementation: parse `cliphist list` for display rows, `cliphist
decode <id>` (id as argv; stdin is fragile about trailing newlines)
for content. Separately, the puck's live-clipboard read via `wl-paste`
— deliberately not on the trait, because history is fed by a watcher
and reading history for the puck races its ingest.

Integration-tested against a scratch database via cliphist's
`-db-path` flag; an in-memory fake serves everything downstream. Exit
criteria include the failure cases: absent or broken cliphist is a
clear stderr error and nonzero exit, empty history is a well-defined
result, empty clipboard is a puck error.

## M3 — Puck mode

`--current` window built on M1+M2: whole surface drags the live
clipboard, exit on drop or Esc, immune to focus loss. Ships with the
niri window-rule that floats and places it — placement is
compositor-side and is most of the puck's UX. Switch the yazi binding,
retire ripdrag. First shipped value.

Outcome (16 August 2026): shipped and validated by hand — DESIGN.md
and a png uploaded to Google Drive by drag, text dragged from helix
into a browser search bar. niri floated and centred the puck without
any window-rule on this machine; contrib/niri.kdl stays as the example
for setups that need it. yazi's Ctrl-n now pipes the selection through
wl-copy into the puck; ripdrag retired.

## M4 — List mode

ListView over the backend: keyboard navigation, filter-as-you-type,
click-to-select with Enter/double-click recall (no
single-click-activate; its select-on-hover steals keyboard selection —
M0 finding), the capture-phase whole-window drag handle, close on Esc
and focus loss, "history empty" row. Daily-drivable picker.

Outcome (19 August 2026): shipped and validated by hand — filter,
Enter/double-click recall, and whole-window drag all work; wired to
Mod+Shift+C with a niri float rule (the resizable list window tiles
without one; the fixed-size puck floats on its own). The modes got
distinct application ids (`dev.yankout.list` / `dev.yankout.puck`) —
under a shared id GApplication uniqueness made one mode re-present the
other, and compositor rules couldn't tell the windows apart. Noted in
validation: binary image entries drag as raw `image/*` bytes, which
file-drop targets refuse — images land as files only via path entries
(see DESIGN.md open questions on temp-file materialization).

## M5 — Fit and finish

`--css` plus the neutral default theme, documented window rules, the
per-row type-marker decision (markers need full decode of visible
rows — see DESIGN.md open questions), and a name decision before
anything gets published.

## M6 — Later

Native `ext-data-control-v1` watcher. Automated drag e2e rig only if
regressions ever justify it.

## Testing strategy

The hard parts need neither a clipboard nor a drag:

- Classification is a pure function over decoded content; unit tests
  with tempfiles.
- Payloads are plain MIME-to-bytes maps assembled by our own code, so
  the interesting assertions (uri-list escaping, multi-file lists,
  sniffed image types) are byte comparisons needing no GTK at all. The
  byte-backed GdkContentProvider adapter is thin enough that its
  in-process interrogation is a smoke test, not a load-bearing one.
- The backend reads a cliphist database, never the live clipboard;
  `-db-path` pointed at a scratch file isolates integration tests
  completely.

What remains on the far side of the provider is GTK's DnD wire code,
not ours: manual smoke testing on niri covers it, keyed off the
observable `drag-end` / `drag-cancel` signals. The clipboard touchpoints
(Enter-to-recall via `wl-copy`, puck read via `wl-paste`) are thin
subprocess calls, likewise smoke-test territory. If automation is ever
wanted, the known rig is a headless wlroots compositor, virtual-pointer
synthesis, and a ~50-line drop-sink app that prints what it receives —
the drop-sink is worth building regardless as a manual test target
showing which MIME type a drop delivered.
