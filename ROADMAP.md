# Roadmap

Milestones toward the design in DESIGN.md. Every milestone ends with
something runnable on the first-deployment machine. Puck mode comes
before list mode deliberately: once the classification core and backend
exist the puck is a day of work and immediately replaces ripdrag, so
real value ships before any list UI exists — and the list mode's
whole-window drag handle then reuses the puck's drag code.

## M0 — Spike

Cargo project, GTK4 window, drag one hardcoded string and one hardcoded
file path out to a browser and a file manager. Proves the toolchain and
the drag path on this machine. Throwaway code allowed.

## M1 — Classification core

Pure `interpret(entry) -> payload` module implementing the drag-time
rules from DESIGN.md, plus payload-to-ContentProvider construction.
Fully unit-tested: tempfiles for path-existence cases, in-process
provider interrogation for MIME output. The heart of the program,
finished before any real UI.

## M2 — cliphist backend

The backend trait, `cliphist list` parsing, `decode` on demand, and a
head-of-clipboard accessor for the puck. Integration-tested against a
scratch database via cliphist's `-db-path` flag; an in-memory fake
serves everything downstream.

## M3 — Puck mode

`--current` window built on M1+M2. Exit on drop or Esc. Switch the yazi
binding, retire ripdrag. First shipped value.

## M4 — List mode

ListView over the backend: keyboard navigation, filter-as-you-type,
Enter-to-recall, whole-window drag handle, close on Esc.
Daily-drivable picker.

## M5 — Fit and finish

`--css` plus the neutral default theme, niri window rule, per-row type
markers, the focus-loss-close decision, and a name decision before
anything gets published.

## M6 — Later

Native `ext-data-control-v1` watcher. Automated drag e2e rig only if
regressions ever justify it.

## Testing strategy

The hard parts need neither a clipboard nor a drag:

- Classification is a pure function; unit tests with tempfiles.
- A GdkContentProvider can be interrogated in-process — build it,
  request `text/uri-list` bytes, assert — no pointer, no drop target,
  no compositor.
- The backend reads a cliphist database, never the live clipboard;
  `-db-path` pointed at a scratch file isolates integration tests
  completely.

What remains on the far side of the provider is GTK's DnD wire code,
not ours: manual smoke testing on niri covers it. The one clipboard
write (Enter-to-recall, a `wl-copy` invocation) is likewise smoke-test
territory. If automation is ever wanted, the known rig is a headless
wlroots compositor, virtual-pointer synthesis, and a ~50-line drop-sink
app that prints what it receives — the drop-sink is worth building
regardless as a manual test target showing which MIME type a drop
delivered.
