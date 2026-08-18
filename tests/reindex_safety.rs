//! P117: reindex must never destroy data it cannot rebuild, and dry-run
//! must be an honest feasibility report.

use mempal::core::db::Database;
use mempal::core::types::{
    AnchorKind, BootstrapEvidenceArgs, Drawer, KnowledgeCard, KnowledgeEvidenceLink,
    KnowledgeEvidenceRole, KnowledgeStatus, KnowledgeTier, MemoryDomain, MemoryKind, SourceType,
};
use mempal::core::utils::build_bootstrap_evidence_drawer_id;
use mempal::embed::Embedder;
use mempal::ingest::normalize::CURRENT_NORMALIZE_VERSION;
use mempal::ingest::reindex::{ReindexError, ReindexMode, ReindexOptions, reindex_sources};
use mempal::ingest::{IngestError, IngestOptions, ingest_file_with_options};
use std::process::Command;
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

/// Violates the embedder batch contract without returning an error.
struct MismatchedEmbedder {
    vector_count: usize,
}

#[async_trait::async_trait]
impl Embedder for MismatchedEmbedder {
    async fn embed(&self, _texts: &[&str]) -> mempal::embed::Result<Vec<Vec<f32>>> {
        Ok(vec![vec![0.1, 0.2, 0.3]; self.vector_count])
    }

    fn dimensions(&self) -> usize {
        3
    }

    fn name(&self) -> &str {
        "mismatched"
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

fn insert_stage1_knowledge_reference(db: &Database, knowledge_id: &str, evidence_id: &str) {
    let mut drawer = Drawer::new_bootstrap_evidence(BootstrapEvidenceArgs {
        id: knowledge_id.to_string(),
        content: "governed knowledge content".to_string(),
        wing: "mempal".to_string(),
        room: Some("knowledge".to_string()),
        source_file: Some(format!("knowledge://project/p117/{knowledge_id}")),
        source_type: SourceType::Manual,
        added_at: "1710000002".to_string(),
        chunk_index: Some(0),
        importance: 4,
    });
    drawer.normalize_version = CURRENT_NORMALIZE_VERSION;
    drawer.memory_kind = MemoryKind::Knowledge;
    drawer.provenance = None;
    drawer.statement = Some("Protected evidence must remain citable.".to_string());
    drawer.tier = Some(KnowledgeTier::Qi);
    drawer.status = Some(KnowledgeStatus::Candidate);
    drawer.supporting_refs = vec![evidence_id.to_string()];
    db.insert_drawer(&drawer)
        .expect("insert Stage-1 knowledge reference");
}

fn insert_phase2_knowledge_reference(db: &Database, evidence_id: &str) {
    db.insert_knowledge_card(&KnowledgeCard {
        id: "card_p117_protected".to_string(),
        statement: "Protected evidence must remain citable.".to_string(),
        content: "P117 reference-safety regression card.".to_string(),
        tier: KnowledgeTier::Qi,
        status: KnowledgeStatus::Candidate,
        domain: MemoryDomain::Project,
        field: "software-engineering".to_string(),
        anchor_kind: AnchorKind::Repo,
        anchor_id: "repo://mempal".to_string(),
        parent_anchor_id: None,
        scope_constraints: None,
        trigger_hints: None,
        created_at: "1710000002".to_string(),
        updated_at: "1710000002".to_string(),
    })
    .expect("insert Phase-2 card");
    db.insert_knowledge_evidence_link(&KnowledgeEvidenceLink {
        id: "link_p117_protected".to_string(),
        card_id: "card_p117_protected".to_string(),
        evidence_drawer_id: evidence_id.to_string(),
        role: KnowledgeEvidenceRole::Supporting,
        note: Some("protect this evidence during reindex".to_string()),
        created_at: "1710000002".to_string(),
    })
    .expect("insert Phase-2 evidence link");
}

fn write_cli_config(home: &std::path::Path, db_path: &std::path::Path) {
    let mempal_dir = home.join(".mempal");
    std::fs::create_dir_all(&mempal_dir).expect("create .mempal");
    std::fs::write(
        mempal_dir.join("config.toml"),
        format!(
            "db_path = \"{}\"\n\n[embed]\nbackend = \"api\"\ndimensions = 3\n",
            db_path.display()
        ),
    )
    .expect("write config");
}

fn mempal_bin() -> String {
    env!("CARGO_BIN_EXE_mempal").to_string()
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

#[test]
fn with_immediate_transaction_rolls_back_on_commit_failure() {
    let tmp = TempDir::new().expect("tempdir");
    let db = Database::open(&tmp.path().join("palace.db")).expect("open db");
    db.conn()
        .execute_batch(
            r#"
            CREATE TABLE deferred_parent (id INTEGER PRIMARY KEY);
            CREATE TABLE deferred_child (
                parent_id INTEGER,
                FOREIGN KEY(parent_id) REFERENCES deferred_parent(id)
                    DEFERRABLE INITIALLY DEFERRED
            );
            "#,
        )
        .expect("create deferred foreign key tables");

    let result: Result<(), mempal::core::db::DbError> = db.with_immediate_transaction(|db| {
        db.conn()
            .execute("INSERT INTO deferred_child(parent_id) VALUES (42)", [])?;
        Ok(())
    });

    assert!(result.is_err(), "deferred foreign key must reject COMMIT");
    assert!(
        db.conn().is_autocommit(),
        "a failed COMMIT must not leave the connection inside a transaction"
    );
    let count: i64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM deferred_child", [], |row| row.get(0))
        .expect("count deferred children");
    assert_eq!(count, 0, "rollback must remove the violating row");
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
async fn reindex_embedding_count_mismatch_preserves_existing_drawers() {
    for actual in [0, 2] {
        let tmp = TempDir::new().expect("tempdir");
        let db = Database::open(&tmp.path().join("palace.db")).expect("open db");
        let source = tmp.path().join("mismatch.md");
        std::fs::write(&source, "fresh content").expect("write source");
        insert_stale_drawer(
            &db,
            "drawer_stale_mismatch",
            &source.to_string_lossy(),
            "precious old content",
        );

        let error = reindex_sources(
            &db,
            &MismatchedEmbedder {
                vector_count: actual,
            },
            ReindexOptions {
                mode: ReindexMode::Stale,
                dry_run: false,
            },
        )
        .await
        .expect_err("a mismatched embedding batch must fail reindex");

        match error {
            ReindexError::Ingest {
                source:
                    IngestError::EmbeddingCountMismatch {
                        expected,
                        actual: reported,
                        ..
                    },
                ..
            } => {
                assert_eq!(expected, 1);
                assert_eq!(reported, actual);
            }
            other => panic!("unexpected reindex error: {other:?}"),
        }
        assert_eq!(
            active_contents(&db),
            vec!["precious old content".to_string()],
            "a mismatched embedding batch must not destroy the existing drawer"
        );
    }
}

#[tokio::test]
async fn reindex_insert_collision_preserves_existing_drawers() {
    let tmp = TempDir::new().expect("tempdir");
    let db = Database::open(&tmp.path().join("palace.db")).expect("open db");
    let source = tmp.path().join("collision.md");
    std::fs::write(&source, "fresh content").expect("write source");
    let source_file = source.to_string_lossy().to_string();
    insert_stale_drawer(
        &db,
        "drawer_stale_collision",
        &source_file,
        "precious old content",
    );

    let replacement_id = build_bootstrap_evidence_drawer_id(
        "mempal",
        Some("reindex"),
        "fresh content",
        &SourceType::Project,
        Some(&source_file),
    );
    let mut colliding = Drawer::new_bootstrap_evidence(BootstrapEvidenceArgs {
        id: replacement_id,
        content: "unrelated source content".to_string(),
        wing: "mempal".to_string(),
        room: Some("other".to_string()),
        source_file: Some("other-source.md".to_string()),
        source_type: SourceType::Project,
        added_at: "1710000001".to_string(),
        chunk_index: Some(0),
        importance: 0,
    });
    colliding.normalize_version = CURRENT_NORMALIZE_VERSION;
    db.insert_drawer(&colliding)
        .expect("insert colliding drawer from another source");

    let error = reindex_sources(
        &db,
        &StubEmbedder,
        ReindexOptions {
            mode: ReindexMode::Stale,
            dry_run: false,
        },
    )
    .await
    .expect_err("a replacement id collision must fail reindex");

    assert!(
        matches!(
            error,
            ReindexError::Ingest {
                source: IngestError::ReplacementDrawerCollision { .. },
                ..
            }
        ),
        "error must identify the replacement collision: {error:?}"
    );
    let contents = active_contents(&db);
    assert_eq!(contents.len(), 2, "both pre-existing drawers must remain");
    assert!(contents.iter().any(|value| value == "precious old content"));
    assert!(
        contents
            .iter()
            .any(|value| value == "unrelated source content")
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

#[tokio::test]
async fn reindex_skips_sources_with_knowledge_references() {
    let tmp = TempDir::new().expect("tempdir");
    let db = Database::open(&tmp.path().join("palace.db")).expect("open db");
    let source = tmp.path().join("protected.md");
    std::fs::write(&source, "fresh content").expect("write source");
    insert_stale_drawer(
        &db,
        "drawer_stale_protected",
        &source.to_string_lossy(),
        "precious governed evidence",
    );
    insert_stage1_knowledge_reference(&db, "drawer_knowledge_protected", "drawer_stale_protected");
    insert_phase2_knowledge_reference(&db, "drawer_stale_protected");

    let dry_run = reindex_sources(
        &db,
        &StubEmbedder,
        ReindexOptions {
            mode: ReindexMode::Stale,
            dry_run: true,
        },
    )
    .await
    .expect("dry-run must report protected sources");
    assert_eq!(dry_run.skipped_protected_sources, 1);
    assert_eq!(dry_run.skipped_protected_drawers, 1);
    assert_eq!(dry_run.protecting_references, 2);

    let real_run = reindex_sources(
        &db,
        &StubEmbedder,
        ReindexOptions {
            mode: ReindexMode::Stale,
            dry_run: false,
        },
    )
    .await
    .expect("real reindex must safely skip protected sources");
    assert_eq!(real_run.processed_sources, 0);
    assert_eq!(real_run.skipped_protected_sources, 1);
    assert_eq!(real_run.skipped_protected_drawers, 1);
    assert_eq!(real_run.protecting_references, 2);

    let evidence = db
        .get_drawer("drawer_stale_protected")
        .expect("load evidence")
        .expect("evidence remains");
    assert_eq!(evidence.content, "precious governed evidence");
    let knowledge = db
        .get_drawer("drawer_knowledge_protected")
        .expect("load knowledge")
        .expect("knowledge remains");
    assert_eq!(knowledge.supporting_refs, vec!["drawer_stale_protected"]);
    assert_eq!(
        db.knowledge_evidence_links_for_drawer("drawer_stale_protected")
            .expect("load Phase-2 links")
            .len(),
        1
    );
}

#[tokio::test]
async fn replace_transaction_rechecks_knowledge_references() {
    let tmp = TempDir::new().expect("tempdir");
    let db = Database::open(&tmp.path().join("palace.db")).expect("open db");
    let source = tmp.path().join("protected-direct.md");
    std::fs::write(&source, "fresh content").expect("write source");
    let source_file = source.to_string_lossy().to_string();
    insert_stale_drawer(
        &db,
        "drawer_stale_direct_protected",
        &source_file,
        "precious direct evidence",
    );
    insert_stage1_knowledge_reference(
        &db,
        "drawer_knowledge_direct_protected",
        "drawer_stale_direct_protected",
    );

    let error = ingest_file_with_options(
        &db,
        &StubEmbedder,
        &source,
        "mempal",
        IngestOptions {
            room: Some("reindex"),
            source_root: source.parent(),
            source_file_override: Some(&source_file),
            replace_existing_source: true,
            replace_across_rooms: true,
            ..IngestOptions::default()
        },
    )
    .await
    .expect_err("the replacement transaction must reject a protected source");

    assert!(
        format!("{error:?}").contains("SourceProtectedByKnowledgeReferences"),
        "error must identify the transactional reference guard: {error:?}"
    );
    assert_eq!(
        db.get_drawer("drawer_stale_direct_protected")
            .expect("load evidence")
            .expect("evidence remains")
            .content,
        "precious direct evidence"
    );
    assert_eq!(
        db.get_drawer("drawer_knowledge_direct_protected")
            .expect("load knowledge")
            .expect("knowledge remains")
            .supporting_refs,
        vec!["drawer_stale_direct_protected"]
    );
}

#[test]
fn reindex_cli_reports_governance_protected_sources() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("palace.db");
    write_cli_config(tmp.path(), &db_path);
    let db = Database::open(&db_path).expect("open db");
    let source = tmp.path().join("protected-cli.md");
    std::fs::write(&source, "fresh content").expect("write source");
    insert_stale_drawer(
        &db,
        "drawer_stale_cli_protected",
        &source.to_string_lossy(),
        "precious CLI evidence",
    );
    insert_stage1_knowledge_reference(
        &db,
        "drawer_knowledge_cli_protected",
        "drawer_stale_cli_protected",
    );

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
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(
            "would skip 1 drawers from 1 governance-protected sources (1 knowledge references)"
        ),
        "protected-source summary missing from stdout: {stdout}"
    );

    let output = Command::new(mempal_bin())
        .args(["reindex", "--stale"])
        .env("HOME", tmp.path())
        .output()
        .expect("run reindex");
    assert!(
        output.status.success(),
        "reindex failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(
            "skipped 1 governance-protected drawers from 1 sources (1 knowledge references)"
        ),
        "real-run protected-source summary missing from stdout: {stdout}"
    );
    assert_eq!(
        db.get_drawer("drawer_stale_cli_protected")
            .expect("load evidence")
            .expect("evidence remains")
            .content,
        "precious CLI evidence"
    );
}
