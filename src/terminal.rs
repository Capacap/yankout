//! The terminal-facing verbs, `list` and `decode`: the same shape as
//! `cliphist list` / `cliphist decode`, so pickers built on those (fzf,
//! fuzzel --dmenu) switch by renaming the command. Both run over the
//! History trait, so they serve whichever backend list mode would.

use std::io::{self, BufRead, Write};

use crate::Error;
use crate::history::History;

/// One `<id>\t<preview>` line per entry, newest first. A closed pipe
/// (`| head`) ends the listing quietly, as a Unix filter should: Rust
/// ignores SIGPIPE, so the EPIPE surfaces here as an io error instead.
pub fn list(history: &dyn History, out: &mut impl Write) -> Result<(), Error> {
    for entry in history.entries()? {
        match writeln!(out, "{}\t{}", entry.id, entry.preview) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::BrokenPipe => return Ok(()),
            Err(e) => return Err(Error(format!("writing list: {e}"))),
        }
    }
    Ok(())
}

/// Raw entry bytes to `out`. The id comes from argv, or failing that
/// from the first line of `input`, where anything after a tab is
/// ignored — a picked `list` line can be piped back whole.
pub fn decode(
    history: &dyn History,
    id: Option<&str>,
    input: &mut impl BufRead,
    out: &mut impl Write,
) -> Result<(), Error> {
    let id = match id {
        Some(id) => id.to_string(),
        None => {
            let mut line = String::new();
            input
                .read_line(&mut line)
                .map_err(|e| Error(format!("reading id from stdin: {e}")))?;
            let id = line.split('\t').next().unwrap_or("").trim();
            if id.is_empty() {
                return Err(Error("decode needs an id, as an argument or on stdin".into()));
            }
            id.to_string()
        }
    };
    let content = history.content(&id)?;
    out.write_all(&content)
        .and_then(|()| out.flush())
        .map_err(|e| Error(format!("writing entry {id}: {e}")))
}

/// Convenience for `decode` from the process's own stdin.
pub fn decode_stdin(history: &dyn History, id: Option<&str>, out: &mut impl Write) -> Result<(), Error> {
    let stdin = io::stdin();
    decode(history, id, &mut stdin.lock(), out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::{Entry, InMemory};

    fn history() -> InMemory {
        InMemory(vec![
            (
                Entry { id: "7".into(), preview: "[[ image/png 2 KiB ]]".into() },
                b"\x89PNG".to_vec(),
            ),
            (
                Entry { id: "3".into(), preview: "hello world".into() },
                b"hello world\n".to_vec(),
            ),
        ])
    }

    #[test]
    fn list_is_id_tab_preview_per_line() {
        let mut out = Vec::new();
        list(&history(), &mut out).unwrap();
        assert_eq!(out, b"7\t[[ image/png 2 KiB ]]\n3\thello world\n");
    }

    #[test]
    fn decode_by_argument_writes_raw_bytes() {
        let mut out = Vec::new();
        decode(&history(), Some("3"), &mut &b""[..], &mut out).unwrap();
        assert_eq!(out, b"hello world\n");
    }

    #[test]
    fn decode_takes_a_whole_list_line_on_stdin() {
        let mut out = Vec::new();
        let mut input = &b"7\t[[ image/png 2 KiB ]]"[..];
        decode(&history(), None, &mut input, &mut out).unwrap();
        assert_eq!(out, b"\x89PNG");
    }

    #[test]
    fn decode_with_nothing_to_go_on_is_an_error() {
        let mut out = Vec::new();
        let err = decode(&history(), None, &mut &b"\n"[..], &mut out).unwrap_err();
        assert!(err.0.contains("needs an id"));
    }

    #[test]
    fn decode_unknown_id_is_the_backend_error() {
        let mut out = Vec::new();
        assert!(decode(&history(), Some("99"), &mut &b""[..], &mut out).is_err());
    }
}
