//! History backends: where list rows and entry content come from. The UI
//! must not care which one it is talking to.
//!
//! Previews are display-only — cliphist truncates them and collapses
//! newlines, so classification always runs on `content()`, never on a
//! preview.

use std::path::PathBuf;
use std::process::Command;

use crate::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: String,
    pub preview: String,
}

pub trait History {
    /// Newest first. An empty history is `Ok(vec![])`, not an error.
    fn entries(&self) -> Result<Vec<Entry>, Error>;
    fn content(&self, id: &str) -> Result<Vec<u8>, Error>;
}

pub struct Cliphist {
    program: String,
    db_path: Option<PathBuf>,
}

impl Cliphist {
    pub fn new() -> Self {
        Self::custom("cliphist", None)
    }

    /// Program and database overrides; tests point these at a scratch db
    /// or a nonexistent binary.
    pub fn custom(program: impl Into<String>, db_path: Option<PathBuf>) -> Self {
        Self {
            program: program.into(),
            db_path,
        }
    }

    fn output(&self, args: &[&str]) -> Result<std::process::Output, Error> {
        let mut cmd = Command::new(&self.program);
        if let Some(db) = &self.db_path {
            cmd.arg("-db-path").arg(db);
        }
        cmd.args(args);
        cmd.output().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error(format!("{} not found on PATH", self.program))
            } else {
                Error(format!("failed to run {}: {e}", self.program))
            }
        })
    }
}

impl Default for Cliphist {
    fn default() -> Self {
        Self::new()
    }
}

impl History for Cliphist {
    fn entries(&self) -> Result<Vec<Entry>, Error> {
        let out = self.output(&["list"])?;
        if !out.status.success() {
            let msg = String::from_utf8_lossy(&out.stderr).to_string()
                + &String::from_utf8_lossy(&out.stdout);
            // cliphist's way of saying the db is empty or doesn't exist yet
            if msg.contains("please store something first") {
                return Ok(Vec::new());
            }
            return Err(Error(format!("cliphist list failed: {}", msg.trim())));
        }
        Ok(parse_list(&String::from_utf8_lossy(&out.stdout)))
    }

    fn content(&self, id: &str) -> Result<Vec<u8>, Error> {
        // id as argv: decode via stdin is fragile about trailing newlines
        let out = self.output(&["decode", id])?;
        if !out.status.success() {
            return Err(Error(format!(
                "cliphist decode {id} failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        Ok(out.stdout)
    }
}

/// `cliphist list` emits `<id>\t<preview>` per line; skip anything else.
fn parse_list(stdout: &str) -> Vec<Entry> {
    stdout
        .lines()
        .filter_map(|line| {
            let (id, preview) = line.split_once('\t')?;
            Some(Entry {
                id: id.to_string(),
                preview: preview.to_string(),
            })
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// Native while a watcher is feeding the store, cliphist otherwise:
    /// unattended native history would silently go stale.
    Auto,
    Cliphist,
    Native,
}

pub fn select(choice: Backend) -> Result<Box<dyn History>, Error> {
    match choice {
        Backend::Cliphist => Ok(Box::new(Cliphist::new())),
        Backend::Native => Ok(Box::new(Native::new(crate::store::default_dir()?))),
        Backend::Auto => {
            let dir = crate::store::default_dir()?;
            if crate::store::watcher_active(&dir) {
                Ok(Box::new(Native::new(dir)))
            } else {
                Ok(Box::new(Cliphist::new()))
            }
        }
    }
}

/// The native store fed by `yankout watch`. Ids are the store's
/// sequence numbers; previews are built here from each entry's first
/// bytes, since the store itself keeps nothing but content.
pub struct Native {
    store: crate::store::Store,
}

/// Enough bytes to sniff an image and fill a preview line.
const PREVIEW_BYTES: usize = 512;
const PREVIEW_CHARS: usize = 100;

impl Native {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Native {
            store: crate::store::Store::open(dir),
        }
    }
}

impl History for Native {
    fn entries(&self) -> Result<Vec<Entry>, Error> {
        Ok(self
            .store
            .list()?
            .into_iter()
            .filter_map(|seq| {
                // a read failure here means the watcher evicted the
                // entry between list and read — it is gone, not an error
                let (prefix, total) = self.store.read_prefix(seq, PREVIEW_BYTES).ok()?;
                Some(Entry {
                    id: seq.to_string(),
                    preview: preview(&prefix, total),
                })
            })
            .collect())
    }

    fn content(&self, id: &str) -> Result<Vec<u8>, Error> {
        let seq: u64 = id
            .parse()
            .map_err(|_| Error(format!("not a native history id: {id}")))?;
        self.store.read(seq)
    }
}

fn preview(prefix: &[u8], total: u64) -> String {
    if let Some(mime) = crate::interpret::sniff_image(prefix) {
        return format!("[[ {mime} {} ]]", human_size(total));
    }
    if prefix.contains(&0) {
        return format!("[[ binary {} ]]", human_size(total));
    }
    let text = String::from_utf8_lossy(prefix);
    // collapse all whitespace runs: previews are one line
    let mut collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() > PREVIEW_CHARS || total > prefix.len() as u64 {
        collapsed = collapsed.chars().take(PREVIEW_CHARS).collect();
        // a prefix cut mid-character leaves a replacement char at the end
        collapsed = collapsed.trim_end_matches('\u{FFFD}').to_string();
        collapsed.push('…');
    }
    collapsed
}

fn human_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{} KiB", bytes / 1024)
    } else {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// Test double for everything downstream of the trait.
pub struct InMemory(pub Vec<(Entry, Vec<u8>)>);

impl History for InMemory {
    fn entries(&self) -> Result<Vec<Entry>, Error> {
        Ok(self.0.iter().map(|(e, _)| e.clone()).collect())
    }

    fn content(&self, id: &str) -> Result<Vec<u8>, Error> {
        self.0
            .iter()
            .find(|(e, _)| e.id == id)
            .map(|(_, c)| c.clone())
            .ok_or_else(|| Error(format!("no entry with id {id}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_id_tab_preview_lines() {
        let entries = parse_list("42\thello world\n41\t/some/path.txt\n");
        assert_eq!(
            entries,
            vec![
                Entry {
                    id: "42".into(),
                    preview: "hello world".into()
                },
                Entry {
                    id: "41".into(),
                    preview: "/some/path.txt".into()
                },
            ]
        );
    }

    #[test]
    fn preview_may_itself_contain_tabs() {
        let entries = parse_list("7\tcol1\tcol2\n");
        assert_eq!(entries[0].preview, "col1\tcol2");
    }

    #[test]
    fn malformed_lines_are_skipped() {
        let entries = parse_list("no tab here\n9\tok\n\n");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "9");
    }

    #[test]
    fn native_backend_lists_and_decodes() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut w = crate::store::Writer::open(dir.path(), 10).unwrap();
            w.store(b"copied  text\nsecond line").unwrap();
            let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
            png.extend_from_slice(&[0u8; 2040]);
            w.store(&png).unwrap();
        }
        let native = Native::new(dir.path());
        let entries = native.entries().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].preview, "[[ image/png 2 KiB ]]");
        assert_eq!(entries[1].preview, "copied text second line");
        assert_eq!(
            native.content(&entries[1].id).unwrap(),
            b"copied  text\nsecond line"
        );
    }

    #[test]
    fn native_long_text_previews_truncated() {
        let dir = tempfile::tempdir().unwrap();
        let long = "word ".repeat(300);
        {
            let mut w = crate::store::Writer::open(dir.path(), 10).unwrap();
            w.store(long.as_bytes()).unwrap();
        }
        let entries = Native::new(dir.path()).entries().unwrap();
        assert!(entries[0].preview.ends_with('…'));
        assert!(entries[0].preview.chars().count() <= PREVIEW_CHARS + 1);
    }

    #[test]
    fn native_empty_store_is_empty_history() {
        let dir = tempfile::tempdir().unwrap();
        let native = Native::new(dir.path().join("never-created"));
        assert_eq!(native.entries().unwrap(), Vec::new());
        assert!(native.content("0").is_err());
        assert!(native.content("not-a-seq").is_err());
    }

    #[test]
    fn in_memory_roundtrip_and_missing_id() {
        let mem = InMemory(vec![(
            Entry {
                id: "1".into(),
                preview: "x".into(),
            },
            b"content".to_vec(),
        )]);
        assert_eq!(mem.content("1").unwrap(), b"content");
        assert!(mem.content("2").is_err());
    }
}
