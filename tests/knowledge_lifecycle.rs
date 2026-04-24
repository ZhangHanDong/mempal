use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use async_trait::async_trait;
use mempal::context::{ContextRequest, assemble_context};
use mempal::core::anchor;
use mempal::core::db::Database;
use mempal::core::types::{
    AnchorKind, BootstrapEvidenceArgs, Drawer, KnowledgeStatus, KnowledgeTier, MemoryDomain,
    MemoryKind, SourceType,
};
use mempal::embed::Embedder;
use rusqlite::params;
use serde_json::Value;
use tempfile::TempDir;

struct StubEmbedder;

#[async_trait]
impl Embedder for StubEmbedder {
    async fn embed(&self, texts: &[&str]) -> mempal::embed::Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|_| vector()).collect())
    }

    fn dimensions(&self) -> usize {
        vector().len()
    }

    fn name(&self) -> &str {
        "stub"
    }
}

fn mempal_bin() -> String {
    env!("CARGO_BIN_EXE_mempal").to_string()
}

fn vector() -> Vec<f32> {
    vec![0.1, 0.2, 0.3]
}

fn setup_home() -> (TempDir, Database) {
    let tmp = TempDir::new().expect("tempdir");
    let mempal_dir = tmp.path().join(".mempal");
    fs::create_dir_all(&mempal_dir).expect("create .mempal");
    let db = Database::open(&mempal_dir.join("palace.db")).expect("open db");
    (tmp, db)
}

fn run_mempal(home: &Path, args: &[&str]) -> Output {
    Command::new(mempal_bin())
        .env("HOME", home)
        .args(args)
        .output()
        .expect("run mempal")
}

fn insert_evidence(db: &Database, id: &str, content: &str) {
    let drawer = Drawer::new_bootstrap_evidence(BootstrapEvidenceArgs {
        id: id.to_string(),
        content: content.to_string(),
        wing: "mempal".to_string(),
        room: Some("lifecycle".to_string()),
        source_file: Some(format!("/tmp/{id}.md")),
        source_type: SourceType::Manual,
        added_at: "1713000000".to_string(),
        chunk_index: Some(0),
        importance: 2,
    });
    db.insert_drawer(&drawer).expect("insert evidence");
    db.insert_vector(id, &vector())
        .expect("insert evidence vector");
}

fn insert_knowledge(
    db: &Database,
    id: &str,
    tier: KnowledgeTier,
    status: KnowledgeStatus,
    statement: &str,
    content: &str,
) {
    let drawer = Drawer {
        id: id.to_string(),
        content: content.to_string(),
        wing: "mempal".to_string(),
        room: Some("lifecycle".to_string()),
        source_file: Some(format!("knowledge://project/lifecycle/{id}")),
        source_type: SourceType::Manual,
        added_at: "1713000000".to_string(),
        chunk_index: Some(0),
        normalize_version: 1,
        importance: 4,
        memory_kind: MemoryKind::Knowledge,
        domain: MemoryDomain::Project,
        field: anchor::DEFAULT_FIELD.to_string(),
        anchor_kind: AnchorKind::Repo,
        anchor_id: anchor::LEGACY_REPO_ANCHOR_ID.to_string(),
        parent_anchor_id: None,
        provenance: None,
        statement: Some(statement.to_string()),
        tier: Some(tier),
        status: Some(status),
        supporting_refs: vec!["drawer_supporting".to_string()],
        counterexample_refs: Vec::new(),
        teaching_refs: Vec::new(),
        verification_refs: Vec::new(),
        scope_constraints: None,
        trigger_hints: None,
    };
    db.insert_drawer(&drawer).expect("insert knowledge");
    db.insert_vector(id, &vector())
        .expect("insert knowledge vector");
}

async fn default_context_ids(db: &Database, cwd: &Path, query: &str) -> Vec<String> {
    let pack = assemble_context(
        db,
        &StubEmbedder,
        ContextRequest {
            query: query.to_string(),
            domain: MemoryDomain::Project,
            field: anchor::DEFAULT_FIELD.to_string(),
            cwd: cwd.to_path_buf(),
            include_evidence: false,
            max_items: 12,
        },
    )
    .await
    .expect("assemble context");
    pack.sections
        .into_iter()
        .flat_map(|section| section.items)
        .map(|item| item.drawer_id)
        .collect()
}

fn vector_row_count(db: &Database, id: &str) -> i64 {
    db.conn()
        .query_row(
            "SELECT COUNT(*) FROM drawer_vectors WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .expect("count vector rows")
}

#[tokio::test]
async fn test_cli_knowledge_promote_updates_status_and_verification_refs() {
    let (home, db) = setup_home();
    insert_evidence(&db, "drawer_verify", "validated lifecycle evidence");
    insert_knowledge(
        &db,
        "drawer_knowledge",
        KnowledgeTier::DaoRen,
        KnowledgeStatus::Candidate,
        "Use lifecycle gates before trusting knowledge.",
        "lifecycle promote candidate",
    );

    let output = run_mempal(
        home.path(),
        &[
            "knowledge",
            "promote",
            "drawer_knowledge",
            "--status",
            "promoted",
            "--verification-ref",
            "drawer_verify",
            "--reason",
            "validated in test",
            "--reviewer",
            "human",
        ],
    );
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let drawer = db
        .get_drawer("drawer_knowledge")
        .expect("load drawer")
        .expect("drawer exists");
    assert_eq!(drawer.status, Some(KnowledgeStatus::Promoted));
    assert_eq!(drawer.verification_refs, vec!["drawer_verify"]);

    let ids = default_context_ids(&db, home.path(), "lifecycle promote").await;
    assert!(ids.contains(&"drawer_knowledge".to_string()));
}

#[tokio::test]
async fn test_cli_knowledge_demote_updates_status_and_counterexample_refs() {
    let (home, db) = setup_home();
    insert_evidence(
        &db,
        "drawer_counterexample",
        "contradicted lifecycle evidence",
    );
    insert_knowledge(
        &db,
        "drawer_knowledge",
        KnowledgeTier::Shu,
        KnowledgeStatus::Promoted,
        "Use the old lifecycle workflow.",
        "lifecycle demote promoted",
    );

    let output = run_mempal(
        home.path(),
        &[
            "knowledge",
            "demote",
            "drawer_knowledge",
            "--status",
            "demoted",
            "--evidence-ref",
            "drawer_counterexample",
            "--reason",
            "contradicted in test",
            "--reason-type",
            "contradicted",
        ],
    );
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let drawer = db
        .get_drawer("drawer_knowledge")
        .expect("load drawer")
        .expect("drawer exists");
    assert_eq!(drawer.status, Some(KnowledgeStatus::Demoted));
    assert_eq!(drawer.counterexample_refs, vec!["drawer_counterexample"]);

    let ids = default_context_ids(&db, home.path(), "lifecycle demote").await;
    assert!(!ids.contains(&"drawer_knowledge".to_string()));
}

#[test]
fn test_cli_knowledge_lifecycle_rejects_evidence_drawer() {
    let (home, db) = setup_home();
    insert_evidence(&db, "drawer_evidence", "raw evidence");
    insert_evidence(&db, "drawer_verify", "validation evidence");

    let output = run_mempal(
        home.path(),
        &[
            "knowledge",
            "promote",
            "drawer_evidence",
            "--status",
            "promoted",
            "--verification-ref",
            "drawer_verify",
            "--reason",
            "bad",
        ],
    );
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("knowledge lifecycle requires a knowledge drawer")
    );
}

#[test]
fn test_cli_knowledge_lifecycle_rejects_invalid_tier_status() {
    let (home, db) = setup_home();
    insert_evidence(&db, "drawer_verify", "validation evidence");
    insert_knowledge(
        &db,
        "drawer_dao_tian",
        KnowledgeTier::DaoTian,
        KnowledgeStatus::Canonical,
        "Evidence precedes assertion.",
        "dao tian lifecycle",
    );

    let output = run_mempal(
        home.path(),
        &[
            "knowledge",
            "promote",
            "drawer_dao_tian",
            "--status",
            "promoted",
            "--verification-ref",
            "drawer_verify",
            "--reason",
            "bad",
        ],
    );
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("dao_tian only allows canonical or demoted")
    );
}

#[test]
fn test_cli_knowledge_lifecycle_writes_audit_entry() {
    let (home, db) = setup_home();
    insert_evidence(&db, "drawer_verify", "validation evidence");
    insert_knowledge(
        &db,
        "drawer_knowledge",
        KnowledgeTier::DaoRen,
        KnowledgeStatus::Candidate,
        "Lifecycle changes require audit.",
        "audit lifecycle",
    );

    let output = run_mempal(
        home.path(),
        &[
            "knowledge",
            "promote",
            "drawer_knowledge",
            "--status",
            "promoted",
            "--verification-ref",
            "drawer_verify",
            "--reason",
            "validated in test",
            "--reviewer",
            "human",
        ],
    );
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let audit_path = home.path().join(".mempal").join("audit.jsonl");
    let audit = fs::read_to_string(audit_path).expect("read audit");
    let last_line = audit.lines().last().expect("audit line");
    let value: Value = serde_json::from_str(last_line).expect("audit json");
    assert_eq!(value["command"], "knowledge_promote");
    assert_eq!(value["details"]["drawer_id"], "drawer_knowledge");
    assert_eq!(value["details"]["old_status"], "candidate");
    assert_eq!(value["details"]["new_status"], "promoted");
    assert_eq!(value["details"]["verification_refs"][0], "drawer_verify");
    assert_eq!(value["details"]["reason"], "validated in test");
    assert_eq!(value["details"]["reviewer"], "human");
}

#[test]
fn test_knowledge_lifecycle_does_not_bump_schema_or_rewrite_vectors() {
    let (home, db) = setup_home();
    insert_evidence(&db, "drawer_verify", "validation evidence");
    insert_knowledge(
        &db,
        "drawer_knowledge",
        KnowledgeTier::DaoRen,
        KnowledgeStatus::Candidate,
        "Lifecycle keeps vector rows stable.",
        "schema vector lifecycle",
    );
    let schema_before = db.schema_version().expect("schema version");
    assert_eq!(vector_row_count(&db, "drawer_knowledge"), 1);

    let output = run_mempal(
        home.path(),
        &[
            "knowledge",
            "promote",
            "drawer_knowledge",
            "--status",
            "promoted",
            "--verification-ref",
            "drawer_verify",
            "--reason",
            "validated in test",
        ],
    );
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert_eq!(db.schema_version().expect("schema version"), schema_before);
    assert_eq!(vector_row_count(&db, "drawer_knowledge"), 1);
}
