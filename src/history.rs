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
