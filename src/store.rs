//! Self-contained history store: a directory of files, one per entry,
//! named by zero-padded sequence number. The filename is both the id
//! and the ordering, so promoting an entry to the front is a rename,
//! never a data rewrite. Writes go temp-then-rename, so a reader never
//! sees a torn entry.
//!
//! `Store` is the read side (safe against a live writer). `Writer` is
//! the watcher's side: it holds a flock for its lifetime, so at most
//! one exists per directory, and its dedup index lives in memory —
//! built once at open, never consulted from disk per store.

use std::collections::{BTreeMap, HashMap, hash_map::DefaultHasher};
use std::fs::{self, File};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use rustix::fs::FlockOperation;

use crate::Error;

pub const DEFAULT_CAP: usize = 750;

const LOCK_FILE: &str = ".lock";

pub fn default_dir() -> Result<PathBuf, Error> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share"))
        })
        .ok_or_else(|| Error("neither XDG_DATA_HOME nor HOME is set".into()))?;
    Ok(base.join("yankout/history"))
}

/// Whether a `Writer` currently holds the store's lock. This is the
/// signal list mode uses to prefer the native backend: history is only
/// trustworthy while something is feeding it.
pub fn watcher_active(dir: &Path) -> bool {
    let Ok(file) = File::open(dir.join(LOCK_FILE)) else {
        return false;
    };
    match rustix::fs::flock(&file, FlockOperation::NonBlockingLockExclusive) {
        Ok(()) => {
            let _ = rustix::fs::flock(&file, FlockOperation::Unlock);
            false
        }
        Err(_) => true,
    }
}

pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn open(dir: impl Into<PathBuf>) -> Self {
        Store { dir: dir.into() }
    }

    /// Sequence numbers present, newest first. A missing directory is
    /// an empty history, not an error.
    pub fn list(&self) -> Result<Vec<u64>, Error> {
        let entries = match fs::read_dir(&self.dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => {
                return Err(Error(format!("reading store {}: {e}", self.dir.display())));
            }
        };
        // Anything non-numeric (the lock file, temp files) is not an entry.
        let mut seqs: Vec<u64> = entries
            .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse().ok())
            .collect();
        seqs.sort_unstable_by(|a, b| b.cmp(a));
        Ok(seqs)
    }

    pub fn read(&self, seq: u64) -> Result<Vec<u8>, Error> {
        fs::read(self.path(seq)).map_err(|e| Error(format!("reading entry {seq}: {e}")))
    }

    /// First `limit` bytes plus the entry's total size — enough for a
    /// preview without pulling a multi-megabyte image into memory.
    pub fn read_prefix(&self, seq: u64, limit: usize) -> Result<(Vec<u8>, u64), Error> {
        use std::io::Read;
        let mut file = File::open(self.path(seq))
            .map_err(|e| Error(format!("reading entry {seq}: {e}")))?;
        let total = file
            .metadata()
            .map_err(|e| Error(format!("reading entry {seq}: {e}")))?
            .len();
        let mut prefix = vec![0; limit.min(total as usize)];
        file.read_exact(&mut prefix)
            .map_err(|e| Error(format!("reading entry {seq}: {e}")))?;
        Ok((prefix, total))
    }

    fn path(&self, seq: u64) -> PathBuf {
        self.dir.join(format!("{seq:020}"))
    }
}

/// (length, hash) of entry content. Hash collisions are guarded by a
/// byte comparison before any bump, so this only needs to be cheap.
type Key = (u64, u64);

fn key_of(content: &[u8]) -> Key {
    let mut h = DefaultHasher::new();
    content.hash(&mut h);
    (content.len() as u64, h.finish())
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Stored {
    New(u64),
    Bumped(u64),
}

pub struct Writer {
    store: Store,
    // flock is released when this handle drops
    _lock: File,
    cap: usize,
    next_seq: u64,
    by_key: HashMap<Key, u64>,
    by_seq: BTreeMap<u64, Key>,
}

impl Writer {
    pub fn open(dir: impl Into<PathBuf>, cap: usize) -> Result<Self, Error> {
        let store = Store::open(dir);
        fs::create_dir_all(&store.dir)
            .map_err(|e| Error(format!("creating store {}: {e}", store.dir.display())))?;
        let lock = File::create(store.dir.join(LOCK_FILE))
            .map_err(|e| Error(format!("creating store lock: {e}")))?;
        rustix::fs::flock(&lock, FlockOperation::NonBlockingLockExclusive).map_err(|_| {
            Error(format!(
                "store {} is locked, another watcher is already running",
                store.dir.display()
            ))
        })?;

        let mut by_key = HashMap::new();
        let mut by_seq = BTreeMap::new();
        for seq in store.list()? {
            let key = key_of(&store.read(seq)?);
            by_seq.insert(seq, key);
            // list() is newest first; a duplicate key keeps its newest seq
            by_key.entry(key).or_insert(seq);
        }
        let next_seq = by_seq.last_key_value().map_or(0, |(s, _)| s + 1);
        Ok(Writer {
            store,
            _lock: lock,
            cap,
            next_seq,
            by_key,
            by_seq,
        })
    }

    pub fn store(&mut self, content: &[u8]) -> Result<Stored, Error> {
        let key = key_of(content);
        if let Some(&old_seq) = self.by_key.get(&key) {
            // Byte-compare before bumping: the key is a cheap hash. A
            // failed read or rename means the entry vanished under us
            // (someone cleared the directory by hand); drop the stale
            // index row and store fresh.
            let matches = self.store.read(old_seq).is_ok_and(|old| old == content);
            if matches {
                let seq = self.take_seq();
                if fs::rename(self.store.path(old_seq), self.store.path(seq)).is_ok() {
                    self.by_seq.remove(&old_seq);
                    self.by_seq.insert(seq, key);
                    self.by_key.insert(key, seq);
                    return Ok(Stored::Bumped(seq));
                }
            }
            if !self.store.path(old_seq).exists() {
                self.by_seq.remove(&old_seq);
                self.by_key.remove(&key);
            }
        }

        let seq = self.take_seq();
        let tmp = self.store.dir.join(format!(".tmp-{seq:020}"));
        fs::write(&tmp, content).map_err(|e| Error(format!("writing entry {seq}: {e}")))?;
        fs::rename(&tmp, self.store.path(seq))
            .map_err(|e| Error(format!("publishing entry {seq}: {e}")))?;
        self.by_seq.insert(seq, key);
        self.by_key.insert(key, seq);
        self.evict();
        Ok(Stored::New(seq))
    }

    fn take_seq(&mut self) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        seq
    }

    fn evict(&mut self) {
        while self.by_seq.len() > self.cap {
            let Some((seq, key)) = self.by_seq.pop_first() else {
                return;
            };
            let _ = fs::remove_file(self.store.path(seq));
            // after a hash collision two seqs share a key; only unmap
            // the one this key actually points at
            if self.by_key.get(&key) == Some(&seq) {
                self.by_key.remove(&key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    fn writer(dir: &Path) -> Writer {
        Writer::open(dir, DEFAULT_CAP).unwrap()
    }

    #[test]
    fn new_entries_list_newest_first_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let mut w = writer(dir.path());
        assert_eq!(w.store(b"first").unwrap(), Stored::New(0));
        assert_eq!(w.store(b"second").unwrap(), Stored::New(1));
        let store = Store::open(dir.path());
        assert_eq!(store.list().unwrap(), vec![1, 0]);
        assert_eq!(store.read(0).unwrap(), b"first");
        assert_eq!(store.read(1).unwrap(), b"second");
    }

    #[test]
    fn duplicate_content_bumps_by_rename() {
        let dir = tempfile::tempdir().unwrap();
        let mut w = writer(dir.path());
        w.store(b"payload").unwrap();
        w.store(b"other").unwrap();
        let store = Store::open(dir.path());
        let inode_before = fs::metadata(store.path(0)).unwrap().ino();

        assert_eq!(w.store(b"payload").unwrap(), Stored::Bumped(2));
        assert_eq!(store.list().unwrap(), vec![2, 1]);
        assert_eq!(store.read(2).unwrap(), b"payload");
        // same inode: the data was moved, not rewritten
        assert_eq!(fs::metadata(store.path(2)).unwrap().ino(), inode_before);
    }

    #[test]
    fn eviction_removes_oldest_beyond_cap() {
        let dir = tempfile::tempdir().unwrap();
        let mut w = Writer::open(dir.path(), 2).unwrap();
        w.store(b"a").unwrap();
        w.store(b"b").unwrap();
        w.store(b"c").unwrap();
        let store = Store::open(dir.path());
        assert_eq!(store.list().unwrap(), vec![2, 1]);
        assert!(store.read(0).is_err());
    }

    #[test]
    fn reopen_rebuilds_index_and_sequence() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut w = writer(dir.path());
            w.store(b"kept").unwrap();
            w.store(b"other").unwrap();
        }
        let mut w = writer(dir.path());
        // dedup survives the restart, and the sequence continues
        assert_eq!(w.store(b"kept").unwrap(), Stored::Bumped(2));
    }

    #[test]
    fn second_writer_is_rejected_while_lock_held() {
        let dir = tempfile::tempdir().unwrap();
        let _w = writer(dir.path());
        assert!(Writer::open(dir.path(), DEFAULT_CAP).is_err());
    }

    #[test]
    fn lock_and_temp_files_are_not_entries() {
        let dir = tempfile::tempdir().unwrap();
        let mut w = writer(dir.path());
        w.store(b"real").unwrap();
        fs::write(dir.path().join(".tmp-junk"), b"torn").unwrap();
        assert_eq!(Store::open(dir.path()).list().unwrap(), vec![0]);
    }

    #[test]
    fn watcher_active_tracks_lock_lifetime() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!watcher_active(dir.path()));
        {
            let _w = writer(dir.path());
            assert!(watcher_active(dir.path()));
        }
        assert!(!watcher_active(dir.path()));
    }

    #[test]
    fn externally_deleted_entry_stores_fresh() {
        let dir = tempfile::tempdir().unwrap();
        let mut w = writer(dir.path());
        w.store(b"gone").unwrap();
        let store = Store::open(dir.path());
        fs::remove_file(store.path(0)).unwrap();
        assert_eq!(w.store(b"gone").unwrap(), Stored::New(1));
        assert_eq!(store.read(1).unwrap(), b"gone");
    }

    #[test]
    fn missing_directory_lists_empty() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("does-not-exist"));
        assert_eq!(store.list().unwrap(), Vec::<u64>::new());
    }
}
