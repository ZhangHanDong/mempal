use mempal::core::db::Database;
use mempal::core::types::{BootstrapEvidenceArgs, Drawer, SourceType};
use rusqlite::{Connection, params};
use tempfile::TempDir;

fn new_db() -> (TempDir, Database) {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("palace.db");
    let db = Database::open(&db_path).expect("open db");
    (tmp, db)
}

fn create_v7_db(path: &std::path::Path) {
    let conn = Connection::open(path).expect("open v7 db");
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
            trigger_hints TEXT,
            normalize_version INTEGER NOT NULL DEFAULT 1
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

        INSERT INTO drawers (
            id,
            content,
            wing,
            room,
            source_file,
            source_type,
            added_at,
            chunk_index,
            importance,
            provenance
        )
        VALUES (
            'drawer_ev_001',
            'existing evidence',
            'mempal',
            'phase2',
            'tests://evidence',
            'manual',
            '1710000000',
            0,
            3,
            'human'
        );

        INSERT INTO triples (id, subject, predicate, object, source_drawer)
        VALUES ('triple_001', 'mempal', 'has_phase', 'phase2', 'drawer_ev_001');

        INSERT INTO taxonomy (wing, room, display_name, keywords)
        VALUES ('mempal', 'phase2', 'Phase 2', '["knowledge"]');

        INSERT INTO tunnels (
            id,
            left_wing,
            left_room,
            right_wing,
            right_room,
            label,
            created_at
        )
        VALUES (
            'tunnel_001',
            'mempal',
            'phase2',
            'mempal',
            'bootstrap',
            'phase links',
            '1710000000'
        );

        PRAGMA user_version = 7;
        "#,
    )
    .expect("create v7 db");
}

fn table_exists(db: &Database, table: &str) -> bool {
    db.conn()
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
            [table],
            |row| row.get::<_, bool>(0),
        )
        .expect("query table existence")
}

fn index_exists(db: &Database, index: &str) -> bool {
    db.conn()
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='index' AND name=?1)",
            [index],
            |row| row.get::<_, bool>(0),
        )
        .expect("query index existence")
}

fn insert_evidence_drawer(db: &Database, id: &str) {
    db.insert_drawer(&Drawer::new_bootstrap_evidence(BootstrapEvidenceArgs {
        id: id.to_string(),
        content: "evidence body".to_string(),
        wing: "mempal".to_string(),
        room: Some("phase2".to_string()),
        source_file: Some("tests://evidence".to_string()),
        source_type: SourceType::Manual,
        added_at: "1710000000".to_string(),
        chunk_index: Some(0),
        importance: 0,
    }))
    .expect("insert evidence drawer");
}

fn insert_card(db: &Database, id: &str) {
    insert_card_with(db, id, "shu", "promoted", "project", "repo").expect("insert card");
}

fn insert_card_with(
    db: &Database,
    id: &str,
    tier: &str,
    status: &str,
    domain: &str,
    anchor_kind: &str,
) -> rusqlite::Result<usize> {
    db.conn().execute(
        r#"
        INSERT INTO knowledge_cards (
            id,
            statement,
            content,
            tier,
            status,
            domain,
            field,
            anchor_kind,
            anchor_id,
            created_at,
            updated_at
        )
        VALUES (?1, 'Use evidence before assertion.', 'Knowledge card body.', ?2, ?3, ?4, 'epistemics', ?5, 'repo://mempal', '1710000000', '1710000000')
        "#,
        params![id, tier, status, domain, anchor_kind],
    )
}

fn insert_link(
    db: &Database,
    id: &str,
    card_id: &str,
    drawer_id: &str,
    role: &str,
) -> rusqlite::Result<usize> {
    db.conn().execute(
        r#"
        INSERT INTO knowledge_evidence_links (
            id,
            card_id,
            evidence_drawer_id,
            role,
            created_at
        )
        VALUES (?1, ?2, ?3, ?4, '1710000000')
        "#,
        params![id, card_id, drawer_id, role],
    )
}

fn insert_event(
    db: &Database,
    id: &str,
    card_id: &str,
    event_type: &str,
) -> rusqlite::Result<usize> {
    db.conn().execute(
        r#"
        INSERT INTO knowledge_events (
            id,
            card_id,
            event_type,
            reason,
            created_at
        )
        VALUES (?1, ?2, ?3, 'because evidence supports it', '1710000000')
        "#,
        params![id, card_id, event_type],
    )
}

#[test]
fn test_new_database_schema_version_is_8() {
    let (_tmp, db) = new_db();

    assert_eq!(db.schema_version().expect("schema version"), 8);
    for table in [
        "knowledge_cards",
        "knowledge_evidence_links",
        "knowledge_events",
    ] {
        assert!(table_exists(&db, table), "{table} should exist");
    }
}

#[test]
fn test_migration_v7_to_v8_adds_knowledge_card_tables_without_data_loss() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("palace.db");
    create_v7_db(&db_path);

    let db = Database::open(&db_path).expect("migrate v7 db");

    assert_eq!(db.schema_version().expect("schema version"), 8);
    assert_eq!(db.drawer_count().expect("drawer count"), 1);
    assert_eq!(db.triple_count().expect("triple count"), 1);
    let taxonomy_count: i64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM taxonomy", [], |row| row.get(0))
        .expect("taxonomy count");
    assert_eq!(taxonomy_count, 1);
    let tunnels_count: i64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM tunnels", [], |row| row.get(0))
        .expect("tunnels count");
    assert_eq!(tunnels_count, 1);

    for table in [
        "knowledge_cards",
        "knowledge_evidence_links",
        "knowledge_events",
    ] {
        let count: i64 = db
            .conn()
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .expect("knowledge table count");
        assert_eq!(count, 0, "{table} should start empty after migration");
    }
}

#[test]
fn test_knowledge_cards_reject_invalid_tier_status_domain_anchor() {
    let (_tmp, db) = new_db();

    assert!(insert_card_with(&db, "card_bad_tier", "bad", "promoted", "project", "repo").is_err());
    assert!(insert_card_with(&db, "card_bad_status", "shu", "bad", "project", "repo").is_err());
    assert!(insert_card_with(&db, "card_bad_domain", "shu", "promoted", "bad", "repo").is_err());
    assert!(insert_card_with(&db, "card_bad_anchor", "shu", "promoted", "project", "bad").is_err());
}

#[test]
fn test_knowledge_evidence_links_reject_invalid_role_and_missing_drawer() {
    let (_tmp, db) = new_db();
    insert_card(&db, "card_link_validation");

    assert!(
        insert_link(
            &db,
            "link_bad_role",
            "card_link_validation",
            "drawer_missing",
            "invalid_role",
        )
        .is_err()
    );
    assert!(
        insert_link(
            &db,
            "link_missing_drawer",
            "card_link_validation",
            "drawer_missing",
            "supporting",
        )
        .is_err()
    );
}

#[test]
fn test_knowledge_evidence_links_dedup_card_drawer_role() {
    let (_tmp, db) = new_db();
    insert_card(&db, "card_dedup");
    insert_evidence_drawer(&db, "drawer_ev_dedup");

    insert_link(
        &db,
        "link_dedup_1",
        "card_dedup",
        "drawer_ev_dedup",
        "supporting",
    )
    .expect("first link insert");
    assert!(
        insert_link(
            &db,
            "link_dedup_2",
            "card_dedup",
            "drawer_ev_dedup",
            "supporting",
        )
        .is_err()
    );
    insert_link(
        &db,
        "link_dedup_verification",
        "card_dedup",
        "drawer_ev_dedup",
        "verification",
    )
    .expect("different role is allowed");
}

#[test]
fn test_knowledge_events_reject_invalid_type_and_missing_card() {
    let (_tmp, db) = new_db();
    insert_card(&db, "card_event_validation");

    assert!(
        insert_event(
            &db,
            "event_bad_type",
            "card_event_validation",
            "invalid_event"
        )
        .is_err()
    );
    assert!(insert_event(&db, "event_missing_card", "card_missing", "created").is_err());
}

#[test]
fn test_knowledge_events_are_append_only() {
    let (_tmp, db) = new_db();
    insert_card(&db, "card_append_only");
    insert_event(&db, "event_append_only", "card_append_only", "created").expect("insert event");

    assert!(
        db.conn()
            .execute(
                "UPDATE knowledge_events SET reason = 'mutated' WHERE id = 'event_append_only'",
                [],
            )
            .is_err()
    );
    assert!(
        db.conn()
            .execute(
                "DELETE FROM knowledge_events WHERE id = 'event_append_only'",
                []
            )
            .is_err()
    );
    let count: i64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM knowledge_events", [], |row| {
            row.get(0)
        })
        .expect("event count");
    assert_eq!(count, 1);
}

#[test]
fn test_knowledge_card_schema_indexes_exist() {
    let (_tmp, db) = new_db();

    for index in [
        "idx_knowledge_cards_tier_status",
        "idx_knowledge_cards_domain_field",
        "idx_knowledge_cards_anchor",
        "idx_knowledge_evidence_links_card",
        "idx_knowledge_evidence_links_evidence",
        "idx_knowledge_events_card_created_at",
    ] {
        assert!(index_exists(&db, index), "{index} should exist");
    }
}
