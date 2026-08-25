# yankout-core

The library behind [yankout](https://crates.io/crates/yankout), a
Wayland clipboard manager whose entries leave by drag. It carries the
parts that need no GTK, from the clipboard history store and watcher
to drag-payload classification.

It is published because crates.io requires it, not as a stable API;
it is versioned in lockstep with the binary and changes whenever the
binary needs it to. Start at the
[yankout README](https://github.com/Capacap/yankout).
