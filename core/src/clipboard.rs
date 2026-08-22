//! Live clipboard access via wl-clipboard. Deliberately not part of the
//! History trait: history is fed by a watcher, and a puck reading history
//! (`wl-copy && yankout --current`) would race the watcher's ingest.

use std::io::Write as _;
use std::process::{Command, Stdio};

use crate::Error;

/// Read the current clipboard as the watcher would store it.
pub fn read() -> Result<Vec<u8>, Error> {
    let out = wl_paste(&["--list-types"])?;
    if !out.status.success() {
        let msg = String::from_utf8_lossy(&out.stderr);
        if msg.contains("Nothing is copied") {
            return Err(Error::ClipboardEmpty);
        }
        return Err(Error::Program {
            program: "wl-paste".into(),
            detail: msg.trim().to_string(),
        });
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mime = crate::mime::pick(stdout.lines()).ok_or(Error::NothingDraggable)?;

    let out = wl_paste(&["--no-newline", "--type", mime])?;
    if !out.status.success() {
        return Err(Error::Program {
            program: format!("wl-paste --type {mime}"),
            detail: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        });
    }
    Ok(out.stdout)
}

/// Promote bytes to the active clipboard (the recall verb).
pub fn write(bytes: &[u8]) -> Result<(), Error> {
    let mut child = Command::new("wl-copy")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(spawn_error("wl-copy"))?;
    child
        .stdin
        .take()
        .expect("stdin was piped")
        .write_all(bytes)
        .map_err(Error::io("writing to wl-copy"))?;
    let status = child.wait().map_err(Error::io("waiting for wl-copy"))?;
    if !status.success() {
        return Err(Error::Program {
            program: "wl-copy".into(),
            detail: status.to_string(),
        });
    }
    Ok(())
}

fn wl_paste(args: &[&str]) -> Result<std::process::Output, Error> {
    Command::new("wl-paste")
        .args(args)
        .output()
        .map_err(spawn_error("wl-paste"))
}

fn spawn_error(program: &'static str) -> impl FnOnce(std::io::Error) -> Error {
    move |e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Error::MissingProgram(format!("{program} (install wl-clipboard)"))
        } else {
            Error::io(format!("running {program}"))(e)
        }
    }
}
