use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use mempal::core::db::Database;
use mempal::core::types::{
    AnchorKind, Drawer, KnowledgeCard, KnowledgeCardFilter, KnowledgeEventType,
    KnowledgeEvidenceRole, KnowledgeStatus, KnowledgeTier, MemoryDomain, MemoryKind, SourceType,
};
use mempal::knowledge_card_backfill::{
    KnowledgeCardBackfillApplyOptions, KnowledgeCardBackfillStatus, apply_backfill,
    build_backfill_report, prospective_card_id,
};
use serde_json::Value;
use tempfile::TempDir;

fn mempal_bin() -> String {
    env!("CARGO_BIN_EXE_mempal").to_string()
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

fn stdout_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

fn knowledge_drawer(id: &str, field: &str) -> Drawer {
    Drawer {
        id: id.to_string(),
        content: format!("Content for {id}."),
        wing: "mempal".to_string(),
        room: Some("knowledge".to_string()),
        source_file: Some(format!("knowledge://project/{field}/{id}")),
        source_type: SourceType::Manual,
        added_at: "1710000000".to_string(),
        chunk_index: Some(0),
        normalize_version: 1,
        importance: 3,
        memory_kind: MemoryKind::Knowledge,
        domain: MemoryDomain::Project,
        field: field.to_string(),
        anchor_kind: AnchorKind::Repo,
        anchor_id: "repo://mempal".to_string(),
        parent_anchor_id: None,
        provenance: None,
        statement: Some(format!("Statement for {id}.")),
        tier: Some(KnowledgeTier::Shu),
        status: Some(KnowledgeStatus::Promoted),
        supporting_refs: vec!["drawer_evidence".to_string()],
        counterexample_refs: Vec::new(),
        teaching_refs: Vec::new(),
        verification_refs: Vec::new(),
        scope_constraints: None,
        trigger_hints: None,
    }
}

fn evidence_drawer(id: &str) -> Drawer {
    Drawer {
        id: id.to_string(),
        content: format!("Evidence for {id}."),
        wing: "mempal".to_string(),
        room: Some("evidence".to_string()),
        source_file: Some(format!("evidence://{id}")),
        source_type: SourceType::Manual,
        added_at: "1710000000".to_string(),
        chunk_index: Some(0),
        normalize_version: 1,
        importance: 3,
        memory_kind: MemoryKind::Evidence,
        domain: MemoryDomain::Project,
        field: "rust".to_string(),
        anchor_kind: AnchorKind::Repo,
        anchor_id: "repo://mempal".to_string(),
        parent_anchor_id: None,
        provenance: None,
        statement: None,
        tier: None,
        status: None,
        supporting_refs: Vec::new(),
        counterexample_refs: Vec::new(),
        teaching_refs: Vec::new(),
        verification_refs: Vec::new(),
        scope_constraints: None,
        trigger_hints: None,
    }
}

fn insert_drawer(db: &Database, drawer: Drawer) {
    db.insert_drawer(&drawer).expect("insert drawer");
}

fn table_count(db: &Database, table_name: &str) -> i64 {
    let sql = format!("SELECT COUNT(*) FROM {table_name}");
    db.conn()
        .query_row(&sql, [], |row| row.get(0))
        .expect("table count")
}

fn insert_existing_card(db: &Database, source_drawer_id: &str) {
    let card = KnowledgeCard {
        id: prospective_card_id(source_drawer_id),
        statement: "Existing card.".to_string(),
        content: "Existing content.".to_string(),
        tier: KnowledgeTier::Shu,
        status: KnowledgeStatus::Promoted,
        domain: MemoryDomain::Project,
        field: "rust".to_string(),
        anchor_kind: AnchorKind::Repo,
        anchor_id: "repo://mempal".to_string(),
        parent_anchor_id: None,
        scope_constraints: None,
        trigger_hints: None,
        created_at: "1710000000".to_string(),
        updated_at: "1710000000".to_string(),
    };
    db.insert_knowledge_card(&card).expect("insert card");
}

#[test]
fn test_knowledge_card_backfill_report_classifies_drawers() {
    let (_home, db) = setup_home();
    insert_drawer(&db, knowledge_drawer("drawer_ready", "rust"));
    let mut missing = knowledge_drawer("drawer_missing", "rust");
    missing.statement = None;
    missing.tier = None;
    insert_drawer(&db, missing);
    insert_drawer(&db, knowledge_drawer("drawer_existing", "rust"));
    insert_existing_card(&db, "drawer_existing");

    let report = build_backfill_report(&db, &KnowledgeCardFilter::default()).expect("build report");

    assert_eq!(report.ready_count, 1);
    assert_eq!(report.skipped_count, 1);
    assert_eq!(report.already_exists_count, 1);

    let ready = report
        .candidates
        .iter()
        .find(|item| item.source_drawer_id == "drawer_ready")
        .expect("ready candidate");
    assert_eq!(ready.status, KnowledgeCardBackfillStatus::Ready);
    assert_eq!(
        ready.prospective_card_id,
        prospective_card_id("drawer_ready")
    );

    let skipped = report
        .candidates
        .iter()
        .find(|item| item.source_drawer_id == "drawer_missing")
        .expect("skipped candidate");
    assert_eq!(skipped.status, KnowledgeCardBackfillStatus::Skipped);
    assert!(skipped.reasons.contains(&"missing statement".to_string()));
    assert!(skipped.reasons.contains(&"missing tier".to_string()));

    let existing = report
        .candidates
        .iter()
        .find(|item| item.source_drawer_id == "drawer_existing")
        .expect("existing candidate");
    assert_eq!(existing.status, KnowledgeCardBackfillStatus::AlreadyExists);
}

#[test]
fn test_knowledge_card_backfill_report_has_no_db_side_effects() {
    let (_home, db) = setup_home();
    insert_drawer(&db, knowledge_drawer("drawer_ready", "rust"));
    let drawer_count = db.drawer_count().expect("drawer count");
    let card_count = db.knowledge_card_count().expect("card count");

    for _ in 0..3 {
        let report =
            build_backfill_report(&db, &KnowledgeCardFilter::default()).expect("build report");
        assert_eq!(report.ready_count, 1);
    }

    assert_eq!(db.drawer_count().expect("drawer count"), drawer_count);
    assert_eq!(db.knowledge_card_count().expect("card count"), card_count);
}

#[test]
fn test_knowledge_card_backfill_apply_dry_run_has_no_side_effects() {
    let (_home, db) = setup_home();
    insert_drawer(&db, evidence_drawer("drawer_evidence"));
    insert_drawer(&db, knowledge_drawer("drawer_ready", "rust"));
    let drawer_count = db.drawer_count().expect("drawer count");
    let card_count = db.knowledge_card_count().expect("card count");
    let link_count = table_count(&db, "knowledge_evidence_links");
    let event_count = table_count(&db, "knowledge_events");

    let result = apply_backfill(
        &db,
        &KnowledgeCardFilter::default(),
        KnowledgeCardBackfillApplyOptions { execute: false },
    )
    .expect("apply dry run");

    assert!(result.dry_run);
    assert_eq!(result.ready_count, 1);
    assert_eq!(result.created_count, 0);
    assert_eq!(result.linked_count, 0);
    assert_eq!(result.event_count, 0);
    assert!(result.link_errors.is_empty());
    assert_eq!(db.drawer_count().expect("drawer count"), drawer_count);
    assert_eq!(db.knowledge_card_count().expect("card count"), card_count);
    assert_eq!(table_count(&db, "knowledge_evidence_links"), link_count);
    assert_eq!(table_count(&db, "knowledge_events"), event_count);
}

#[test]
fn test_knowledge_card_backfill_apply_execute_creates_card_links_event() {
    let (_home, db) = setup_home();
    for id in [
        "drawer_supporting",
        "drawer_verification",
        "drawer_counterexample",
        "drawer_teaching",
    ] {
        insert_drawer(&db, evidence_drawer(id));
    }
    let mut drawer = knowledge_drawer("drawer_ready", "rust");
    drawer.supporting_refs = vec!["drawer_supporting".to_string()];
    drawer.verification_refs = vec!["drawer_verification".to_string()];
    drawer.counterexample_refs = vec!["drawer_counterexample".to_string()];
    drawer.teaching_refs = vec!["drawer_teaching".to_string()];
    insert_drawer(&db, drawer);

    let result = apply_backfill(
        &db,
        &KnowledgeCardFilter::default(),
        KnowledgeCardBackfillApplyOptions { execute: true },
    )
    .expect("apply execute");

    let card_id = prospective_card_id("drawer_ready");
    assert!(!result.dry_run);
    assert_eq!(result.created_count, 1);
    assert_eq!(result.linked_count, 4);
    assert_eq!(result.event_count, 1);
    assert!(result.link_errors.is_empty());
    let card = db
        .get_knowledge_card(&card_id)
        .expect("get card")
        .expect("card exists");
    assert_eq!(card.statement, "Statement for drawer_ready.");

    let mut roles = db
        .knowledge_evidence_links(&card_id)
        .expect("links")
        .into_iter()
        .map(|link| link.role)
        .collect::<Vec<_>>();
    roles.sort_by_key(|role| format!("{role:?}"));
    assert_eq!(
        roles,
        vec![
            KnowledgeEvidenceRole::Counterexample,
            KnowledgeEvidenceRole::Supporting,
            KnowledgeEvidenceRole::Teaching,
            KnowledgeEvidenceRole::Verification,
        ]
    );

    let events = db.knowledge_events(&card_id).expect("events");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, KnowledgeEventType::Created);
}

#[test]
fn test_knowledge_card_backfill_apply_execute_is_idempotent() {
    let (_home, db) = setup_home();
    insert_drawer(&db, evidence_drawer("drawer_evidence"));
    insert_drawer(&db, knowledge_drawer("drawer_ready", "rust"));

    let first = apply_backfill(
        &db,
        &KnowledgeCardFilter::default(),
        KnowledgeCardBackfillApplyOptions { execute: true },
    )
    .expect("first apply");
    let link_count = table_count(&db, "knowledge_evidence_links");
    let event_count = table_count(&db, "knowledge_events");

    let second = apply_backfill(
        &db,
        &KnowledgeCardFilter::default(),
        KnowledgeCardBackfillApplyOptions { execute: true },
    )
    .expect("second apply");

    assert_eq!(first.created_count, 1);
    assert_eq!(second.created_count, 0);
    assert_eq!(second.already_exists_count, 1);
    assert_eq!(table_count(&db, "knowledge_evidence_links"), link_count);
    assert_eq!(table_count(&db, "knowledge_events"), event_count);
}

#[test]
fn test_knowledge_card_backfill_apply_reports_invalid_evidence_refs() {
    let (_home, db) = setup_home();
    insert_drawer(&db, evidence_drawer("drawer_evidence"));
    let mut drawer = knowledge_drawer("drawer_ready", "rust");
    drawer.supporting_refs = vec!["drawer_evidence".to_string(), "drawer_missing".to_string()];
    insert_drawer(&db, drawer);

    let result = apply_backfill(
        &db,
        &KnowledgeCardFilter::default(),
        KnowledgeCardBackfillApplyOptions { execute: true },
    )
    .expect("apply execute");

    let card_id = prospective_card_id("drawer_ready");
    assert_eq!(result.created_count, 1);
    assert_eq!(result.linked_count, 1);
    assert_eq!(result.link_errors.len(), 1);
    assert_eq!(result.link_errors[0].card_id, card_id);
    assert_eq!(result.link_errors[0].evidence_drawer_id, "drawer_missing");
    assert!(db.get_knowledge_card(&card_id).expect("get card").is_some());
}

#[test]
fn test_cli_knowledge_card_backfill_plan_plain() {
    let (home, db) = setup_home();
    insert_drawer(&db, knowledge_drawer("drawer_ready", "rust"));
    let mut missing = knowledge_drawer("drawer_missing", "rust");
    missing.statement = None;
    insert_drawer(&db, missing);
    insert_drawer(&db, knowledge_drawer("drawer_existing", "rust"));
    insert_existing_card(&db, "drawer_existing");

    let output = run_mempal(home.path(), &["knowledge-card", "backfill-plan"]);
    assert!(output.status.success(), "{}", stderr_text(&output));
    let stdout = stdout_text(&output);
    assert!(stdout.contains("ready=1 skipped=1 already_exists=1"));
    assert!(stdout.contains("drawer_ready"));
    assert!(stdout.contains(&prospective_card_id("drawer_ready")));
}

#[test]
fn test_cli_knowledge_card_backfill_apply_defaults_to_dry_run() {
    let (home, db) = setup_home();
    insert_drawer(&db, evidence_drawer("drawer_evidence"));
    insert_drawer(&db, knowledge_drawer("drawer_ready", "rust"));

    let output = run_mempal(home.path(), &["knowledge-card", "backfill-apply"]);

    assert!(output.status.success(), "{}", stderr_text(&output));
    let stdout = stdout_text(&output);
    assert!(stdout.contains("dry_run=true"));
    assert!(stdout.contains("created_count=0"));
    assert_eq!(db.knowledge_card_count().expect("card count"), 0);
}

#[test]
fn test_cli_knowledge_card_backfill_apply_execute_json_filters() {
    let (home, db) = setup_home();
    insert_drawer(&db, evidence_drawer("drawer_evidence"));
    insert_drawer(&db, knowledge_drawer("drawer_rust", "rust"));
    insert_drawer(&db, knowledge_drawer("drawer_docs", "docs"));

    let output = run_mempal(
        home.path(),
        &[
            "knowledge-card",
            "backfill-apply",
            "--execute",
            "--field",
            "rust",
            "--format",
            "json",
        ],
    );

    assert!(output.status.success(), "{}", stderr_text(&output));
    let value: Value = serde_json::from_slice(&output.stdout).expect("parse json");
    assert_eq!(value["dry_run"], false);
    assert_eq!(value["created_count"], 1);
    assert_eq!(value["linked_count"], 1);
    assert!(
        db.get_knowledge_card(&prospective_card_id("drawer_rust"))
            .expect("get rust card")
            .is_some()
    );
    assert!(
        db.get_knowledge_card(&prospective_card_id("drawer_docs"))
            .expect("get docs card")
            .is_none()
    );
}

#[test]
fn test_cli_knowledge_card_backfill_apply_rejects_invalid_format() {
    let (home, _db) = setup_home();

    let output = run_mempal(
        home.path(),
        &["knowledge-card", "backfill-apply", "--format", "yaml"],
    );
    assert!(!output.status.success());
    assert!(stderr_text(&output).contains("unsupported knowledge-card backfill-apply format"));
}

#[test]
fn test_cli_knowledge_card_backfill_plan_json_filters() {
    let (home, db) = setup_home();
    insert_drawer(&db, knowledge_drawer("drawer_rust", "rust"));
    insert_drawer(&db, knowledge_drawer("drawer_docs", "docs"));

    let output = run_mempal(
        home.path(),
        &[
            "knowledge-card",
            "backfill-plan",
            "--field",
            "rust",
            "--format",
            "json",
        ],
    );
    assert!(output.status.success(), "{}", stderr_text(&output));
    let value: Value = serde_json::from_slice(&output.stdout).expect("parse json");
    assert_eq!(value["ready_count"], 1);
    let candidates = value["candidates"].as_array().expect("candidates");
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0]["source_drawer_id"], "drawer_rust");
}

#[test]
fn test_cli_knowledge_card_backfill_plan_rejects_invalid_format() {
    let (home, _db) = setup_home();

    let output = run_mempal(
        home.path(),
        &["knowledge-card", "backfill-plan", "--format", "yaml"],
    );
    assert!(!output.status.success());
    assert!(stderr_text(&output).contains("unsupported knowledge-card backfill-plan format"));
}
