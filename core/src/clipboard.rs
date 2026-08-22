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
            return Err(Error("clipboard is empty".into()));
        }
        return Err(Error(format!("wl-paste failed: {}", msg.trim())));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mime = crate::mime::pick(stdout.lines())
        .ok_or_else(|| Error("clipboard holds nothing draggable".into()))?;

    let out = wl_paste(&["--no-newline", "--type", mime])?;
    if !out.status.success() {
        return Err(Error(format!(
            "wl-paste --type {mime} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(out.stdout)
}

/// Promote bytes to the active clipboard (the recall verb).
pub fn write(bytes: &[u8]) -> Result<(), Error> {
    let mut child = Command::new("wl-copy")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error("wl-copy not found on PATH (install wl-clipboard)".into())
            } else {
                Error(format!("failed to run wl-copy: {e}"))
            }
        })?;
    child
        .stdin
        .take()
        .expect("stdin was piped")
        .write_all(bytes)
        .map_err(|e| Error(format!("writing to wl-copy: {e}")))?;
    let status = child
        .wait()
        .map_err(|e| Error(format!("waiting for wl-copy: {e}")))?;
    if !status.success() {
        return Err(Error("wl-copy failed".into()));
    }
    Ok(())
}

fn wl_paste(args: &[&str]) -> Result<std::process::Output, Error> {
    Command::new("wl-paste").args(args).output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Error("wl-paste not found on PATH (install wl-clipboard)".into())
        } else {
            Error(format!("failed to run wl-paste: {e}"))
        }
    })
}
