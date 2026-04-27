use mempal::core::db::Database;
use mempal::core::types::{BootstrapEvidenceArgs, Drawer, SourceType};
use mempal::embed::{Embedder, EmbedderFactory};
use mempal::ingest::lock::{acquire_source_lock, source_key};
use mempal::ingest::normalize::CURRENT_NORMALIZE_VERSION;
use mempal::ingest::reindex::{ReindexMode, ReindexOptions, reindex_sources};
use mempal::ingest::{IngestOptions, ingest_file_with_options};
use mempal::mcp::MempalMcpServer;
use rusqlite::Connection;
use std::process::Command;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

struct StubEmbedder;

struct StubEmbedderFactory;

#[async_trait::async_trait]
impl EmbedderFactory for StubEmbedderFactory {
    async fn build(&self) -> mempal::embed::Result<Box<dyn Embedder>> {
        Ok(Box::new(StubEmbedder))
    }
}

#[async_trait::async_trait]
impl Embedder for StubEmbedder {
    async fn embed(&self, texts: &[&str]) -> mempal::embed::Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|_| vec![0.1, 0.2, 0.3]).collect())
    }

    fn dimensions(&self) -> usize {
        3
    }

    fn name(&self) -> &str {
        "stub"
    }
}

fn create_v6_db(path: &std::path::Path, drawer_count: usize) {
    let conn = Connection::open(path).expect("open v6 db");
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = ON;

        CREATE TABLE drawers (
            id TEXT PRIMARY KEY,
            content TEXT NOT NULL,
            wing TEXT NOT NULL,
            room TEXT,
            source_file TEXT,
            source_type TEXT NOT NULL CHECK(source_type IN ('project', 'conversation', 'manual')),
            added_at TEXT NOT NULL,
            chunk_index INTEGER,
            deleted_at TEXT,
            importance INTEGER DEFAULT 0,
            memory_kind TEXT NOT NULL CHECK(memory_kind IN ('evidence', 'knowledge')) DEFAULT 'evidence',
            domain TEXT NOT NULL CHECK(domain IN ('project', 'agent', 'skill', 'global')) DEFAULT 'project',
            field TEXT NOT NULL DEFAULT 'general',
            anchor_kind TEXT NOT NULL CHECK(anchor_kind IN ('global', 'repo', 'worktree')) DEFAULT 'repo',
            anchor_id TEXT NOT NULL DEFAULT 'repo://legacy',
            parent_anchor_id TEXT,
            provenance TEXT CHECK(provenance IN ('runtime', 'research', 'human')),
            statement TEXT,
            tier TEXT CHECK(tier IN ('qi', 'shu', 'dao_ren', 'dao_tian')),
            status TEXT CHECK(status IN ('candidate', 'promoted', 'canonical', 'demoted', 'retired')),
            supporting_refs TEXT NOT NULL DEFAULT '[]',
            counterexample_refs TEXT NOT NULL DEFAULT '[]',
            teaching_refs TEXT NOT NULL DEFAULT '[]',
            verification_refs TEXT NOT NULL DEFAULT '[]',
            scope_constraints TEXT,
            trigger_hints TEXT
        );

        CREATE TABLE triples (
            id TEXT PRIMARY KEY,
            subject TEXT NOT NULL,
            predicate TEXT NOT NULL,
            object TEXT NOT NULL,
            valid_from TEXT,
            valid_to TEXT,
            confidence REAL DEFAULT 1.0,
            source_drawer TEXT REFERENCES drawers(id)
        );

        CREATE TABLE taxonomy (
            wing TEXT NOT NULL,
            room TEXT NOT NULL DEFAULT '',
            display_name TEXT,
            keywords TEXT,
            PRIMARY KEY (wing, room)
        );

        CREATE TABLE tunnels (
            id TEXT PRIMARY KEY,
            left_wing TEXT NOT NULL,
            left_room TEXT,
            right_wing TEXT NOT NULL,
            right_room TEXT,
            label TEXT NOT NULL,
            created_at TEXT NOT NULL,
            created_by TEXT,
            deleted_at TEXT
        );

        CREATE INDEX idx_drawers_wing ON drawers(wing);
        CREATE INDEX idx_drawers_wing_room ON drawers(wing, room);
        CREATE INDEX idx_drawers_deleted_at ON drawers(deleted_at);
        CREATE INDEX idx_triples_subject ON triples(subject);
        CREATE INDEX idx_triples_object ON triples(object);
        CREATE INDEX idx_tunnels_left ON tunnels(left_wing, left_room) WHERE deleted_at IS NULL;
        CREATE INDEX idx_tunnels_right ON tunnels(right_wing, right_room) WHERE deleted_at IS NULL;

        CREATE VIRTUAL TABLE drawers_fts USING fts5(
            content,
            content='drawers',
            content_rowid='rowid'
        );

        CREATE TRIGGER drawers_ai AFTER INSERT ON drawers BEGIN
            INSERT INTO drawers_fts(rowid, content) VALUES (new.rowid, new.content);
        END;

        CREATE TRIGGER drawers_au_softdelete AFTER UPDATE OF deleted_at ON drawers
            WHEN new.deleted_at IS NOT NULL AND old.deleted_at IS NULL BEGIN
            INSERT INTO drawers_fts(drawers_fts, rowid, content)
            VALUES ('delete', old.rowid, old.content);
        END;

        PRAGMA user_version = 6;
        "#,
    )
    .expect("apply v6 schema");

    for index in 0..drawer_count {
        conn.execute(
            r#"
            INSERT INTO drawers (
                id, content, wing, room, source_file, source_type, added_at, chunk_index,
                deleted_at, importance, provenance
            )
            VALUES (?1, ?2, 'mempal', 'normalize', ?3, 'project', '1710000000', ?4, NULL, 0, 'research')
            "#,
            (
                format!("drawer_{index:03}"),
                format!("content {index}"),
                format!("doc-{index}.md"),
                index as i64,
            ),
        )
        .expect("insert v6 drawer");
    }
}

fn count_normalize_version(db: &Database, version: u32) -> i64 {
    db.conn()
        .query_row(
            "SELECT COUNT(*) FROM drawers WHERE normalize_version = ?1",
            [version],
            |row| row.get(0),
        )
        .expect("count normalize_version")
}

fn insert_versioned_drawer(
    db: &Database,
    id: &str,
    source_file: &str,
    content: &str,
    normalize_version: u32,
) {
    let mut drawer = Drawer::new_bootstrap_evidence(BootstrapEvidenceArgs {
        id: id.to_string(),
        content: content.to_string(),
        wing: "mempal".to_string(),
        room: Some("normalize".to_string()),
        source_file: Some(source_file.to_string()),
        source_type: SourceType::Project,
        added_at: "1710000000".to_string(),
        chunk_index: Some(0),
        importance: 0,
    });
    drawer.normalize_version = normalize_version;
    db.insert_drawer(&drawer).expect("insert versioned drawer");
}

fn active_drawer_versions(db: &Database) -> Vec<u32> {
    let mut statement = db
        .conn()
        .prepare(
            r#"
            SELECT normalize_version
            FROM drawers
            WHERE deleted_at IS NULL
            ORDER BY id
            "#,
        )
        .expect("prepare active versions");
    statement
        .query_map([], |row| row.get::<_, u32>(0))
        .expect("query active versions")
        .collect::<std::result::Result<Vec<_>, _>>()
        .expect("collect active versions")
}

fn stale_drawer_count_raw(db: &Database) -> i64 {
    db.conn()
        .query_row(
            "SELECT COUNT(*) FROM drawers WHERE deleted_at IS NULL AND normalize_version < ?1",
            [CURRENT_NORMALIZE_VERSION],
            |row| row.get(0),
        )
        .expect("count stale drawers")
}

fn mempal_bin() -> String {
    env!("CARGO_BIN_EXE_mempal").to_string()
}

fn write_cli_config(home: &std::path::Path, db_path: &std::path::Path) {
    let mempal_dir = home.join(".mempal");
    std::fs::create_dir_all(&mempal_dir).expect("create .mempal");
    std::fs::write(
        mempal_dir.join("config.toml"),
        format!("db_path = \"{}\"\n", db_path.display()),
    )
    .expect("write config");
}

fn setup_mcp_server() -> (TempDir, Database, MempalMcpServer) {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("palace.db");
    let db = Database::open(&db_path).expect("open db");
    let server = MempalMcpServer::new_with_factory(db_path, Arc::new(StubEmbedderFactory));
    (tmp, db, server)
}

#[test]
fn test_migration_v6_to_v7_stamps_normalize_version_1() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("palace.db");
    create_v6_db(&db_path, 20);

    let db = Database::open(&db_path).expect("migrate v6 db");

    assert_eq!(db.schema_version().expect("schema version"), 8);
    assert_eq!(db.drawer_count().expect("drawer count"), 20);
    assert_eq!(count_normalize_version(&db, 1), 20);
}

#[test]
fn test_drawer_count_by_normalize_version_and_stale_count() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("palace.db");
    create_v6_db(&db_path, 20);
    let db = Database::open(&db_path).expect("migrate v6 db");

    db.conn()
        .execute(
            "UPDATE drawers SET normalize_version = 0 WHERE id IN ('drawer_000', 'drawer_001', 'drawer_002', 'drawer_003', 'drawer_004')",
            [],
        )
        .expect("mark stale drawers");

    assert_eq!(db.stale_drawer_count(1).expect("stale count"), 5);
    assert_eq!(
        db.drawer_count_by_normalize_version()
            .expect("version histogram"),
        vec![(0, 5), (1, 15)]
    );
}

#[tokio::test]
async fn test_new_ingest_writes_current_normalize_version() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("palace.db");
    let db = Database::open(&db_path).expect("open db");
    let source = tmp.path().join("doc.md");
    std::fs::write(&source, "normalize version source content").expect("write source");

    ingest_file_with_options(
        &db,
        &StubEmbedder,
        &source,
        "mempal",
        IngestOptions {
            room: Some("normalize"),
            source_root: source.parent(),
            dry_run: false,
            source_file_override: None,
            replace_existing_source: false,
            no_strip_noise: false,
            ..IngestOptions::default()
        },
    )
    .await
    .expect("ingest source");

    let versions = distinct_versions_for_source(&db, "doc.md");
    assert_eq!(versions, vec![CURRENT_NORMALIZE_VERSION]);
}

fn distinct_versions_for_source(db: &Database, source_file: &str) -> Vec<u32> {
    let mut statement = db
        .conn()
        .prepare(
            r#"
            SELECT DISTINCT normalize_version
            FROM drawers
            WHERE source_file = ?1
            ORDER BY normalize_version
            "#,
        )
        .expect("prepare versions");
    statement
        .query_map([source_file], |row| row.get::<_, u32>(0))
        .expect("query versions")
        .collect::<std::result::Result<Vec<_>, _>>()
        .expect("collect versions")
}

#[tokio::test]
async fn test_reindex_dry_run_no_writes() {
    let tmp = TempDir::new().expect("tempdir");
    let db = Database::open(&tmp.path().join("palace.db")).expect("open db");
    let source = tmp.path().join("doc.md");
    std::fs::write(&source, "fresh source content").expect("write source");
    insert_versioned_drawer(
        &db,
        "drawer_stale_001",
        &source.to_string_lossy(),
        "old source content",
        0,
    );

    let report = reindex_sources(
        &db,
        &StubEmbedder,
        ReindexOptions {
            mode: ReindexMode::Stale,
            dry_run: true,
        },
    )
    .await
    .expect("dry-run reindex");

    assert_eq!(report.candidate_drawers, 1);
    assert_eq!(report.candidate_sources, 1);
    assert_eq!(report.processed_sources, 0);
    assert_eq!(stale_drawer_count_raw(&db), 1);
}

#[tokio::test]
async fn test_reindex_stale_only_reprocesses_outdated() {
    let tmp = TempDir::new().expect("tempdir");
    let db = Database::open(&tmp.path().join("palace.db")).expect("open db");
    let stale_source = tmp.path().join("stale.md");
    let current_source = tmp.path().join("current.md");
    std::fs::write(&stale_source, "fresh stale replacement").expect("write stale source");
    std::fs::write(&current_source, "current source").expect("write current source");

    for index in 0..5 {
        insert_versioned_drawer(
            &db,
            &format!("drawer_stale_{index:03}"),
            &stale_source.to_string_lossy(),
            &format!("old stale {index}"),
            0,
        );
    }
    for index in 0..15 {
        insert_versioned_drawer(
            &db,
            &format!("drawer_current_{index:03}"),
            &current_source.to_string_lossy(),
            &format!("current {index}"),
            CURRENT_NORMALIZE_VERSION,
        );
    }

    let report = reindex_sources(
        &db,
        &StubEmbedder,
        ReindexOptions {
            mode: ReindexMode::Stale,
            dry_run: false,
        },
    )
    .await
    .expect("stale reindex");

    assert_eq!(report.candidate_drawers, 5);
    assert_eq!(report.candidate_sources, 1);
    assert_eq!(report.processed_sources, 1);
    assert_eq!(stale_drawer_count_raw(&db), 0);
    assert!(
        active_drawer_versions(&db)
            .into_iter()
            .all(|version| version == CURRENT_NORMALIZE_VERSION)
    );
    assert_eq!(
        distinct_versions_for_source(&db, &current_source.to_string_lossy()),
        vec![CURRENT_NORMALIZE_VERSION]
    );
}

#[tokio::test]
async fn test_reindex_force_reprocesses_all() {
    let tmp = TempDir::new().expect("tempdir");
    let db = Database::open(&tmp.path().join("palace.db")).expect("open db");
    let source_a = tmp.path().join("a.md");
    let source_b = tmp.path().join("b.md");
    std::fs::write(&source_a, "source a replacement").expect("write source a");
    std::fs::write(&source_b, "source b replacement").expect("write source b");
    insert_versioned_drawer(
        &db,
        "drawer_force_a",
        &source_a.to_string_lossy(),
        "old a",
        0,
    );
    insert_versioned_drawer(
        &db,
        "drawer_force_b",
        &source_b.to_string_lossy(),
        "old b",
        CURRENT_NORMALIZE_VERSION,
    );

    let report = reindex_sources(
        &db,
        &StubEmbedder,
        ReindexOptions {
            mode: ReindexMode::Force,
            dry_run: false,
        },
    )
    .await
    .expect("force reindex");

    assert_eq!(report.candidate_drawers, 2);
    assert_eq!(report.candidate_sources, 2);
    assert_eq!(report.processed_sources, 2);
    assert_eq!(stale_drawer_count_raw(&db), 0);
}

#[tokio::test]
async fn test_reindex_skips_missing_source_file() {
    let tmp = TempDir::new().expect("tempdir");
    let db = Database::open(&tmp.path().join("palace.db")).expect("open db");
    let missing = tmp.path().join("missing.md");
    insert_versioned_drawer(
        &db,
        "drawer_missing_source",
        &missing.to_string_lossy(),
        "old missing",
        0,
    );

    let report = reindex_sources(
        &db,
        &StubEmbedder,
        ReindexOptions {
            mode: ReindexMode::Stale,
            dry_run: false,
        },
    )
    .await
    .expect("missing-source reindex");

    assert_eq!(report.candidate_drawers, 1);
    assert_eq!(report.processed_sources, 0);
    assert_eq!(report.skipped_missing_sources, 1);
    assert_eq!(report.skipped_missing_drawers, 1);
    assert_eq!(stale_drawer_count_raw(&db), 1);
}

#[test]
fn test_cli_reindex_stale_dry_run_reports_without_writes() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("palace.db");
    write_cli_config(tmp.path(), &db_path);
    let db = Database::open(&db_path).expect("open db");
    let missing = tmp.path().join("missing.md");
    for index in 0..5 {
        insert_versioned_drawer(
            &db,
            &format!("drawer_cli_stale_{index:03}"),
            &missing.to_string_lossy(),
            &format!("old cli stale {index}"),
            0,
        );
    }

    let output = Command::new(mempal_bin())
        .args(["reindex", "--stale", "--dry-run"])
        .env("HOME", tmp.path())
        .output()
        .expect("run reindex dry-run");

    assert!(
        output.status.success(),
        "reindex dry-run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("would reprocess 5 drawers"));
    assert_eq!(stale_drawer_count_raw(&db), 5);
}

#[tokio::test]
async fn test_status_exposes_stale_count() {
    let (_tmp, db, server) = setup_mcp_server();
    for index in 0..5 {
        insert_versioned_drawer(
            &db,
            &format!("drawer_status_stale_{index:03}"),
            &format!("status-stale-{index}.md"),
            &format!("old status stale {index}"),
            0,
        );
    }
    for index in 0..15 {
        insert_versioned_drawer(
            &db,
            &format!("drawer_status_current_{index:03}"),
            &format!("status-current-{index}.md"),
            &format!("current status {index}"),
            CURRENT_NORMALIZE_VERSION,
        );
    }

    let status = server
        .status_json_for_test()
        .await
        .expect("status should succeed");

    assert_eq!(status.normalize_version_current, CURRENT_NORMALIZE_VERSION);
    assert_eq!(status.stale_drawer_count, 5);
}

#[test]
fn test_reindex_respects_per_source_lock() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("palace.db");
    let db = Database::open(&db_path).expect("open db");
    let source = tmp.path().join("locked.md");
    std::fs::write(&source, "fresh locked source").expect("write locked source");
    let source_file = source.to_string_lossy().to_string();
    insert_versioned_drawer(&db, "drawer_locked", &source_file, "old locked", 0);
    drop(db);

    let key = source_key(std::path::Path::new(&source_file));
    let guard = acquire_source_lock(tmp.path(), &key, Duration::from_secs(1))
        .expect("acquire manual source lock");
    let thread_db_path = db_path.clone();
    let handle = thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        runtime.block_on(async move {
            let db = Database::open(&thread_db_path).expect("open db in thread");
            reindex_sources(
                &db,
                &StubEmbedder,
                ReindexOptions {
                    mode: ReindexMode::Stale,
                    dry_run: false,
                },
            )
            .await
            .expect("reindex under lock")
        })
    });

    thread::sleep(Duration::from_millis(150));
    assert!(
        !handle.is_finished(),
        "reindex must wait for the per-source lock"
    );
    drop(guard);
    let report = handle.join().expect("join reindex thread");

    assert_eq!(report.processed_sources, 1);
    let db = Database::open(&db_path).expect("reopen db");
    assert_eq!(stale_drawer_count_raw(&db), 0);
}
