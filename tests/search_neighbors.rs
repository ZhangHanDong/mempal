use mempal::core::db::Database;
use mempal::core::types::{BootstrapEvidenceArgs, Drawer, SourceType};
use mempal::embed::{Embedder, EmbedderFactory};
use mempal::ingest::{IngestOptions, ingest_file_with_options};
use mempal::mcp::MempalMcpServer;
use mempal::search::{SearchFilters, SearchOptions, search_with_vector_options};
use serde_json::{Value, json};
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::thread;
use tempfile::TempDir;

struct StubEmbedder;

struct StubEmbedderFactory;

struct ChunkSeed<'a> {
    id: &'a str,
    content: &'a str,
    wing: &'a str,
    room: Option<&'a str>,
    source_file: &'a str,
    chunk_index: i64,
}

#[async_trait::async_trait]
impl EmbedderFactory for StubEmbedderFactory {
    async fn build(&self) -> mempal::embed::Result<Box<dyn Embedder>> {
        Ok(Box::new(StubEmbedder))
    }
}

#[async_trait::async_trait]
impl Embedder for StubEmbedder {
    async fn embed(&self, texts: &[&str]) -> mempal::embed::Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|_| vector()).collect())
    }

    fn dimensions(&self) -> usize {
        3
    }

    fn name(&self) -> &str {
        "stub"
    }
}

fn vector() -> Vec<f32> {
    vec![0.1, 0.2, 0.3]
}

fn route(wing: &str, room: Option<&str>) -> mempal::core::types::RouteDecision {
    mempal::core::types::RouteDecision {
        wing: Some(wing.to_string()),
        room: room.map(str::to_string),
        confidence: 1.0,
        reason: "test".to_string(),
    }
}

fn new_db() -> (TempDir, Database) {
    let tmp = TempDir::new().expect("tempdir");
    let db = Database::open(&tmp.path().join("palace.db")).expect("open db");
    (tmp, db)
}

fn insert_chunk(
    db: &Database,
    id: &str,
    content: &str,
    wing: &str,
    room: Option<&str>,
    source_file: &str,
    chunk_index: i64,
) {
    insert_chunk_with_vector(
        db,
        ChunkSeed {
            id,
            content,
            wing,
            room,
            source_file,
            chunk_index,
        },
        &vector(),
    );
}

fn insert_chunk_with_vector(db: &Database, seed: ChunkSeed<'_>, vector: &[f32]) {
    db.insert_drawer(&Drawer::new_bootstrap_evidence(BootstrapEvidenceArgs {
        id: seed.id.to_string(),
        content: seed.content.to_string(),
        wing: seed.wing.to_string(),
        room: seed.room.map(str::to_string),
        source_file: Some(seed.source_file.to_string()),
        source_type: SourceType::Project,
        added_at: format!("171000000{}", seed.chunk_index),
        chunk_index: Some(seed.chunk_index),
        importance: 0,
    }))
    .expect("insert chunk drawer");
    db.insert_vector(seed.id, vector).expect("insert vector");
}

fn insert_doc_chunks(db: &Database, count: usize) {
    insert_doc_chunks_with_vector(db, count, &vector());
}

fn insert_doc_chunks_with_vector(db: &Database, count: usize, vector: &[f32]) {
    for index in 0..count {
        let needle = if index == 2 { " needle" } else { "" };
        insert_chunk_with_vector(
            db,
            ChunkSeed {
                id: &format!("drawer_{index}"),
                content: &format!("chunk {index}{needle}"),
                wing: "mempal",
                room: Some("docs"),
                source_file: "doc.md",
                chunk_index: index as i64,
            },
            vector,
        );
    }
}

async fn search_neighbors(
    db: &Database,
    query: &str,
    top_k: usize,
    with_neighbors: bool,
) -> Vec<mempal::core::types::SearchResult> {
    search_with_vector_options(
        db,
        query,
        &vector(),
        route("mempal", Some("docs")),
        SearchOptions {
            filters: SearchFilters::default(),
            with_neighbors,
        },
        top_k,
    )
    .expect("search with vector")
}

#[tokio::test]
async fn test_search_with_neighbors_includes_prev_next() {
    let (_tmp, db) = new_db();
    insert_doc_chunks(&db, 5);

    let results = search_neighbors(&db, "needle", 5, true).await;
    let hit = results
        .iter()
        .find(|result| result.drawer_id == "drawer_2")
        .expect("needle hit");
    let neighbors = hit.neighbors.as_ref().expect("neighbors");

    assert_eq!(
        neighbors.prev.as_ref().map(|chunk| chunk.chunk_index),
        Some(1)
    );
    assert_eq!(
        neighbors.next.as_ref().map(|chunk| chunk.chunk_index),
        Some(3)
    );
    assert!(!neighbors.prev.as_ref().unwrap().content.is_empty());
    assert!(!neighbors.next.as_ref().unwrap().content.is_empty());
}

#[tokio::test]
async fn test_with_neighbors_omit_backward_compat() {
    let (_tmp, db_path, server) = setup_mcp_server();
    let db = Database::open(&db_path).expect("open db");
    insert_doc_chunks(&db, 3);

    let response = server
        .search_json_for_test(json!({
            "query": "needle",
            "wing": "mempal",
            "room": "docs",
            "top_k": 3
        }))
        .await
        .expect("search");
    let json = serde_json::to_value(response.results.first().expect("result")).expect("json");

    assert!(json.get("neighbors").is_none());
    assert_eq!(json["drawer_id"], "drawer_2");
}

#[tokio::test]
async fn test_first_chunk_has_no_prev() {
    let (_tmp, db) = new_db();
    insert_doc_chunks(&db, 3);

    let results = search_neighbors(&db, "chunk 0", 3, true).await;
    let hit = results
        .iter()
        .find(|result| result.drawer_id == "drawer_0")
        .expect("first hit");
    let neighbors = hit.neighbors.as_ref().expect("neighbors");

    assert!(neighbors.prev.is_none());
    assert_eq!(
        neighbors.next.as_ref().map(|chunk| chunk.chunk_index),
        Some(1)
    );
}

#[tokio::test]
async fn test_last_chunk_has_no_next() {
    let (_tmp, db) = new_db();
    insert_doc_chunks(&db, 3);

    let results = search_neighbors(&db, "chunk 2", 3, true).await;
    let hit = results
        .iter()
        .find(|result| result.drawer_id == "drawer_2")
        .expect("last hit");
    let neighbors = hit.neighbors.as_ref().expect("neighbors");

    assert!(neighbors.next.is_none());
    assert_eq!(
        neighbors.prev.as_ref().map(|chunk| chunk.chunk_index),
        Some(1)
    );
}

#[tokio::test]
async fn test_top_k_over_10_skips_neighbors() {
    let (_tmp, db) = new_db();
    insert_doc_chunks(&db, 20);

    let results = search_neighbors(&db, "needle", 20, true).await;

    assert!(!results.is_empty());
    assert!(results.iter().all(|result| result.neighbors.is_none()));
}

#[tokio::test]
async fn test_neighbors_limited_to_same_wing() {
    let (_tmp, db) = new_db();
    insert_chunk(&db, "a_0", "A chunk 0", "A", Some("docs"), "doc.md", 0);
    insert_chunk(&db, "a_1", "A chunk 1", "A", Some("docs"), "doc.md", 1);
    insert_chunk(
        &db,
        "a_2",
        "A chunk 2 needle",
        "A",
        Some("docs"),
        "doc.md",
        2,
    );
    insert_chunk(&db, "b_3", "B chunk 3", "B", Some("docs"), "doc.md", 3);

    let results = search_with_vector_options(
        &db,
        "needle",
        &vector(),
        route("A", Some("docs")),
        SearchOptions {
            filters: SearchFilters::default(),
            with_neighbors: true,
        },
        4,
    )
    .expect("search");
    let hit = results
        .iter()
        .find(|result| result.drawer_id == "a_2")
        .expect("A hit");
    let neighbors = hit.neighbors.as_ref().expect("neighbors");

    assert_eq!(
        neighbors
            .prev
            .as_ref()
            .map(|chunk| chunk.drawer_id.as_str()),
        Some("a_1")
    );
    assert!(neighbors.next.is_none());
}

#[test]
fn test_current_schema_has_chunk_index() {
    let (_tmp, db) = new_db();

    assert_eq!(db.schema_version().expect("schema version"), 8);
    let exists: bool = db
        .conn()
        .query_row(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM pragma_table_info('drawers')
                WHERE name = 'chunk_index'
            )
            "#,
            [],
            |row| row.get(0),
        )
        .expect("chunk_index exists");
    assert!(exists);
}

#[tokio::test]
async fn test_new_ingest_writes_chunk_index_sequentially() {
    let tmp = TempDir::new().expect("tempdir");
    let db = Database::open(&tmp.path().join("palace.db")).expect("open db");
    let source = tmp.path().join("long.md");
    let content = (0..2600)
        .map(|index| char::from(b'a' + (index % 26) as u8))
        .collect::<String>();
    fs::write(&source, content).expect("write source");

    ingest_file_with_options(
        &db,
        &StubEmbedder,
        &source,
        "mempal",
        IngestOptions {
            room: Some("docs"),
            source_root: source.parent(),
            dry_run: false,
            source_file_override: None,
            replace_existing_source: false,
            no_strip_noise: false,
            ..IngestOptions::default()
        },
    )
    .await
    .expect("ingest source");

    let indexes = active_chunk_indexes(&db, "long.md");
    let expected = (0..indexes.len() as i64).collect::<Vec<_>>();
    assert!(
        indexes.len() >= 4,
        "expected at least 4 chunks: {indexes:?}"
    );
    assert_eq!(indexes, expected);
}

#[tokio::test]
async fn test_cli_search_with_neighbors_json() {
    let (tmp, db) = setup_cli_home();
    insert_doc_chunks_with_vector(&db, 5, &vec![0.1; 384]);
    let results = run_cli_search_json(tmp.path(), "needle", &["--with-neighbors"]);
    let hit = results
        .iter()
        .find(|result| result["drawer_id"] == "drawer_2")
        .expect("needle hit");

    assert_eq!(hit["neighbors"]["prev"]["chunk_index"], 1);
    assert_eq!(hit["neighbors"]["next"]["chunk_index"], 3);
}

fn active_chunk_indexes(db: &Database, source_file: &str) -> Vec<i64> {
    let mut statement = db
        .conn()
        .prepare(
            r#"
            SELECT chunk_index
            FROM drawers
            WHERE source_file = ?1 AND deleted_at IS NULL
            ORDER BY chunk_index
            "#,
        )
        .expect("prepare chunk indexes");
    statement
        .query_map([source_file], |row| row.get::<_, i64>(0))
        .expect("query chunk indexes")
        .collect::<std::result::Result<Vec<_>, _>>()
        .expect("collect chunk indexes")
}

fn setup_mcp_server() -> (TempDir, std::path::PathBuf, MempalMcpServer) {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("palace.db");
    let server = MempalMcpServer::new_with_factory(db_path.clone(), Arc::new(StubEmbedderFactory));
    (tmp, db_path, server)
}

fn mempal_bin() -> String {
    env!("CARGO_BIN_EXE_mempal").to_string()
}

fn setup_cli_home() -> (TempDir, Database) {
    let tmp = TempDir::new().expect("tempdir");
    let mempal_dir = tmp.path().join(".mempal");
    fs::create_dir_all(&mempal_dir).expect("create .mempal");
    let db = Database::open(&mempal_dir.join("palace.db")).expect("open cli db");
    (tmp, db)
}

fn write_cli_api_config(home: &Path, endpoint: &str) {
    let config_path = home.join(".mempal").join("config.toml");
    fs::write(
        config_path,
        format!(
            "[embed]\nbackend = \"api\"\napi_endpoint = \"{endpoint}\"\napi_model = \"test-model\"\n"
        ),
    )
    .expect("write cli config");
}

fn run_cli_search_json(home: &Path, query: &str, extra_args: &[&str]) -> Vec<Value> {
    let (endpoint, handle) = start_openai_embedding_stub(query, vec![0.1; 384]);
    write_cli_api_config(home, &endpoint);

    let mut args = vec![
        "search".to_string(),
        query.to_string(),
        "--wing".to_string(),
        "mempal".to_string(),
        "--room".to_string(),
        "docs".to_string(),
    ];
    args.extend(extra_args.iter().map(|value| (*value).to_string()));
    args.extend(["--top-k".to_string(), "5".to_string(), "--json".to_string()]);

    let output = Command::new(mempal_bin())
        .args(args)
        .env("HOME", home)
        .output()
        .expect("run mempal search");
    assert!(
        output.status.success(),
        "search command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    handle.join().expect("join embedding stub");

    serde_json::from_slice(&output.stdout).expect("parse cli search json")
}

fn start_openai_embedding_stub(
    expected_query: &str,
    vector: Vec<f32>,
) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind embedding stub");
    listener.set_nonblocking(true).expect("set nonblocking");
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
            .expect("embedding stub timed out");
        let mut request = [0_u8; 4096];
        let bytes_read = stream.read(&mut request).expect("read request");
        let request = String::from_utf8_lossy(&request[..bytes_read]);
        let (_, body) = request.split_once("\r\n\r\n").expect("headers/body");
        let payload: Value = serde_json::from_str(body).expect("parse request body");
        assert_eq!(payload["model"], "test-model");
        assert_eq!(payload["input"][0], expected_query);

        let body = serde_json::to_string(&json!({
            "data": [{ "embedding": vector }]
        }))
        .expect("serialize response");
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .expect("write response");
    });

    (format!("http://{address}/v1/embeddings"), handle)
}
