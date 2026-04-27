use mempal::core::db::Database;
use mempal::core::types::{BootstrapEvidenceArgs, Drawer, SourceType, TunnelEndpoint};
use mempal::embed::{Embedder, EmbedderFactory};
use mempal::mcp::MempalMcpServer;
use rusqlite::Connection;
use serde_json::json;
use std::process::Command;
use std::sync::Arc;
use tempfile::TempDir;

struct StubEmbedderFactory;

struct StubEmbedder;

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

fn new_db() -> (TempDir, Database) {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("palace.db");
    let db = Database::open(&db_path).expect("open db");
    (tmp, db)
}

fn setup_mcp_server() -> (TempDir, Database, MempalMcpServer) {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("palace.db");
    let db = Database::open(&db_path).expect("open db");
    let server = MempalMcpServer::new_with_factory(db_path, Arc::new(StubEmbedderFactory));
    (tmp, db, server)
}

fn endpoint(wing: &str, room: Option<&str>) -> TunnelEndpoint {
    TunnelEndpoint {
        wing: wing.to_string(),
        room: room.map(ToOwned::to_owned),
    }
}

fn insert_passive_drawer(db: &Database, id: &str, wing: &str, room: &str) {
    db.insert_drawer(&Drawer::new_bootstrap_evidence(BootstrapEvidenceArgs {
        id: id.to_string(),
        content: format!("{wing} {room}"),
        wing: wing.to_string(),
        room: Some(room.to_string()),
        source_file: Some(format!("{wing}.md")),
        source_type: SourceType::Project,
        added_at: "1710000000".to_string(),
        chunk_index: Some(0),
        importance: 0,
    }))
    .expect("insert passive drawer");
}

fn insert_search_drawer(db: &Database, id: &str, wing: &str, room: &str, content: &str) {
    db.insert_drawer(&Drawer::new_bootstrap_evidence(BootstrapEvidenceArgs {
        id: id.to_string(),
        content: content.to_string(),
        wing: wing.to_string(),
        room: Some(room.to_string()),
        source_file: Some(format!("{wing}-{room}.md")),
        source_type: SourceType::Project,
        added_at: "1710000000".to_string(),
        chunk_index: Some(0),
        importance: 0,
    }))
    .expect("insert search drawer");
    db.insert_vector(id, &[0.1, 0.2, 0.3])
        .expect("insert search vector");
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

fn create_v5_db(path: &std::path::Path) {
    let conn = Connection::open(path).expect("open v5 db");
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

        CREATE INDEX idx_drawers_wing ON drawers(wing);
        CREATE INDEX idx_drawers_wing_room ON drawers(wing, room);
        CREATE INDEX idx_drawers_deleted_at ON drawers(deleted_at);
        CREATE INDEX idx_triples_subject ON triples(subject);
        CREATE INDEX idx_triples_object ON triples(object);

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

        INSERT INTO drawers (
            id, content, wing, room, source_file, source_type, added_at, chunk_index,
            deleted_at, importance, provenance
        )
        VALUES
            ('drawer_001', 'content 1', 'mempal', 'auth', 'a.md', 'project', '1710000001', 0, NULL, 1, 'research'),
            ('drawer_002', 'content 2', 'robrix2', 'matrix', 'b.md', 'project', '1710000002', 0, NULL, 2, 'research');

        INSERT INTO triples (id, subject, predicate, object)
        VALUES ('triple_001', 'mempal', 'relates_to', 'robrix2');

        PRAGMA user_version = 5;
        "#,
    )
    .expect("apply v5 schema");
}

#[test]
fn test_schema_v5_to_v6_migration_preserves_data() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("palace.db");
    create_v5_db(&db_path);

    let db = Database::open(&db_path).expect("migrate v5 db");

    assert_eq!(db.schema_version().expect("schema version"), 8);
    assert_eq!(db.drawer_count().expect("drawer count"), 2);
    assert_eq!(db.triple_count().expect("triple count"), 1);
    let tunnels_count: i64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM tunnels", [], |row| row.get(0))
        .expect("tunnels table should exist");
    assert_eq!(tunnels_count, 0);
    let normalize_version_count: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM drawers WHERE normalize_version = 1",
            [],
            |row| row.get(0),
        )
        .expect("normalize_version should exist");
    assert_eq!(normalize_version_count, 2);
}

#[test]
fn test_add_tunnel_dedup_unordered() {
    let (_tmp, db) = new_db();
    let left = endpoint("mempal", Some("auth"));
    let right = endpoint("robrix2", Some("matrix"));

    let first = db
        .create_tunnel(&left, &right, "both handle user auth", Some("codex"))
        .expect("create first tunnel");
    let second = db
        .create_tunnel(&right, &left, "duplicate label ignored", Some("claude"))
        .expect("create duplicate tunnel");

    assert_eq!(first.id, second.id);
    let count: i64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM tunnels", [], |row| row.get(0))
        .expect("count tunnels");
    assert_eq!(count, 1);
}

#[test]
fn test_add_self_tunnel_rejected() {
    let (_tmp, db) = new_db();
    let left = endpoint("mempal", Some("auth"));

    let error = db
        .create_tunnel(&left, &left, "self", Some("codex"))
        .expect_err("self-link should be rejected");

    assert!(error.to_string().contains("self-link"));
    let count: i64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM tunnels", [], |row| row.get(0))
        .expect("count tunnels");
    assert_eq!(count, 0);
}

#[test]
fn test_delete_explicit_tunnel_soft_delete() {
    let (_tmp, db) = new_db();
    let tunnel = db
        .create_tunnel(
            &endpoint("mempal", Some("auth")),
            &endpoint("robrix2", Some("matrix")),
            "both handle user auth",
            Some("codex"),
        )
        .expect("create tunnel");

    assert!(
        db.delete_explicit_tunnel(&tunnel.id)
            .expect("delete explicit tunnel")
    );
    let deleted_at: Option<String> = db
        .conn()
        .query_row(
            "SELECT deleted_at FROM tunnels WHERE id = ?1",
            [&tunnel.id],
            |row| row.get(0),
        )
        .expect("read deleted_at");
    assert!(deleted_at.is_some());
    assert!(
        db.list_explicit_tunnels(None)
            .expect("list explicit tunnels")
            .is_empty()
    );
}

#[tokio::test]
async fn test_add_and_list_explicit_tunnel() {
    let (_tmp, _db, server) = setup_mcp_server();
    let add = server
        .tunnels_json_for_test(json!({
            "action": "add",
            "left": {"wing": "mempal", "room": "auth"},
            "right": {"wing": "robrix2", "room": "matrix-routing"},
            "label": "both handle user auth"
        }))
        .await
        .expect("add explicit tunnel");
    let tunnel_id = add.tunnels[0].tunnel_id.clone();

    let list = server
        .tunnels_json_for_test(json!({
            "action": "list",
            "wing": "mempal",
            "kind": "explicit"
        }))
        .await
        .expect("list explicit tunnels");

    assert_eq!(list.tunnels.len(), 1);
    assert_eq!(list.tunnels[0].tunnel_id, tunnel_id);
    assert_eq!(list.tunnels[0].kind, "explicit");
    assert_eq!(
        list.tunnels[0].label.as_deref(),
        Some("both handle user auth")
    );
}

#[tokio::test]
async fn test_follow_one_hop() {
    let (_tmp, _db, server) = setup_mcp_server();
    for (right_wing, right_room) in [("robrix2", "matrix"), ("octos", "login")] {
        server
            .tunnels_json_for_test(json!({
                "action": "add",
                "left": {"wing": "mempal", "room": "auth"},
                "right": {"wing": right_wing, "room": right_room},
                "label": "auth-related"
            }))
            .await
            .expect("add tunnel");
    }

    let follow = server
        .tunnels_json_for_test(json!({
            "action": "follow",
            "from": {"wing": "mempal", "room": "auth"},
            "max_hops": 1
        }))
        .await
        .expect("follow tunnels");

    let endpoints = follow
        .tunnels
        .iter()
        .map(|tunnel| {
            (
                tunnel.left.as_ref().expect("follow endpoint").wing.as_str(),
                tunnel
                    .left
                    .as_ref()
                    .expect("follow endpoint")
                    .room
                    .as_deref(),
                tunnel.hop,
            )
        })
        .collect::<Vec<_>>();
    assert!(endpoints.contains(&("robrix2", Some("matrix"), Some(1))));
    assert!(endpoints.contains(&("octos", Some("login"), Some(1))));
}

#[tokio::test]
async fn test_follow_two_hops() {
    let (_tmp, _db, server) = setup_mcp_server();
    for (left_wing, left_room, right_wing, right_room) in [
        ("mempal", "auth", "robrix2", "matrix"),
        ("robrix2", "matrix", "hermes-agent", "sso"),
    ] {
        server
            .tunnels_json_for_test(json!({
                "action": "add",
                "left": {"wing": left_wing, "room": left_room},
                "right": {"wing": right_wing, "room": right_room},
                "label": "auth-related"
            }))
            .await
            .expect("add tunnel");
    }

    let follow = server
        .tunnels_json_for_test(json!({
            "action": "follow",
            "from": {"wing": "mempal", "room": "auth"},
            "max_hops": 2
        }))
        .await
        .expect("follow tunnels");

    assert!(follow.tunnels.iter().any(|tunnel| {
        tunnel.left.as_ref().is_some_and(|endpoint| {
            endpoint.wing == "hermes-agent" && endpoint.room.as_deref() == Some("sso")
        }) && tunnel.hop == Some(2)
    }));
}

#[tokio::test]
async fn test_delete_passive_tunnel_rejected() {
    let (_tmp, db, server) = setup_mcp_server();
    insert_passive_drawer(&db, "drawer_passive_001", "mempal", "matrix-routing");
    insert_passive_drawer(&db, "drawer_passive_002", "robrix2", "matrix-routing");
    let passive = server
        .tunnels_json_for_test(json!({
            "action": "list",
            "kind": "passive"
        }))
        .await
        .expect("list passive tunnels");
    let passive_id = passive.tunnels[0].tunnel_id.clone();

    let error = server
        .tunnels_json_for_test(json!({
            "action": "delete",
            "tunnel_id": passive_id
        }))
        .await
        .expect_err("passive tunnel delete should reject");

    assert!(error.to_string().contains("passive"));
}

#[tokio::test]
async fn test_search_tunnel_hints_merges_passive_and_explicit() {
    let (_tmp, db, server) = setup_mcp_server();
    insert_search_drawer(
        &db,
        "drawer_search_mempal_auth",
        "mempal",
        "auth",
        "auth search target",
    );
    insert_search_drawer(
        &db,
        "drawer_search_octos_auth",
        "octos",
        "auth",
        "other passive auth",
    );
    db.create_tunnel(
        &endpoint("mempal", Some("auth")),
        &endpoint("robrix2", Some("matrix-routing")),
        "both handle auth routing",
        Some("codex"),
    )
    .expect("create explicit tunnel");

    let response = server
        .search_json_for_test(json!({
            "query": "auth search target",
            "wing": "mempal",
            "top_k": 1
        }))
        .await
        .expect("search should succeed");
    let result = response.results.first().expect("search result");

    assert!(result.tunnel_hints.contains(&"octos".to_string()));
    assert!(
        result
            .tunnel_hints
            .contains(&"robrix2:matrix-routing".to_string())
    );
}

#[test]
fn test_cli_tunnels_add_list_follow_delete() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("palace.db");
    write_cli_config(tmp.path(), &db_path);

    let add = Command::new(mempal_bin())
        .args([
            "tunnels",
            "add",
            "--left",
            "mempal:auth",
            "--right",
            "robrix2:matrix",
            "--label",
            "both handle user auth",
        ])
        .env("HOME", tmp.path())
        .output()
        .expect("run tunnels add");
    assert!(
        add.status.success(),
        "add failed: {}",
        String::from_utf8_lossy(&add.stderr)
    );
    let add_stdout = String::from_utf8_lossy(&add.stdout);
    let tunnel_id = add_stdout
        .split_whitespace()
        .find(|part| part.starts_with("tunnel_"))
        .expect("add output should include tunnel id")
        .to_string();

    let list = Command::new(mempal_bin())
        .args(["tunnels", "list", "--kind", "explicit"])
        .env("HOME", tmp.path())
        .output()
        .expect("run tunnels list");
    assert!(list.status.success());
    assert!(String::from_utf8_lossy(&list.stdout).contains("both handle user auth"));

    let follow = Command::new(mempal_bin())
        .args(["tunnels", "follow", "--from", "mempal:auth", "--hops", "1"])
        .env("HOME", tmp.path())
        .output()
        .expect("run tunnels follow");
    assert!(follow.status.success());
    assert!(String::from_utf8_lossy(&follow.stdout).contains("robrix2:matrix"));

    let delete = Command::new(mempal_bin())
        .args(["tunnels", "delete", &tunnel_id])
        .env("HOME", tmp.path())
        .output()
        .expect("run tunnels delete");
    assert!(
        delete.status.success(),
        "delete failed: {}",
        String::from_utf8_lossy(&delete.stderr)
    );

    let list_after_delete = Command::new(mempal_bin())
        .args(["tunnels", "list", "--kind", "explicit"])
        .env("HOME", tmp.path())
        .output()
        .expect("run tunnels list after delete");
    assert!(list_after_delete.status.success());
    assert!(!String::from_utf8_lossy(&list_after_delete.stdout).contains(&tunnel_id));
}
