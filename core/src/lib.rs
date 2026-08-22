pub mod clipboard;
pub mod history;
pub mod mime;
pub mod payload;
pub mod store;
pub mod terminal;
pub mod watch;

use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A filesystem or pipe operation failed; `doing` names it.
    #[error("{doing}: {source}")]
    Io {
        doing: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{0} not found on PATH")]
    MissingProgram(String),
    /// A helper program ran and reported failure.
    #[error("{program} failed: {detail}")]
    Program { program: String, detail: String },
    #[error("no entry with id {0}")]
    NoSuchEntry(String),
    #[error("decode needs an id, as an argument or on stdin")]
    MissingId,
    #[error("clipboard is empty")]
    ClipboardEmpty,
    #[error("clipboard holds nothing draggable")]
    NothingDraggable,
    #[error("neither XDG_DATA_HOME nor HOME is set")]
    NoDataDir,
    #[error("store {} is locked, another watcher is already running", .0.display())]
    StoreLocked(PathBuf),
    #[error("selection exceeds {0} bytes, skipped")]
    SelectionTooLarge(usize),
    #[error("source stalled for {0:?}, selection skipped")]
    SourceStalled(Duration),
    #[error("compositor advertised no wl_seat")]
    NoSeat,
    #[error(
        "compositor offers neither ext-data-control-v1 nor zwlr-data-control; \
         use cliphist's wl-paste watchers instead"
    )]
    NoDataControl,
    #[error("compositor closed the data-control device")]
    DeviceFinished,
    /// The Wayland connection or protocol failed; `doing` names the step.
    #[error("{doing}: {detail}")]
    Wayland { doing: &'static str, detail: String },
}

impl Error {
    pub(crate) fn io(doing: impl Into<String>) -> impl FnOnce(std::io::Error) -> Error {
        move |source| Error::Io {
            doing: doing.into(),
            source,
        }
    }

    pub(crate) fn wayland<E: std::fmt::Display>(doing: &'static str) -> impl FnOnce(E) -> Error {
        move |e| Error::Wayland {
            doing,
            detail: e.to_string(),
        }
    }
}
