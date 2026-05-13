use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Output};
use std::thread;

use mempal::core::db::Database;
use mempal::core::types::{
    KnowledgeCardFilter, KnowledgeEvidenceRole, KnowledgeStatus, MemoryKind, RuntimeAdoptionFilter,
};
use serde_json::{Value, json};
use tempfile::TempDir;

fn mempal_bin() -> String {
    env!("CARGO_BIN_EXE_mempal").to_string()
}

fn setup_cli_home() -> TempDir {
    let tmp = TempDir::new().expect("tempdir");
    fs::create_dir_all(tmp.path().join(".mempal")).expect("create .mempal");
    tmp
}

fn run_mempal(home: &Path, args: &[&str]) -> Output {
    Command::new(mempal_bin())
        .env("HOME", home)
        .args(args)
        .output()
        .expect("run mempal")
}

fn stdout_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("parse stdout json")
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

fn vector() -> Vec<f32> {
    vec![0.25; 384]
}

fn start_embedding_stub(
    expected_inputs: usize,
    expected_fragment: &str,
) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind embedding stub");
    listener
        .set_nonblocking(true)
        .expect("set embedding stub nonblocking");
    let address = listener.local_addr().expect("local addr");
    let expected_fragment = expected_fragment.to_string();

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
        let mut request = [0_u8; 8192];
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
        assert_eq!(input.len(), expected_inputs);
        assert!(
            input.iter().any(|value| value
                .as_str()
                .is_some_and(|raw| raw.contains(&expected_fragment))),
            "expected embedding input to contain fragment {expected_fragment:?}, got {input:?}"
        );

        let body = serde_json::to_string(&json!({
            "data": input.iter().map(|_| json!({ "embedding": vector() })).collect::<Vec<_>>()
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
    fs::write(
        home.join(".mempal").join("config.toml"),
        format!(
            "[embed]\nbackend = \"api\"\napi_endpoint = \"{endpoint}\"\napi_model = \"test-model\"\n"
        ),
    )
    .expect("write cli config");
}

fn run_with_embedding_stub(
    home: &Path,
    args: &[&str],
    expected_inputs: usize,
    expected_fragment: &str,
) -> Output {
    let (endpoint, handle) = start_embedding_stub(expected_inputs, expected_fragment);
    write_cli_api_config(home, &endpoint);
    let output = run_mempal(home, args);
    handle.join().expect("join embedding stub");
    output
}

#[test]
fn test_cli_self_evolution_replay_research_to_context_to_adoption() {
    let home = setup_cli_home();
    let report_path = home.path().join("research-report.json");
    fs::write(
        &report_path,
        json!({
            "report_id": "research_p71_loop",
            "title": "Self-evolution loop replay",
            "sources": [{"id": "src_1", "url": "https://example.invalid/p71"}],
            "findings": [
                {"summary": "Research findings must enter memory as evidence before runtime adoption."}
            ],
            "candidate_insights": [
                {"statement": "Research-backed tool guidance should be promoted only through evidence gates."}
            ]
        })
        .to_string(),
    )
    .expect("write report");

    let ingest = run_with_embedding_stub(
        home.path(),
        &[
            "phase3",
            "research-ingest-plan",
            report_path.to_str().expect("report path"),
            "--execute",
            "--format",
            "json",
        ],
        1,
        "Research findings must enter memory as evidence before runtime adoption.",
    );
    assert!(
        ingest.status.success(),
        "ingest failed: {}",
        stderr_text(&ingest)
    );
    let ingest_json = stdout_json(&ingest);
    assert_eq!(ingest_json["valid"], true);
    assert_eq!(ingest_json["writes"], true);
    assert_eq!(ingest_json["created_count"], 1);
    let evidence_id = ingest_json["evidence_drawers"][0]["drawer_id"]
        .as_str()
        .expect("evidence drawer id");

    let statement = "Research-backed tool guidance should be promoted only through evidence gates.";
    let create = run_mempal(
        home.path(),
        &[
            "knowledge-card",
            "create",
            "--id",
            "card_p71_loop",
            "--statement",
            statement,
            "--content",
            "P71 replay card distilled from research evidence.",
            "--tier",
            "qi",
            "--status",
            "candidate",
            "--field",
            "general",
            "--anchor-kind",
            "repo",
            "--anchor-id",
            "repo://legacy",
            "--intent-tag",
            "research",
            "--workflow-bias",
            "evidence-gate",
            "--tool-need",
            "mempal",
            "--format",
            "json",
        ],
    );
    assert!(
        create.status.success(),
        "card create failed: {}",
        stderr_text(&create)
    );
    assert_eq!(stdout_json(&create)["status"], "candidate");

    let link = run_mempal(
        home.path(),
        &[
            "knowledge-card",
            "link",
            "card_p71_loop",
            evidence_id,
            "--role",
            "supporting",
        ],
    );
    assert!(
        link.status.success(),
        "card link failed: {}",
        stderr_text(&link)
    );

    let promote = run_mempal(
        home.path(),
        &[
            "knowledge-card",
            "promote",
            "card_p71_loop",
            "--status",
            "promoted",
            "--verification-ref",
            evidence_id,
            "--reason",
            "P71 replay verified the research-backed card.",
            "--format",
            "json",
        ],
    );
    assert!(
        promote.status.success(),
        "card promote failed: {}",
        stderr_text(&promote)
    );
    let promote_json = stdout_json(&promote);
    assert_eq!(promote_json["old_status"], "candidate");
    assert_eq!(promote_json["new_status"], "promoted");
    assert_eq!(promote_json["gate"]["allowed"], true);

    let context = run_with_embedding_stub(
        home.path(),
        &[
            "context",
            "research-backed tool guidance",
            "--include-cards",
            "--format",
            "json",
        ],
        1,
        "research-backed tool guidance",
    );
    assert!(
        context.status.success(),
        "context failed: {}",
        stderr_text(&context)
    );
    let context_json = stdout_json(&context);
    let items = context_json["sections"]
        .as_array()
        .expect("sections")
        .iter()
        .flat_map(|section| section["items"].as_array().expect("items"))
        .collect::<Vec<_>>();
    let card_item = items
        .iter()
        .find(|item| item["card_id"] == "card_p71_loop")
        .expect("context contains promoted card");
    assert_eq!(card_item["text"], statement);
    assert!(
        card_item["evidence_citations"]
            .as_array()
            .expect("citations")
            .iter()
            .any(|citation| citation["evidence_drawer_id"] == evidence_id
                && citation["role"] == "supporting")
    );

    let adoption = run_mempal(
        home.path(),
        &[
            "phase3",
            "adoption",
            "record-checked",
            "--track",
            "card_context",
            "--signal",
            "accepted",
            "--feature",
            "include_cards",
            "--query",
            "research-backed tool guidance",
            "--card-id",
            "card_p71_loop",
            "--research-report-id",
            "research_p71_loop",
            "--note",
            "P71 replay context returned the promoted research-backed card.",
            "--format",
            "json",
        ],
    );
    assert!(
        adoption.status.success(),
        "record-checked failed: {}",
        stderr_text(&adoption)
    );
    let adoption_json = stdout_json(&adoption);
    assert_eq!(adoption_json["writes"], true);
    assert_eq!(adoption_json["blocked"], false);
    assert_eq!(adoption_json["record_quality"]["quality"], "ready");

    let db = Database::open(&home.path().join(".mempal/palace.db")).expect("open db");
    let evidence = db
        .get_drawer(evidence_id)
        .expect("load evidence")
        .expect("evidence exists");
    assert_eq!(evidence.memory_kind, MemoryKind::Evidence);
    let card = db
        .get_knowledge_card("card_p71_loop")
        .expect("load card")
        .expect("card exists");
    assert_eq!(card.status, KnowledgeStatus::Promoted);
    let links = db
        .knowledge_evidence_links("card_p71_loop")
        .expect("card links");
    assert!(
        links
            .iter()
            .any(|link| link.evidence_drawer_id == evidence_id
                && link.role == KnowledgeEvidenceRole::Supporting)
    );
    assert!(
        links
            .iter()
            .any(|link| link.evidence_drawer_id == evidence_id
                && link.role == KnowledgeEvidenceRole::Verification)
    );
    assert_eq!(
        db.knowledge_events("card_p71_loop")
            .expect("card events")
            .len(),
        1
    );
    let adoption_events = db
        .list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
        .expect("adoption events");
    assert_eq!(adoption_events.len(), 1);
    assert_eq!(adoption_events[0].feature, "include_cards");
}

#[test]
fn test_cli_self_evolution_replay_invalid_research_no_artifacts() {
    let home = setup_cli_home();
    let report_path = home.path().join("bad-research-report.json");
    fs::write(&report_path, "{}").expect("write invalid report");

    let output = run_mempal(
        home.path(),
        &[
            "phase3",
            "research-ingest-plan",
            report_path.to_str().expect("report path"),
            "--execute",
            "--format",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "invalid plan should return a report: {}",
        stderr_text(&output)
    );
    let report = stdout_json(&output);
    assert_eq!(report["valid"], false);
    assert_eq!(report["writes"], false);

    let db = Database::open(&home.path().join(".mempal/palace.db")).expect("open db");
    assert_eq!(db.top_drawers(10).expect("drawers").len(), 0);
    assert!(
        db.list_knowledge_cards(&KnowledgeCardFilter::default())
            .expect("cards")
            .is_empty()
    );
    assert!(
        db.list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
            .expect("adoption events")
            .is_empty()
    );
}
