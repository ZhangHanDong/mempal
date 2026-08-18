//! P117: reindex must never destroy data it cannot rebuild, and dry-run
//! must be an honest feasibility report.

use mempal::core::db::Database;
use mempal::core::types::{BootstrapEvidenceArgs, Drawer, SourceType};
use mempal::embed::Embedder;
use mempal::ingest::reindex::{ReindexMode, ReindexOptions, reindex_sources};
use tempfile::TempDir;

struct StubEmbedder;

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

/// Fails every embed call — models the embedding backend being down.
struct FailingEmbedder;

#[async_trait::async_trait]
impl Embedder for FailingEmbedder {
    async fn embed(&self, _texts: &[&str]) -> mempal::embed::Result<Vec<Vec<f32>>> {
        Err(mempal::embed::EmbedError::Runtime(
            "embedding backend unavailable".into(),
        ))
    }

    fn dimensions(&self) -> usize {
        3
    }

    fn name(&self) -> &str {
        "failing"
    }
}

fn insert_stale_drawer(db: &Database, id: &str, source_file: &str, content: &str) {
    let mut drawer = Drawer::new_bootstrap_evidence(BootstrapEvidenceArgs {
        id: id.to_string(),
        content: content.to_string(),
        wing: "mempal".to_string(),
        room: Some("reindex".to_string()),
        source_file: Some(source_file.to_string()),
        source_type: SourceType::Conversation,
        added_at: "1710000000".to_string(),
        chunk_index: Some(0),
        importance: 0,
    });
    drawer.normalize_version = 1;
    db.insert_drawer(&drawer).expect("insert stale drawer");
}

fn active_contents(db: &Database) -> Vec<String> {
    let mut statement = db
        .conn()
        .prepare("SELECT content FROM drawers WHERE deleted_at IS NULL ORDER BY id")
        .expect("prepare");
    statement
        .query_map([], |row| row.get::<_, String>(0))
        .expect("query")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect")
}

#[test]
fn with_immediate_transaction_rolls_back_on_error() {
    let tmp = TempDir::new().expect("tempdir");
    let db = Database::open(&tmp.path().join("palace.db")).expect("open db");
    insert_stale_drawer(&db, "drawer_txn_keep", "/tmp/p117-txn.md", "keep me");

    let result: Result<(), _> = db.with_immediate_transaction(|db| {
        db.replace_active_source_drawers_across_rooms_in_txn("/tmp/p117-txn.md", "mempal")?;
        Err(mempal::core::db::DbError::InvalidDrawerMetadata(
            "boom".into(),
        ))
    });

    assert!(result.is_err());
    assert_eq!(
        active_contents(&db),
        vec!["keep me".to_string()],
        "an error inside the transaction must roll back the delete"
    );
}

#[tokio::test]
async fn reindex_embed_failure_preserves_existing_drawers() {
    let tmp = TempDir::new().expect("tempdir");
    let db = Database::open(&tmp.path().join("palace.db")).expect("open db");
    let source = tmp.path().join("claude.jsonl");
    std::fs::write(
        &source,
        serde_json::json!({"type":"assistant","message":"precious content"}).to_string(),
    )
    .expect("write source");
    insert_stale_drawer(
        &db,
        "drawer_stale_precious",
        &source.to_string_lossy(),
        "precious content",
    );

    let result = reindex_sources(
        &db,
        &FailingEmbedder,
        ReindexOptions {
            mode: ReindexMode::Stale,
            dry_run: false,
        },
    )
    .await;

    assert!(result.is_err(), "reindex must surface the embed failure");
    let contents = active_contents(&db);
    assert_eq!(
        contents,
        vec!["precious content".to_string()],
        "an embed failure must not destroy the existing drawers"
    );
}

#[tokio::test]
async fn reindex_replaces_source_without_duplicates() {
    let tmp = TempDir::new().expect("tempdir");
    let db = Database::open(&tmp.path().join("palace.db")).expect("open db");
    let source = tmp.path().join("claude.jsonl");
    std::fs::write(
        &source,
        serde_json::json!({"type":"assistant","message":"fresh content"}).to_string(),
    )
    .expect("write source");
    insert_stale_drawer(
        &db,
        "drawer_stale_old",
        &source.to_string_lossy(),
        "outdated normalized content",
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
    .expect("reindex");

    assert_eq!(report.processed_sources, 1);
    let contents = active_contents(&db);
    assert_eq!(
        contents,
        vec!["fresh content".to_string()],
        "old drawers replaced by freshly normalized ones, no leftovers"
    );
    let version: u32 = db
        .conn()
        .query_row(
            "SELECT normalize_version FROM drawers WHERE deleted_at IS NULL",
            [],
            |row| row.get(0),
        )
        .expect("read normalize_version");
    assert_eq!(
        version,
        mempal::ingest::normalize::CURRENT_NORMALIZE_VERSION
    );
}

#[tokio::test]
async fn reindex_dry_run_reports_missing_sources() {
    let tmp = TempDir::new().expect("tempdir");
    let db = Database::open(&tmp.path().join("palace.db")).expect("open db");
    let existing = tmp.path().join("exists.jsonl");
    std::fs::write(
        &existing,
        serde_json::json!({"type":"assistant","message":"still here"}).to_string(),
    )
    .expect("write source");
    insert_stale_drawer(
        &db,
        "drawer_stale_exists",
        &existing.to_string_lossy(),
        "still here",
    );
    insert_stale_drawer(
        &db,
        "drawer_stale_gone",
        &tmp.path().join("gone.jsonl").to_string_lossy(),
        "source vanished",
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

    assert_eq!(report.candidate_sources, 2);
    assert_eq!(report.candidate_drawers, 2);
    assert_eq!(
        report.skipped_missing_sources, 1,
        "dry-run must surface sources whose file no longer exists: {report:?}"
    );
    assert_eq!(report.skipped_missing_drawers, 1);

    // dry-run must not have touched anything
    let contents = active_contents(&db);
    assert_eq!(contents.len(), 2);
}
