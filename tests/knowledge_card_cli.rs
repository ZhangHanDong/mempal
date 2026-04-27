use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use mempal::core::db::Database;
use mempal::core::types::{
    AnchorKind, BootstrapEvidenceArgs, Drawer, KnowledgeCard, KnowledgeEvidenceLink,
    KnowledgeEvidenceRole, KnowledgeStatus, KnowledgeTier, MemoryDomain, MemoryKind, SourceType,
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

fn card(id: &str, tier: KnowledgeTier, status: KnowledgeStatus, field: &str) -> KnowledgeCard {
    KnowledgeCard {
        id: id.to_string(),
        statement: format!("Statement for {id}."),
        content: format!("Content for {id}."),
        tier,
        status,
        domain: MemoryDomain::Project,
        field: field.to_string(),
        anchor_kind: AnchorKind::Repo,
        anchor_id: "repo://mempal".to_string(),
        parent_anchor_id: None,
        scope_constraints: None,
        trigger_hints: None,
        created_at: "1710000000".to_string(),
        updated_at: "1710000000".to_string(),
    }
}

fn insert_card(db: &Database, card: KnowledgeCard) {
    db.insert_knowledge_card(&card).expect("insert card");
}

fn insert_evidence_drawer(db: &Database, id: &str) {
    db.insert_drawer(&Drawer::new_bootstrap_evidence(BootstrapEvidenceArgs {
        id: id.to_string(),
        content: "Evidence body.".to_string(),
        wing: "mempal".to_string(),
        room: Some("phase2".to_string()),
        source_file: Some(format!("tests://{id}")),
        source_type: SourceType::Manual,
        added_at: "1710000000".to_string(),
        chunk_index: Some(0),
        importance: 0,
    }))
    .expect("insert evidence drawer");
}

fn insert_link(
    db: &Database,
    id: &str,
    card_id: &str,
    evidence_drawer_id: &str,
    role: KnowledgeEvidenceRole,
) {
    db.insert_knowledge_evidence_link(&KnowledgeEvidenceLink {
        id: id.to_string(),
        card_id: card_id.to_string(),
        evidence_drawer_id: evidence_drawer_id.to_string(),
        role,
        note: None,
        created_at: "1710000000".to_string(),
    })
    .expect("insert link");
}

fn insert_knowledge_drawer(db: &Database, id: &str) {
    let mut drawer = Drawer::new_bootstrap_evidence(BootstrapEvidenceArgs {
        id: id.to_string(),
        content: "Knowledge body.".to_string(),
        wing: "mempal".to_string(),
        room: Some("phase2".to_string()),
        source_file: Some(format!("knowledge://project/phase2/{id}")),
        source_type: SourceType::Manual,
        added_at: "1710000000".to_string(),
        chunk_index: Some(0),
        importance: 0,
    });
    drawer.memory_kind = MemoryKind::Knowledge;
    drawer.provenance = None;
    drawer.statement = Some("Knowledge statement.".to_string());
    drawer.tier = Some(KnowledgeTier::Shu);
    drawer.status = Some(KnowledgeStatus::Promoted);
    drawer.supporting_refs = vec!["drawer_evidence".to_string()];
    db.insert_drawer(&drawer).expect("insert knowledge drawer");
}

fn stdout_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

#[test]
fn test_cli_knowledge_card_create_get_json() {
    let (home, _db) = setup_home();

    let create = run_mempal(
        home.path(),
        &[
            "knowledge-card",
            "create",
            "--id",
            "card_cli",
            "--statement",
            "Evidence-backed knowledge cards are managed separately.",
            "--content",
            "Cards store distilled statements and link back to raw evidence drawers.",
            "--tier",
            "dao_ren",
            "--status",
            "promoted",
            "--domain",
            "project",
            "--field",
            "rust",
            "--anchor-kind",
            "repo",
            "--anchor-id",
            "repo://mempal",
            "--intent-tag",
            "memory",
            "--tool-need",
            "cargo",
        ],
    );
    assert!(create.status.success(), "{}", stderr_text(&create));
    assert!(stdout_text(&create).contains("card_id=card_cli"));

    let get = run_mempal(
        home.path(),
        &["knowledge-card", "get", "card_cli", "--format", "json"],
    );
    assert!(get.status.success(), "{}", stderr_text(&get));
    let value: Value = serde_json::from_slice(&get.stdout).expect("parse card json");
    assert_eq!(value["id"], "card_cli");
    assert_eq!(
        value["statement"],
        "Evidence-backed knowledge cards are managed separately."
    );
    assert_eq!(value["tier"], "dao_ren");
    assert_eq!(value["status"], "promoted");
    assert_eq!(value["domain"], "project");
    assert_eq!(value["field"], "rust");
    assert_eq!(value["anchor_kind"], "repo");
    assert_eq!(value["anchor_id"], "repo://mempal");
    assert_eq!(value["trigger_hints"]["intent_tags"][0], "memory");
}

#[test]
fn test_cli_knowledge_card_create_generates_id() {
    let (home, _db) = setup_home();

    let create = run_mempal(
        home.path(),
        &[
            "knowledge-card",
            "create",
            "--statement",
            "Generated card ids are stable.",
            "--content",
            "The CLI hashes card metadata when no id is provided.",
            "--tier",
            "shu",
            "--status",
            "promoted",
            "--field",
            "rust",
            "--anchor-id",
            "repo://mempal",
        ],
    );
    assert!(create.status.success(), "{}", stderr_text(&create));
    let stdout = stdout_text(&create);
    let card_id = stdout
        .split_whitespace()
        .find_map(|part| part.strip_prefix("card_id="))
        .expect("generated card id");
    assert!(card_id.starts_with("card_"));

    let get = run_mempal(home.path(), &["knowledge-card", "get", card_id]);
    assert!(get.status.success(), "{}", stderr_text(&get));
    assert!(stdout_text(&get).contains("Generated card ids are stable."));
}

#[test]
fn test_cli_knowledge_card_list_filters_plain() {
    let (home, db) = setup_home();
    insert_card(
        &db,
        card(
            "card_match",
            KnowledgeTier::DaoRen,
            KnowledgeStatus::Promoted,
            "rust",
        ),
    );
    insert_card(
        &db,
        card(
            "card_wrong_tier",
            KnowledgeTier::Shu,
            KnowledgeStatus::Promoted,
            "rust",
        ),
    );
    insert_card(
        &db,
        card(
            "card_wrong_field",
            KnowledgeTier::DaoRen,
            KnowledgeStatus::Promoted,
            "docs",
        ),
    );

    let list = run_mempal(
        home.path(),
        &[
            "knowledge-card",
            "list",
            "--tier",
            "dao_ren",
            "--status",
            "promoted",
            "--field",
            "rust",
        ],
    );
    assert!(list.status.success(), "{}", stderr_text(&list));
    let stdout = stdout_text(&list);
    assert!(stdout.contains("card_match"));
    assert!(!stdout.contains("card_wrong_tier"));
    assert!(!stdout.contains("card_wrong_field"));
}

#[test]
fn test_cli_knowledge_card_link_requires_evidence_drawer() {
    let (home, db) = setup_home();
    insert_card(
        &db,
        card(
            "card_cli",
            KnowledgeTier::Shu,
            KnowledgeStatus::Promoted,
            "rust",
        ),
    );
    insert_evidence_drawer(&db, "drawer_evidence");
    insert_knowledge_drawer(&db, "drawer_knowledge");

    let ok = run_mempal(
        home.path(),
        &[
            "knowledge-card",
            "link",
            "card_cli",
            "drawer_evidence",
            "--role",
            "supporting",
        ],
    );
    assert!(ok.status.success(), "{}", stderr_text(&ok));
    assert!(stdout_text(&ok).contains("link_id="));

    let rejected = run_mempal(
        home.path(),
        &[
            "knowledge-card",
            "link",
            "card_cli",
            "drawer_knowledge",
            "--role",
            "supporting",
        ],
    );
    assert!(!rejected.status.success());
    assert!(stderr_text(&rejected).contains("must be an evidence drawer"));
}

#[test]
fn test_cli_knowledge_card_event_append_and_list_json() {
    let (home, db) = setup_home();
    insert_card(
        &db,
        card(
            "card_cli",
            KnowledgeTier::Shu,
            KnowledgeStatus::Promoted,
            "rust",
        ),
    );

    let append = run_mempal(
        home.path(),
        &[
            "knowledge-card",
            "event",
            "card_cli",
            "--type",
            "created",
            "--reason",
            "seeded",
            "--actor",
            "codex",
        ],
    );
    assert!(append.status.success(), "{}", stderr_text(&append));
    assert!(stdout_text(&append).contains("event_id="));

    let events = run_mempal(
        home.path(),
        &["knowledge-card", "events", "card_cli", "--format", "json"],
    );
    assert!(events.status.success(), "{}", stderr_text(&events));
    let value: Value = serde_json::from_slice(&events.stdout).expect("parse events json");
    let array = value.as_array().expect("events array");
    assert_eq!(array.len(), 1);
    assert_eq!(array[0]["card_id"], "card_cli");
    assert_eq!(array[0]["event_type"], "created");
    assert_eq!(array[0]["reason"], "seeded");
    assert_eq!(array[0]["actor"], "codex");
}

#[test]
fn test_cli_knowledge_card_gate_counts_links() {
    let (home, db) = setup_home();
    insert_card(
        &db,
        card(
            "card_gate",
            KnowledgeTier::Shu,
            KnowledgeStatus::Promoted,
            "rust",
        ),
    );
    insert_evidence_drawer(&db, "drawer_supporting");
    insert_evidence_drawer(&db, "drawer_verification");
    insert_link(
        &db,
        "link_supporting",
        "card_gate",
        "drawer_supporting",
        KnowledgeEvidenceRole::Supporting,
    );
    insert_link(
        &db,
        "link_verification",
        "card_gate",
        "drawer_verification",
        KnowledgeEvidenceRole::Verification,
    );

    let output = run_mempal(
        home.path(),
        &[
            "knowledge-card",
            "gate",
            "card_gate",
            "--target-status",
            "promoted",
            "--format",
            "json",
        ],
    );

    assert!(output.status.success(), "{}", stderr_text(&output));
    let value: Value = serde_json::from_slice(&output.stdout).expect("parse gate json");
    assert_eq!(value["card_id"], "card_gate");
    assert_eq!(value["allowed"], true);
    assert_eq!(value["evidence_counts"]["supporting"], 1);
    assert_eq!(value["evidence_counts"]["verification"], 1);
}

#[test]
fn test_cli_knowledge_card_promote_gate_enforced_and_appends_event() {
    let (home, db) = setup_home();
    insert_card(
        &db,
        card(
            "card_promote",
            KnowledgeTier::Qi,
            KnowledgeStatus::Candidate,
            "rust",
        ),
    );
    insert_evidence_drawer(&db, "drawer_supporting");
    insert_evidence_drawer(&db, "drawer_verification");
    insert_link(
        &db,
        "link_supporting",
        "card_promote",
        "drawer_supporting",
        KnowledgeEvidenceRole::Supporting,
    );

    let output = run_mempal(
        home.path(),
        &[
            "knowledge-card",
            "promote",
            "card_promote",
            "--status",
            "promoted",
            "--verification-ref",
            "drawer_verification",
            "--reason",
            "verified in CLI test",
            "--format",
            "json",
        ],
    );

    assert!(output.status.success(), "{}", stderr_text(&output));
    let value: Value = serde_json::from_slice(&output.stdout).expect("parse promote json");
    assert_eq!(value["old_status"], "candidate");
    assert_eq!(value["new_status"], "promoted");
    let db_card = db
        .get_knowledge_card("card_promote")
        .expect("get card")
        .expect("card exists");
    assert_eq!(db_card.status, KnowledgeStatus::Promoted);
    let events = db.knowledge_events("card_promote").expect("events");
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].event_type,
        mempal::core::types::KnowledgeEventType::Promoted
    );
}

#[test]
fn test_cli_knowledge_card_promote_gate_failure_does_not_mutate() {
    let (home, db) = setup_home();
    insert_card(
        &db,
        card(
            "card_blocked",
            KnowledgeTier::Shu,
            KnowledgeStatus::Promoted,
            "rust",
        ),
    );
    insert_evidence_drawer(&db, "drawer_verification");

    let output = run_mempal(
        home.path(),
        &[
            "knowledge-card",
            "promote",
            "card_blocked",
            "--status",
            "promoted",
            "--verification-ref",
            "drawer_verification",
            "--reason",
            "missing supporting evidence",
        ],
    );

    assert!(!output.status.success());
    assert!(stderr_text(&output).contains("promotion gate failed"));
    let db_card = db
        .get_knowledge_card("card_blocked")
        .expect("get card")
        .expect("card exists");
    assert_eq!(db_card.status, KnowledgeStatus::Promoted);
    assert_eq!(
        db.knowledge_evidence_links("card_blocked")
            .expect("links")
            .len(),
        0
    );
    assert_eq!(
        db.knowledge_events("card_blocked").expect("events").len(),
        0
    );
}

#[test]
fn test_cli_knowledge_card_demote_links_counterexample_and_appends_event() {
    let (home, db) = setup_home();
    insert_card(
        &db,
        card(
            "card_demote",
            KnowledgeTier::Shu,
            KnowledgeStatus::Promoted,
            "rust",
        ),
    );
    insert_evidence_drawer(&db, "drawer_counterexample");

    let output = run_mempal(
        home.path(),
        &[
            "knowledge-card",
            "demote",
            "card_demote",
            "--status",
            "demoted",
            "--evidence-ref",
            "drawer_counterexample",
            "--reason",
            "contradicted by runtime evidence",
            "--reason-type",
            "contradicted",
            "--format",
            "json",
        ],
    );

    assert!(output.status.success(), "{}", stderr_text(&output));
    let value: Value = serde_json::from_slice(&output.stdout).expect("parse demote json");
    assert_eq!(value["old_status"], "promoted");
    assert_eq!(value["new_status"], "demoted");
    let db_card = db
        .get_knowledge_card("card_demote")
        .expect("get card")
        .expect("card exists");
    assert_eq!(db_card.status, KnowledgeStatus::Demoted);
    assert_eq!(
        db.knowledge_evidence_links("card_demote").expect("links")[0].role,
        KnowledgeEvidenceRole::Counterexample
    );
    let events = db.knowledge_events("card_demote").expect("events");
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].event_type,
        mempal::core::types::KnowledgeEventType::Demoted
    );
}

#[test]
fn test_cli_knowledge_card_rejects_invalid_format() {
    let (home, _db) = setup_home();

    let output = run_mempal(home.path(), &["knowledge-card", "list", "--format", "yaml"]);
    assert!(!output.status.success());
    assert!(stderr_text(&output).contains("unsupported knowledge-card format"));
}
