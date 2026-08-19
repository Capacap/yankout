//! Live clipboard access via wl-clipboard. Deliberately not part of the
//! History trait: history is fed by a watcher, and a puck reading history
//! (`wl-copy && yankout --current`) would race the watcher's ingest.

use std::io::Write as _;
use std::process::{Command, Stdio};

use crate::Error;

/// Read the current clipboard, preferring the offered type that survives
/// our drag-time interpretation best: a real image type over the text
/// fallback most sources also offer.
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
    let types: Vec<&str> = stdout.lines().collect();
    let mime = choose_mime(&types).ok_or_else(|| Error("clipboard is empty".into()))?;

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

/// Sources usually offer several types for one copy. Prefer a concrete
/// image type (classification would sniff it anyway; the text fallback of
/// an image copy is usually a useless path or nothing), then honest text,
/// then whatever is first.
fn choose_mime<'a>(types: &[&'a str]) -> Option<&'a str> {
    if let Some(t) = types.iter().find(|t| **t == "image/png") {
        return Some(t);
    }
    if let Some(t) = types.iter().find(|t| t.starts_with("image/")) {
        return Some(t);
    }
    for preferred in [
        "text/plain;charset=utf-8",
        "text/plain",
        "UTF8_STRING",
        "TEXT",
        "STRING",
    ] {
        if let Some(t) = types.iter().find(|t| **t == preferred) {
            return Some(t);
        }
    }
    types.first().copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_beats_text() {
        let types = ["text/plain;charset=utf-8", "image/png", "image/jpeg"];
        assert_eq!(choose_mime(&types), Some("image/png"));
    }

    #[test]
    fn any_image_beats_text() {
        let types = ["text/plain", "image/jpeg"];
        assert_eq!(choose_mime(&types), Some("image/jpeg"));
    }

    #[test]
    fn utf8_text_beats_legacy_atoms() {
        let types = ["STRING", "UTF8_STRING", "text/plain;charset=utf-8"];
        assert_eq!(choose_mime(&types), Some("text/plain;charset=utf-8"));
    }

    #[test]
    fn unknown_types_fall_back_to_first() {
        let types = ["application/x-something", "application/other"];
        assert_eq!(choose_mime(&types), Some("application/x-something"));
    }

    #[test]
    fn empty_offer_is_none() {
        assert_eq!(choose_mime(&[]), None);
    }
}
