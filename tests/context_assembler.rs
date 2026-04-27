use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::Command;
use std::thread;

use async_trait::async_trait;
use mempal::context::{ContextRequest, assemble_context};
use mempal::core::anchor;
use mempal::core::db::Database;
use mempal::core::types::{
    AnchorKind, Drawer, KnowledgeStatus, KnowledgeTier, MemoryDomain, MemoryKind, Provenance,
    SourceType, TriggerHints,
};
use mempal::embed::Embedder;
use serde_json::{Value, json};
use tempfile::TempDir;

struct StubEmbedder {
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

fn new_db() -> (TempDir, Database) {
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

fn mempal_bin() -> String {
    env!("CARGO_BIN_EXE_mempal").to_string()
}

fn vector() -> Vec<f32> {
    vec![0.25; 384]
}

fn embedder() -> StubEmbedder {
    StubEmbedder { vector: vector() }
}

fn default_request(query: &str, cwd: &Path) -> ContextRequest {
    ContextRequest {
        query: query.to_string(),
        domain: MemoryDomain::Project,
        field: "general".to_string(),
        cwd: cwd.to_path_buf(),
        include_evidence: false,
        max_items: 12,
        dao_tian_limit: 1,
    }
}

fn knowledge_drawer(
    id: &str,
    tier: KnowledgeTier,
    status: KnowledgeStatus,
    statement: &str,
    content: &str,
) -> Drawer {
    Drawer {
        id: id.to_string(),
        content: content.to_string(),
        wing: "mempal".to_string(),
        room: Some("context".to_string()),
        source_file: Some(format!("knowledge://project/context/{id}")),
        source_type: SourceType::Manual,
        added_at: "1710000000".to_string(),
        chunk_index: Some(0),
        normalize_version: 1,
        importance: 3,
        memory_kind: MemoryKind::Knowledge,
        domain: MemoryDomain::Project,
        field: "general".to_string(),
        anchor_kind: AnchorKind::Repo,
        anchor_id: anchor::LEGACY_REPO_ANCHOR_ID.to_string(),
        parent_anchor_id: None,
        provenance: None,
        statement: Some(statement.to_string()),
        tier: Some(tier),
        status: Some(status),
        supporting_refs: vec!["drawer_supporting_ev".to_string()],
        counterexample_refs: Vec::new(),
        teaching_refs: Vec::new(),
        verification_refs: Vec::new(),
        scope_constraints: None,
        trigger_hints: None,
    }
}

fn evidence_drawer(id: &str, content: &str) -> Drawer {
    Drawer {
        id: id.to_string(),
        content: content.to_string(),
        wing: "mempal".to_string(),
        room: Some("context".to_string()),
        source_file: Some(format!("tests://context/{id}")),
        source_type: SourceType::Manual,
        added_at: "1710000000".to_string(),
        chunk_index: Some(0),
        normalize_version: 1,
        importance: 2,
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

fn insert_fixture(db: &Database, drawer: &Drawer) {
    db.insert_drawer(drawer).expect("insert drawer");
    db.insert_vector(&drawer.id, &vector())
        .expect("insert vector");
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

fn run_context_json(home: &Path, query: &str, extra_args: &[&str]) -> Value {
    let (endpoint, handle) = start_openai_embedding_stub(query, vector());
    write_cli_api_config(home, &endpoint);

    let mut args = vec!["context".to_string(), query.to_string()];
    args.extend(extra_args.iter().map(|arg| (*arg).to_string()));
    args.extend(["--format".to_string(), "json".to_string()]);

    let output = Command::new(mempal_bin())
        .args(args)
        .env("HOME", home)
        .output()
        .expect("run mempal context");
    assert!(
        output.status.success(),
        "context command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    handle.join().expect("join embedding stub");
    serde_json::from_slice(&output.stdout).expect("parse context json")
}

fn run_context_plain(home: &Path, query: &str, extra_args: &[&str]) -> String {
    let (endpoint, handle) = start_openai_embedding_stub(query, vector());
    write_cli_api_config(home, &endpoint);

    let mut args = vec!["context".to_string(), query.to_string()];
    args.extend(extra_args.iter().map(|arg| (*arg).to_string()));

    let output = Command::new(mempal_bin())
        .args(args)
        .env("HOME", home)
        .output()
        .expect("run mempal context");
    assert!(
        output.status.success(),
        "context command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    handle.join().expect("join embedding stub");
    String::from_utf8(output.stdout).expect("context stdout utf8")
}

#[tokio::test]
async fn test_context_groups_knowledge_by_tier_order() {
    let (tmp, db) = new_db();
    let items = [
        knowledge_drawer(
            "drawer_qi",
            KnowledgeTier::Qi,
            KnowledgeStatus::Promoted,
            "Use cargo test for verification.",
            "qi body debug failing build",
        ),
        knowledge_drawer(
            "drawer_shu",
            KnowledgeTier::Shu,
            KnowledgeStatus::Promoted,
            "Reproduce before patching.",
            "shu body debug failing build",
        ),
        knowledge_drawer(
            "drawer_dao_ren",
            KnowledgeTier::DaoRen,
            KnowledgeStatus::Promoted,
            "Software changes need executable feedback.",
            "dao ren body debug failing build",
        ),
        knowledge_drawer(
            "drawer_dao_tian",
            KnowledgeTier::DaoTian,
            KnowledgeStatus::Canonical,
            "Evidence precedes assertion.",
            "dao tian body debug failing build",
        ),
    ];
    for item in &items {
        insert_fixture(&db, item);
    }

    let mut request = default_request("debug failing build", tmp.path());
    request.field = "general".to_string();
    let pack = assemble_context(&db, &embedder(), request)
        .await
        .expect("assemble context");
    let names: Vec<_> = pack
        .sections
        .iter()
        .map(|section| section.name.as_str())
        .collect();
    assert_eq!(names, vec!["dao_tian", "dao_ren", "shu", "qi"]);
    for section in pack.sections {
        assert_eq!(section.items.len(), 1);
        assert!(!section.items[0].drawer_id.is_empty());
        assert!(!section.items[0].source_file.is_empty());
    }
}

#[tokio::test]
async fn test_context_default_caps_dao_tian_to_one() {
    let (tmp, db) = new_db();
    for item in [
        knowledge_drawer(
            "drawer_dao_tian_one",
            KnowledgeTier::DaoTian,
            KnowledgeStatus::Canonical,
            "Evidence precedes assertion.",
            "debug universal principle one",
        ),
        knowledge_drawer(
            "drawer_dao_tian_two",
            KnowledgeTier::DaoTian,
            KnowledgeStatus::Canonical,
            "Claims need source-backed verification.",
            "debug universal principle two",
        ),
        knowledge_drawer(
            "drawer_dao_ren",
            KnowledgeTier::DaoRen,
            KnowledgeStatus::Promoted,
            "Software changes need executable feedback.",
            "debug domain principle",
        ),
    ] {
        insert_fixture(&db, &item);
    }

    let pack = assemble_context(&db, &embedder(), default_request("debug", tmp.path()))
        .await
        .expect("assemble context");
    let dao_tian = pack
        .sections
        .iter()
        .find(|section| section.name == "dao_tian")
        .expect("dao_tian section");
    assert_eq!(dao_tian.items.len(), 1);
    assert!(
        pack.sections
            .iter()
            .any(|section| section.name == "dao_ren")
    );
}

#[tokio::test]
async fn test_context_max_items_still_caps_raised_dao_tian_limit() {
    let (tmp, db) = new_db();
    for item in [
        knowledge_drawer(
            "drawer_dao_tian_one",
            KnowledgeTier::DaoTian,
            KnowledgeStatus::Canonical,
            "Evidence precedes assertion.",
            "debug universal principle one",
        ),
        knowledge_drawer(
            "drawer_dao_tian_two",
            KnowledgeTier::DaoTian,
            KnowledgeStatus::Canonical,
            "Claims need source-backed verification.",
            "debug universal principle two",
        ),
    ] {
        insert_fixture(&db, &item);
    }

    let mut request = default_request("debug", tmp.path());
    request.max_items = 1;
    request.dao_tian_limit = 2;
    let pack = assemble_context(&db, &embedder(), request)
        .await
        .expect("assemble context");
    let item_count: usize = pack
        .sections
        .iter()
        .map(|section| section.items.len())
        .sum();
    assert_eq!(item_count, 1);
}

#[tokio::test]
async fn test_context_prefers_worktree_anchor_before_repo_and_global() {
    let (tmp, db) = new_db();
    let repo_path = tmp.path().join("repo");
    fs::create_dir_all(&repo_path).expect("create repo");
    Command::new("git")
        .arg("init")
        .current_dir(&repo_path)
        .output()
        .expect("git init");
    let derived = anchor::derive_anchor_from_cwd(Some(&repo_path)).expect("derive anchor");

    let mut worktree = knowledge_drawer(
        "drawer_worktree",
        KnowledgeTier::Shu,
        KnowledgeStatus::Promoted,
        "Worktree-local rule.",
        "local experiment worktree",
    );
    worktree.anchor_kind = AnchorKind::Worktree;
    worktree.anchor_id = derived.anchor_id.clone();
    worktree.parent_anchor_id = derived.parent_anchor_id.clone();

    let mut repo = knowledge_drawer(
        "drawer_repo",
        KnowledgeTier::Shu,
        KnowledgeStatus::Promoted,
        "Repo rule.",
        "local experiment repo",
    );
    repo.anchor_kind = AnchorKind::Repo;
    repo.anchor_id = derived.parent_anchor_id.clone().expect("repo anchor");

    let mut global = knowledge_drawer(
        "drawer_global",
        KnowledgeTier::Shu,
        KnowledgeStatus::Promoted,
        "Global rule.",
        "local experiment global",
    );
    global.domain = MemoryDomain::Global;
    global.anchor_kind = AnchorKind::Global;
    global.anchor_id = "global://default".to_string();

    for item in [&worktree, &repo, &global] {
        insert_fixture(&db, item);
    }

    let mut request = default_request("local experiment", &repo_path);
    request.max_items = 3;
    let pack = assemble_context(&db, &embedder(), request)
        .await
        .expect("assemble context");
    let shu = pack
        .sections
        .iter()
        .find(|section| section.name == "shu")
        .expect("shu section");
    let ids: Vec<_> = shu
        .items
        .iter()
        .map(|item| item.drawer_id.as_str())
        .collect();
    assert_eq!(ids, vec!["drawer_worktree", "drawer_repo", "drawer_global"]);
}

#[tokio::test]
async fn test_context_knowledge_item_uses_statement_before_content() {
    let (tmp, db) = new_db();
    insert_fixture(
        &db,
        &knowledge_drawer(
            "drawer_statement",
            KnowledgeTier::Shu,
            KnowledgeStatus::Promoted,
            "Reproduce before patching.",
            "Long rationale explaining why reproduction prevents false fixes.",
        ),
    );
    let pack = assemble_context(&db, &embedder(), default_request("debug", tmp.path()))
        .await
        .expect("assemble context");
    let text = &pack.sections[0].items[0].text;
    assert!(text.contains("Reproduce before patching."));
    assert!(!text.contains("Long rationale explaining"));
}

#[tokio::test]
async fn test_context_excludes_inactive_knowledge_statuses() {
    let (tmp, db) = new_db();
    for (id, status) in [
        ("drawer_candidate", KnowledgeStatus::Candidate),
        ("drawer_demoted", KnowledgeStatus::Demoted),
        ("drawer_retired", KnowledgeStatus::Retired),
        ("drawer_promoted", KnowledgeStatus::Promoted),
        ("drawer_canonical", KnowledgeStatus::Canonical),
    ] {
        insert_fixture(
            &db,
            &knowledge_drawer(id, KnowledgeTier::Shu, status, id, "debug status body"),
        );
    }
    let pack = assemble_context(&db, &embedder(), default_request("debug", tmp.path()))
        .await
        .expect("assemble context");
    let ids: Vec<_> = pack.sections[0]
        .items
        .iter()
        .map(|item| item.drawer_id.as_str())
        .collect();
    assert!(ids.contains(&"drawer_promoted"));
    assert!(ids.contains(&"drawer_canonical"));
    assert!(!ids.contains(&"drawer_candidate"));
    assert!(!ids.contains(&"drawer_demoted"));
    assert!(!ids.contains(&"drawer_retired"));
}

#[tokio::test]
async fn test_context_omits_evidence_by_default() {
    let (tmp, db) = new_db();
    insert_fixture(&db, &evidence_drawer("drawer_evidence", "observed failure"));
    let pack = assemble_context(
        &db,
        &embedder(),
        default_request("observed failure", tmp.path()),
    )
    .await
    .expect("assemble context");
    assert!(
        pack.sections
            .iter()
            .all(|section| section.name != "evidence")
    );
}

#[tokio::test]
async fn test_context_include_evidence_adds_evidence_section_after_qi() {
    let (tmp, db) = new_db();
    insert_fixture(
        &db,
        &knowledge_drawer(
            "drawer_qi",
            KnowledgeTier::Qi,
            KnowledgeStatus::Promoted,
            "Use cargo test.",
            "observed failure qi",
        ),
    );
    insert_fixture(&db, &evidence_drawer("drawer_evidence", "observed failure"));
    let mut request = default_request("observed failure", tmp.path());
    request.include_evidence = true;
    let pack = assemble_context(&db, &embedder(), request)
        .await
        .expect("assemble context");
    let names: Vec<_> = pack
        .sections
        .iter()
        .map(|section| section.name.as_str())
        .collect();
    assert_eq!(names, vec!["qi", "evidence"]);
    let evidence = pack
        .sections
        .iter()
        .find(|section| section.name == "evidence")
        .expect("evidence section");
    assert_eq!(evidence.items[0].text, "observed failure");
}

#[test]
fn test_context_json_output_exposes_stable_pack_shape() {
    let (tmp, db) = setup_cli_home();
    let mut drawer = knowledge_drawer(
        "drawer_trigger",
        KnowledgeTier::Shu,
        KnowledgeStatus::Promoted,
        "Debug by reproducing.",
        "debug trigger body",
    );
    drawer.trigger_hints = Some(TriggerHints {
        intent_tags: vec!["debugging".to_string()],
        workflow_bias: vec!["tdd".to_string()],
        tool_needs: vec!["cargo-test".to_string()],
    });
    insert_fixture(&db, &drawer);
    let value = run_context_json(tmp.path(), "debug", &[]);
    assert_eq!(value["query"], "debug");
    assert_eq!(value["domain"], "project");
    assert_eq!(value["field"], "general");
    assert!(value["anchors"].is_array());
    assert!(value["sections"].is_array());
    let item = &value["sections"][0]["items"][0];
    assert_eq!(item["drawer_id"], "drawer_trigger");
    assert_eq!(
        item["source_file"],
        "knowledge://project/context/drawer_trigger"
    );
    assert_eq!(item["text"], "Debug by reproducing.");
    assert_eq!(item["tier"], "shu");
    assert_eq!(item["status"], "promoted");
    assert_eq!(item["anchor_kind"], "repo");
    assert!(item["anchor_id"].is_string());
    assert_eq!(item["trigger_hints"]["intent_tags"][0], "debugging");
}

#[test]
fn test_cli_context_dao_tian_limit_zero_omits_section() {
    let (tmp, db) = setup_cli_home();
    for item in [
        knowledge_drawer(
            "drawer_dao_tian",
            KnowledgeTier::DaoTian,
            KnowledgeStatus::Canonical,
            "Evidence precedes assertion.",
            "debug universal principle",
        ),
        knowledge_drawer(
            "drawer_dao_ren",
            KnowledgeTier::DaoRen,
            KnowledgeStatus::Promoted,
            "Software changes need executable feedback.",
            "debug domain principle",
        ),
    ] {
        insert_fixture(&db, &item);
    }

    let value = run_context_json(tmp.path(), "debug", &["--dao-tian-limit", "0"]);
    let names: Vec<_> = value["sections"]
        .as_array()
        .expect("sections")
        .iter()
        .map(|section| section["name"].as_str().expect("section name"))
        .collect();
    assert!(!names.contains(&"dao_tian"));
    assert!(names.contains(&"dao_ren"));
}

#[test]
fn test_cli_context_dao_tian_limit_two_allows_two_items() {
    let (tmp, db) = setup_cli_home();
    for item in [
        knowledge_drawer(
            "drawer_dao_tian_one",
            KnowledgeTier::DaoTian,
            KnowledgeStatus::Canonical,
            "Evidence precedes assertion.",
            "debug universal principle one",
        ),
        knowledge_drawer(
            "drawer_dao_tian_two",
            KnowledgeTier::DaoTian,
            KnowledgeStatus::Canonical,
            "Claims need source-backed verification.",
            "debug universal principle two",
        ),
    ] {
        insert_fixture(&db, &item);
    }

    let value = run_context_json(tmp.path(), "debug", &["--dao-tian-limit", "2"]);
    let dao_tian = value["sections"]
        .as_array()
        .expect("sections")
        .iter()
        .find(|section| section["name"] == "dao_tian")
        .expect("dao_tian section");
    assert_eq!(dao_tian["items"].as_array().expect("items").len(), 2);
}

#[tokio::test]
async fn test_context_domain_and_field_filters_exclude_unrelated_knowledge() {
    let (tmp, db) = new_db();
    let mut target = knowledge_drawer(
        "drawer_skill_debugging",
        KnowledgeTier::Shu,
        KnowledgeStatus::Promoted,
        "Skill debugging rule.",
        "debug",
    );
    target.domain = MemoryDomain::Skill;
    target.field = "debugging".to_string();

    let mut wrong_domain = knowledge_drawer(
        "drawer_project_debugging",
        KnowledgeTier::Shu,
        KnowledgeStatus::Promoted,
        "Project debugging rule.",
        "debug",
    );
    wrong_domain.field = "debugging".to_string();

    let mut wrong_field = knowledge_drawer(
        "drawer_skill_writing",
        KnowledgeTier::Shu,
        KnowledgeStatus::Promoted,
        "Skill writing rule.",
        "debug",
    );
    wrong_field.domain = MemoryDomain::Skill;
    wrong_field.field = "writing".to_string();

    for drawer in [&target, &wrong_domain, &wrong_field] {
        insert_fixture(&db, drawer);
    }

    let mut request = default_request("debug", tmp.path());
    request.domain = MemoryDomain::Skill;
    request.field = "debugging".to_string();
    let pack = assemble_context(&db, &embedder(), request)
        .await
        .expect("assemble context");
    let ids: Vec<_> = pack.sections[0]
        .items
        .iter()
        .map(|item| item.drawer_id.as_str())
        .collect();
    assert_eq!(ids, vec!["drawer_skill_debugging"]);
}

#[tokio::test]
async fn test_field_taxonomy_does_not_restrict_custom_context_field() {
    let (tmp, db) = new_db();
    let mut target = knowledge_drawer(
        "drawer_compiler_design",
        KnowledgeTier::DaoRen,
        KnowledgeStatus::Promoted,
        "Compiler lowering should preserve source-level intent.",
        "compiler design custom field",
    );
    target.field = "compiler-design".to_string();
    insert_fixture(&db, &target);

    let mut request = default_request("compiler design", tmp.path());
    request.field = "compiler-design".to_string();
    let pack = assemble_context(&db, &embedder(), request)
        .await
        .expect("assemble context");
    assert_eq!(
        pack.sections[0].items[0].drawer_id,
        "drawer_compiler_design"
    );
}

#[test]
fn test_context_empty_result_exits_successfully() {
    let (tmp, _db) = setup_cli_home();
    let value = run_context_json(tmp.path(), "no such topic", &[]);
    assert_eq!(value["sections"].as_array().expect("sections").len(), 0);
}

#[test]
fn test_context_rejects_invalid_max_items() {
    let (tmp, _db) = setup_cli_home();
    let output = Command::new(mempal_bin())
        .args(["context", "debug", "--max-items", "0"])
        .env("HOME", tmp.path())
        .output()
        .expect("run mempal context");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--max-items"));
    assert!(stderr.contains("greater than 0"));
}

#[tokio::test]
async fn test_context_assembler_returns_typed_pack() {
    let (tmp, db) = new_db();
    insert_fixture(
        &db,
        &knowledge_drawer(
            "drawer_typed",
            KnowledgeTier::Shu,
            KnowledgeStatus::Promoted,
            "Typed context pack.",
            "debug typed body",
        ),
    );
    let pack = assemble_context(&db, &embedder(), default_request("debug", tmp.path()))
        .await
        .expect("assemble context");
    assert_eq!(pack.query, "debug");
    assert_eq!(pack.sections[0].name, "shu");
    assert_eq!(pack.sections[0].items[0].drawer_id, "drawer_typed");
}

#[test]
fn test_context_assembler_does_not_bump_schema() {
    let (tmp, db) = setup_cli_home();
    assert_eq!(db.schema_version().expect("schema"), 8);
    let before_tables = table_names(&db);
    let _ = run_context_plain(tmp.path(), "debug", &[]);
    assert_eq!(db.schema_version().expect("schema"), 8);
    assert_eq!(table_names(&db), before_tables);
}

fn table_names(db: &Database) -> Vec<String> {
    let mut statement = db
        .conn()
        .prepare(
            "SELECT name FROM sqlite_master WHERE type IN ('table', 'virtual table') ORDER BY name",
        )
        .expect("prepare table names");
    statement
        .query_map([], |row| row.get::<_, String>(0))
        .expect("query table names")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect table names")
}
