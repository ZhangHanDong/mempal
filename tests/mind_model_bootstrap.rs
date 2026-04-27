//! Integration tests for P12 stage-1 mind-model bootstrap schema/core work.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::sync::Arc;
use std::thread;
use std::{fs, path::Path};

use async_trait::async_trait;
#[cfg(feature = "rest")]
use axum::{
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header::CONTENT_TYPE},
};
use mempal::aaak::{AaakCodec, AaakDocument};
use mempal::core::types::{
    AnchorKind, BootstrapEvidenceArgs, BootstrapIdentityParts, Drawer, KnowledgeStatus,
    KnowledgeTier, MemoryDomain, MemoryKind, Provenance, SourceType, TriggerHints,
};
use mempal::core::utils::{build_bootstrap_drawer_id_from_parts, build_drawer_id};
use mempal::core::{anchor, db::Database, protocol::MEMORY_PROTOCOL};
use mempal::embed::{Embedder, EmbedderFactory};
use mempal::ingest::{IngestOptions, ingest_file_with_options};
use mempal::mcp::MempalMcpServer;
use rusqlite::Connection;
use serde_json::{Value, json};
use tempfile::TempDir;
#[cfg(feature = "rest")]
use tower::ServiceExt;

fn create_v4_db(path: &std::path::Path) {
    let conn = Connection::open(path).expect("open v4 db");
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
            importance INTEGER DEFAULT 0
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

        PRAGMA user_version = 4;
        "#,
    )
    .expect("apply v4 schema");
}

fn new_db() -> (TempDir, Database) {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("palace.db");
    let db = Database::open(&db_path).expect("open db");
    (tmp, db)
}

struct StubEmbedderFactory {
    vector: Vec<f32>,
}

struct StubEmbedder {
    vector: Vec<f32>,
}

#[async_trait]
impl EmbedderFactory for StubEmbedderFactory {
    async fn build(&self) -> mempal::embed::Result<Box<dyn Embedder>> {
        Ok(Box::new(StubEmbedder {
            vector: self.vector.clone(),
        }))
    }
}

#[async_trait]
impl Embedder for StubEmbedder {
    async fn embed(&self, texts: &[&str]) -> mempal::embed::Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|_| self.vector.clone()).collect())
    }

    fn dimensions(&self) -> usize {
        self.vector.len()
    }

    fn name(&self) -> &str {
        "stub"
    }
}

fn setup_mcp_server() -> (TempDir, Database, MempalMcpServer) {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("palace.db");
    let db = Database::open(&db_path).expect("open db");
    let server = MempalMcpServer::new_with_factory(
        db_path,
        Arc::new(StubEmbedderFactory {
            vector: vec![0.1, 0.2, 0.3],
        }),
    );
    (tmp, db, server)
}

#[cfg(feature = "rest")]
fn setup_rest_mcp_server() -> (TempDir, Database, MempalMcpServer, mempal::api::ApiState) {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("palace.db");
    let db = Database::open(&db_path).expect("open db");
    let factory = Arc::new(StubEmbedderFactory {
        vector: vec![0.1, 0.2, 0.3],
    });
    let server = MempalMcpServer::new_with_factory(db_path.clone(), factory.clone());
    let state = mempal::api::ApiState::new(db_path, factory);
    (tmp, db, server, state)
}

fn expected_bootstrap_evidence_id(
    wing: &str,
    room: Option<&str>,
    content: &str,
    source_type: &SourceType,
) -> String {
    let defaults = anchor::bootstrap_defaults(source_type);
    let memory_kind = MemoryKind::Evidence;
    let domain = MemoryDomain::Project;
    let empty_refs: &[String] = &[];
    build_bootstrap_drawer_id_from_parts(
        wing,
        room,
        content,
        BootstrapIdentityParts {
            memory_kind: &memory_kind,
            domain: &domain,
            field: &defaults.field,
            anchor_kind: &defaults.anchor_kind,
            anchor_id: &defaults.anchor_id,
            parent_anchor_id: defaults.parent_anchor_id.as_deref(),
            provenance: Some(&defaults.provenance),
            statement: None,
            tier: None,
            status: None,
            supporting_refs: empty_refs,
            counterexample_refs: empty_refs,
            teaching_refs: empty_refs,
            verification_refs: empty_refs,
            scope_constraints: None,
            trigger_hints: None,
        },
    )
}

#[cfg(feature = "rest")]
async fn post_rest_ingest(state: mempal::api::ApiState, payload: Value) -> (StatusCode, Value) {
    let response = mempal::api::router(state)
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/ingest")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&payload).expect("serialize rest payload"),
                ))
                .expect("build rest request"),
        )
        .await
        .expect("rest request should complete");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read rest response body");
    let body = serde_json::from_slice(&bytes).expect("parse rest response body");
    (status, body)
}

fn mempal_bin() -> String {
    env!("CARGO_BIN_EXE_mempal").to_string()
}

fn init_git_repo(path: &Path) {
    Command::new("git")
        .arg("init")
        .current_dir(path)
        .output()
        .expect("git init should run");
    fs::write(path.join("README.md"), "seed\n").expect("write seed file");
    Command::new("git")
        .args(["add", "README.md"])
        .current_dir(path)
        .output()
        .expect("git add should run");
    Command::new("git")
        .args([
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "init",
        ])
        .current_dir(path)
        .output()
        .expect("git commit should run");
}

fn setup_cli_home() -> (TempDir, Database) {
    let tmp = TempDir::new().expect("tempdir");
    let mempal_dir = tmp.path().join(".mempal");
    fs::create_dir_all(&mempal_dir).expect("create mempal home");
    let db = Database::open(&mempal_dir.join("palace.db")).expect("open cli db");
    (tmp, db)
}

fn start_openai_embedding_stub(
    expected_query: &str,
    vector: Vec<f32>,
) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind embedding stub");
    listener
        .set_nonblocking(true)
        .expect("set embedding stub nonblocking");
    let address = listener.local_addr().expect("local addr");
    let expected_query = expected_query.to_string();

    let handle = thread::spawn(move || {
        let (mut stream, _) = (0..50)
            .find_map(|_| match listener.accept() {
                Ok(connection) => Some(connection),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(std::time::Duration::from_millis(100));
                    None
                }
                Err(error) => panic!("accept request: {error}"),
            })
            .expect("embedding stub timed out waiting for request");
        let mut request = [0_u8; 4096];
        let bytes_read = stream.read(&mut request).expect("read embedding request");
        assert!(bytes_read > 0, "expected non-empty HTTP request");
        let request = String::from_utf8_lossy(&request[..bytes_read]);
        let (headers, body) = request
            .split_once("\r\n\r\n")
            .expect("request should contain HTTP headers and JSON body");
        let request_line = headers.lines().next().expect("request line");
        assert_eq!(request_line, "POST /v1/embeddings HTTP/1.1");

        let payload: Value = serde_json::from_str(body).expect("parse embedding request body");
        assert_eq!(payload["model"], "test-model");
        let input = payload["input"]
            .as_array()
            .expect("input should be an array");
        assert_eq!(input.len(), 1, "expected a single embedding query");
        assert_eq!(input[0], expected_query);

        let body = serde_json::to_string(&json!({
            "data": [{ "embedding": vector }]
        }))
        .expect("serialize response body");
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .expect("write embedding response");
    });

    (format!("http://{address}/v1/embeddings"), handle)
}

fn vector_of(dimensions: usize, value: f32) -> Vec<f32> {
    vec![value; dimensions]
}

fn write_cli_api_config(home: &Path, endpoint: &str) {
    let config_path = home.join(".mempal").join("config.toml");
    fs::write(
        &config_path,
        format!(
            "[embed]\nbackend = \"api\"\napi_endpoint = \"{endpoint}\"\napi_model = \"test-model\"\n"
        ),
    )
    .expect("write cli config");
    let config = mempal::core::config::Config::load_from(&config_path).expect("load cli config");
    assert_eq!(config.embed.backend, "api");
}

fn run_cli_search_json(home: &Path, query: &str, extra_args: &[&str]) -> Vec<Value> {
    let (endpoint, server_handle) = start_openai_embedding_stub(query, vector_of(384, 0.25));
    write_cli_api_config(home, &endpoint);

    let mut args = vec![
        "search".to_string(),
        query.to_string(),
        "--wing".to_string(),
        "mempal".to_string(),
        "--room".to_string(),
        "bootstrap".to_string(),
    ];
    args.extend(extra_args.iter().map(|arg| (*arg).to_string()));
    args.extend(["--top-k".to_string(), "5".to_string(), "--json".to_string()]);

    let output = Command::new(mempal_bin())
        .args(&args)
        .env("HOME", home)
        .output()
        .expect("run mempal search");

    assert!(
        output.status.success(),
        "search command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    server_handle.join().expect("join embedding stub");

    serde_json::from_slice(&output.stdout).expect("parse cli search json")
}

fn run_cli_wake_up(home: &Path, format: Option<&str>) -> String {
    let mut command = Command::new(mempal_bin());
    command.arg("wake-up").env("HOME", home);
    if let Some(format) = format {
        command.args(["--format", format]);
    }
    let output = command.output().expect("run mempal wake-up");
    assert!(
        output.status.success(),
        "wake-up command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("wake-up stdout should be utf8")
}

fn assert_cli_filter_selects_only(
    query: &str,
    extra_args: &[&str],
    target: &Drawer,
    distractor: &Drawer,
) {
    let (tmp, db) = setup_cli_home();
    insert_search_fixture(&db, target, &vector_of(384, 0.25));
    insert_search_fixture(&db, distractor, &vector_of(384, 0.25));

    let results = run_cli_search_json(tmp.path(), query, extra_args);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["drawer_id"], target.id);
}

fn bootstrap_drawer(
    id: &str,
    content: &str,
    memory_kind: MemoryKind,
    tier: Option<KnowledgeTier>,
    status: Option<KnowledgeStatus>,
    statement: Option<&str>,
) -> Drawer {
    Drawer {
        id: id.to_string(),
        content: content.to_string(),
        wing: "mempal".to_string(),
        room: Some("bootstrap".to_string()),
        source_file: Some(match memory_kind {
            MemoryKind::Evidence => format!("tests://{id}"),
            MemoryKind::Knowledge => format!("knowledge://project/bootstrap/{id}"),
        }),
        source_type: SourceType::Manual,
        added_at: "1710009999".to_string(),
        chunk_index: Some(0),
        normalize_version: 1,
        importance: 2,
        memory_kind: memory_kind.clone(),
        domain: MemoryDomain::Project,
        field: anchor::DEFAULT_FIELD.to_string(),
        anchor_kind: AnchorKind::Repo,
        anchor_id: format!("repo://{id}"),
        parent_anchor_id: None,
        provenance: match memory_kind {
            MemoryKind::Evidence => Some(Provenance::Human),
            MemoryKind::Knowledge => None,
        },
        statement: statement.map(ToOwned::to_owned),
        tier,
        status,
        supporting_refs: if matches!(memory_kind, MemoryKind::Knowledge) {
            vec!["drawer_ev_search_source".to_string()]
        } else {
            Vec::new()
        },
        counterexample_refs: Vec::new(),
        teaching_refs: Vec::new(),
        verification_refs: Vec::new(),
        scope_constraints: None,
        trigger_hints: None,
    }
}

fn insert_search_fixture(db: &Database, drawer: &Drawer, vector: &[f32]) {
    db.insert_drawer(drawer).expect("insert search drawer");
    db.insert_vector(&drawer.id, vector)
        .expect("insert search vector");
}

fn insert_cli_wake_up_drawer(db: &Database, drawer: &Drawer) {
    db.insert_drawer(drawer).expect("insert wake-up drawer");
}

#[test]
fn test_migration_backfills_legacy_drawers_with_bootstrap_defaults() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("palace.db");
    create_v4_db(&db_path);

    {
        let conn = Connection::open(&db_path).expect("reopen v4 db");
        conn.execute(
            r#"
            INSERT INTO drawers (
                id,
                content,
                wing,
                room,
                source_file,
                source_type,
                added_at,
                chunk_index,
                importance
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
            (
                "drawer_legacy_001",
                "Legacy evidence body",
                "mempal",
                Some("bootstrap"),
                Some("docs/specs/legacy.md"),
                "project",
                "1710000000",
                Some(0_i64),
                4_i32,
            ),
        )
        .expect("insert legacy drawer");
        conn.execute(
            r#"
            INSERT INTO drawers (
                id,
                content,
                wing,
                room,
                source_file,
                source_type,
                added_at,
                chunk_index,
                importance
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
            (
                "drawer_legacy_002",
                "Legacy conversation note",
                "mempal",
                Some("bootstrap"),
                Some("session://legacy"),
                "conversation",
                "1710000001",
                Some(1_i64),
                1_i32,
            ),
        )
        .expect("insert legacy conversation drawer");
        conn.execute(
            r#"
            INSERT INTO drawers (
                id,
                content,
                wing,
                room,
                source_file,
                source_type,
                added_at,
                chunk_index,
                importance
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
            (
                "drawer_legacy_003",
                "Legacy manual note",
                "mempal",
                Some("bootstrap"),
                Some("manual://legacy"),
                "manual",
                "1710000002",
                Some(2_i64),
                2_i32,
            ),
        )
        .expect("insert legacy manual drawer");
    }

    let db = Database::open(&db_path).expect("migrate db to latest");
    assert_eq!(db.schema_version().expect("schema version"), 8);

    let drawer = db
        .get_drawer("drawer_legacy_001")
        .expect("load drawer")
        .expect("drawer exists");

    assert_eq!(drawer.memory_kind, MemoryKind::Evidence);
    assert_eq!(drawer.domain, MemoryDomain::Project);
    assert_eq!(drawer.field, "general");
    assert_eq!(drawer.anchor_kind, AnchorKind::Repo);
    assert_eq!(drawer.anchor_id, "repo://legacy");
    assert_eq!(drawer.parent_anchor_id, None);
    assert_eq!(drawer.provenance, Some(Provenance::Research));
    assert_eq!(drawer.statement, None);
    assert_eq!(drawer.tier, None);
    assert_eq!(drawer.status, None);
    assert!(drawer.supporting_refs.is_empty());
    assert!(drawer.counterexample_refs.is_empty());
    assert!(drawer.teaching_refs.is_empty());
    assert!(drawer.verification_refs.is_empty());
    assert_eq!(drawer.scope_constraints, None);
    assert_eq!(drawer.trigger_hints, None);

    let conversation_drawer = db
        .get_drawer("drawer_legacy_002")
        .expect("load conversation drawer")
        .expect("conversation drawer exists");
    assert_eq!(conversation_drawer.memory_kind, MemoryKind::Evidence);
    assert_eq!(conversation_drawer.provenance, Some(Provenance::Human));

    let manual_drawer = db
        .get_drawer("drawer_legacy_003")
        .expect("load manual drawer")
        .expect("manual drawer exists");
    assert_eq!(manual_drawer.memory_kind, MemoryKind::Evidence);
    assert_eq!(manual_drawer.provenance, Some(Provenance::Human));
}

#[tokio::test]
async fn test_global_anchor_rejected_for_non_global_domain() {
    let (_tmp, _db, server) = setup_mcp_server();
    let error = server
        .ingest_json_for_test(json!({
            "content": "repo-local note",
            "wing": "mempal",
            "memory_kind": "evidence",
            "domain": "project",
            "anchor_kind": "global",
            "anchor_id": "global://all"
        }))
        .await
        .expect_err("global anchor should reject non-global domain");
    let message = error.to_string();
    assert!(
        message.contains("global") && message.contains("domain"),
        "unexpected error: {message}"
    );
}

#[test]
fn test_insert_load_roundtrip_preserves_json_metadata_and_read_paths() {
    let (_tmp, db) = new_db();
    let drawer = Drawer {
        id: "drawer_knowledge_roundtrip".to_string(),
        content: "Detailed rationale body".to_string(),
        wing: "mempal".to_string(),
        room: Some("bootstrap".to_string()),
        source_file: Some("knowledge://project/bootstrap/typed-drawer".to_string()),
        source_type: SourceType::Manual,
        added_at: "1710002000".to_string(),
        chunk_index: Some(0),
        normalize_version: 1,
        importance: 3,
        memory_kind: MemoryKind::Knowledge,
        domain: MemoryDomain::Project,
        field: anchor::DEFAULT_FIELD.to_string(),
        anchor_kind: AnchorKind::Repo,
        anchor_id: anchor::LEGACY_REPO_ANCHOR_ID.to_string(),
        parent_anchor_id: None,
        provenance: Some(Provenance::Human),
        statement: Some("Typed drawers persist structured metadata.".to_string()),
        tier: Some(KnowledgeTier::Shu),
        status: Some(KnowledgeStatus::Promoted),
        supporting_refs: vec!["drawer_ev_001".to_string(), "drawer_ev_002".to_string()],
        counterexample_refs: vec!["drawer_cex_001".to_string()],
        teaching_refs: Vec::new(),
        verification_refs: vec!["drawer_verify_001".to_string()],
        scope_constraints: Some("Task 1 only".to_string()),
        trigger_hints: Some(TriggerHints {
            intent_tags: vec!["schema".to_string(), "bootstrap".to_string()],
            workflow_bias: vec!["tdd".to_string()],
            tool_needs: vec!["cargo-check".to_string()],
        }),
    };

    db.insert_drawer(&drawer).expect("insert drawer");

    let loaded = db
        .get_drawer(&drawer.id)
        .expect("get drawer")
        .expect("drawer exists");
    assert_eq!(loaded.supporting_refs, drawer.supporting_refs);
    assert_eq!(loaded.counterexample_refs, drawer.counterexample_refs);
    assert_eq!(loaded.trigger_hints, drawer.trigger_hints);

    let top = db.top_drawers(5).expect("top drawers");
    let top_loaded = top
        .into_iter()
        .find(|candidate| candidate.id == drawer.id)
        .expect("drawer present in top_drawers");
    assert_eq!(top_loaded.supporting_refs, drawer.supporting_refs);
    assert_eq!(top_loaded.counterexample_refs, drawer.counterexample_refs);
    assert_eq!(top_loaded.trigger_hints, drawer.trigger_hints);
}

#[test]
fn test_read_path_rejects_non_array_or_non_string_list_payloads() {
    let (_tmp, db) = new_db();
    db.conn()
        .execute(
            r#"
            INSERT INTO drawers (
                id, content, wing, room, source_file, source_type, added_at, chunk_index, importance,
                memory_kind, domain, field, anchor_kind, anchor_id, parent_anchor_id, provenance,
                statement, tier, status, supporting_refs, counterexample_refs, teaching_refs,
                verification_refs, scope_constraints, trigger_hints
            )
            VALUES (?1, ?2, ?3, NULL, NULL, ?4, ?5, NULL, 0, ?6, ?7, ?8, ?9, ?10, NULL, ?11,
                    NULL, NULL, NULL, ?12, '[]', '[]', '[]', NULL, NULL)
            "#,
            (
                "drawer_bad_json",
                "bad payload",
                "mempal",
                "manual",
                "1710003000",
                "evidence",
                "project",
                anchor::DEFAULT_FIELD,
                "repo",
                anchor::LEGACY_REPO_ANCHOR_ID,
                "human",
                r#"["ok", 42]"#,
            ),
        )
        .expect("insert malformed drawer");

    let error = db
        .get_drawer("drawer_bad_json")
        .expect_err("malformed list payload should fail");
    let message = error.to_string();
    assert!(
        message.contains("JSON") || message.contains("list"),
        "unexpected error: {message}"
    );
}

#[tokio::test]
async fn test_mcp_ingest_defaults_to_evidence_drawer_bootstrap_metadata() {
    let (_tmp, db, server) = setup_mcp_server();
    let response = server
        .ingest_json_for_test(json!({
            "content": "Bootstrap evidence body",
            "wing": "mempal",
            "room": "bootstrap",
            "source": "notes://bootstrap",
            "importance": 2
        }))
        .await
        .expect("ingest should succeed");

    let drawer = db
        .get_drawer(&response.drawer_id)
        .expect("load drawer")
        .expect("drawer exists");

    assert_eq!(drawer.memory_kind, MemoryKind::Evidence);
    assert_eq!(drawer.domain, MemoryDomain::Project);
    assert_eq!(drawer.field, "general");
    assert_eq!(drawer.provenance, Some(Provenance::Human));
    assert_eq!(drawer.statement, None);
    assert_eq!(drawer.tier, None);
    assert_eq!(drawer.status, None);
}

#[tokio::test]
async fn test_mcp_ingest_default_drawer_id_matches_bootstrap_identity() {
    let (_tmp, _db, server) = setup_mcp_server();
    let content = "Default MCP identity body";
    let response = server
        .ingest_json_for_test(json!({
            "content": content,
            "wing": "mempal",
            "room": "identity"
        }))
        .await
        .expect("default ingest should succeed");

    let expected =
        expected_bootstrap_evidence_id("mempal", Some("identity"), content, &SourceType::Manual);

    assert_eq!(response.drawer_id, expected);
    assert_ne!(
        response.drawer_id,
        build_drawer_id("mempal", Some("identity"), content)
    );
}

#[cfg(feature = "rest")]
#[tokio::test]
async fn test_rest_ingest_default_evidence_drawer_id_matches_mcp() {
    let (_tmp, _db, server, state) = setup_rest_mcp_server();
    let content = "REST default identity body";
    let (status, body) = post_rest_ingest(
        state,
        json!({
            "content": content,
            "wing": "mempal",
            "room": "identity"
        }),
    )
    .await;
    let mcp = server
        .ingest_json_for_test(json!({
            "content": content,
            "wing": "mempal",
            "room": "identity",
            "dry_run": true
        }))
        .await
        .expect("mcp dry-run should succeed");

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["drawer_id"], mcp.drawer_id);
    assert_ne!(
        body["drawer_id"],
        build_drawer_id("mempal", Some("identity"), content)
    );
}

#[cfg(feature = "rest")]
#[tokio::test]
async fn test_rest_after_mcp_default_ingest_reuses_existing_bootstrap_drawer() {
    let (_tmp, db, server, state) = setup_rest_mcp_server();
    let content = "Shared default identity body";
    let mcp = server
        .ingest_json_for_test(json!({
            "content": content,
            "wing": "mempal",
            "room": "identity"
        }))
        .await
        .expect("mcp write should succeed");

    let (status, body) = post_rest_ingest(
        state,
        json!({
            "content": content,
            "wing": "mempal",
            "room": "identity"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["drawer_id"], mcp.drawer_id);
    assert_eq!(db.drawer_count().expect("drawer count"), 1);
}

#[cfg(feature = "rest")]
#[tokio::test]
async fn test_rest_ingest_does_not_claim_typed_field_parity() {
    let (_tmp, db, _server, state) = setup_rest_mcp_server();
    let content = "REST typed fields remain ignored";
    let (status, body) = post_rest_ingest(
        state,
        json!({
            "content": content,
            "wing": "mempal",
            "room": "identity",
            "memory_kind": "knowledge",
            "statement": "REST should not accept typed knowledge yet.",
            "tier": "shu",
            "status": "promoted",
            "supporting_refs": ["drawer_ev_001"]
        }),
    )
    .await;
    let expected =
        expected_bootstrap_evidence_id("mempal", Some("identity"), content, &SourceType::Manual);

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["drawer_id"], expected);
    let drawer = db
        .get_drawer(&expected)
        .expect("load rest drawer")
        .expect("rest drawer exists");
    assert_eq!(drawer.memory_kind, MemoryKind::Evidence);
    assert_eq!(drawer.statement, None);
    assert_eq!(drawer.tier, None);
    assert_eq!(drawer.status, None);
}

#[tokio::test]
async fn test_file_ingest_uses_bootstrap_identity_for_evidence_drawer() {
    let (tmp, db) = new_db();
    let file = tmp.path().join("identity-note.md");
    let content = "File ingest should use bootstrap evidence identity.";
    fs::write(&file, content).expect("write ingest file");
    let embedder = StubEmbedder {
        vector: vec![0.1, 0.2, 0.3],
    };

    let stats = ingest_file_with_options(
        &db,
        &embedder,
        &file,
        "mempal",
        IngestOptions {
            room: Some("identity"),
            source_root: file.parent(),
            dry_run: false,
            source_file_override: None,
            replace_existing_source: false,
            no_strip_noise: false,
            ..IngestOptions::default()
        },
    )
    .await
    .expect("file ingest should succeed");
    let expected =
        expected_bootstrap_evidence_id("mempal", Some("identity"), content, &SourceType::Project);

    assert_eq!(stats.chunks, 1);
    assert_ne!(
        expected,
        build_drawer_id("mempal", Some("identity"), content)
    );
    assert!(
        db.get_drawer(&expected)
            .expect("load file drawer")
            .is_some()
    );
}

#[tokio::test]
async fn test_bootstrap_identity_separates_same_content_with_different_anchors() {
    let (_tmp, db, server) = setup_mcp_server();
    let content = "Same content must remain anchor-local.";
    let first = server
        .ingest_json_for_test(json!({
            "content": content,
            "wing": "mempal",
            "room": "identity",
            "memory_kind": "evidence",
            "anchor_kind": "repo",
            "anchor_id": "repo://anchor-one"
        }))
        .await
        .expect("first anchored evidence should succeed");
    let second = server
        .ingest_json_for_test(json!({
            "content": content,
            "wing": "mempal",
            "room": "identity",
            "memory_kind": "evidence",
            "anchor_kind": "repo",
            "anchor_id": "repo://anchor-two"
        }))
        .await
        .expect("second anchored evidence should succeed");

    assert_ne!(first.drawer_id, second.drawer_id);
    assert_eq!(db.drawer_count().expect("drawer count"), 2);
    let first_drawer = db
        .get_drawer(&first.drawer_id)
        .expect("load first anchored drawer")
        .expect("first anchored drawer exists");
    let second_drawer = db
        .get_drawer(&second.drawer_id)
        .expect("load second anchored drawer")
        .expect("second anchored drawer exists");
    assert_eq!(first_drawer.content, content);
    assert_eq!(second_drawer.content, content);
    assert_eq!(first_drawer.anchor_id, "repo://anchor-one");
    assert_eq!(second_drawer.anchor_id, "repo://anchor-two");
}

#[test]
fn test_p13b_does_not_rewrite_existing_drawer_ids() {
    let (_tmp, db) = new_db();
    let db_path = db.path().to_path_buf();
    let content = "Legacy drawer id must survive P13B unchanged.";
    let old_id = build_drawer_id("mempal", Some("identity"), content);
    let new_id =
        expected_bootstrap_evidence_id("mempal", Some("identity"), content, &SourceType::Manual);
    assert_ne!(old_id, new_id);

    let drawer = Drawer::new_bootstrap_evidence(BootstrapEvidenceArgs {
        id: old_id.clone(),
        content: content.to_string(),
        wing: "mempal".to_string(),
        room: Some("identity".to_string()),
        source_file: Some("legacy://p13b".to_string()),
        source_type: SourceType::Manual,
        added_at: "1710004000".to_string(),
        chunk_index: Some(0),
        importance: 0,
    });
    db.insert_drawer(&drawer).expect("insert legacy-id drawer");
    drop(db);

    let reopened = Database::open(&db_path).expect("reopen db");
    assert!(
        reopened
            .get_drawer(&old_id)
            .expect("load old id drawer")
            .is_some()
    );
    assert!(
        reopened
            .get_drawer(&new_id)
            .expect("probe new id drawer")
            .is_none()
    );
}

#[test]
fn test_knowledge_bootstrap_identity_changes_when_governance_component_changes() {
    let content = "Governed identity body";
    let supporting_refs = vec!["drawer_ev_001".to_string(), "drawer_ev_002".to_string()];
    let counterexample_refs = vec!["drawer_cex_001".to_string()];
    let teaching_refs = vec!["drawer_teach_001".to_string()];
    let verification_refs = vec!["drawer_verify_001".to_string()];
    let trigger_hints = TriggerHints {
        intent_tags: vec!["debug".to_string(), "identity".to_string()],
        workflow_bias: vec!["tdd".to_string()],
        tool_needs: vec!["cargo-test".to_string()],
    };

    let build = |memory_kind: &MemoryKind,
                 domain: &MemoryDomain,
                 field: &str,
                 anchor_kind: &AnchorKind,
                 anchor_id: &str,
                 parent_anchor_id: Option<&str>,
                 statement: Option<&str>,
                 tier: Option<&KnowledgeTier>,
                 status: Option<&KnowledgeStatus>,
                 supporting_refs: &[String],
                 counterexample_refs: &[String],
                 teaching_refs: &[String],
                 verification_refs: &[String],
                 scope_constraints: Option<&str>,
                 trigger_hints: Option<&TriggerHints>| {
        build_bootstrap_drawer_id_from_parts(
            "mempal",
            Some("identity"),
            content,
            BootstrapIdentityParts {
                memory_kind,
                domain,
                field,
                anchor_kind,
                anchor_id,
                parent_anchor_id,
                provenance: None,
                statement,
                tier,
                status,
                supporting_refs,
                counterexample_refs,
                teaching_refs,
                verification_refs,
                scope_constraints,
                trigger_hints,
            },
        )
    };

    let base_kind = MemoryKind::Knowledge;
    let evidence_kind = MemoryKind::Evidence;
    let base_domain = MemoryDomain::Skill;
    let global_domain = MemoryDomain::Global;
    let base_anchor_kind = AnchorKind::Repo;
    let worktree_anchor_kind = AnchorKind::Worktree;
    let base_tier = KnowledgeTier::Shu;
    let dao_tier = KnowledgeTier::DaoRen;
    let base_status = KnowledgeStatus::Promoted;
    let candidate_status = KnowledgeStatus::Candidate;
    let base = build(
        &base_kind,
        &base_domain,
        "debugging",
        &base_anchor_kind,
        "repo://identity",
        Some("repo://parent"),
        Some("Debug by reproducing before patching."),
        Some(&base_tier),
        Some(&base_status),
        &supporting_refs,
        &counterexample_refs,
        &teaching_refs,
        &verification_refs,
        Some("Rust code only"),
        Some(&trigger_hints),
    );

    let empty_refs: Vec<String> = Vec::new();
    let changed_trigger_hints = TriggerHints {
        intent_tags: vec!["debug".to_string(), "different".to_string()],
        workflow_bias: vec!["tdd".to_string()],
        tool_needs: vec!["cargo-test".to_string()],
    };
    let variants = [
        (
            "memory_kind",
            build(
                &evidence_kind,
                &base_domain,
                "debugging",
                &base_anchor_kind,
                "repo://identity",
                Some("repo://parent"),
                Some("Debug by reproducing before patching."),
                Some(&base_tier),
                Some(&base_status),
                &supporting_refs,
                &counterexample_refs,
                &teaching_refs,
                &verification_refs,
                Some("Rust code only"),
                Some(&trigger_hints),
            ),
        ),
        (
            "domain",
            build(
                &base_kind,
                &global_domain,
                "debugging",
                &base_anchor_kind,
                "repo://identity",
                Some("repo://parent"),
                Some("Debug by reproducing before patching."),
                Some(&base_tier),
                Some(&base_status),
                &supporting_refs,
                &counterexample_refs,
                &teaching_refs,
                &verification_refs,
                Some("Rust code only"),
                Some(&trigger_hints),
            ),
        ),
        (
            "field",
            build(
                &base_kind,
                &base_domain,
                "testing",
                &base_anchor_kind,
                "repo://identity",
                Some("repo://parent"),
                Some("Debug by reproducing before patching."),
                Some(&base_tier),
                Some(&base_status),
                &supporting_refs,
                &counterexample_refs,
                &teaching_refs,
                &verification_refs,
                Some("Rust code only"),
                Some(&trigger_hints),
            ),
        ),
        (
            "anchor_kind",
            build(
                &base_kind,
                &base_domain,
                "debugging",
                &worktree_anchor_kind,
                "repo://identity",
                Some("repo://parent"),
                Some("Debug by reproducing before patching."),
                Some(&base_tier),
                Some(&base_status),
                &supporting_refs,
                &counterexample_refs,
                &teaching_refs,
                &verification_refs,
                Some("Rust code only"),
                Some(&trigger_hints),
            ),
        ),
        (
            "anchor_id",
            build(
                &base_kind,
                &base_domain,
                "debugging",
                &base_anchor_kind,
                "repo://other",
                Some("repo://parent"),
                Some("Debug by reproducing before patching."),
                Some(&base_tier),
                Some(&base_status),
                &supporting_refs,
                &counterexample_refs,
                &teaching_refs,
                &verification_refs,
                Some("Rust code only"),
                Some(&trigger_hints),
            ),
        ),
        (
            "parent_anchor_id",
            build(
                &base_kind,
                &base_domain,
                "debugging",
                &base_anchor_kind,
                "repo://identity",
                Some("repo://other-parent"),
                Some("Debug by reproducing before patching."),
                Some(&base_tier),
                Some(&base_status),
                &supporting_refs,
                &counterexample_refs,
                &teaching_refs,
                &verification_refs,
                Some("Rust code only"),
                Some(&trigger_hints),
            ),
        ),
        (
            "statement",
            build(
                &base_kind,
                &base_domain,
                "debugging",
                &base_anchor_kind,
                "repo://identity",
                Some("repo://parent"),
                Some("Patch only after a concrete reproduction."),
                Some(&base_tier),
                Some(&base_status),
                &supporting_refs,
                &counterexample_refs,
                &teaching_refs,
                &verification_refs,
                Some("Rust code only"),
                Some(&trigger_hints),
            ),
        ),
        (
            "tier",
            build(
                &base_kind,
                &base_domain,
                "debugging",
                &base_anchor_kind,
                "repo://identity",
                Some("repo://parent"),
                Some("Debug by reproducing before patching."),
                Some(&dao_tier),
                Some(&base_status),
                &supporting_refs,
                &counterexample_refs,
                &teaching_refs,
                &verification_refs,
                Some("Rust code only"),
                Some(&trigger_hints),
            ),
        ),
        (
            "status",
            build(
                &base_kind,
                &base_domain,
                "debugging",
                &base_anchor_kind,
                "repo://identity",
                Some("repo://parent"),
                Some("Debug by reproducing before patching."),
                Some(&base_tier),
                Some(&candidate_status),
                &supporting_refs,
                &counterexample_refs,
                &teaching_refs,
                &verification_refs,
                Some("Rust code only"),
                Some(&trigger_hints),
            ),
        ),
        (
            "supporting_refs",
            build(
                &base_kind,
                &base_domain,
                "debugging",
                &base_anchor_kind,
                "repo://identity",
                Some("repo://parent"),
                Some("Debug by reproducing before patching."),
                Some(&base_tier),
                Some(&base_status),
                &empty_refs,
                &counterexample_refs,
                &teaching_refs,
                &verification_refs,
                Some("Rust code only"),
                Some(&trigger_hints),
            ),
        ),
        (
            "counterexample_refs",
            build(
                &base_kind,
                &base_domain,
                "debugging",
                &base_anchor_kind,
                "repo://identity",
                Some("repo://parent"),
                Some("Debug by reproducing before patching."),
                Some(&base_tier),
                Some(&base_status),
                &supporting_refs,
                &empty_refs,
                &teaching_refs,
                &verification_refs,
                Some("Rust code only"),
                Some(&trigger_hints),
            ),
        ),
        (
            "teaching_refs",
            build(
                &base_kind,
                &base_domain,
                "debugging",
                &base_anchor_kind,
                "repo://identity",
                Some("repo://parent"),
                Some("Debug by reproducing before patching."),
                Some(&base_tier),
                Some(&base_status),
                &supporting_refs,
                &counterexample_refs,
                &empty_refs,
                &verification_refs,
                Some("Rust code only"),
                Some(&trigger_hints),
            ),
        ),
        (
            "verification_refs",
            build(
                &base_kind,
                &base_domain,
                "debugging",
                &base_anchor_kind,
                "repo://identity",
                Some("repo://parent"),
                Some("Debug by reproducing before patching."),
                Some(&base_tier),
                Some(&base_status),
                &supporting_refs,
                &counterexample_refs,
                &teaching_refs,
                &empty_refs,
                Some("Rust code only"),
                Some(&trigger_hints),
            ),
        ),
        (
            "scope_constraints",
            build(
                &base_kind,
                &base_domain,
                "debugging",
                &base_anchor_kind,
                "repo://identity",
                Some("repo://parent"),
                Some("Debug by reproducing before patching."),
                Some(&base_tier),
                Some(&base_status),
                &supporting_refs,
                &counterexample_refs,
                &teaching_refs,
                &verification_refs,
                Some("Python code only"),
                Some(&trigger_hints),
            ),
        ),
        (
            "trigger_hints",
            build(
                &base_kind,
                &base_domain,
                "debugging",
                &base_anchor_kind,
                "repo://identity",
                Some("repo://parent"),
                Some("Debug by reproducing before patching."),
                Some(&base_tier),
                Some(&base_status),
                &supporting_refs,
                &counterexample_refs,
                &teaching_refs,
                &verification_refs,
                Some("Rust code only"),
                Some(&changed_trigger_hints),
            ),
        ),
    ];

    for (component, variant) in variants {
        assert_ne!(base, variant, "{component} must participate in identity");
    }
}

#[tokio::test]
async fn test_knowledge_drawer_keeps_statement_separate_from_content() {
    let (_tmp, db, server) = setup_mcp_server();
    let statement = "Debug by reproducing before patching.";
    let content = "Start from a concrete reproduction, then isolate scope before patching.";

    let response = server
        .ingest_json_for_test(json!({
            "content": content,
            "wing": "mempal",
            "memory_kind": "knowledge",
            "domain": "skill",
            "field": "debugging",
            "statement": statement,
            "tier": "shu",
            "status": "promoted",
            "supporting_refs": ["drawer_ev_001"]
        }))
        .await
        .expect("knowledge ingest should succeed");

    let drawer = db
        .get_drawer(&response.drawer_id)
        .expect("load knowledge drawer")
        .expect("knowledge drawer exists");

    assert_eq!(drawer.memory_kind, MemoryKind::Knowledge);
    assert_eq!(drawer.domain, MemoryDomain::Skill);
    assert_eq!(drawer.field, "debugging");
    assert_eq!(drawer.statement.as_deref(), Some(statement));
    assert_eq!(drawer.content, content);
    assert_ne!(drawer.statement.as_deref(), Some(drawer.content.as_str()));
    assert_eq!(drawer.tier, Some(KnowledgeTier::Shu));
    assert_eq!(drawer.status, Some(KnowledgeStatus::Promoted));
    assert_eq!(drawer.supporting_refs, vec!["drawer_ev_001"]);
}

#[tokio::test]
async fn test_evidence_drawer_rejects_knowledge_only_fields() {
    let (_tmp, _db, server) = setup_mcp_server();
    let error = server
        .ingest_json_for_test(json!({
            "content": "Evidence should not carry knowledge governance metadata",
            "wing": "mempal",
            "memory_kind": "evidence",
            "tier": "qi"
        }))
        .await
        .expect_err("knowledge-only fields should be rejected");
    let message = error.to_string();

    assert!(
        message.contains("evidence") && message.contains("knowledge-only"),
        "unexpected error: {message}"
    );
}

#[tokio::test]
async fn test_knowledge_drawer_requires_statement_and_supporting_refs() {
    let (_tmp, _db, server) = setup_mcp_server();
    let error = server
        .ingest_json_for_test(json!({
            "content": "Knowledge body without the bootstrap metadata contract",
            "wing": "mempal",
            "memory_kind": "knowledge",
            "domain": "skill",
            "field": "debugging",
            "tier": "shu",
            "status": "promoted"
        }))
        .await
        .expect_err("knowledge drawers should require statement and supporting refs");
    let message = error.to_string();

    assert!(
        message.contains("knowledge") && message.contains("statement"),
        "unexpected error: {message}"
    );
    assert!(
        message.contains("supporting_refs"),
        "unexpected error: {message}"
    );
}

#[tokio::test]
async fn test_knowledge_drawer_rejects_non_drawer_supporting_refs() {
    let (_tmp, _db, server) = setup_mcp_server();
    let error = server
        .ingest_json_for_test(json!({
            "content": "Knowledge body with malformed refs",
            "wing": "mempal",
            "memory_kind": "knowledge",
            "statement": "Debug by reproducing before patching.",
            "tier": "shu",
            "status": "promoted",
            "supporting_refs": ["not-a-drawer-id"]
        }))
        .await
        .expect_err("knowledge drawers should reject malformed supporting refs");
    let message = error.to_string();

    assert!(
        message.contains("supporting_refs") && message.contains("drawer"),
        "unexpected error: {message}"
    );
}

#[tokio::test]
async fn test_knowledge_drawer_rejects_non_drawer_other_ref_lists() {
    let (_tmp, _db, server) = setup_mcp_server();
    for field in ["counterexample_refs", "teaching_refs", "verification_refs"] {
        let error = server
            .ingest_json_for_test(json!({
                "content": format!("Knowledge body with malformed {field}"),
                "wing": "mempal",
                "memory_kind": "knowledge",
                "statement": "Debug by reproducing before patching.",
                "tier": "shu",
                "status": "promoted",
                "supporting_refs": ["drawer_ev_001"],
                field: ["not-a-drawer-id"]
            }))
            .await
            .expect_err("knowledge drawers should reject malformed ref lists");
        let message = error.to_string();

        assert!(
            message.contains(field) && message.contains("drawer"),
            "unexpected error for {field}: {message}"
        );
    }
}

#[tokio::test]
async fn test_dao_tian_rejects_noncanonical_status() {
    let (_tmp, _db, server) = setup_mcp_server();
    let error = server
        .ingest_json_for_test(json!({
            "content": "Canonical epistemic policy",
            "wing": "mempal",
            "memory_kind": "knowledge",
            "statement": "Evidence precedes assertion.",
            "tier": "dao_tian",
            "status": "candidate",
            "supporting_refs": ["drawer_ev_001"]
        }))
        .await
        .expect_err("dao_tian candidate should be rejected");
    let message = error.to_string();

    assert!(
        message.contains("dao_tian")
            && message.contains("canonical")
            && message.contains("demoted"),
        "unexpected error: {message}"
    );
}

#[tokio::test]
async fn test_mcp_ingest_same_content_different_anchors_stays_distinct() {
    let (_tmp, db, server) = setup_mcp_server();
    let first = server
        .ingest_json_for_test(json!({
            "content": "Anchor-local memory body",
            "wing": "mempal",
            "memory_kind": "evidence",
            "anchor_kind": "repo",
            "anchor_id": "repo://anchor-a"
        }))
        .await
        .expect("first ingest should succeed");
    let second = server
        .ingest_json_for_test(json!({
            "content": "Anchor-local memory body",
            "wing": "mempal",
            "memory_kind": "knowledge",
            "statement": "Anchor-local memory body.",
            "tier": "shu",
            "status": "promoted",
            "supporting_refs": ["drawer_ev_001"],
            "anchor_kind": "repo",
            "anchor_id": "repo://anchor-b"
        }))
        .await
        .expect("second ingest should succeed");

    assert_ne!(first.drawer_id, second.drawer_id);
    let first_drawer = db
        .get_drawer(&first.drawer_id)
        .expect("load first drawer")
        .expect("first drawer exists");
    let second_drawer = db
        .get_drawer(&second.drawer_id)
        .expect("load second drawer")
        .expect("second drawer exists");
    assert_ne!(first_drawer.anchor_id, second_drawer.anchor_id);
    assert_ne!(first_drawer.memory_kind, second_drawer.memory_kind);
}

#[tokio::test]
async fn test_mcp_ingest_rejects_malformed_explicit_anchor() {
    let (_tmp, _db, server) = setup_mcp_server();
    let error = server
        .ingest_json_for_test(json!({
            "content": "Malformed explicit anchor",
            "wing": "mempal",
            "anchor_kind": "worktree",
            "anchor_id": "/tmp/repo"
        }))
        .await
        .expect_err("malformed explicit anchor should fail");
    let message = error.to_string();

    assert!(
        message.contains("invalid explicit anchor") && message.contains("worktree://"),
        "unexpected error: {message}"
    );
}

#[tokio::test]
async fn test_evidence_drawer_accepts_explicit_runtime_or_research_provenance() {
    let (_tmp, db, server) = setup_mcp_server();
    let runtime = server
        .ingest_json_for_test(json!({
            "content": "Runtime evidence body",
            "wing": "mempal",
            "memory_kind": "evidence",
            "provenance": "runtime",
            "anchor_kind": "repo",
            "anchor_id": "repo://runtime"
        }))
        .await
        .expect("runtime provenance evidence should succeed");
    let research = server
        .ingest_json_for_test(json!({
            "content": "Research evidence body",
            "wing": "mempal",
            "memory_kind": "evidence",
            "provenance": "research",
            "anchor_kind": "repo",
            "anchor_id": "repo://research"
        }))
        .await
        .expect("research provenance evidence should succeed");

    assert_eq!(
        db.get_drawer(&runtime.drawer_id)
            .expect("load runtime drawer")
            .expect("runtime drawer exists")
            .provenance,
        Some(Provenance::Runtime)
    );
    assert_eq!(
        db.get_drawer(&research.drawer_id)
            .expect("load research drawer")
            .expect("research drawer exists")
            .provenance,
        Some(Provenance::Research)
    );
}

#[tokio::test]
async fn test_git_worktree_derives_worktree_anchor_and_repo_parent() {
    let (tmp, db, server) = setup_mcp_server();
    let repo_root = tmp.path().join("repo");
    fs::create_dir_all(&repo_root).expect("create git root");
    init_git_repo(&repo_root);
    let worktree = tmp.path().join("repo-worktree");
    let output = Command::new("git")
        .args([
            "worktree",
            "add",
            "-b",
            "mind-model-bootstrap",
            worktree.to_str().expect("utf8 worktree path"),
        ])
        .current_dir(&repo_root)
        .output()
        .expect("git worktree add should run");
    assert!(
        output.status.success(),
        "git worktree add failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response = server
        .ingest_json_for_test(json!({
            "content": "Git anchored evidence",
            "wing": "mempal",
            "cwd": worktree.to_string_lossy()
        }))
        .await
        .expect("git cwd ingest should succeed");

    let drawer = db
        .get_drawer(&response.drawer_id)
        .expect("load git drawer")
        .expect("git drawer exists");

    let expected_worktree = format!(
        "worktree://{}",
        worktree
            .canonicalize()
            .expect("canonicalize worktree")
            .display()
    );
    let expected_repo_parent = format!(
        "repo://{}",
        repo_root
            .join(".git")
            .canonicalize()
            .expect("canonicalize git dir")
            .display()
    );

    assert_eq!(drawer.anchor_kind, AnchorKind::Worktree);
    assert_eq!(drawer.anchor_id, expected_worktree);
    assert_eq!(
        drawer.parent_anchor_id.as_deref(),
        Some(expected_repo_parent.as_str())
    );
}

#[tokio::test]
async fn test_non_git_cwd_falls_back_to_standalone_worktree_anchor() {
    let (tmp, db, server) = setup_mcp_server();
    let non_git = tmp.path().join("standalone");
    fs::create_dir_all(&non_git).expect("create standalone dir");

    let response = server
        .ingest_json_for_test(json!({
            "content": "Standalone anchored evidence",
            "wing": "mempal",
            "cwd": non_git.to_string_lossy()
        }))
        .await
        .expect("non-git cwd ingest should succeed");

    let drawer = db
        .get_drawer(&response.drawer_id)
        .expect("load non-git drawer")
        .expect("non-git drawer exists");

    let expected_worktree = format!(
        "worktree://{}",
        non_git
            .canonicalize()
            .expect("canonicalize standalone dir")
            .display()
    );

    assert_eq!(drawer.anchor_kind, AnchorKind::Worktree);
    assert_eq!(drawer.anchor_id, expected_worktree);
    assert_eq!(drawer.parent_anchor_id, None);
}

#[tokio::test]
async fn test_knowledge_drawer_gets_synthetic_knowledge_source_uri() {
    let (_tmp, db, server) = setup_mcp_server();
    let response = server
        .ingest_json_for_test(json!({
            "content": "Use source-backed verification before load-bearing claims.",
            "wing": "mempal",
            "memory_kind": "knowledge",
            "domain": "skill",
            "field": "debugging",
            "statement": "Debug by reproducing before patching.",
            "tier": "dao_tian",
            "status": "canonical",
            "supporting_refs": ["drawer_ev_001"]
        }))
        .await
        .expect("knowledge ingest should succeed");

    let drawer = db
        .get_drawer(&response.drawer_id)
        .expect("load knowledge drawer")
        .expect("knowledge drawer exists");
    let source = drawer.source_file.expect("knowledge source uri");
    assert!(source.starts_with("knowledge://skill/debugging/dao_tian/"));
}

#[tokio::test]
async fn test_bootstrap_identity_ignores_ref_and_hint_order() {
    let (_tmp, _db, server) = setup_mcp_server();
    let first = server
        .ingest_json_for_test(json!({
            "content": "Order-insensitive bootstrap identity",
            "wing": "mempal",
            "memory_kind": "knowledge",
            "domain": "skill",
            "field": "debugging",
            "statement": "Debug by reproducing before patching.",
            "tier": "shu",
            "status": "promoted",
            "supporting_refs": ["drawer_ev_002", "drawer_ev_001"],
            "counterexample_refs": ["drawer_cex_002", "drawer_cex_001"],
            "teaching_refs": ["drawer_teach_002", "drawer_teach_001"],
            "verification_refs": ["drawer_verify_002", "drawer_verify_001"],
            "trigger_hints": {
                "intent_tags": ["zeta", "alpha"],
                "workflow_bias": ["later", "earlier"],
                "tool_needs": ["tool-b", "tool-a"]
            },
            "anchor_kind": "repo",
            "anchor_id": "repo://identity"
        }))
        .await
        .expect("first dry-run-ish identity request should succeed");
    let second = server
        .ingest_json_for_test(json!({
            "content": "Order-insensitive bootstrap identity",
            "wing": "mempal",
            "memory_kind": "knowledge",
            "domain": "skill",
            "field": "debugging",
            "statement": "Debug by reproducing before patching.",
            "tier": "shu",
            "status": "promoted",
            "supporting_refs": ["drawer_ev_001", "drawer_ev_002"],
            "counterexample_refs": ["drawer_cex_001", "drawer_cex_002"],
            "teaching_refs": ["drawer_teach_001", "drawer_teach_002"],
            "verification_refs": ["drawer_verify_001", "drawer_verify_002"],
            "trigger_hints": {
                "intent_tags": ["alpha", "zeta"],
                "workflow_bias": ["earlier", "later"],
                "tool_needs": ["tool-a", "tool-b"]
            },
            "anchor_kind": "repo",
            "anchor_id": "repo://identity",
            "dry_run": true
        }))
        .await
        .expect("second identity request should succeed");

    assert_eq!(first.drawer_id, second.drawer_id);
}

#[tokio::test]
async fn test_search_result_exposes_knowledge_metadata_without_rewriting_content() {
    let (_tmp, db, server) = setup_mcp_server();
    let raw_content =
        "Raw knowledge body: preserve this exact content even when statement differs.";
    let statement = "Promote the normalized statement, not the stored body.";
    let drawer = bootstrap_drawer(
        "drawer_search_knowledge",
        raw_content,
        MemoryKind::Knowledge,
        Some(KnowledgeTier::Shu),
        Some(KnowledgeStatus::Promoted),
        Some(statement),
    );
    insert_search_fixture(&db, &drawer, &[0.1, 0.2, 0.3]);

    let response = server
        .search_json_for_test(json!({
            "query": "preserve exact content",
            "wing": "mempal",
            "room": "bootstrap",
            "top_k": 5
        }))
        .await
        .expect("search should succeed");

    let result = response.results.first().expect("search result");
    assert_eq!(result.drawer_id, drawer.id);
    assert_eq!(result.content, raw_content);
    assert_ne!(result.content, statement);
    assert_eq!(result.memory_kind, "knowledge");
    assert_eq!(result.domain, "project");
    assert_eq!(result.field, anchor::DEFAULT_FIELD);
    assert_eq!(result.statement.as_deref(), Some(statement));
    assert_eq!(result.tier.as_deref(), Some("shu"));
    assert_eq!(result.status.as_deref(), Some("promoted"));
    assert_eq!(result.anchor_kind, "repo");
    assert_eq!(result.anchor_id, "repo://drawer_search_knowledge");
    assert_eq!(result.parent_anchor_id, None);
}

#[tokio::test]
async fn test_search_filters_by_memory_kind_and_tier_without_rerank_changes() {
    let (_tmp, db, server) = setup_mcp_server();
    let evidence = bootstrap_drawer(
        "drawer_search_evidence",
        "alpha alpha alpha evidence body",
        MemoryKind::Evidence,
        None,
        None,
        None,
    );
    let knowledge_shu = bootstrap_drawer(
        "drawer_search_knowledge_shu",
        "alpha alpha knowledge shu body",
        MemoryKind::Knowledge,
        Some(KnowledgeTier::Shu),
        Some(KnowledgeStatus::Promoted),
        Some("Knowledge shu statement"),
    );
    let knowledge_qi = bootstrap_drawer(
        "drawer_search_knowledge_qi",
        "alpha knowledge qi body",
        MemoryKind::Knowledge,
        Some(KnowledgeTier::Qi),
        Some(KnowledgeStatus::Candidate),
        Some("Knowledge qi statement"),
    );

    insert_search_fixture(&db, &evidence, &[0.1, 0.2, 0.3]);
    insert_search_fixture(&db, &knowledge_shu, &[0.2, 0.2, 0.3]);
    insert_search_fixture(&db, &knowledge_qi, &[0.3, 0.2, 0.3]);

    let unfiltered = server
        .search_json_for_test(json!({
            "query": "alpha",
            "wing": "mempal",
            "room": "bootstrap",
            "top_k": 3
        }))
        .await
        .expect("unfiltered search should succeed");
    let knowledge_only = server
        .search_json_for_test(json!({
            "query": "alpha",
            "wing": "mempal",
            "room": "bootstrap",
            "memory_kind": "knowledge",
            "top_k": 3
        }))
        .await
        .expect("knowledge-only search should succeed");
    let shu_only = server
        .search_json_for_test(json!({
            "query": "alpha",
            "wing": "mempal",
            "room": "bootstrap",
            "memory_kind": "knowledge",
            "tier": "shu",
            "top_k": 3
        }))
        .await
        .expect("shu-only search should succeed");

    let unfiltered_ids: Vec<&str> = unfiltered
        .results
        .iter()
        .map(|result| result.drawer_id.as_str())
        .collect();
    let knowledge_only_ids: Vec<&str> = knowledge_only
        .results
        .iter()
        .map(|result| result.drawer_id.as_str())
        .collect();
    let shu_only_ids: Vec<&str> = shu_only
        .results
        .iter()
        .map(|result| result.drawer_id.as_str())
        .collect();

    assert_eq!(
        unfiltered_ids,
        vec![
            "drawer_search_evidence",
            "drawer_search_knowledge_shu",
            "drawer_search_knowledge_qi"
        ]
    );
    assert_eq!(
        knowledge_only_ids,
        vec!["drawer_search_knowledge_shu", "drawer_search_knowledge_qi"]
    );
    assert_eq!(shu_only_ids, vec!["drawer_search_knowledge_shu"]);
}

#[tokio::test]
async fn test_search_filters_by_domain_field_status_and_anchor_kind() {
    let (_tmp, db, server) = setup_mcp_server();

    let domain_skill = Drawer {
        domain: MemoryDomain::Skill,
        ..bootstrap_drawer(
            "drawer_filter_domain_skill",
            "domain focus domain focus",
            MemoryKind::Knowledge,
            Some(KnowledgeTier::Shu),
            Some(KnowledgeStatus::Promoted),
            Some("Skill-domain statement"),
        )
    };
    let domain_agent = Drawer {
        domain: MemoryDomain::Agent,
        ..bootstrap_drawer(
            "drawer_filter_domain_agent",
            "domain focus domain focus",
            MemoryKind::Knowledge,
            Some(KnowledgeTier::Shu),
            Some(KnowledgeStatus::Promoted),
            Some("Agent-domain statement"),
        )
    };
    let field_debugging = Drawer {
        domain: MemoryDomain::Skill,
        field: "debugging".to_string(),
        ..bootstrap_drawer(
            "drawer_filter_field_debugging",
            "field focus field focus",
            MemoryKind::Knowledge,
            Some(KnowledgeTier::Shu),
            Some(KnowledgeStatus::Promoted),
            Some("Debugging-field statement"),
        )
    };
    let field_tooling = Drawer {
        domain: MemoryDomain::Skill,
        field: "tooling".to_string(),
        ..bootstrap_drawer(
            "drawer_filter_field_tooling",
            "field focus field focus",
            MemoryKind::Knowledge,
            Some(KnowledgeTier::Shu),
            Some(KnowledgeStatus::Promoted),
            Some("Tooling-field statement"),
        )
    };
    let status_promoted = Drawer {
        domain: MemoryDomain::Skill,
        field: "debugging".to_string(),
        status: Some(KnowledgeStatus::Promoted),
        ..bootstrap_drawer(
            "drawer_filter_status_promoted",
            "status focus status focus",
            MemoryKind::Knowledge,
            Some(KnowledgeTier::Shu),
            Some(KnowledgeStatus::Promoted),
            Some("Promoted-status statement"),
        )
    };
    let status_retired = Drawer {
        domain: MemoryDomain::Skill,
        field: "debugging".to_string(),
        status: Some(KnowledgeStatus::Retired),
        ..bootstrap_drawer(
            "drawer_filter_status_retired",
            "status focus status focus",
            MemoryKind::Knowledge,
            Some(KnowledgeTier::Shu),
            Some(KnowledgeStatus::Retired),
            Some("Retired-status statement"),
        )
    };
    let anchor_repo = Drawer {
        domain: MemoryDomain::Skill,
        field: "debugging".to_string(),
        status: Some(KnowledgeStatus::Promoted),
        anchor_kind: AnchorKind::Repo,
        anchor_id: "repo://filter-anchor".to_string(),
        ..bootstrap_drawer(
            "drawer_filter_anchor_repo",
            "anchor focus anchor focus",
            MemoryKind::Knowledge,
            Some(KnowledgeTier::Shu),
            Some(KnowledgeStatus::Promoted),
            Some("Repo-anchor statement"),
        )
    };
    let anchor_worktree = Drawer {
        domain: MemoryDomain::Skill,
        field: "debugging".to_string(),
        status: Some(KnowledgeStatus::Promoted),
        anchor_kind: AnchorKind::Worktree,
        anchor_id: "worktree:///tmp/filter-anchor".to_string(),
        ..bootstrap_drawer(
            "drawer_filter_anchor_worktree",
            "anchor focus anchor focus",
            MemoryKind::Knowledge,
            Some(KnowledgeTier::Shu),
            Some(KnowledgeStatus::Promoted),
            Some("Worktree-anchor statement"),
        )
    };

    for (index, drawer) in [
        &domain_skill,
        &domain_agent,
        &field_debugging,
        &field_tooling,
        &status_promoted,
        &status_retired,
        &anchor_repo,
        &anchor_worktree,
    ]
    .into_iter()
    .enumerate()
    {
        insert_search_fixture(&db, drawer, &[0.1 + index as f32, 0.2, 0.3]);
    }

    let domain_results = server
        .search_json_for_test(json!({
            "query": "domain focus",
            "wing": "mempal",
            "room": "bootstrap",
            "domain": "skill",
            "top_k": 5
        }))
        .await
        .expect("domain-filtered search should succeed");
    let field_results = server
        .search_json_for_test(json!({
            "query": "field focus",
            "wing": "mempal",
            "room": "bootstrap",
            "field": "debugging",
            "top_k": 5
        }))
        .await
        .expect("field-filtered search should succeed");
    let status_results = server
        .search_json_for_test(json!({
            "query": "status focus",
            "wing": "mempal",
            "room": "bootstrap",
            "status": "promoted",
            "top_k": 5
        }))
        .await
        .expect("status-filtered search should succeed");
    let anchor_results = server
        .search_json_for_test(json!({
            "query": "anchor focus",
            "wing": "mempal",
            "room": "bootstrap",
            "anchor_kind": "repo",
            "top_k": 5
        }))
        .await
        .expect("anchor-filtered search should succeed");

    let domain_ids: Vec<&str> = domain_results
        .results
        .iter()
        .map(|result| result.drawer_id.as_str())
        .collect();
    let field_ids: Vec<&str> = field_results
        .results
        .iter()
        .map(|result| result.drawer_id.as_str())
        .collect();
    let status_ids: Vec<&str> = status_results
        .results
        .iter()
        .map(|result| result.drawer_id.as_str())
        .collect();
    let anchor_ids: Vec<&str> = anchor_results
        .results
        .iter()
        .map(|result| result.drawer_id.as_str())
        .collect();

    assert!(domain_ids.contains(&"drawer_filter_domain_skill"));
    assert!(!domain_ids.contains(&"drawer_filter_domain_agent"));
    assert!(
        domain_results
            .results
            .iter()
            .all(|result| result.domain == "skill")
    );

    assert!(field_ids.contains(&"drawer_filter_field_debugging"));
    assert!(!field_ids.contains(&"drawer_filter_field_tooling"));
    assert!(
        field_results
            .results
            .iter()
            .all(|result| result.field == "debugging")
    );

    assert!(status_ids.contains(&"drawer_filter_status_promoted"));
    assert!(!status_ids.contains(&"drawer_filter_status_retired"));
    assert!(
        status_results
            .results
            .iter()
            .all(|result| result.status.as_deref() == Some("promoted"))
    );

    assert!(anchor_ids.contains(&"drawer_filter_anchor_repo"));
    assert!(!anchor_ids.contains(&"drawer_filter_anchor_worktree"));
    assert!(
        anchor_results
            .results
            .iter()
            .all(|result| result.anchor_kind == "repo")
    );
}

#[test]
fn test_cli_search_json_exposes_bootstrap_metadata_fields() {
    let (tmp, db) = setup_cli_home();
    let target = Drawer {
        domain: MemoryDomain::Skill,
        field: "debugging".to_string(),
        anchor_kind: AnchorKind::Repo,
        anchor_id: "repo://cli-metadata".to_string(),
        ..bootstrap_drawer(
            "drawer_cli_metadata",
            "cli metadata focus cli metadata focus",
            MemoryKind::Knowledge,
            Some(KnowledgeTier::Shu),
            Some(KnowledgeStatus::Promoted),
            Some("CLI statement stays separate."),
        )
    };
    insert_search_fixture(&db, &target, &vector_of(384, 0.25));
    let results = run_cli_search_json(
        tmp.path(),
        "cli metadata focus cli metadata focus",
        &["--memory-kind", "knowledge"],
    );
    assert_eq!(results.len(), 1, "expected one filtered search result");
    let result = &results[0];

    assert_eq!(result["drawer_id"], "drawer_cli_metadata");
    assert_eq!(result["content"], target.content);
    assert_eq!(result["memory_kind"], "knowledge");
    assert_eq!(result["domain"], "skill");
    assert_eq!(result["field"], "debugging");
    assert_eq!(result["statement"], "CLI statement stays separate.");
    assert_eq!(result["tier"], "shu");
    assert_eq!(result["status"], "promoted");
    assert_eq!(result["anchor_kind"], "repo");
    assert_eq!(result["anchor_id"], "repo://cli-metadata");
    assert!(result["parent_anchor_id"].is_null());
}

#[test]
fn test_cli_search_json_filters_are_wired_individually() {
    let memory_kind_target = bootstrap_drawer(
        "drawer_cli_memory_kind_target",
        "memorykindtoken",
        MemoryKind::Knowledge,
        Some(KnowledgeTier::Shu),
        Some(KnowledgeStatus::Promoted),
        Some("Memory-kind target"),
    );
    let memory_kind_distractor = bootstrap_drawer(
        "drawer_cli_memory_kind_distractor",
        "memorykindtoken",
        MemoryKind::Evidence,
        None,
        None,
        None,
    );

    let domain_target = Drawer {
        domain: MemoryDomain::Skill,
        ..bootstrap_drawer(
            "drawer_cli_domain_target",
            "domaintoken",
            MemoryKind::Knowledge,
            Some(KnowledgeTier::Shu),
            Some(KnowledgeStatus::Promoted),
            Some("Domain target"),
        )
    };
    let domain_distractor = Drawer {
        domain: MemoryDomain::Agent,
        ..bootstrap_drawer(
            "drawer_cli_domain_distractor",
            "domaintoken",
            MemoryKind::Knowledge,
            Some(KnowledgeTier::Shu),
            Some(KnowledgeStatus::Promoted),
            Some("Domain distractor"),
        )
    };

    let field_target = Drawer {
        field: "debugging".to_string(),
        ..bootstrap_drawer(
            "drawer_cli_field_target",
            "fieldtoken",
            MemoryKind::Knowledge,
            Some(KnowledgeTier::Shu),
            Some(KnowledgeStatus::Promoted),
            Some("Field target"),
        )
    };
    let field_distractor = Drawer {
        field: "tooling".to_string(),
        ..bootstrap_drawer(
            "drawer_cli_field_distractor",
            "fieldtoken",
            MemoryKind::Knowledge,
            Some(KnowledgeTier::Shu),
            Some(KnowledgeStatus::Promoted),
            Some("Field distractor"),
        )
    };

    let tier_target = Drawer {
        tier: Some(KnowledgeTier::Shu),
        ..bootstrap_drawer(
            "drawer_cli_tier_target",
            "tiertoken",
            MemoryKind::Knowledge,
            Some(KnowledgeTier::Shu),
            Some(KnowledgeStatus::Promoted),
            Some("Tier target"),
        )
    };
    let tier_distractor = Drawer {
        tier: Some(KnowledgeTier::Qi),
        status: Some(KnowledgeStatus::Candidate),
        ..bootstrap_drawer(
            "drawer_cli_tier_distractor",
            "tiertoken",
            MemoryKind::Knowledge,
            Some(KnowledgeTier::Qi),
            Some(KnowledgeStatus::Candidate),
            Some("Tier distractor"),
        )
    };

    let status_target = Drawer {
        status: Some(KnowledgeStatus::Promoted),
        ..bootstrap_drawer(
            "drawer_cli_status_target",
            "statustoken",
            MemoryKind::Knowledge,
            Some(KnowledgeTier::Shu),
            Some(KnowledgeStatus::Promoted),
            Some("Status target"),
        )
    };
    let status_distractor = Drawer {
        status: Some(KnowledgeStatus::Retired),
        ..bootstrap_drawer(
            "drawer_cli_status_distractor",
            "statustoken",
            MemoryKind::Knowledge,
            Some(KnowledgeTier::Shu),
            Some(KnowledgeStatus::Retired),
            Some("Status distractor"),
        )
    };

    let anchor_target = Drawer {
        anchor_kind: AnchorKind::Repo,
        anchor_id: "repo://cli-anchor-target".to_string(),
        ..bootstrap_drawer(
            "drawer_cli_anchor_target",
            "anchortoken",
            MemoryKind::Knowledge,
            Some(KnowledgeTier::Shu),
            Some(KnowledgeStatus::Promoted),
            Some("Anchor target"),
        )
    };
    let anchor_distractor = Drawer {
        anchor_kind: AnchorKind::Worktree,
        anchor_id: "worktree:///tmp/cli-anchor-distractor".to_string(),
        ..bootstrap_drawer(
            "drawer_cli_anchor_distractor",
            "anchortoken",
            MemoryKind::Knowledge,
            Some(KnowledgeTier::Shu),
            Some(KnowledgeStatus::Promoted),
            Some("Anchor distractor"),
        )
    };

    assert_cli_filter_selects_only(
        "memorykindtoken",
        &["--memory-kind", "knowledge"],
        &memory_kind_target,
        &memory_kind_distractor,
    );
    assert_cli_filter_selects_only(
        "domaintoken",
        &["--domain", "skill"],
        &domain_target,
        &domain_distractor,
    );
    assert_cli_filter_selects_only(
        "fieldtoken",
        &["--field", "debugging"],
        &field_target,
        &field_distractor,
    );
    assert_cli_filter_selects_only(
        "tiertoken",
        &["--tier", "shu"],
        &tier_target,
        &tier_distractor,
    );
    assert_cli_filter_selects_only(
        "statustoken",
        &["--status", "promoted"],
        &status_target,
        &status_distractor,
    );
    assert_cli_filter_selects_only(
        "anchortoken",
        &["--anchor-kind", "repo"],
        &anchor_target,
        &anchor_distractor,
    );
}

#[test]
fn test_wake_up_prefers_knowledge_statement_in_plain_output() {
    let (tmp, db) = setup_cli_home();
    let drawer = bootstrap_drawer(
        "drawer_kn_wake_statement",
        "Start from a concrete reproduction, then isolate scope before patching.",
        MemoryKind::Knowledge,
        Some(KnowledgeTier::Shu),
        Some(KnowledgeStatus::Promoted),
        Some("Debug by reproducing before patching."),
    );
    insert_cli_wake_up_drawer(&db, &drawer);

    let output = run_cli_wake_up(tmp.path(), None);

    assert!(output.contains("Debug by reproducing before patching."));
    assert!(
        !output.contains("Start from a concrete reproduction, then isolate scope before patching.")
    );
}

#[test]
fn test_wake_up_evidence_drawer_still_uses_content() {
    let (tmp, db) = setup_cli_home();
    let drawer = bootstrap_drawer(
        "drawer_ev_wake_content",
        "Observed that tests failed after the patch.",
        MemoryKind::Evidence,
        None,
        None,
        None,
    );
    insert_cli_wake_up_drawer(&db, &drawer);

    let output = run_cli_wake_up(tmp.path(), None);

    assert!(output.contains("Observed that tests failed after the patch."));
}

#[test]
fn test_wake_up_knowledge_without_statement_falls_back_to_content() {
    let (tmp, db) = setup_cli_home();
    let drawer = bootstrap_drawer(
        "drawer_kn_wake_fallback",
        "Fallback to the rationale body when statement is missing.",
        MemoryKind::Knowledge,
        Some(KnowledgeTier::Shu),
        Some(KnowledgeStatus::Promoted),
        Some("   "),
    );
    insert_cli_wake_up_drawer(&db, &drawer);

    let output = run_cli_wake_up(tmp.path(), None);

    assert!(output.contains("Fallback to the rationale body when statement is missing."));
}

#[test]
fn test_wake_up_estimated_tokens_use_effective_text() {
    let (tmp, db) = setup_cli_home();
    let drawer = bootstrap_drawer(
        "drawer_kn_wake_tokens",
        "This longer rationale body should not drive the token estimate anymore.",
        MemoryKind::Knowledge,
        Some(KnowledgeTier::Shu),
        Some(KnowledgeStatus::Promoted),
        Some("Short wake text"),
    );
    insert_cli_wake_up_drawer(&db, &drawer);

    let output = run_cli_wake_up(tmp.path(), None);

    assert!(output.contains("estimated_tokens: 3"));
}

#[test]
fn test_wake_up_protocol_output_unchanged() {
    let (tmp, db) = setup_cli_home();
    let drawer = bootstrap_drawer(
        "drawer_kn_protocol",
        "Protocol mode should ignore drawer content.",
        MemoryKind::Knowledge,
        Some(KnowledgeTier::Shu),
        Some(KnowledgeStatus::Promoted),
        Some("Protocol mode should ignore statements too."),
    );
    insert_cli_wake_up_drawer(&db, &drawer);

    let output = run_cli_wake_up(tmp.path(), Some("protocol"));

    assert_eq!(output.trim(), MEMORY_PROTOCOL);
}

#[test]
fn test_wake_up_aaak_prefers_knowledge_statement() {
    let (tmp, db) = setup_cli_home();
    let drawer = bootstrap_drawer(
        "drawer_kn_wake_aaak",
        "Long rationale body should not be encoded for AAAK wake-up.",
        MemoryKind::Knowledge,
        Some(KnowledgeTier::Shu),
        Some(KnowledgeStatus::Promoted),
        Some("Use the short statement for AAAK wake-up."),
    );
    insert_cli_wake_up_drawer(&db, &drawer);

    let output = run_cli_wake_up(tmp.path(), Some("aaak"));
    let document = AaakDocument::parse(output.trim()).expect("parse aaak wake-up output");
    let decoded = AaakCodec::default().decode(&document);

    assert!(decoded.contains("Use the short statement for AAAK wake-up."));
    assert!(!decoded.contains("Long rationale body should not be encoded for AAAK wake-up."));
}

#[test]
fn test_wake_up_does_not_assemble_mind_model_sections() {
    let (tmp, db) = setup_cli_home();
    let dao_tian = Drawer {
        importance: 5,
        added_at: "1710011002".to_string(),
        ..bootstrap_drawer(
            "drawer_kn_wake_boundary_dao_tian",
            "Universal principle rationale should not create a dao_tian section.",
            MemoryKind::Knowledge,
            Some(KnowledgeTier::DaoTian),
            Some(KnowledgeStatus::Canonical),
            Some("Evidence precedes assertion."),
        )
    };
    let dao_ren = Drawer {
        importance: 4,
        added_at: "1710011001".to_string(),
        ..bootstrap_drawer(
            "drawer_kn_wake_boundary_dao_ren",
            "Domain rule rationale should not create a dao_ren section.",
            MemoryKind::Knowledge,
            Some(KnowledgeTier::DaoRen),
            Some(KnowledgeStatus::Promoted),
            Some("Rust changes must submit to cargo check."),
        )
    };
    insert_cli_wake_up_drawer(&db, &dao_tian);
    insert_cli_wake_up_drawer(&db, &dao_ren);

    let output = run_cli_wake_up(tmp.path(), None);

    assert!(output.contains("## L0"));
    assert!(output.contains("## L1"));
    assert!(output.contains("Evidence precedes assertion."));
    assert!(output.contains("Rust changes must submit to cargo check."));
    for heading in ["## dao_tian", "## dao_ren", "## shu", "## qi"] {
        assert!(
            !output.contains(heading),
            "wake-up must not assemble typed context section {heading}"
        );
    }
}

#[test]
fn test_wake_up_aaak_does_not_assemble_mind_model_sections() {
    let (tmp, db) = setup_cli_home();
    let dao_tian = Drawer {
        importance: 5,
        added_at: "1710012002".to_string(),
        ..bootstrap_drawer(
            "drawer_kn_wake_aaak_boundary_dao_tian",
            "Universal principle rationale should not create an AAAK dao_tian section.",
            MemoryKind::Knowledge,
            Some(KnowledgeTier::DaoTian),
            Some(KnowledgeStatus::Canonical),
            Some("Keep universal principles sparse."),
        )
    };
    let qi = Drawer {
        importance: 4,
        added_at: "1710012001".to_string(),
        ..bootstrap_drawer(
            "drawer_kn_wake_aaak_boundary_qi",
            "Tool usage rationale should not create an AAAK qi section.",
            MemoryKind::Knowledge,
            Some(KnowledgeTier::Qi),
            Some(KnowledgeStatus::Promoted),
            Some("Use cargo check as the Rust ground truth."),
        )
    };
    insert_cli_wake_up_drawer(&db, &dao_tian);
    insert_cli_wake_up_drawer(&db, &qi);

    let output = run_cli_wake_up(tmp.path(), Some("aaak"));
    let document = AaakDocument::parse(output.trim()).expect("parse aaak wake-up output");
    let decoded = AaakCodec::default().decode(&document);

    assert!(decoded.contains("Keep universal principles sparse."));
    assert!(decoded.contains("Use cargo check as the Rust ground truth."));
    for heading in ["## dao_tian", "## dao_ren", "## shu", "## qi"] {
        assert!(
            !decoded.contains(heading),
            "AAAK wake-up must not assemble typed context section {heading}"
        );
    }
}

#[test]
fn test_wake_up_preserves_existing_top_drawer_order() {
    let (tmp, db) = setup_cli_home();
    let highest = Drawer {
        importance: 5,
        added_at: "1710010002".to_string(),
        ..bootstrap_drawer(
            "drawer_kn_order_high",
            "High priority rationale body.",
            MemoryKind::Knowledge,
            Some(KnowledgeTier::Shu),
            Some(KnowledgeStatus::Promoted),
            Some("High priority statement."),
        )
    };
    let lower = Drawer {
        importance: 1,
        added_at: "1710010001".to_string(),
        ..bootstrap_drawer(
            "drawer_kn_order_low",
            "Low priority rationale body.",
            MemoryKind::Knowledge,
            Some(KnowledgeTier::Shu),
            Some(KnowledgeStatus::Promoted),
            Some("Low priority statement."),
        )
    };
    insert_cli_wake_up_drawer(&db, &highest);
    insert_cli_wake_up_drawer(&db, &lower);

    let output = run_cli_wake_up(tmp.path(), None);
    let highest_pos = output
        .find("drawer_kn_order_high")
        .expect("high drawer in output");
    let lower_pos = output
        .find("drawer_kn_order_low")
        .expect("low drawer in output");

    assert!(highest_pos < lower_pos);
}
