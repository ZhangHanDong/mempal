use mempal::core::db::Database;
use mempal::core::types::{
    AnchorKind, BootstrapEvidenceArgs, Drawer, KnowledgeCard, KnowledgeCardEvent,
    KnowledgeCardFilter, KnowledgeEventType, KnowledgeEvidenceLink, KnowledgeEvidenceRole,
    KnowledgeStatus, KnowledgeTier, MemoryDomain, MemoryKind, SourceType, TriggerHints,
};
use serde_json::json;
use tempfile::TempDir;

fn new_db() -> (TempDir, Database) {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("palace.db");
    let db = Database::open(&db_path).expect("open db");
    (tmp, db)
}

fn card(id: &str) -> KnowledgeCard {
    KnowledgeCard {
        id: id.to_string(),
        statement: format!("Statement for {id}."),
        content: format!("Detailed rationale for {id}."),
        tier: KnowledgeTier::Shu,
        status: KnowledgeStatus::Promoted,
        domain: MemoryDomain::Project,
        field: "debugging".to_string(),
        anchor_kind: AnchorKind::Repo,
        anchor_id: "repo://mempal".to_string(),
        parent_anchor_id: None,
        scope_constraints: Some("Only applies to Rust code.".to_string()),
        trigger_hints: Some(TriggerHints {
            intent_tags: vec!["debugging".to_string()],
            workflow_bias: vec!["test-first".to_string()],
            tool_needs: vec!["cargo".to_string()],
        }),
        created_at: "1710000000".to_string(),
        updated_at: "1710000000".to_string(),
    }
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

fn insert_knowledge_drawer(db: &Database, id: &str) {
    let mut drawer = Drawer::new_bootstrap_evidence(BootstrapEvidenceArgs {
        id: id.to_string(),
        content: "Knowledge drawer body.".to_string(),
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
    drawer.statement = Some("Knowledge drawer statement.".to_string());
    drawer.tier = Some(KnowledgeTier::Shu);
    drawer.status = Some(KnowledgeStatus::Promoted);
    drawer.supporting_refs = vec!["drawer_ev_linkable".to_string()];
    db.insert_drawer(&drawer).expect("insert knowledge drawer");
}

fn link(
    id: &str,
    card_id: &str,
    drawer_id: &str,
    role: KnowledgeEvidenceRole,
) -> KnowledgeEvidenceLink {
    KnowledgeEvidenceLink {
        id: id.to_string(),
        card_id: card_id.to_string(),
        evidence_drawer_id: drawer_id.to_string(),
        role,
        note: Some("supports the card".to_string()),
        created_at: "1710000000".to_string(),
    }
}

fn event(id: &str, card_id: &str, event_type: KnowledgeEventType) -> KnowledgeCardEvent {
    KnowledgeCardEvent {
        id: id.to_string(),
        card_id: card_id.to_string(),
        event_type,
        from_status: Some(KnowledgeStatus::Candidate),
        to_status: Some(KnowledgeStatus::Promoted),
        reason: "validated by evidence".to_string(),
        actor: Some("codex".to_string()),
        metadata: Some(json!({
            "verification_refs": ["drawer_ev_linkable"],
            "score": 1
        })),
        created_at: "1710000000".to_string(),
    }
}

#[test]
fn test_knowledge_card_insert_get_roundtrip() {
    let (_tmp, db) = new_db();
    let card = card("card_roundtrip");

    db.insert_knowledge_card(&card).expect("insert card");
    let loaded = db
        .get_knowledge_card("card_roundtrip")
        .expect("get card")
        .expect("card exists");

    assert_eq!(loaded, card);
    assert_eq!(
        loaded.trigger_hints.expect("trigger hints").tool_needs,
        vec!["cargo"]
    );
}

#[test]
fn test_knowledge_card_list_filters() {
    let (_tmp, db) = new_db();
    let mut rust_card = card("card_rust");
    rust_card.tier = KnowledgeTier::DaoRen;
    rust_card.status = KnowledgeStatus::Promoted;
    rust_card.domain = MemoryDomain::Project;
    rust_card.field = "software-engineering".to_string();
    rust_card.anchor_kind = AnchorKind::Worktree;
    rust_card.anchor_id = "worktree:///tmp/mempal".to_string();

    let mut global_card = card("card_global");
    global_card.tier = KnowledgeTier::DaoTian;
    global_card.status = KnowledgeStatus::Canonical;
    global_card.domain = MemoryDomain::Global;
    global_card.field = "epistemics".to_string();
    global_card.anchor_kind = AnchorKind::Global;
    global_card.anchor_id = "global://all".to_string();

    db.insert_knowledge_card(&rust_card)
        .expect("insert rust card");
    db.insert_knowledge_card(&global_card)
        .expect("insert global card");

    let by_tier = db
        .list_knowledge_cards(&KnowledgeCardFilter {
            tier: Some(KnowledgeTier::DaoTian),
            ..KnowledgeCardFilter::default()
        })
        .expect("list by tier");
    assert_eq!(by_tier, vec![global_card.clone()]);

    let by_field = db
        .list_knowledge_cards(&KnowledgeCardFilter {
            domain: Some(MemoryDomain::Project),
            field: Some("software-engineering".to_string()),
            anchor_kind: Some(AnchorKind::Worktree),
            anchor_id: Some("worktree:///tmp/mempal".to_string()),
            ..KnowledgeCardFilter::default()
        })
        .expect("list by field");
    assert_eq!(by_field, vec![rust_card]);
}

#[test]
fn test_knowledge_card_update_preserves_identity_and_created_at() {
    let (_tmp, db) = new_db();
    let mut card = card("card_update");
    db.insert_knowledge_card(&card).expect("insert card");

    card.statement = "Updated statement.".to_string();
    card.content = "Updated content.".to_string();
    card.status = KnowledgeStatus::Demoted;
    card.scope_constraints = Some("Updated constraints.".to_string());
    card.trigger_hints = None;
    card.created_at = "9999999999".to_string();
    card.updated_at = "1710009999".to_string();

    assert!(db.update_knowledge_card(&card).expect("update card"));
    let loaded = db
        .get_knowledge_card("card_update")
        .expect("get card")
        .expect("card exists");

    assert_eq!(loaded.id, "card_update");
    assert_eq!(loaded.created_at, "1710000000");
    assert_eq!(loaded.updated_at, "1710009999");
    assert_eq!(loaded.statement, "Updated statement.");
    assert_eq!(loaded.status, KnowledgeStatus::Demoted);
    assert_eq!(loaded.trigger_hints, None);
}

#[test]
fn test_knowledge_evidence_link_requires_evidence_drawer() {
    let (_tmp, db) = new_db();
    db.insert_knowledge_card(&card("card_linkable"))
        .expect("insert card");
    insert_evidence_drawer(&db, "drawer_ev_linkable");
    insert_knowledge_drawer(&db, "drawer_kn_not_evidence");

    db.insert_knowledge_evidence_link(&link(
        "link_ok",
        "card_linkable",
        "drawer_ev_linkable",
        KnowledgeEvidenceRole::Supporting,
    ))
    .expect("link evidence drawer");

    let wrong_kind = db
        .insert_knowledge_evidence_link(&link(
            "link_wrong_kind",
            "card_linkable",
            "drawer_kn_not_evidence",
            KnowledgeEvidenceRole::Supporting,
        ))
        .expect_err("knowledge drawer must not link as evidence");
    assert!(
        wrong_kind
            .to_string()
            .contains("must be an evidence drawer")
    );

    let missing = db
        .insert_knowledge_evidence_link(&link(
            "link_missing",
            "card_linkable",
            "drawer_missing",
            KnowledgeEvidenceRole::Supporting,
        ))
        .expect_err("missing drawer must fail");
    assert!(missing.to_string().contains("does not exist"));
}

#[test]
fn test_knowledge_evidence_links_list_by_card() {
    let (_tmp, db) = new_db();
    db.insert_knowledge_card(&card("card_links_a"))
        .expect("insert card a");
    db.insert_knowledge_card(&card("card_links_b"))
        .expect("insert card b");
    insert_evidence_drawer(&db, "drawer_ev_a");
    insert_evidence_drawer(&db, "drawer_ev_b");

    db.insert_knowledge_evidence_link(&link(
        "link_a",
        "card_links_a",
        "drawer_ev_a",
        KnowledgeEvidenceRole::Supporting,
    ))
    .expect("insert link a");
    db.insert_knowledge_evidence_link(&link(
        "link_b",
        "card_links_b",
        "drawer_ev_b",
        KnowledgeEvidenceRole::Verification,
    ))
    .expect("insert link b");

    let links = db
        .knowledge_evidence_links("card_links_a")
        .expect("list links");
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].id, "link_a");
    assert_eq!(links[0].role, KnowledgeEvidenceRole::Supporting);
}

#[test]
fn test_knowledge_events_append_and_list_by_card() {
    let (_tmp, db) = new_db();
    db.insert_knowledge_card(&card("card_events_a"))
        .expect("insert card a");
    db.insert_knowledge_card(&card("card_events_b"))
        .expect("insert card b");

    db.append_knowledge_event(&event(
        "event_a_created",
        "card_events_a",
        KnowledgeEventType::Created,
    ))
    .expect("append event a");
    db.append_knowledge_event(&event(
        "event_b_created",
        "card_events_b",
        KnowledgeEventType::Created,
    ))
    .expect("append event b");

    let events = db.knowledge_events("card_events_a").expect("list events");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].id, "event_a_created");
    assert_eq!(events[0].event_type, KnowledgeEventType::Created);
    assert_eq!(events[0].metadata.as_ref().expect("metadata")["score"], 1);
}
