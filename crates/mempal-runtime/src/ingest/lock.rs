//! Per-source filesystem advisory lock for ingest critical sections.
//!
//! Eliminates the TOCTOU race between concurrent `ingest_file_with_options`
//! calls (e.g. Claude Code + Codex ingesting the same file at the same
//! time). Pattern:
//!
//!   1. acquire_source_lock(home, source_key, timeout)  ← blocks if held
//!   2. re-check dedup (may have changed while waiting)
//!   3. delete-then-insert drawers + vectors
//!   4. IngestLock dropped on return → file closed → OS releases flock
//!
//! Unix: `flock(fd, LOCK_EX | LOCK_NB)` via inline extern (no libc dep).
//! Windows: no-op fallback (concurrent ingest on Windows is not
//! race-protected; follow-up work to adopt `LockFileEx`).

use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum LockError {
    #[error("timed out after {} ms acquiring ingest lock on {path}", timeout.as_millis())]
    Timeout { path: PathBuf, timeout: Duration },
    #[error("io error on ingest lock {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("invalid source_key `{0}` (empty or contains path separators)")]
    InvalidSourceKey(String),
}

/// RAII guard. Dropping releases the lock (OS-level close of file handle
/// releases the flock on Unix).
#[derive(Debug)]
pub struct IngestLock {
    _file: File,
    path: PathBuf,
    wait: Duration,
}

impl IngestLock {
    pub fn wait_duration(&self) -> Duration {
        self.wait
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Compute a short, filename-safe source key from a path.
///
/// Uses the std `DefaultHasher` (SipHash) → 16 hex chars. Not
/// cryptographic; collision probability on realistic workloads is
/// negligible and collisions only cause false-serialization of unrelated
/// sources (correctness-preserving).
pub fn source_key(source_file: &Path) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source_file.to_string_lossy().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

pub fn acquire_source_lock(
    mempal_home: &Path,
    source_key: &str,
    timeout: Duration,
) -> Result<IngestLock, LockError> {
    if source_key.is_empty()
        || source_key.contains('/')
        || source_key.contains('\\')
        || source_key.contains("..")
    {
        return Err(LockError::InvalidSourceKey(source_key.to_string()));
    }

    let locks_dir = mempal_home.join("locks");
    if !locks_dir.exists() {
        std::fs::create_dir_all(&locks_dir).map_err(|e| LockError::Io {
            path: locks_dir.clone(),
            source: e,
        })?;
    }
    let lock_path = locks_dir.join(format!("{source_key}.lock"));

    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .truncate(false)
        .write(true)
        .open(&lock_path)
        .map_err(|e| LockError::Io {
            path: lock_path.clone(),
            source: e,
        })?;

    let start = Instant::now();
    loop {
        match imp::try_lock_exclusive(&file) {
            Ok(()) => {
                return Ok(IngestLock {
                    _file: file,
                    path: lock_path,
                    wait: start.elapsed(),
                });
            }
            Err(imp::LockAcquire::WouldBlock) => {
                if start.elapsed() >= timeout {
                    return Err(LockError::Timeout {
                        path: lock_path,
                        timeout,
                    });
                }
                std::thread::sleep(Duration::from_millis(50 + jitter_ms()));
            }
            Err(imp::LockAcquire::Io(e)) => {
                return Err(LockError::Io {
                    path: lock_path,
                    source: e,
                });
            }
        }
    }
}

fn jitter_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64 % 30)
        .unwrap_or(0)
}

#[cfg(unix)]
mod imp {
    use std::fs::File;
    use std::io;
    use std::os::fd::AsRawFd;

    const LOCK_EX: i32 = 2;
    const LOCK_NB: i32 = 4;
    const EWOULDBLOCK: i32 = 35; // macOS; Linux EWOULDBLOCK/EAGAIN both route here

    unsafe extern "C" {
        fn flock(fd: i32, operation: i32) -> i32;
    }

    pub enum LockAcquire {
        WouldBlock,
        Io(io::Error),
    }

    pub fn try_lock_exclusive(file: &File) -> Result<(), LockAcquire> {
        let fd = file.as_raw_fd();
        let ret = unsafe { flock(fd, LOCK_EX | LOCK_NB) };
        if ret == 0 {
            return Ok(());
        }
        let err = io::Error::last_os_error();
        // Linux uses EAGAIN (11) for flock contention, macOS uses
        // EWOULDBLOCK (35). Accept both.
        match err.raw_os_error() {
            Some(code) if code == EWOULDBLOCK || code == 11 => Err(LockAcquire::WouldBlock),
            _ => Err(LockAcquire::Io(err)),
        }
    }
}

#[cfg(windows)]
mod imp {
    use std::fs::File;
    use std::io;

    pub enum LockAcquire {
        WouldBlock,
        Io(io::Error),
    }

    /// Windows fallback: always succeeds. Concurrent ingest on Windows is
    /// not race-protected in 0.3.x; follow-up spec to adopt LockFileEx.
    pub fn try_lock_exclusive(_file: &File) -> Result<(), LockAcquire> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_key_is_deterministic() {
        let p = Path::new("/tmp/foo/bar.md");
        assert_eq!(source_key(p), source_key(p));
    }

    #[test]
    fn test_source_key_is_fs_safe() {
        let k = source_key(Path::new("/tmp/a/b/c.md"));
        assert_eq!(k.len(), 16);
        assert!(k.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_invalid_source_key_with_slash_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let err = acquire_source_lock(tmp.path(), "a/b", Duration::from_millis(100));
        assert!(matches!(err, Err(LockError::InvalidSourceKey(_))));
    }

    #[test]
    fn test_invalid_source_key_with_traversal_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let err = acquire_source_lock(tmp.path(), "..", Duration::from_millis(100));
        assert!(matches!(err, Err(LockError::InvalidSourceKey(_))));
    }

    #[cfg(unix)]
    #[test]
    fn test_acquire_then_release_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let key = source_key(Path::new("/tmp/test-source"));

        let guard1 =
            acquire_source_lock(tmp.path(), &key, Duration::from_secs(1)).expect("first acquire");
        drop(guard1);

        let guard2 = acquire_source_lock(tmp.path(), &key, Duration::from_millis(500))
            .expect("second acquire after drop");
        assert!(guard2.wait_duration() < Duration::from_millis(500));
    }

    #[cfg(unix)]
    #[test]
    fn test_concurrent_holders_serialize() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::thread;

        let tmp = Arc::new(tempfile::tempdir().unwrap());
        let key = source_key(Path::new("/tmp/concurrent-source"));
        let counter = Arc::new(AtomicUsize::new(0));

        let handles: Vec<_> = (0..3)
            .map(|_| {
                let tmp = Arc::clone(&tmp);
                let key = key.clone();
                let counter = Arc::clone(&counter);
                thread::spawn(move || {
                    let guard = acquire_source_lock(tmp.path(), &key, Duration::from_secs(5))
                        .expect("acquire");
                    // Enter critical section: ensure no other thread is
                    // inside simultaneously.
                    let inside = counter.fetch_add(1, Ordering::SeqCst);
                    assert_eq!(inside, 0, "serial critical section violated");
                    thread::sleep(Duration::from_millis(50));
                    counter.fetch_sub(1, Ordering::SeqCst);
                    drop(guard);
                })
            })
            .collect();

        for h in handles {
            h.join().expect("thread");
        }
    }
}
