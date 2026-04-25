use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Output};
use std::thread;

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

#[derive(Default)]
struct KnowledgeRefs {
    supporting: Vec<String>,
    counterexample: Vec<String>,
    teaching: Vec<String>,
    verification: Vec<String>,
}

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
    vec![0.1; 384]
}

fn setup_home() -> (TempDir, Database) {
    let tmp = TempDir::new().expect("tempdir");
    let mempal_dir = tmp.path().join(".mempal");
    fs::create_dir_all(&mempal_dir).expect("create .mempal");
    let db = Database::open(&mempal_dir.join("palace.db")).expect("open db");
    (tmp, db)
}

fn start_openai_embedding_stub(expected_input: &str) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind embedding stub");
    listener
        .set_nonblocking(false)
        .expect("set embedding stub blocking");
    let address = listener.local_addr().expect("local addr");
    let expected_input = expected_input.to_string();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept embedding request");
        let mut request = [0_u8; 8192];
        let bytes_read = stream.read(&mut request).expect("read embedding request");
        let request_text = String::from_utf8_lossy(&request[..bytes_read]);
        let body = request_text.split("\r\n\r\n").nth(1).expect("request body");
        let payload: Value = serde_json::from_str(body).expect("parse embedding request body");
        let input = payload["input"].as_array().expect("input array");
        assert_eq!(input[0].as_str(), Some(expected_input.as_str()));
        let response = serde_json::json!({
            "data": [{ "embedding": vector() }]
        });
        let response_body = response.to_string();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            response_body.len(),
            response_body
        )
        .expect("write embedding response");
    });
    (format!("http://{address}/v1/embeddings"), handle)
}

fn write_api_config(home: &Path, endpoint: &str) {
    let config_path = home.join(".mempal").join("config.toml");
    fs::write(
        config_path,
        format!(
            "[embed]\nbackend = \"api\"\napi_endpoint = \"{endpoint}\"\napi_model = \"test-model\"\n"
        ),
    )
    .expect("write config");
}

fn run_mempal(home: &Path, args: &[&str]) -> Output {
    Command::new(mempal_bin())
        .env("HOME", home)
        .args(args)
        .output()
        .expect("run mempal")
}

fn parse_drawer_id(stdout: &[u8]) -> String {
    let text = String::from_utf8_lossy(stdout);
    text.split_whitespace()
        .find_map(|part| part.strip_prefix("drawer_id="))
        .expect("drawer_id in output")
        .to_string()
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
    insert_knowledge_with_refs(
        db,
        id,
        tier,
        status,
        statement,
        content,
        KnowledgeRefs {
            supporting: vec!["drawer_supporting".to_string()],
            ..KnowledgeRefs::default()
        },
    );
}

fn insert_knowledge_with_refs(
    db: &Database,
    id: &str,
    tier: KnowledgeTier,
    status: KnowledgeStatus,
    statement: &str,
    content: &str,
    refs: KnowledgeRefs,
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
        supporting_refs: refs.supporting,
        counterexample_refs: refs.counterexample,
        teaching_refs: refs.teaching,
        verification_refs: refs.verification,
        scope_constraints: None,
        trigger_hints: None,
    };
    db.insert_drawer(&drawer).expect("insert knowledge");
    db.insert_vector(id, &vector())
        .expect("insert knowledge vector");
}

struct KnowledgeAnchorArgs<'a> {
    domain: MemoryDomain,
    anchor_kind: AnchorKind,
    anchor_id: &'a str,
    parent_anchor_id: Option<&'a str>,
}

fn insert_knowledge_with_anchor(
    db: &Database,
    id: &str,
    status: KnowledgeStatus,
    anchor: KnowledgeAnchorArgs<'_>,
) {
    let drawer = Drawer {
        id: id.to_string(),
        content: format!("{id} content"),
        wing: "mempal".to_string(),
        room: Some("lifecycle".to_string()),
        source_file: Some(format!("knowledge://project/lifecycle/{id}")),
        source_type: SourceType::Manual,
        added_at: "1713000000".to_string(),
        chunk_index: Some(0),
        normalize_version: 1,
        importance: 4,
        memory_kind: MemoryKind::Knowledge,
        domain: anchor.domain,
        field: anchor::DEFAULT_FIELD.to_string(),
        anchor_kind: anchor.anchor_kind,
        anchor_id: anchor.anchor_id.to_string(),
        parent_anchor_id: anchor.parent_anchor_id.map(str::to_string),
        provenance: None,
        statement: Some(format!("{id} statement")),
        tier: Some(KnowledgeTier::Shu),
        status: Some(status),
        supporting_refs: vec!["drawer_supporting".to_string()],
        counterexample_refs: Vec::new(),
        teaching_refs: Vec::new(),
        verification_refs: Vec::new(),
        scope_constraints: None,
        trigger_hints: None,
    };
    db.insert_drawer(&drawer)
        .expect("insert anchored knowledge");
    db.insert_vector(id, &vector())
        .expect("insert anchored knowledge vector");
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

fn knowledge_status(db: &Database, id: &str) -> KnowledgeStatus {
    db.get_drawer(id)
        .expect("load drawer")
        .expect("drawer exists")
        .status
        .expect("knowledge status")
}

fn audit_line_count(home: &Path) -> usize {
    let audit_path = home.join(".mempal").join("audit.jsonl");
    fs::read_to_string(audit_path)
        .map(|content| content.lines().count())
        .unwrap_or(0)
}

fn last_audit_entry(home: &Path) -> Value {
    let audit_path = home.join(".mempal").join("audit.jsonl");
    let content = fs::read_to_string(audit_path).expect("read audit log");
    serde_json::from_str(content.lines().last().expect("last audit line")).expect("audit json")
}

fn table_count(db: &Database) -> i64 {
    db.conn()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'",
            [],
            |row| row.get(0),
        )
        .expect("table count")
}

fn gate_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("gate json")
}

#[test]
fn test_cli_knowledge_publish_anchor_worktree_to_repo() {
    let (home, db) = setup_home();
    insert_knowledge_with_anchor(
        &db,
        "drawer_publish_worktree",
        KnowledgeStatus::Promoted,
        KnowledgeAnchorArgs {
            domain: MemoryDomain::Project,
            anchor_kind: AnchorKind::Worktree,
            anchor_id: "worktree:///tmp/publish-worktree",
            parent_anchor_id: Some("repo://parent"),
        },
    );
    let before = db
        .get_drawer("drawer_publish_worktree")
        .expect("load drawer")
        .expect("drawer exists");
    let vector_count_before = vector_row_count(&db, "drawer_publish_worktree");
    let audit_before = audit_line_count(home.path());

    let output = run_mempal(
        home.path(),
        &[
            "knowledge",
            "publish-anchor",
            "drawer_publish_worktree",
            "--to",
            "repo",
            "--reason",
            "share stable rule",
        ],
    );

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(
            "published drawer_publish_worktree: worktree:worktree:///tmp/publish-worktree -> repo:repo://parent"
        ),
        "stdout={stdout}"
    );
    let after = db
        .get_drawer("drawer_publish_worktree")
        .expect("load drawer")
        .expect("drawer exists");
    assert_eq!(after.anchor_kind, AnchorKind::Repo);
    assert_eq!(after.anchor_id, "repo://parent");
    assert_eq!(after.parent_anchor_id, None);
    assert_eq!(after.content, before.content);
    assert_eq!(after.statement, before.statement);
    assert_eq!(after.status, before.status);
    assert_eq!(after.supporting_refs, before.supporting_refs);
    assert_eq!(
        vector_row_count(&db, "drawer_publish_worktree"),
        vector_count_before
    );
    assert_eq!(audit_line_count(home.path()), audit_before + 1);
    assert_eq!(
        last_audit_entry(home.path())["command"],
        "knowledge_publish_anchor"
    );
}

#[test]
fn test_cli_knowledge_publish_anchor_repo_to_global() {
    let (home, db) = setup_home();
    insert_knowledge_with_anchor(
        &db,
        "drawer_publish_global",
        KnowledgeStatus::Canonical,
        KnowledgeAnchorArgs {
            domain: MemoryDomain::Global,
            anchor_kind: AnchorKind::Repo,
            anchor_id: "repo://global-ready",
            parent_anchor_id: None,
        },
    );

    let output = run_mempal(
        home.path(),
        &[
            "knowledge",
            "publish-anchor",
            "drawer_publish_global",
            "--to",
            "global",
            "--target-anchor-id",
            "global://epistemics",
            "--reason",
            "global law",
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
        .get_drawer("drawer_publish_global")
        .expect("load drawer")
        .expect("drawer exists");
    assert_eq!(drawer.anchor_kind, AnchorKind::Global);
    assert_eq!(drawer.anchor_id, "global://epistemics");
    let audit = last_audit_entry(home.path());
    assert_eq!(audit["command"], "knowledge_publish_anchor");
    assert_eq!(audit["details"]["reviewer"], "human");
}

#[test]
fn test_cli_knowledge_publish_anchor_rejects_worktree_to_global() {
    let (home, db) = setup_home();
    insert_knowledge_with_anchor(
        &db,
        "drawer_publish_skip",
        KnowledgeStatus::Promoted,
        KnowledgeAnchorArgs {
            domain: MemoryDomain::Global,
            anchor_kind: AnchorKind::Worktree,
            anchor_id: "worktree:///tmp/publish-skip",
            parent_anchor_id: Some("repo://parent"),
        },
    );
    let before = db
        .get_drawer("drawer_publish_skip")
        .expect("load drawer")
        .expect("drawer exists");

    let output = run_mempal(
        home.path(),
        &[
            "knowledge",
            "publish-anchor",
            "drawer_publish_skip",
            "--to",
            "global",
            "--target-anchor-id",
            "global://x",
            "--reason",
            "skip",
        ],
    );

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("worktree -> global publication is not allowed")
    );
    let after = db
        .get_drawer("drawer_publish_skip")
        .expect("load drawer")
        .expect("drawer exists");
    assert_eq!(after.anchor_kind, before.anchor_kind);
    assert_eq!(after.anchor_id, before.anchor_id);
    assert_eq!(after.parent_anchor_id, before.parent_anchor_id);
}

#[test]
fn test_cli_knowledge_publish_anchor_rejects_inactive_or_evidence() {
    let (home, db) = setup_home();
    insert_evidence(
        &db,
        "drawer_publish_evidence",
        "evidence cannot be published",
    );
    insert_knowledge_with_anchor(
        &db,
        "drawer_publish_candidate",
        KnowledgeStatus::Candidate,
        KnowledgeAnchorArgs {
            domain: MemoryDomain::Project,
            anchor_kind: AnchorKind::Worktree,
            anchor_id: "worktree:///tmp/publish-candidate",
            parent_anchor_id: Some("repo://parent"),
        },
    );

    let evidence_output = run_mempal(
        home.path(),
        &[
            "knowledge",
            "publish-anchor",
            "drawer_publish_evidence",
            "--to",
            "repo",
            "--reason",
            "bad",
        ],
    );
    assert!(!evidence_output.status.success());
    assert!(
        String::from_utf8_lossy(&evidence_output.stderr)
            .contains("knowledge anchor publication requires a knowledge drawer")
    );

    let candidate_output = run_mempal(
        home.path(),
        &[
            "knowledge",
            "publish-anchor",
            "drawer_publish_candidate",
            "--to",
            "repo",
            "--reason",
            "bad",
        ],
    );
    assert!(!candidate_output.status.success());
    assert!(
        String::from_utf8_lossy(&candidate_output.stderr)
            .contains("publish-anchor requires promoted or canonical knowledge")
    );
}

#[test]
fn test_cli_knowledge_publish_anchor_rejects_invalid_target_anchor() {
    let (home, db) = setup_home();
    insert_knowledge_with_anchor(
        &db,
        "drawer_publish_invalid_target",
        KnowledgeStatus::Promoted,
        KnowledgeAnchorArgs {
            domain: MemoryDomain::Global,
            anchor_kind: AnchorKind::Repo,
            anchor_id: "repo://invalid-target",
            parent_anchor_id: None,
        },
    );

    let missing_global = run_mempal(
        home.path(),
        &[
            "knowledge",
            "publish-anchor",
            "drawer_publish_invalid_target",
            "--to",
            "global",
            "--reason",
            "missing target",
        ],
    );
    assert!(!missing_global.status.success());
    assert!(
        String::from_utf8_lossy(&missing_global.stderr)
            .contains("--target-anchor-id is required for global publication")
    );

    let wrong_prefix = run_mempal(
        home.path(),
        &[
            "knowledge",
            "publish-anchor",
            "drawer_publish_invalid_target",
            "--to",
            "repo",
            "--target-anchor-id",
            "global://wrong",
            "--reason",
            "bad",
        ],
    );
    assert!(!wrong_prefix.status.success());
    assert!(String::from_utf8_lossy(&wrong_prefix.stderr).contains("expected prefix repo://"));
}

#[test]
fn test_cli_knowledge_publish_anchor_does_not_bump_schema() {
    let (home, db) = setup_home();
    insert_knowledge_with_anchor(
        &db,
        "drawer_publish_schema",
        KnowledgeStatus::Promoted,
        KnowledgeAnchorArgs {
            domain: MemoryDomain::Project,
            anchor_kind: AnchorKind::Worktree,
            anchor_id: "worktree:///tmp/publish-schema",
            parent_anchor_id: Some("repo://parent"),
        },
    );
    let schema_before = db.schema_version().expect("schema");
    let table_count_before = table_count(&db);

    let output = run_mempal(
        home.path(),
        &[
            "knowledge",
            "publish-anchor",
            "drawer_publish_schema",
            "--to",
            "repo",
            "--reason",
            "stable",
        ],
    );

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(db.schema_version().expect("schema"), schema_before);
    assert_eq!(table_count(&db), table_count_before);
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

#[test]
fn test_cli_knowledge_promote_rejects_malformed_verification_ref() {
    let (home, db) = setup_home();
    insert_knowledge(
        &db,
        "drawer_knowledge",
        KnowledgeTier::DaoRen,
        KnowledgeStatus::Candidate,
        "Lifecycle refs must be drawer ids.",
        "malformed ref lifecycle",
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
            "not_a_drawer",
            "--reason",
            "bad",
        ],
    );
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("lifecycle refs must contain drawer ids")
    );
    assert_eq!(
        knowledge_status(&db, "drawer_knowledge"),
        KnowledgeStatus::Candidate
    );
}

#[test]
fn test_cli_knowledge_promote_rejects_knowledge_verification_ref() {
    let (home, db) = setup_home();
    insert_knowledge(
        &db,
        "drawer_knowledge",
        KnowledgeTier::DaoRen,
        KnowledgeStatus::Candidate,
        "Promotion requires evidence refs.",
        "wrong kind lifecycle",
    );
    insert_knowledge(
        &db,
        "drawer_other_knowledge",
        KnowledgeTier::Qi,
        KnowledgeStatus::Candidate,
        "Knowledge is not evidence.",
        "wrong kind ref",
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
            "drawer_other_knowledge",
            "--reason",
            "bad",
        ],
    );
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("lifecycle refs must point to evidence drawers")
    );
    assert_eq!(
        knowledge_status(&db, "drawer_knowledge"),
        KnowledgeStatus::Candidate
    );
}

#[test]
fn test_cli_knowledge_demote_rejects_missing_evidence_ref() {
    let (home, db) = setup_home();
    insert_knowledge(
        &db,
        "drawer_knowledge",
        KnowledgeTier::Shu,
        KnowledgeStatus::Promoted,
        "Demotion requires existing evidence refs.",
        "missing evidence lifecycle",
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
            "drawer_missing",
            "--reason",
            "bad",
            "--reason-type",
            "contradicted",
        ],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("ref drawer not found"));
    assert_eq!(
        knowledge_status(&db, "drawer_knowledge"),
        KnowledgeStatus::Promoted
    );
}

#[test]
fn test_cli_knowledge_lifecycle_accepts_evidence_refs() {
    let (home, db) = setup_home();
    insert_evidence(&db, "drawer_verify", "validation evidence");
    insert_knowledge(
        &db,
        "drawer_knowledge",
        KnowledgeTier::DaoRen,
        KnowledgeStatus::Candidate,
        "Lifecycle accepts real evidence.",
        "accepted evidence lifecycle",
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
            "validated",
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
}

#[test]
fn test_cli_knowledge_gate_allows_dao_ren_promotion() {
    let (home, db) = setup_home();
    insert_evidence(&db, "drawer_supporting_a", "supporting evidence a");
    insert_evidence(&db, "drawer_supporting_b", "supporting evidence b");
    insert_evidence(&db, "drawer_verify", "verification evidence");
    insert_knowledge_with_refs(
        &db,
        "drawer_knowledge",
        KnowledgeTier::DaoRen,
        KnowledgeStatus::Candidate,
        "Gate dao_ren with enough evidence.",
        "gate dao ren",
        KnowledgeRefs {
            supporting: vec![
                "drawer_supporting_a".to_string(),
                "drawer_supporting_b".to_string(),
            ],
            verification: vec!["drawer_verify".to_string()],
            ..KnowledgeRefs::default()
        },
    );
    let schema_before = db.schema_version().expect("schema version");
    let audit_before = audit_line_count(home.path());
    assert_eq!(vector_row_count(&db, "drawer_knowledge"), 1);

    let output = run_mempal(home.path(), &["knowledge", "gate", "drawer_knowledge"]);
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("allowed=true"));
    assert_eq!(
        knowledge_status(&db, "drawer_knowledge"),
        KnowledgeStatus::Candidate
    );
    assert_eq!(audit_line_count(home.path()), audit_before);
    assert_eq!(db.schema_version().expect("schema version"), schema_before);
    assert_eq!(vector_row_count(&db, "drawer_knowledge"), 1);
}

#[test]
fn test_cli_knowledge_gate_rejects_missing_verification() {
    let (home, db) = setup_home();
    insert_evidence(&db, "drawer_supporting_a", "supporting evidence a");
    insert_evidence(&db, "drawer_supporting_b", "supporting evidence b");
    insert_knowledge_with_refs(
        &db,
        "drawer_knowledge",
        KnowledgeTier::DaoRen,
        KnowledgeStatus::Candidate,
        "Gate needs verification.",
        "missing verification",
        KnowledgeRefs {
            supporting: vec![
                "drawer_supporting_a".to_string(),
                "drawer_supporting_b".to_string(),
            ],
            ..KnowledgeRefs::default()
        },
    );

    let output = run_mempal(
        home.path(),
        &["knowledge", "gate", "drawer_knowledge", "--format", "json"],
    );
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value = gate_json(&output);
    assert_eq!(value["allowed"], false);
    assert!(
        value["reasons"]
            .as_array()
            .expect("reasons")
            .iter()
            .any(|reason| reason.as_str().expect("reason").contains("verification"))
    );
}

#[test]
fn test_cli_knowledge_gate_requires_reviewer_for_dao_tian() {
    let (home, db) = setup_home();
    for id in [
        "drawer_supporting_a",
        "drawer_supporting_b",
        "drawer_supporting_c",
        "drawer_verify_a",
        "drawer_verify_b",
        "drawer_teaching",
    ] {
        insert_evidence(&db, id, "dao tian gate evidence");
    }
    insert_knowledge_with_refs(
        &db,
        "drawer_dao_tian",
        KnowledgeTier::DaoTian,
        KnowledgeStatus::Canonical,
        "Dao tian requires reviewer.",
        "dao tian gate",
        KnowledgeRefs {
            supporting: vec![
                "drawer_supporting_a".to_string(),
                "drawer_supporting_b".to_string(),
                "drawer_supporting_c".to_string(),
            ],
            teaching: vec!["drawer_teaching".to_string()],
            verification: vec!["drawer_verify_a".to_string(), "drawer_verify_b".to_string()],
            ..KnowledgeRefs::default()
        },
    );

    let missing_reviewer = run_mempal(home.path(), &["knowledge", "gate", "drawer_dao_tian"]);
    assert!(missing_reviewer.status.success());
    assert!(String::from_utf8_lossy(&missing_reviewer.stdout).contains("allowed=false"));
    assert!(String::from_utf8_lossy(&missing_reviewer.stdout).contains("reviewer"));

    let reviewed = run_mempal(
        home.path(),
        &[
            "knowledge",
            "gate",
            "drawer_dao_tian",
            "--reviewer",
            "human",
            "--format",
            "json",
        ],
    );
    assert!(reviewed.status.success());
    let value = gate_json(&reviewed);
    assert_eq!(value["allowed"], true);
}

#[test]
fn test_cli_knowledge_gate_allows_shu_promotion() {
    let (home, db) = setup_home();
    insert_evidence(&db, "drawer_supporting", "supporting evidence");
    insert_evidence(&db, "drawer_verify", "verification evidence");
    insert_knowledge_with_refs(
        &db,
        "drawer_shu",
        KnowledgeTier::Shu,
        KnowledgeStatus::Promoted,
        "Shu gate uses one support and verification.",
        "shu gate",
        KnowledgeRefs {
            supporting: vec!["drawer_supporting".to_string()],
            verification: vec!["drawer_verify".to_string()],
            ..KnowledgeRefs::default()
        },
    );

    let output = run_mempal(
        home.path(),
        &[
            "knowledge",
            "gate",
            "drawer_shu",
            "--target-status",
            "promoted",
            "--format",
            "json",
        ],
    );
    assert!(output.status.success());
    let value = gate_json(&output);
    assert_eq!(value["allowed"], true);
    assert_eq!(value["evidence_counts"]["supporting"], 1);
    assert_eq!(value["evidence_counts"]["verification"], 1);
}

#[test]
fn test_cli_knowledge_gate_blocks_counterexamples_by_default() {
    let (home, db) = setup_home();
    insert_evidence(&db, "drawer_supporting", "supporting evidence");
    insert_evidence(&db, "drawer_verify", "verification evidence");
    insert_evidence(&db, "drawer_counterexample", "counterexample evidence");
    insert_knowledge_with_refs(
        &db,
        "drawer_qi",
        KnowledgeTier::Qi,
        KnowledgeStatus::Candidate,
        "Qi gate blocks counterexamples.",
        "qi gate",
        KnowledgeRefs {
            supporting: vec!["drawer_supporting".to_string()],
            counterexample: vec!["drawer_counterexample".to_string()],
            verification: vec!["drawer_verify".to_string()],
            ..KnowledgeRefs::default()
        },
    );

    let blocked = run_mempal(
        home.path(),
        &["knowledge", "gate", "drawer_qi", "--format", "json"],
    );
    assert!(blocked.status.success());
    let blocked_json = gate_json(&blocked);
    assert_eq!(blocked_json["allowed"], false);
    assert!(
        blocked_json["reasons"]
            .as_array()
            .expect("reasons")
            .iter()
            .any(|reason| reason.as_str().expect("reason").contains("counterexample"))
    );

    let allowed = run_mempal(
        home.path(),
        &[
            "knowledge",
            "gate",
            "drawer_qi",
            "--allow-counterexamples",
            "--format",
            "json",
        ],
    );
    assert!(allowed.status.success());
    let allowed_json = gate_json(&allowed);
    assert_eq!(allowed_json["allowed"], true);
}

#[test]
fn test_cli_knowledge_gate_rejects_evidence_drawer() {
    let (home, db) = setup_home();
    insert_evidence(&db, "drawer_evidence", "raw evidence");

    let output = run_mempal(home.path(), &["knowledge", "gate", "drawer_evidence"]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("knowledge gate requires a knowledge drawer")
    );
}

#[test]
fn test_cli_knowledge_gate_validates_role_refs() {
    let (home, db) = setup_home();
    insert_evidence(&db, "drawer_verify", "verification evidence");
    insert_knowledge(
        &db,
        "drawer_other_knowledge",
        KnowledgeTier::Qi,
        KnowledgeStatus::Candidate,
        "Knowledge ref is not evidence.",
        "wrong gate ref",
    );
    insert_knowledge_with_refs(
        &db,
        "drawer_knowledge",
        KnowledgeTier::DaoRen,
        KnowledgeStatus::Candidate,
        "Gate validates refs.",
        "bad role refs",
        KnowledgeRefs {
            supporting: vec!["drawer_other_knowledge".to_string()],
            verification: vec!["drawer_verify".to_string()],
            ..KnowledgeRefs::default()
        },
    );

    let output = run_mempal(home.path(), &["knowledge", "gate", "drawer_knowledge"]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("gate refs must point to evidence drawers")
    );
}

#[test]
fn test_cli_knowledge_gate_rejects_invalid_target_status() {
    let (home, db) = setup_home();
    insert_knowledge(
        &db,
        "drawer_dao_tian",
        KnowledgeTier::DaoTian,
        KnowledgeStatus::Canonical,
        "Dao tian rejects promoted.",
        "invalid target",
    );

    let output = run_mempal(
        home.path(),
        &[
            "knowledge",
            "gate",
            "drawer_dao_tian",
            "--target-status",
            "promoted",
        ],
    );
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("dao_tian only allows canonical or demoted")
    );
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

#[tokio::test]
async fn test_cli_knowledge_distill_creates_candidate_knowledge() {
    let (home, db) = setup_home();
    insert_evidence(&db, "drawer_evidence", "evidence first observation");
    let content = "Use cited evidence before asserting project facts.";
    let (endpoint, handle) = start_openai_embedding_stub(content);
    write_api_config(home.path(), &endpoint);

    let output = run_mempal(
        home.path(),
        &[
            "knowledge",
            "distill",
            "--statement",
            "Prefer evidence first",
            "--content",
            content,
            "--tier",
            "dao_ren",
            "--supporting-ref",
            "drawer_evidence",
        ],
    );
    handle.join().expect("join embedding stub");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let drawer_id = parse_drawer_id(&output.stdout);
    let drawer = db
        .get_drawer(&drawer_id)
        .expect("load drawer")
        .expect("drawer exists");
    assert_eq!(drawer.memory_kind, MemoryKind::Knowledge);
    assert_eq!(drawer.status, Some(KnowledgeStatus::Candidate));
    assert_eq!(drawer.tier, Some(KnowledgeTier::DaoRen));
    assert_eq!(drawer.supporting_refs, vec!["drawer_evidence"]);

    let ids = default_context_ids(&db, home.path(), "evidence first").await;
    assert!(!ids.contains(&drawer_id));
}

#[test]
fn test_cli_knowledge_distill_dry_run_no_write() {
    let (home, db) = setup_home();
    insert_evidence(&db, "drawer_evidence", "dry run evidence");
    let baseline = db.drawer_count().expect("drawer count");
    let args = [
        "knowledge",
        "distill",
        "--statement",
        "Dry run candidate",
        "--content",
        "This should not be written.",
        "--tier",
        "qi",
        "--supporting-ref",
        "drawer_evidence",
        "--dry-run",
    ];

    let first = run_mempal(home.path(), &args);
    let second = run_mempal(home.path(), &args);
    assert!(first.status.success());
    assert!(second.status.success());
    assert_eq!(
        parse_drawer_id(&first.stdout),
        parse_drawer_id(&second.stdout)
    );
    assert_eq!(db.drawer_count().expect("drawer count"), baseline);
}

#[test]
fn test_cli_knowledge_distill_rejects_dao_tian_candidate() {
    let (home, db) = setup_home();
    insert_evidence(&db, "drawer_evidence", "dao tian evidence");
    let output = run_mempal(
        home.path(),
        &[
            "knowledge",
            "distill",
            "--statement",
            "Universal law",
            "--content",
            "This should not be candidate dao_tian.",
            "--tier",
            "dao_tian",
            "--supporting-ref",
            "drawer_evidence",
        ],
    );
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("distill only allows candidate dao_ren or qi")
    );
}

#[test]
fn test_cli_knowledge_distill_rejects_missing_supporting_refs() {
    let (home, db) = setup_home();
    let baseline = db.drawer_count().expect("drawer count");
    let output = run_mempal(
        home.path(),
        &[
            "knowledge",
            "distill",
            "--statement",
            "Missing refs",
            "--content",
            "This should fail before writing.",
            "--tier",
            "qi",
        ],
    );
    assert!(!output.status.success());
    assert_eq!(db.drawer_count().expect("drawer count"), baseline);
}

#[test]
fn test_cli_knowledge_distill_stores_trigger_hints() {
    let (home, db) = setup_home();
    insert_evidence(&db, "drawer_evidence", "trigger hint evidence");
    let content = "Reproduce failures before changing code.";
    let (endpoint, handle) = start_openai_embedding_stub(content);
    write_api_config(home.path(), &endpoint);

    let output = run_mempal(
        home.path(),
        &[
            "knowledge",
            "distill",
            "--statement",
            "Reproduce before patching",
            "--content",
            content,
            "--tier",
            "qi",
            "--supporting-ref",
            "drawer_evidence",
            "--intent-tag",
            "debugging",
            "--workflow-bias",
            "reproduce-first",
            "--tool-need",
            "cargo-test",
        ],
    );
    handle.join().expect("join embedding stub");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let drawer_id = parse_drawer_id(&output.stdout);
    let drawer = db
        .get_drawer(&drawer_id)
        .expect("load drawer")
        .expect("drawer exists");
    let hints = drawer.trigger_hints.expect("trigger hints");
    assert_eq!(hints.intent_tags, vec!["debugging"]);
    assert_eq!(hints.workflow_bias, vec!["reproduce-first"]);
    assert_eq!(hints.tool_needs, vec!["cargo-test"]);
}

#[test]
fn test_cli_knowledge_distill_writes_audit_and_preserves_schema() {
    let (home, db) = setup_home();
    insert_evidence(&db, "drawer_evidence", "audit distill evidence");
    let schema_before = db.schema_version().expect("schema version");
    let content = "Audit every distilled candidate.";
    let (endpoint, handle) = start_openai_embedding_stub(content);
    write_api_config(home.path(), &endpoint);

    let output = run_mempal(
        home.path(),
        &[
            "knowledge",
            "distill",
            "--statement",
            "Audit distilled candidates",
            "--content",
            content,
            "--tier",
            "dao_ren",
            "--supporting-ref",
            "drawer_evidence",
        ],
    );
    handle.join().expect("join embedding stub");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(db.schema_version().expect("schema version"), schema_before);

    let audit_path = home.path().join(".mempal").join("audit.jsonl");
    let audit = fs::read_to_string(audit_path).expect("read audit");
    let last_line = audit.lines().last().expect("audit line");
    let value: Value = serde_json::from_str(last_line).expect("audit json");
    assert_eq!(value["command"], "knowledge_distill");
    assert_eq!(value["details"]["status"], "candidate");
    assert_eq!(value["details"]["tier"], "dao_ren");
    assert_eq!(value["details"]["supporting_refs"][0], "drawer_evidence");
}
