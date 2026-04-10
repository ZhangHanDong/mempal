use std::path::{Path, PathBuf};
use std::sync::Arc;

use mempal_core::{
    db::Database,
    types::{Drawer, SourceType},
};
use mempal_embed::{Embedder, EmbedderFactory};
use mempal_mcp::MempalMcpServer;
use rmcp::{ServiceExt, model::CallToolRequestParams};
use serde_json::Value;
use tempfile::tempdir;

#[derive(Default)]
struct TestEmbedder;

#[async_trait::async_trait]
impl Embedder for TestEmbedder {
    async fn embed(
        &self,
        texts: &[&str],
    ) -> std::result::Result<Vec<Vec<f32>>, mempal_embed::EmbedError> {
        Ok(texts.iter().map(|text| fake_embedding(text)).collect())
    }

    fn dimensions(&self) -> usize {
        384
    }

    fn name(&self) -> &str {
        "test"
    }
}

#[derive(Clone, Default)]
struct TestEmbedderFactory;

#[async_trait::async_trait]
impl EmbedderFactory for TestEmbedderFactory {
    async fn build(&self) -> std::result::Result<Box<dyn Embedder>, mempal_embed::EmbedError> {
        Ok(Box::new(TestEmbedder))
    }
}

fn fake_embedding(text: &str) -> Vec<f32> {
    let mut embedding = vec![0.0_f32; 384];
    for (index, byte) in text.bytes().enumerate() {
        embedding[index % 384] += f32::from(byte) / 255.0;
    }
    embedding
}

fn open_db(path: &Path) -> Database {
    Database::open(path).expect("database should open")
}

fn insert_drawer(db: &Database, id: &str, content: &str, wing: &str, room: Option<&str>) {
    db.insert_drawer(&Drawer {
        id: id.to_string(),
        content: content.to_string(),
        wing: wing.to_string(),
        room: room.map(ToOwned::to_owned),
        source_file: Some(format!("/tmp/{id}.md")),
        source_type: SourceType::Project,
        added_at: "1712640000".to_string(),
        chunk_index: Some(0),
    })
    .expect("drawer insert should succeed");

    let vector_json =
        serde_json::to_string(&fake_embedding(content)).expect("vector JSON should serialize");
    db.conn()
        .execute(
            "INSERT INTO drawer_vectors (id, embedding) VALUES (?1, vec_f32(?2))",
            (id, vector_json.as_str()),
        )
        .expect("vector insert should succeed");
}

async fn spawn_server(
    db_path: PathBuf,
) -> anyhow::Result<rmcp::service::RunningService<rmcp::RoleClient, ()>> {
    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let server = MempalMcpServer::new_with_factory(db_path, Arc::new(TestEmbedderFactory));
    tokio::spawn(async move {
        let service = server.serve(server_transport).await?;
        service.waiting().await?;
        anyhow::Ok(())
    });

    let client = ().serve(client_transport).await?;
    Ok(client)
}

#[tokio::test]
async fn test_mcp_server_start() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let client = spawn_server(dir.path().join("palace.db")).await?;

    let tools = client.list_all_tools().await?;
    let names = tools
        .iter()
        .map(|tool| tool.name.as_ref())
        .collect::<Vec<_>>();

    assert_eq!(names.len(), 5);
    assert!(names.contains(&"mempal_status"));
    assert!(names.contains(&"mempal_search"));
    assert!(names.contains(&"mempal_ingest"));
    assert!(names.contains(&"mempal_delete"));
    assert!(names.contains(&"mempal_taxonomy"));

    client.cancel().await?;
    Ok(())
}

/// Guards the wing/room field docs on SearchRequest that teach clients not
/// to guess wing names (which silently return zero results). If someone
/// strips the `///` comments off `SearchRequest.wing` the JSON schema loses
/// the OMIT guidance and this test fails.
#[tokio::test]
async fn test_mempal_search_schema_warns_about_wing_guessing() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let client = spawn_server(dir.path().join("palace.db")).await?;

    let tools = client.list_all_tools().await?;
    let search_tool = tools
        .iter()
        .find(|tool| tool.name.as_ref() == "mempal_search")
        .expect("mempal_search tool should be exposed");
    let schema_json = serde_json::to_string(search_tool.input_schema.as_ref())?;
    assert!(
        schema_json.contains("OMIT"),
        "mempal_search wing/room schema should warn 'OMIT' for global search; got: {schema_json}"
    );
    assert!(
        schema_json.contains("global search"),
        "mempal_search schema should mention 'global search' fallback; got: {schema_json}"
    );

    client.cancel().await?;
    Ok(())
}

/// Guards the primary path by which AI clients discover the memory protocol:
/// the MCP `initialize` response's `instructions` field. If someone removes
/// `.with_instructions(MEMORY_PROTOCOL)` from server.rs `get_info`, this test
/// fails loudly. Without it, all other tests would stay green.
#[tokio::test]
async fn test_mcp_initialize_exposes_memory_protocol() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let client = spawn_server(dir.path().join("palace.db")).await?;

    let server_info = client
        .peer_info()
        .expect("client should have received server info after initialize");
    let instructions = server_info
        .instructions
        .as_deref()
        .expect("server info should include instructions");

    assert!(
        instructions.contains("MEMPAL MEMORY PROTOCOL"),
        "instructions should contain the memory protocol header, got: {instructions}"
    );
    assert!(
        instructions.contains("VERIFY BEFORE ASSERTING"),
        "instructions should include the verify rule"
    );
    assert!(
        instructions.contains("FIRST-TIME SETUP"),
        "instructions should include the FIRST-TIME SETUP rule teaching clients to call mempal_status"
    );
    assert!(
        instructions.len() > 1000,
        "instructions should be the full protocol (>1000 chars), got {} chars",
        instructions.len()
    );

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn test_mcp_search() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("palace.db");
    let db = open_db(&db_path);
    insert_drawer(
        &db,
        "drawer_auth",
        "We chose Clerk for the auth decision.",
        "myapp",
        Some("auth"),
    );
    let client = spawn_server(db_path).await?;

    let result = client
        .call_tool(
            CallToolRequestParams::new("mempal_search").with_arguments(
                serde_json::json!({
                    "query": "auth decision clerk",
                    "top_k": 5
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await?;

    let payload = result
        .structured_content
        .expect("structured result should exist");
    let first = payload
        .get("results")
        .and_then(Value::as_array)
        .and_then(|results| results.first())
        .expect("search should return one result");
    assert!(first.get("drawer_id").is_some());
    assert!(first.get("source_file").is_some());
    assert!(first.get("similarity").is_some());

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn test_mcp_ingest() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("palace.db");
    let client = spawn_server(db_path.clone()).await?;

    let result = client
        .call_tool(
            CallToolRequestParams::new("mempal_ingest").with_arguments(
                serde_json::json!({
                    "content": "decided to use Clerk",
                    "wing": "myapp",
                    "room": "auth",
                    "source": "/tmp/decision.md"
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await?;

    let payload = result
        .structured_content
        .expect("structured result should exist");
    let drawer_id = payload
        .get("drawer_id")
        .and_then(Value::as_str)
        .expect("drawer_id should be returned");

    let db = open_db(&db_path);
    let count: i64 = db.conn().query_row(
        "SELECT COUNT(*) FROM drawers WHERE id = ?1",
        [drawer_id],
        |row| row.get(0),
    )?;
    assert_eq!(count, 1);

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn test_mcp_ingest_defaults_source() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("palace.db");
    let client = spawn_server(db_path.clone()).await?;

    let result = client
        .call_tool(
            CallToolRequestParams::new("mempal_ingest").with_arguments(
                serde_json::json!({
                    "content": "decided to use Clerk",
                    "wing": "myapp",
                    "room": "auth"
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await?;

    let payload = result
        .structured_content
        .expect("structured result should exist");
    let drawer_id = payload
        .get("drawer_id")
        .and_then(Value::as_str)
        .expect("drawer_id should be returned");

    let db = open_db(&db_path);
    let source_file: Option<String> = db.conn().query_row(
        "SELECT source_file FROM drawers WHERE id = ?1",
        [drawer_id],
        |row| row.get(0),
    )?;
    assert_eq!(
        source_file.as_deref(),
        Some(format!("mempal://drawer/{drawer_id}").as_str())
    );

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn test_mcp_status_and_taxonomy() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("palace.db");
    let db = open_db(&db_path);
    insert_drawer(&db, "drawer_auth", "auth decision", "myapp", Some("auth"));
    db.conn().execute(
        "INSERT INTO taxonomy (wing, room, display_name, keywords) VALUES (?1, ?2, ?3, ?4)",
        ("myapp", "auth", "auth", "[\"auth\",\"login\"]"),
    )?;
    let client = spawn_server(db_path).await?;

    let status = client
        .call_tool(CallToolRequestParams::new("mempal_status"))
        .await?;
    let status_payload = status
        .structured_content
        .expect("status payload should exist");
    assert_eq!(
        status_payload.get("schema_version").and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        status_payload.get("drawer_count").and_then(Value::as_i64),
        Some(1)
    );
    // Fallback path: if a client ignores initialize.instructions, AIs can still
    // retrieve the protocol via mempal_status. Guard both payload fields.
    let memory_protocol = status_payload
        .get("memory_protocol")
        .and_then(Value::as_str)
        .expect("status should expose memory_protocol");
    assert!(
        memory_protocol.contains("MEMPAL MEMORY PROTOCOL"),
        "status.memory_protocol should contain the protocol header"
    );
    let aaak_spec = status_payload
        .get("aaak_spec")
        .and_then(Value::as_str)
        .expect("status should expose aaak_spec");
    assert!(
        aaak_spec.contains("AAAK"),
        "status.aaak_spec should contain the format name"
    );

    let taxonomy = client
        .call_tool(
            CallToolRequestParams::new("mempal_taxonomy").with_arguments(
                serde_json::json!({
                    "action": "list"
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await?;
    let taxonomy_payload = taxonomy
        .structured_content
        .expect("taxonomy payload should exist");
    assert_eq!(
        taxonomy_payload
            .get("entries")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(1)
    );

    client.cancel().await?;
    Ok(())
}
