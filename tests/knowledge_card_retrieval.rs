use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::thread;

use async_trait::async_trait;
use mempal::core::anchor;
use mempal::core::db::Database;
use mempal::core::types::{
    AnchorKind, Drawer, KnowledgeCard, KnowledgeEvidenceLink, KnowledgeEvidenceRole,
    KnowledgeStatus, KnowledgeTier, MemoryDomain, MemoryKind, Provenance, SourceType,
};
use mempal::embed::{Embedder, EmbedderFactory};
use mempal::knowledge_card_retrieval::{KnowledgeCardRetrievalRequest, retrieve_knowledge_cards};
use mempal::mcp::MempalMcpServer;
use mempal::search::{SearchOptions, search_with_options};
use serde_json::{Value, json};
use tempfile::TempDir;

struct StubEmbedder {
    vector: Vec<f32>,
}

#[derive(Clone)]
struct StubEmbedderFactory {
    vector: Vec<f32>,
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

#[async_trait]
impl EmbedderFactory for StubEmbedderFactory {
    async fn build(&self) -> mempal::embed::Result<Box<dyn Embedder>> {
        Ok(Box::new(StubEmbedder {
            vector: self.vector.clone(),
        }))
    }
}

fn vector() -> Vec<f32> {
    vec![0.25; 384]
}

fn embedder() -> StubEmbedder {
    StubEmbedder { vector: vector() }
}

fn mempal_bin() -> String {
    env!("CARGO_BIN_EXE_mempal").to_string()
}

fn setup_db() -> (TempDir, Database) {
    let tmp = TempDir::new().expect("tempdir");
    let db = Database::open(&tmp.path().join("palace.db")).expect("open db");
    (tmp, db)
}

fn setup_cli_home() -> (TempDir, Database) {
    let tmp = TempDir::new().expect("tempdir");
    let mempal_dir = tmp.path().join(".mempal");
    fs::create_dir_all(&mempal_dir).expect("create .mempal");
    let db = Database::open(&mempal_dir.join("palace.db")).expect("open cli db");
    (tmp, db)
}

fn evidence_drawer(id: &str, content: &str) -> Drawer {
    Drawer {
        id: id.to_string(),
        content: content.to_string(),
        wing: "mempal".to_string(),
        room: Some("retrieval".to_string()),
        source_file: Some(format!("tests://retrieval/{id}.md")),
        source_type: SourceType::Manual,
        added_at: "1710000000".to_string(),
        chunk_index: Some(0),
        normalize_version: 1,
        importance: 3,
        memory_kind: MemoryKind::Evidence,
        domain: MemoryDomain::Project,
        field: "general".to_string(),
        anchor_kind: AnchorKind::Repo,
        anchor_id: anchor::LEGACY_REPO_ANCHOR_ID.to_string(),
        parent_anchor_id: None,
        provenance: Some(Provenance::Human),
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

fn card(id: &str, status: KnowledgeStatus, statement: &str) -> KnowledgeCard {
    KnowledgeCard {
        id: id.to_string(),
        statement: statement.to_string(),
        content: format!("Card content for {id}."),
        tier: KnowledgeTier::Shu,
        status,
        domain: MemoryDomain::Project,
        field: "general".to_string(),
        anchor_kind: AnchorKind::Repo,
        anchor_id: anchor::LEGACY_REPO_ANCHOR_ID.to_string(),
        parent_anchor_id: None,
        scope_constraints: None,
        trigger_hints: None,
        created_at: "1710000000".to_string(),
        updated_at: "1710000000".to_string(),
    }
}

fn insert_evidence(db: &Database, drawer: &Drawer) {
    db.insert_drawer(drawer).expect("insert drawer");
    db.insert_vector(&drawer.id, &vector())
        .expect("insert vector");
}

fn insert_card(db: &Database, card: &KnowledgeCard) {
    db.insert_knowledge_card(card).expect("insert card");
}

fn insert_link(db: &Database, id: &str, card_id: &str, evidence_drawer_id: &str) {
    db.insert_knowledge_evidence_link(&KnowledgeEvidenceLink {
        id: id.to_string(),
        card_id: card_id.to_string(),
        evidence_drawer_id: evidence_drawer_id.to_string(),
        role: KnowledgeEvidenceRole::Supporting,
        note: None,
        created_at: "1710000000".to_string(),
    })
    .expect("insert link");
}

fn request(query: &str, cwd: &Path) -> KnowledgeCardRetrievalRequest {
    KnowledgeCardRetrievalRequest {
        query: query.to_string(),
        domain: MemoryDomain::Project,
        field: "general".to_string(),
        cwd: cwd.to_path_buf(),
        top_k: 5,
        evidence_top_k: 10,
    }
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
                Ok(pair) => Some(pair),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(std::time::Duration::from_millis(20));
                    None
                }
                Err(error) => panic!("embedding stub accept failed: {error}"),
            })
            .expect("embedding stub timed out waiting for request");
        let mut request = [0_u8; 8192];
        let bytes_read = stream.read(&mut request).expect("read embedding request");
        let request_text = String::from_utf8_lossy(&request[..bytes_read]);
        let body = request_text
            .split("\r\n\r\n")
            .nth(1)
            .expect("embedding request body");
        let payload: Value = serde_json::from_str(body).expect("parse embedding request body");
        assert_eq!(payload["input"], json!([expected_query]));
        let response_body = json!({
            "data": [{ "embedding": vector }]
        })
        .to_string();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        stream
            .write_all(response.as_bytes())
            .expect("write embedding response");
    });

    (format!("http://{address}/v1/embeddings"), handle)
}

fn write_cli_api_config(home: &Path, endpoint: &str) {
    fs::write(
        home.join(".mempal").join("config.toml"),
        format!(
            "[embed]\nbackend = \"api\"\napi_endpoint = \"{endpoint}\"\napi_model = \"test-model\"\n"
        ),
    )
    .expect("write cli config");
}

fn run_cli_retrieve_json(home: &Path, query: &str) -> Value {
    let (endpoint, handle) = start_openai_embedding_stub(query, vector());
    write_cli_api_config(home, &endpoint);
    let output = Command::new(mempal_bin())
        .args([
            "knowledge-card",
            "retrieve",
            query,
            "--format",
            "json",
            "--top-k",
            "5",
        ])
        .env("HOME", home)
        .output()
        .expect("run retrieve");
    assert!(
        output.status.success(),
        "retrieve command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    handle.join().expect("join embedding stub");
    serde_json::from_slice(&output.stdout).expect("parse retrieve json")
}

#[test]
fn test_cli_knowledge_card_retrieve_rejects_zero_top_k() {
    let (tmp, _db) = setup_cli_home();
    let output = Command::new(mempal_bin())
        .args([
            "knowledge-card",
            "retrieve",
            "alpha",
            "--top-k",
            "0",
            "--format",
            "json",
        ])
        .env("HOME", tmp.path())
        .output()
        .expect("run retrieve");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--top-k must be greater than 0"));
}

#[tokio::test]
async fn test_cli_knowledge_card_retrieve_json_returns_active_card() {
    let (tmp, db) = setup_cli_home();
    insert_evidence(
        &db,
        &evidence_drawer(
            "drawer_ev_active",
            "alpha evidence supports a reusable method",
        ),
    );
    insert_card(
        &db,
        &card(
            "card_active",
            KnowledgeStatus::Promoted,
            "Alpha method should be reused.",
        ),
    );
    insert_link(&db, "link_active", "card_active", "drawer_ev_active");

    let value = run_cli_retrieve_json(tmp.path(), "alpha evidence");
    assert_eq!(value[0]["card"]["id"], "card_active");
    assert_eq!(
        value[0]["evidence_citations"][0]["evidence_drawer_id"],
        "drawer_ev_active"
    );
    assert_eq!(value[0]["evidence_citations"][0]["role"], "supporting");
    assert_eq!(
        value[0]["evidence_citations"][0]["source_file"],
        "tests://retrieval/drawer_ev_active.md"
    );
    assert!(value[0]["evidence_citations"][0]["score"].is_number());
}

#[tokio::test]
async fn test_card_retrieve_excludes_candidate_cards() {
    let (tmp, db) = setup_db();
    insert_evidence(&db, &evidence_drawer("drawer_ev", "alpha evidence"));
    insert_card(
        &db,
        &card("card_promoted", KnowledgeStatus::Promoted, "Promoted card."),
    );
    insert_card(
        &db,
        &card(
            "card_candidate",
            KnowledgeStatus::Candidate,
            "Candidate card.",
        ),
    );
    insert_link(&db, "link_promoted", "card_promoted", "drawer_ev");
    insert_link(&db, "link_candidate", "card_candidate", "drawer_ev");

    let results = retrieve_knowledge_cards(&db, &embedder(), request("alpha", tmp.path()))
        .await
        .expect("retrieve cards");

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].card.id, "card_promoted");
}

#[tokio::test]
async fn test_mcp_knowledge_cards_retrieve_action() {
    let (tmp, db) = setup_db();
    let db_path = tmp.path().join("palace.db");
    drop(db);
    let db = Database::open(&db_path).expect("reopen db");
    insert_evidence(&db, &evidence_drawer("drawer_ev_mcp", "alpha mcp evidence"));
    insert_card(
        &db,
        &card(
            "card_mcp",
            KnowledgeStatus::Canonical,
            "MCP card retrieval works.",
        ),
    );
    insert_link(&db, "link_mcp", "card_mcp", "drawer_ev_mcp");
    let server = MempalMcpServer::new_with_factory(
        db_path,
        Arc::new(StubEmbedderFactory { vector: vector() }),
    );

    let response = server
        .knowledge_cards_json_for_test(json!({
            "action": "retrieve",
            "query": "alpha mcp",
            "top_k": 3,
            "cwd": tmp.path().to_string_lossy()
        }))
        .await
        .expect("mcp retrieve");

    assert_eq!(response.retrieved.len(), 1);
    assert_eq!(response.retrieved[0].card.id, "card_mcp");
    assert_eq!(
        response.retrieved[0].evidence_citations[0].evidence_drawer_id,
        "drawer_ev_mcp"
    );
}

#[tokio::test]
async fn test_card_retrieve_has_no_db_side_effects() {
    let (tmp, db) = setup_db();
    insert_evidence(&db, &evidence_drawer("drawer_ev_side", "alpha evidence"));
    insert_card(
        &db,
        &card("card_side", KnowledgeStatus::Promoted, "Side effect card."),
    );
    insert_link(&db, "link_side", "card_side", "drawer_ev_side");
    let drawer_count = db.drawer_count().expect("drawer count");
    let card_count = db.knowledge_card_count().expect("card count");
    let link_count: i64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM knowledge_evidence_links", [], |row| {
            row.get(0)
        })
        .expect("link count");
    let event_count: i64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM knowledge_events", [], |row| {
            row.get(0)
        })
        .expect("event count");

    let _results = retrieve_knowledge_cards(&db, &embedder(), request("alpha", tmp.path()))
        .await
        .expect("retrieve cards");

    assert_eq!(db.drawer_count().expect("drawer count"), drawer_count);
    assert_eq!(db.knowledge_card_count().expect("card count"), card_count);
    let after_link_count: i64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM knowledge_evidence_links", [], |row| {
            row.get(0)
        })
        .expect("link count");
    let after_event_count: i64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM knowledge_events", [], |row| {
            row.get(0)
        })
        .expect("event count");
    assert_eq!(after_link_count, link_count);
    assert_eq!(after_event_count, event_count);
}

#[tokio::test]
async fn test_card_retrieve_does_not_change_mempal_search() {
    let (_tmp, db) = setup_db();
    insert_evidence(&db, &evidence_drawer("drawer_ev_search", "alpha evidence"));
    insert_card(
        &db,
        &card(
            "card_search",
            KnowledgeStatus::Promoted,
            "Search boundary card.",
        ),
    );
    insert_link(&db, "link_search", "card_search", "drawer_ev_search");

    let results = search_with_options(
        &db,
        &embedder(),
        "alpha",
        Some("mempal"),
        Some("retrieval"),
        SearchOptions::default(),
        5,
    )
    .await
    .expect("search");

    assert!(
        results
            .iter()
            .any(|result| result.drawer_id == "drawer_ev_search")
    );
    assert!(
        results
            .iter()
            .all(|result| result.drawer_id != "card_search")
    );
}
