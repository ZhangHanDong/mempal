//! Integration tests for P9-B per-source ingest lock.
//!
//! Validates TOCTOU protection for concurrent Claude↔Codex ingest of
//! the same source file, plus timeout / dry-run / panic-release
//! semantics.

use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use async_trait::async_trait;
use mempal::core::db::Database;
use mempal::embed::{Embedder, Result as EmbedResult};
use mempal::ingest::{IngestOptions, ingest_dir_with_options, ingest_file_with_options};
use mempal::path_filter::ProjectPathFilterOptions;
use tempfile::TempDir;

/// Stub embedder: returns a fixed vector regardless of input. 3 dims so
/// `sqlite-vec` can store it without bloating the test DB.
struct StubEmbedder;

#[async_trait]
impl Embedder for StubEmbedder {
    async fn embed(&self, texts: &[&str]) -> EmbedResult<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|_| vec![0.1, 0.2, 0.3]).collect())
    }
    fn dimensions(&self) -> usize {
        3
    }
    fn name(&self) -> &str {
        "stub"
    }
}

struct HoldEmbedder {
    delay: Duration,
    entered: Option<mpsc::Sender<()>>,
}

#[async_trait]
impl Embedder for HoldEmbedder {
    async fn embed(&self, texts: &[&str]) -> EmbedResult<Vec<Vec<f32>>> {
        if let Some(tx) = &self.entered {
            let _ = tx.send(());
        }
        if !self.delay.is_zero() {
            tokio::time::sleep(self.delay).await;
        }
        Ok(texts.iter().map(|_| vec![0.1, 0.2, 0.3]).collect())
    }

    fn dimensions(&self) -> usize {
        3
    }

    fn name(&self) -> &str {
        "hold"
    }
}

fn write_file(dir: &Path, name: &str, content: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, content).expect("write fixture");
    path
}

fn write_bytes(dir: &Path, name: &str, content: &[u8]) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, content).expect("write fixture");
    path
}

fn active_source_files(db: &Database) -> Vec<String> {
    let mut stmt = db
        .conn()
        .prepare("SELECT source_file FROM drawers WHERE deleted_at IS NULL ORDER BY source_file")
        .expect("prepare source_file query");
    stmt.query_map([], |row| row.get::<_, Option<String>>(0))
        .expect("query source files")
        .map(|row| row.expect("source file row").unwrap_or_default())
        .collect()
}

/// Run an ingest on a fresh tokio runtime — matches the cross-process
/// topology that the lock actually protects. `Database` is !Sync so we
/// cannot share `&Database` across tokio tasks within one runtime; each
/// thread must own its own Database + Runtime, exactly mirroring the
/// Claude Code / Codex process pair.
fn ingest_in_thread(
    db_path: std::path::PathBuf,
    file: std::path::PathBuf,
) -> mempal::ingest::IngestStats {
    ingest_in_thread_with_embedder(db_path, file, StubEmbedder, IngestOptions::default())
}

fn ingest_in_thread_with_embedder<E: Embedder + 'static>(
    db_path: std::path::PathBuf,
    file: std::path::PathBuf,
    embedder: E,
    options: IngestOptions<'static>,
) -> mempal::ingest::IngestStats {
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    rt.block_on(async move {
        let db = Database::open(&db_path).expect("open db");
        ingest_file_with_options(&db, &embedder, &file, "test", options)
            .await
            .expect("ingest")
    })
}

#[test]
fn test_concurrent_ingest_same_source_single_drawer() {
    use std::thread;

    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("palace.db");
    Database::open(&db_path).expect("init db");

    let file = write_file(tmp.path(), "doc.md", "hello P9-B test content");

    let db_path_a = db_path.clone();
    let db_path_b = db_path.clone();
    let file_a = file.clone();
    let file_b = file.clone();
    let (entered_tx, entered_rx) = mpsc::channel();

    let handle_a = thread::spawn(move || {
        ingest_in_thread_with_embedder(
            db_path_a,
            file_a,
            HoldEmbedder {
                delay: Duration::from_millis(250),
                entered: Some(entered_tx),
            },
            IngestOptions::default(),
        )
    });
    entered_rx
        .recv()
        .expect("first ingest entered critical section");
    let handle_b = thread::spawn(move || ingest_in_thread(db_path_b, file_b));

    let stats_a = handle_a.join().expect("thread a");
    let stats_b = handle_b.join().expect("thread b");

    let db = Database::open(&db_path).expect("reopen");
    let drawer_count = db.drawer_count().expect("drawer_count");

    // Content-addressed drawer_id means both threads target the same id;
    // only one inserts, the other sees `drawer_exists == true`.
    assert_eq!(
        drawer_count, 1,
        "expected exactly 1 drawer; a={stats_a:?} b={stats_b:?}"
    );

    // Both threads must have recorded lock_wait_ms (non-dry-run path).
    assert!(stats_a.lock_wait_ms.is_some());
    assert!(stats_b.lock_wait_ms.is_some());

    let waits = [
        stats_a.lock_wait_ms.unwrap_or(0),
        stats_b.lock_wait_ms.unwrap_or(0),
    ];
    let waited = waits.into_iter().filter(|ms| *ms > 0).count();
    assert_eq!(
        waited, 1,
        "expected exactly one waiter; a={stats_a:?} b={stats_b:?}"
    );
}

#[test]
fn test_concurrent_ingest_different_source_no_blocking() {
    use std::thread;

    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("palace.db");
    Database::open(&db_path).expect("init db");

    let file_a = write_file(tmp.path(), "a.md", "content A unique");
    let file_b = write_file(tmp.path(), "b.md", "content B unique");

    let db_path_a = db_path.clone();
    let db_path_b = db_path.clone();

    let handle_a = thread::spawn(move || ingest_in_thread(db_path_a, file_a));
    let handle_b = thread::spawn(move || ingest_in_thread(db_path_b, file_b));

    let stats_a = handle_a.join().expect("thread a");
    let stats_b = handle_b.join().expect("thread b");

    let wait_a = stats_a.lock_wait_ms.unwrap_or(0);
    let wait_b = stats_b.lock_wait_ms.unwrap_or(0);
    assert!(
        wait_a < 100 && wait_b < 100,
        "different sources should not block: a={wait_a}ms b={wait_b}ms"
    );

    let db = Database::open(&db_path).unwrap();
    assert_eq!(db.drawer_count().unwrap(), 2);
}

#[test]
fn test_ingest_same_content_different_sources_get_distinct_drawers() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("palace.db");
    let db = Database::open(&db_path).expect("init db");
    let rt = tokio::runtime::Runtime::new().expect("runtime");

    let file_a = write_file(tmp.path(), "a.md", "identical boilerplate content");
    let file_b = write_file(tmp.path(), "b.md", "identical boilerplate content");

    let stats_a = rt
        .block_on(ingest_file_with_options(
            &db,
            &StubEmbedder,
            &file_a,
            "test",
            IngestOptions {
                room: Some("docs"),
                source_root: Some(tmp.path()),
                ..IngestOptions::default()
            },
        ))
        .expect("ingest first source");
    let stats_b = rt
        .block_on(ingest_file_with_options(
            &db,
            &StubEmbedder,
            &file_b,
            "test",
            IngestOptions {
                room: Some("docs"),
                source_root: Some(tmp.path()),
                ..IngestOptions::default()
            },
        ))
        .expect("ingest second source");

    assert_eq!(stats_a.chunks, 1);
    assert_eq!(stats_b.chunks, 1);
    let drawers = db.all_active_drawers().expect("drawers");
    assert_eq!(drawers.len(), 2, "different source files must not collide");
    assert_ne!(drawers[0].0, drawers[1].0);
}

#[test]
fn test_ingest_dir_skips_platform_and_binary_files() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("palace.db");
    let db = Database::open(&db_path).expect("init db");
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    let source_dir = tmp.path().join("source");
    std::fs::create_dir(&source_dir).expect("create source dir");

    write_file(&source_dir, "note.md", "only this text should be ingested");
    write_bytes(&source_dir, ".DS_Store", b"ignored metadata");
    write_bytes(&source_dir, "._note.md", b"ignored appledouble");
    write_bytes(&source_dir, "image.png", b"\x89PNG\r\n\x1a\n");

    let stats = rt
        .block_on(ingest_dir_with_options(
            &db,
            &StubEmbedder,
            &source_dir,
            "test",
            IngestOptions {
                room: Some("docs"),
                source_root: Some(&source_dir),
                ..IngestOptions::default()
            },
        ))
        .expect("ingest dir");

    assert_eq!(stats.files, 1, "only the markdown file is read");
    assert_eq!(stats.chunks, 1);
    assert_eq!(stats.skipped, 3, "three platform/binary files are skipped");
    assert_eq!(db.drawer_count().expect("drawer count"), 1);
}

#[test]
fn test_ingest_dir_respects_gitignore_and_mempalignore() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("palace.db");
    let db = Database::open(&db_path).expect("init db");
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    let source_dir = tmp.path().join("source");
    std::fs::create_dir(&source_dir).expect("source dir");

    write_file(&source_dir, "keep.md", "keep this document");
    std::fs::create_dir_all(source_dir.join("dist")).expect("dist");
    std::fs::create_dir_all(source_dir.join("generated")).expect("generated");
    std::fs::create_dir_all(source_dir.join("target")).expect("target");
    write_file(&source_dir.join("dist"), "ignored.md", "git ignored");
    write_file(
        &source_dir.join("generated"),
        "ignored.md",
        "mempal ignored",
    );
    write_file(&source_dir.join("target"), "ignored.md", "hard ignored");
    std::fs::write(source_dir.join(".gitignore"), "dist/\n").expect("gitignore");
    std::fs::write(source_dir.join(".mempalignore"), "generated/\n").expect("mempalignore");

    let stats = rt
        .block_on(ingest_dir_with_options(
            &db,
            &StubEmbedder,
            &source_dir,
            "test",
            IngestOptions {
                room: Some("docs"),
                source_root: Some(&source_dir),
                ..IngestOptions::default()
            },
        ))
        .expect("ingest dir");

    assert_eq!(stats.files, 1);
    assert_eq!(stats.chunks, 1);
    assert_eq!(active_source_files(&db), vec!["keep.md"]);
}

#[test]
fn test_ingest_no_mempalignore_keeps_gitignore() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("palace.db");
    let db = Database::open(&db_path).expect("init db");
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    let source_dir = tmp.path().join("source");
    std::fs::create_dir_all(source_dir.join("dist")).expect("dist");
    std::fs::create_dir_all(source_dir.join("generated")).expect("generated");
    write_file(&source_dir.join("dist"), "ignored.md", "git ignored");
    write_file(
        &source_dir.join("generated"),
        "remember.md",
        "mempal ignore disabled",
    );
    std::fs::write(source_dir.join(".gitignore"), "dist/\n").expect("gitignore");
    std::fs::write(source_dir.join(".mempalignore"), "generated/\n").expect("mempalignore");

    let stats = rt
        .block_on(ingest_dir_with_options(
            &db,
            &StubEmbedder,
            &source_dir,
            "test",
            IngestOptions {
                room: Some("docs"),
                source_root: Some(&source_dir),
                project_filter: ProjectPathFilterOptions {
                    respect_mempalignore: false,
                    ..ProjectPathFilterOptions::default()
                },
                ..IngestOptions::default()
            },
        ))
        .expect("ingest dir");

    assert_eq!(stats.files, 1);
    assert_eq!(active_source_files(&db), vec!["generated/remember.md"]);
}

#[test]
fn test_ingest_custom_ignore_file_excludes_sources() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("palace.db");
    let db = Database::open(&db_path).expect("init db");
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    let source_dir = tmp.path().join("source");
    std::fs::create_dir_all(source_dir.join("notes")).expect("notes");
    let custom = source_dir.join("custom.ignore");
    write_file(&source_dir, "keep.md", "keep this");
    write_file(&source_dir.join("notes"), "ignored.md", "ignore this");
    std::fs::write(&custom, "notes/\n").expect("custom ignore");

    let stats = rt
        .block_on(ingest_dir_with_options(
            &db,
            &StubEmbedder,
            &source_dir,
            "test",
            IngestOptions {
                room: Some("docs"),
                source_root: Some(&source_dir),
                project_filter: ProjectPathFilterOptions {
                    custom_ignore_files: vec![custom],
                    ..ProjectPathFilterOptions::default()
                },
                ..IngestOptions::default()
            },
        ))
        .expect("ingest dir");

    assert_eq!(stats.files, 1);
    assert_eq!(active_source_files(&db), vec!["keep.md"]);
}

#[tokio::test]
async fn test_ingest_explicit_file_bypasses_project_ignore() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("palace.db");
    let db = Database::open(&db_path).expect("init db");
    let dist = tmp.path().join("dist");
    std::fs::create_dir(&dist).expect("dist");
    std::fs::write(tmp.path().join(".gitignore"), "dist/\n").expect("gitignore");
    let file = write_file(&dist, "manual.md", "explicit file still ingests");

    let stats = ingest_file_with_options(
        &db,
        &StubEmbedder,
        &file,
        "test",
        IngestOptions {
            room: Some("docs"),
            source_root: Some(tmp.path()),
            ..IngestOptions::default()
        },
    )
    .await
    .expect("explicit ingest");

    assert_eq!(stats.files, 1);
    assert_eq!(stats.chunks, 1);
    assert_eq!(db.drawer_count().expect("drawer count"), 1);
    assert_eq!(active_source_files(&db), vec!["dist/manual.md"]);
}

#[tokio::test]
async fn test_dry_run_does_not_acquire_lock() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("palace.db");
    Database::open(&db_path).expect("init db");

    let file = write_file(tmp.path(), "doc.md", "dry run content");
    let db = Database::open(&db_path).expect("open");

    let stats = ingest_file_with_options(
        &db,
        &StubEmbedder,
        &file,
        "test",
        IngestOptions {
            dry_run: true,
            ..IngestOptions::default()
        },
    )
    .await
    .expect("dry_run");

    assert!(
        stats.lock_wait_ms.is_none(),
        "dry-run must not acquire lock"
    );
    // No writes.
    assert_eq!(db.drawer_count().unwrap(), 0);
}

#[test]
fn test_double_check_after_lock_skips_duplicate() {
    use std::thread;

    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("palace.db");
    Database::open(&db_path).expect("init db");

    let file = write_file(tmp.path(), "doc.md", "second ingest should dedup");

    let db_path_a = db_path.clone();
    let db_path_b = db_path.clone();
    let file_a = file.clone();
    let file_b = file.clone();
    let (entered_tx, entered_rx) = mpsc::channel();

    let handle_a = thread::spawn(move || {
        ingest_in_thread_with_embedder(
            db_path_a,
            file_a,
            HoldEmbedder {
                delay: Duration::from_millis(250),
                entered: Some(entered_tx),
            },
            IngestOptions::default(),
        )
    });
    entered_rx
        .recv()
        .expect("first ingest entered critical section");
    let handle_b = thread::spawn(move || ingest_in_thread(db_path_b, file_b));

    let stats_1 = handle_a.join().expect("thread a");
    let stats_2 = handle_b.join().expect("thread b");

    assert_eq!(stats_1.chunks, 1);
    assert_eq!(stats_2.chunks, 0, "second ingest writes no new chunks");
    assert!(stats_2.skipped >= 1, "second ingest should report skipped");
    assert!(
        stats_2.lock_wait_ms.unwrap_or(0) > 0,
        "second ingest must wait for the lock"
    );

    let db = Database::open(&db_path).expect("reopen");
    assert_eq!(db.drawer_count().unwrap(), 1);
}

#[test]
fn test_lock_released_on_guard_drop() {
    use mempal::ingest::lock::{acquire_source_lock, source_key};

    let tmp = TempDir::new().unwrap();
    let key = source_key(Path::new("/tmp/test-drop-release"));

    let guard1 =
        acquire_source_lock(tmp.path(), &key, Duration::from_secs(1)).expect("first acquire");
    drop(guard1);

    // Second acquire must succeed quickly.
    let guard2 = acquire_source_lock(tmp.path(), &key, Duration::from_millis(200))
        .expect("second acquire after drop");
    assert!(guard2.wait_duration() < Duration::from_millis(200));
}

#[cfg(unix)]
#[test]
fn test_lock_timeout_returns_error() {
    use mempal::ingest::lock::{LockError, acquire_source_lock, source_key};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;

    let tmp = Arc::new(TempDir::new().unwrap());
    let key = source_key(Path::new("/tmp/test-timeout"));
    let done = Arc::new(AtomicBool::new(false));

    let tmp_a = Arc::clone(&tmp);
    let key_a = key.clone();
    let done_a = Arc::clone(&done);
    let holder = thread::spawn(move || {
        let _guard = acquire_source_lock(tmp_a.path(), &key_a, Duration::from_secs(1))
            .expect("holder acquire");
        while !done_a.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(20));
        }
    });

    // Give holder time to grab the lock.
    thread::sleep(Duration::from_millis(100));

    let result = acquire_source_lock(tmp.path(), &key, Duration::from_millis(300));
    assert!(
        matches!(result, Err(LockError::Timeout { .. })),
        "expected Timeout; got {result:?}"
    );

    done.store(true, Ordering::SeqCst);
    holder.join().unwrap();
}

#[tokio::test]
async fn test_ingest_records_lock_wait_ms_field() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("palace.db");
    Database::open(&db_path).expect("init db");

    let file = write_file(tmp.path(), "doc.md", "lock wait ms visibility test");
    let db = Database::open(&db_path).expect("open");

    let stats =
        ingest_file_with_options(&db, &StubEmbedder, &file, "test", IngestOptions::default())
            .await
            .expect("ingest");

    assert!(
        stats.lock_wait_ms.is_some(),
        "non-dry-run ingest must record lock_wait_ms"
    );
    // Uncontested acquire → wait should be near zero.
    assert!(stats.lock_wait_ms.unwrap() < 100);
}

#[cfg(unix)]
#[test]
fn test_panic_in_critical_section_releases_lock() {
    use mempal::ingest::lock::{acquire_source_lock, source_key};

    let tmp = Arc::new(TempDir::new().unwrap());
    let key = source_key(Path::new("/tmp/test-panic-release"));

    let tmp_panic = Arc::clone(&tmp);
    let key_panic = key.clone();
    let result = std::panic::catch_unwind(move || {
        let _guard = acquire_source_lock(tmp_panic.path(), &key_panic, Duration::from_secs(1))
            .expect("acquire in panic thread");
        panic!("simulated panic inside critical section");
    });
    assert!(result.is_err(), "panic should have been caught");

    // Second acquire from main thread must succeed — OS released flock on
    // file close when the guard was dropped during unwind.
    let guard = acquire_source_lock(tmp.path(), &key, Duration::from_millis(500))
        .expect("acquire after panic");
    assert!(guard.wait_duration() < Duration::from_millis(500));

    let lock_path = tmp.path().join("locks").join(format!("{key}.lock"));
    assert!(
        lock_path.exists(),
        "lock file should remain on disk for reuse"
    );
}
