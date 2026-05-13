use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::thread;

use mempal::core::db::Database;
use mempal::core::types::{
    AnchorKind, Drawer, KnowledgeCard, KnowledgeEvidenceLink, KnowledgeEvidenceRole,
    KnowledgeStatus, KnowledgeTier, MemoryDomain, MemoryKind, Provenance, RuntimeAdoptionEvent,
    RuntimeAdoptionFilter, RuntimeAdoptionSignal, RuntimeAdoptionTrack, SourceType,
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

fn run_mempal(home: &TempDir, args: &[&str]) -> std::process::Output {
    Command::new(mempal_bin())
        .args(args)
        .env("HOME", home.path())
        .output()
        .expect("run mempal")
}

fn record_card_context_acceptance(home: &TempDir, id: &str) {
    let output = run_mempal(
        home,
        &[
            "phase3",
            "adoption",
            "record",
            "--id",
            id,
            "--track",
            "card_context",
            "--signal",
            "accepted",
            "--feature",
            "include_cards",
            "--query",
            "skill trigger context",
        ],
    );
    assert!(
        output.status.success(),
        "record failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn insert_test_card(home: &TempDir, card_id: &str) {
    let db = Database::open(&home.path().join(".mempal/palace.db")).expect("open db");
    db.insert_knowledge_card(&KnowledgeCard {
        id: card_id.to_string(),
        statement: format!("Statement for {card_id}."),
        content: format!("Content for {card_id}."),
        tier: KnowledgeTier::Shu,
        status: KnowledgeStatus::Promoted,
        domain: MemoryDomain::Project,
        field: "general".to_string(),
        anchor_kind: AnchorKind::Repo,
        anchor_id: "repo://mempal".to_string(),
        parent_anchor_id: None,
        scope_constraints: None,
        trigger_hints: None,
        created_at: "1713000000".to_string(),
        updated_at: "1713000000".to_string(),
    })
    .expect("insert test card");
}

fn evidence_drawer(id: &str, content: &str) -> Drawer {
    Drawer {
        id: id.to_string(),
        content: content.to_string(),
        wing: "mempal".to_string(),
        room: Some("phase3".to_string()),
        source_file: Some(format!("tests://phase3/{id}")),
        source_type: SourceType::Manual,
        added_at: "1710000000".to_string(),
        chunk_index: Some(0),
        normalize_version: 1,
        importance: 2,
        memory_kind: MemoryKind::Evidence,
        domain: MemoryDomain::Project,
        field: "general".to_string(),
        anchor_kind: AnchorKind::Repo,
        anchor_id: "repo://mempal".to_string(),
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

fn insert_test_card_with_evidence(home: &TempDir, card_id: &str, evidence_id: &str) {
    let db = Database::open(&home.path().join(".mempal/palace.db")).expect("open db");
    let drawer = evidence_drawer(evidence_id, "card context evidence");
    db.insert_drawer(&drawer).expect("insert evidence drawer");
    db.insert_vector(evidence_id, &vec![0.25; 384])
        .expect("insert evidence vector");
    drop(db);
    insert_test_card(home, card_id);
    let db = Database::open(&home.path().join(".mempal/palace.db")).expect("open db");
    db.insert_knowledge_evidence_link(&KnowledgeEvidenceLink {
        id: format!("link_{card_id}_{evidence_id}"),
        card_id: card_id.to_string(),
        evidence_drawer_id: evidence_id.to_string(),
        role: KnowledgeEvidenceRole::Supporting,
        note: None,
        created_at: "1710000000".to_string(),
    })
    .expect("insert card evidence link");
}

fn start_openai_embedding_stub(query: &str) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind embedding stub");
    listener
        .set_nonblocking(true)
        .expect("set embedding stub nonblocking");
    let address = listener.local_addr().expect("local addr");
    let query = query.to_string();
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
        let request = String::from_utf8_lossy(&request[..bytes_read]);
        let (_, body) = request
            .split_once("\r\n\r\n")
            .expect("request should contain JSON body");
        let payload: Value = serde_json::from_str(body).expect("parse embedding request");
        assert_eq!(payload["input"][0], query);
        let body = serde_json::to_string(&json!({
            "data": [{ "embedding": vec![0.25; 384] }]
        }))
        .expect("serialize embedding response");
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

fn write_cli_api_config(home: &TempDir, endpoint: &str) {
    fs::write(
        home.path().join(".mempal/config.toml"),
        format!(
            "[embed]\nbackend = \"api\"\napi_endpoint = \"{endpoint}\"\napi_model = \"test-model\"\n"
        ),
    )
    .expect("write config");
}

#[test]
fn test_runtime_adoption_event_roundtrip_db() {
    let tmp = TempDir::new().expect("tempdir");
    let db = Database::open(&tmp.path().join("palace.db")).expect("open db");
    assert_eq!(db.schema_version().expect("schema version"), 9);

    let event = RuntimeAdoptionEvent {
        id: "adoption_test".to_string(),
        track: RuntimeAdoptionTrack::RuntimeAdoption,
        signal: RuntimeAdoptionSignal::Accepted,
        feature: "context-pack".to_string(),
        query: Some("how should the agent choose skills?".to_string()),
        context_hash: Some("ctx123".to_string()),
        card_id: None,
        evaluator_id: None,
        research_report_id: None,
        note: Some("agent used the context pack".to_string()),
        metadata: Some(json!({"source": "test"})),
        created_at: "1777710000".to_string(),
    };
    db.insert_runtime_adoption_event(&event)
        .expect("insert adoption event");

    let events = db
        .list_runtime_adoption_events(
            &RuntimeAdoptionFilter {
                track: Some(RuntimeAdoptionTrack::RuntimeAdoption),
                feature: Some("context-pack".to_string()),
            },
            10,
        )
        .expect("list adoption events");
    assert_eq!(events, vec![event]);
}

#[test]
fn test_cli_phase3_adoption_record_stats_and_gate() {
    let home = setup_cli_home();
    for i in 0..3 {
        let id = format!("card_context_accept_{i}");
        let output = run_mempal(
            &home,
            &[
                "phase3",
                "adoption",
                "record",
                "--id",
                &id,
                "--track",
                "card_context",
                "--signal",
                "accepted",
                "--feature",
                "include_cards",
                "--query",
                "skill trigger context",
            ],
        );
        assert!(
            output.status.success(),
            "record failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let stats = run_mempal(
        &home,
        &[
            "phase3",
            "adoption",
            "stats",
            "--track",
            "card_context",
            "--format",
            "json",
        ],
    );
    assert!(
        stats.status.success(),
        "stats failed: {}",
        String::from_utf8_lossy(&stats.stderr)
    );
    let stats_json: Value = serde_json::from_slice(&stats.stdout).expect("stats json");
    assert_eq!(stats_json["accepted"], 3);
    assert_eq!(stats_json["rollbacks"], 0);

    let gate = run_mempal(
        &home,
        &["phase3", "gate", "card-context-default", "--format", "json"],
    );
    assert!(
        gate.status.success(),
        "gate failed: {}",
        String::from_utf8_lossy(&gate.stderr)
    );
    let gate_json: Value = serde_json::from_slice(&gate.stdout).expect("gate json");
    assert_eq!(gate_json["ready"], true);
    assert_eq!(gate_json["required_track"], "card_context");
}

#[test]
fn test_cli_phase3_readiness_card_context_default_ready() {
    let home = setup_cli_home();
    for i in 0..3 {
        let id = format!("readiness_accept_{i}");
        let output = run_mempal(
            &home,
            &[
                "phase3",
                "adoption",
                "record",
                "--id",
                &id,
                "--track",
                "card_context",
                "--signal",
                "accepted",
                "--feature",
                "include_cards",
                "--query",
                "skill trigger context",
            ],
        );
        assert!(
            output.status.success(),
            "record failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let output = run_mempal(
        &home,
        &[
            "phase3",
            "readiness",
            "card-context-default",
            "--format",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "readiness failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("readiness json");
    assert_eq!(report["writes"], false);
    assert_eq!(report["ready"], true);
    assert_eq!(report["decision"], "eligible_for_future_default_spec");
    assert_eq!(report["review"]["stats"]["accepted"], 3);

    let db = Database::open(&home.path().join(".mempal/palace.db")).expect("open db");
    let events = db
        .list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
        .expect("list events");
    assert_eq!(events.len(), 3);
}

#[test]
fn test_cli_phase3_readiness_card_context_default_blocks_without_evidence() {
    let home = setup_cli_home();
    let output = run_mempal(
        &home,
        &[
            "phase3",
            "readiness",
            "card-context-default",
            "--format",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "readiness failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("readiness json");
    assert_eq!(report["writes"], false);
    assert_eq!(report["ready"], false);
    assert_eq!(report["decision"], "keep_opt_in");
    let reasons = report["reasons"].as_array().expect("reasons");
    assert!(reasons.iter().any(|reason| {
        reason
            .as_str()
            .expect("reason")
            .contains("insufficient accepted evidence")
    }));

    let db = Database::open(&home.path().join(".mempal/palace.db")).expect("open db");
    let events = db
        .list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
        .expect("list events");
    assert!(events.is_empty());
}

#[test]
fn test_cli_phase3_default_proposal_card_context_ready() {
    let home = setup_cli_home();
    for i in 0..3 {
        record_card_context_acceptance(&home, &format!("proposal_accept_{i}"));
    }

    let output = run_mempal(
        &home,
        &[
            "phase3",
            "default-proposal",
            "card-context",
            "--rollback-criterion",
            "rollback on contradiction or user-visible degradation",
            "--format",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "proposal failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("proposal json");
    assert_eq!(report["writes"], false);
    assert_eq!(report["candidate"], "card-context");
    assert_eq!(report["proposal_ready"], true);
    assert_eq!(report["decision"], "eligible_for_default_on_spec");
    assert_eq!(report["readiness"]["ready"], true);

    let db = Database::open(&home.path().join(".mempal/palace.db")).expect("open db");
    let events = db
        .list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
        .expect("list events");
    assert_eq!(events.len(), 3);
}

#[test]
fn test_cli_phase3_default_proposal_requires_rollback_criteria() {
    let home = setup_cli_home();
    for i in 0..3 {
        record_card_context_acceptance(&home, &format!("proposal_no_rollback_{i}"));
    }

    let output = run_mempal(
        &home,
        &[
            "phase3",
            "default-proposal",
            "card-context",
            "--format",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "proposal failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("proposal json");
    assert_eq!(report["proposal_ready"], false);
    let reasons = report["reasons"].as_array().expect("reasons");
    assert!(reasons.iter().any(|reason| {
        reason
            .as_str()
            .is_some_and(|value| value.contains("rollback criteria are required"))
    }));
}

#[test]
fn test_cli_phase3_default_proposal_blocks_without_readiness() {
    let home = setup_cli_home();
    let output = run_mempal(
        &home,
        &[
            "phase3",
            "default-proposal",
            "card-context",
            "--rollback-criterion",
            "rollback on contradiction or user-visible degradation",
            "--format",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "proposal failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("proposal json");
    assert_eq!(report["proposal_ready"], false);
    assert_eq!(report["readiness"]["ready"], false);
}

#[test]
fn test_cli_phase3_default_proposal_keeps_context_cards_opt_in() {
    let home = setup_cli_home();
    insert_test_card_with_evidence(&home, "card_default_off", "drawer_default_off_evidence");
    for i in 0..3 {
        record_card_context_acceptance(&home, &format!("proposal_default_off_{i}"));
    }
    let proposal = run_mempal(
        &home,
        &[
            "phase3",
            "default-proposal",
            "card-context",
            "--rollback-criterion",
            "rollback on contradiction or user-visible degradation",
            "--format",
            "json",
        ],
    );
    assert!(
        proposal.status.success(),
        "proposal failed: {}",
        String::from_utf8_lossy(&proposal.stderr)
    );

    let query = "card-aware";
    let (endpoint, handle) = start_openai_embedding_stub(query);
    write_cli_api_config(&home, &endpoint);
    let context = run_mempal(&home, &["context", query, "--format", "json"]);
    assert!(
        context.status.success(),
        "context failed: {}",
        String::from_utf8_lossy(&context.stderr)
    );
    handle.join().expect("join embedding stub");
    let context: Value = serde_json::from_slice(&context.stdout).expect("context json");
    let has_card = context["sections"]
        .as_array()
        .expect("sections")
        .iter()
        .flat_map(|section| section["items"].as_array().expect("items"))
        .any(|item| item["card_id"] == "card_default_off");
    assert!(!has_card, "cards should remain opt-in by default");
}

#[test]
fn test_cli_phase3_default_proposal_rejects_unknown_candidate() {
    let home = setup_cli_home();
    let output = run_mempal(
        &home,
        &["phase3", "default-proposal", "unknown", "--format", "json"],
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unsupported phase3 default proposal candidate"));
}

#[test]
fn test_cli_phase3_readiness_card_context_default_blocks_rollback() {
    let home = setup_cli_home();
    for (id, signal) in [
        ("readiness_accept_1", "accepted"),
        ("readiness_accept_2", "accepted"),
        ("readiness_accept_3", "accepted"),
        ("readiness_rollback_1", "rollback"),
    ] {
        let output = run_mempal(
            &home,
            &[
                "phase3",
                "adoption",
                "record",
                "--id",
                id,
                "--track",
                "card_context",
                "--signal",
                signal,
                "--feature",
                "include_cards",
            ],
        );
        assert!(
            output.status.success(),
            "record failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let output = run_mempal(
        &home,
        &[
            "phase3",
            "readiness",
            "card-context-default",
            "--format",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "readiness failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("readiness json");
    assert_eq!(report["ready"], false);
    assert_eq!(report["decision"], "keep_opt_in");
    let reasons = report["reasons"].as_array().expect("reasons");
    assert!(reasons.iter().any(|reason| {
        reason
            .as_str()
            .expect("reason")
            .contains("rollback evidence")
    }));
}

#[test]
fn test_cli_phase3_readiness_rejects_unknown_candidate() {
    let home = setup_cli_home();
    let output = run_mempal(
        &home,
        &["phase3", "readiness", "unknown", "--format", "json"],
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unsupported phase3 readiness candidate"));
}

#[test]
fn test_cli_phase3_adoption_guidance_json_is_read_only() {
    let home = setup_cli_home();
    let output = run_mempal(
        &home,
        &["phase3", "adoption", "guidance", "--format", "json"],
    );
    assert!(
        output.status.success(),
        "guidance failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let guidance: Value = serde_json::from_slice(&output.stdout).expect("guidance json");
    assert_eq!(guidance["version"], 1);
    assert_eq!(
        guidance["recording_rule"],
        "record only concrete runtime outcomes, not speculation"
    );
    let required_fields = guidance["required_fields"]
        .as_array()
        .expect("required fields");
    assert!(required_fields.iter().any(|field| field == "track"));
    assert!(required_fields.iter().any(|field| field == "signal"));
    assert!(required_fields.iter().any(|field| field == "feature"));
    assert!(
        guidance["signals"]
            .as_array()
            .expect("signals")
            .iter()
            .any(|signal| signal["signal"] == "used"
                && signal["when"]
                    .as_str()
                    .expect("when")
                    .contains("actually consumed"))
    );
    assert!(
        guidance["signals"]
            .as_array()
            .expect("signals")
            .iter()
            .any(|signal| signal["signal"] == "rollback"
                && signal["when"].as_str().expect("when").contains("reverted"))
    );
    assert!(
        guidance["tracks"]
            .as_array()
            .expect("tracks")
            .iter()
            .any(|track| track["track"] == "card_context"
                && track["feature_examples"]
                    .as_array()
                    .expect("feature examples")
                    .iter()
                    .any(|feature| feature == "include_cards"))
    );

    let db = Database::open(&home.path().join(".mempal/palace.db")).expect("open db");
    let events = db
        .list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
        .expect("list events");
    assert!(events.is_empty());
}

#[test]
fn test_cli_phase3_adoption_guidance_plain() {
    let home = setup_cli_home();
    let output = run_mempal(&home, &["phase3", "adoption", "guidance"]);
    assert!(
        output.status.success(),
        "guidance failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("version=1"));
    assert!(
        stdout.contains("recording_rule=record only concrete runtime outcomes, not speculation")
    );
    assert!(stdout.contains("signal=used"));
    assert!(stdout.contains("track=card_context"));
}

#[test]
fn test_cli_phase3_adoption_guidance_rejects_invalid_format() {
    let home = setup_cli_home();
    let output = run_mempal(
        &home,
        &["phase3", "adoption", "guidance", "--format", "yaml"],
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unsupported phase3 adoption format"));
}

#[test]
fn test_cli_phase3_adoption_instrumentation_policy_json_is_read_only() {
    let home = setup_cli_home();
    let output = run_mempal(
        &home,
        &[
            "phase3",
            "adoption",
            "instrumentation-policy",
            "--format",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "instrumentation policy failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let policy: Value = serde_json::from_slice(&output.stdout).expect("policy json");
    assert_eq!(policy["writes"], false);
    assert_eq!(policy["version"], 1);
    assert_eq!(policy["default_mode"], "manual_only");
    assert!(
        policy["allowed_modes"]
            .as_array()
            .expect("allowed modes")
            .iter()
            .any(|mode| mode["mode"] == "opt_in_wrapper"
                && mode["requires_execute"] == true
                && mode["requires_checked_capture"] == true)
    );
    assert!(
        policy["forbidden_modes"]
            .as_array()
            .expect("forbidden modes")
            .iter()
            .any(|mode| mode == "implicit_background_capture")
    );
    assert!(
        policy["requirements"]
            .as_array()
            .expect("requirements")
            .iter()
            .any(|requirement| requirement
                .as_str()
                .expect("requirement")
                .contains("opt-out"))
    );
    assert!(
        policy["rollback_requirements"]
            .as_array()
            .expect("rollback requirements")
            .iter()
            .any(|requirement| requirement
                .as_str()
                .expect("rollback requirement")
                .contains("rollback"))
    );

    let db = Database::open(&home.path().join(".mempal/palace.db")).expect("open db");
    let events = db
        .list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
        .expect("list events");
    assert!(events.is_empty());
}

#[test]
fn test_cli_phase3_adoption_instrumentation_policy_rejects_invalid_format() {
    let home = setup_cli_home();
    let output = run_mempal(
        &home,
        &[
            "phase3",
            "adoption",
            "instrumentation-policy",
            "--format",
            "yaml",
        ],
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unsupported phase3 adoption format"));
}

#[test]
fn test_cli_phase3_adoption_prepare_record_json_is_read_only() {
    let home = setup_cli_home();
    let output = run_mempal(
        &home,
        &[
            "phase3",
            "adoption",
            "prepare-record",
            "--track",
            "card_context",
            "--signal",
            "accepted",
            "--feature",
            "include_cards",
            "--query",
            "skill trigger",
            "--format",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "prepare-record failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan: Value = serde_json::from_slice(&output.stdout).expect("prepare-record json");
    assert_eq!(plan["writes"], false);
    let command = plan["record_command"].as_array().expect("record command");
    assert_eq!(command[0], "mempal");
    assert_eq!(command[1], "phase3");
    assert_eq!(command[2], "adoption");
    assert_eq!(command[3], "record");
    assert_eq!(plan["record_payload"]["action"], "record");
    assert_eq!(plan["record_payload"]["track"], "card_context");
    assert_eq!(plan["record_payload"]["signal"], "accepted");
    assert_eq!(plan["record_payload"]["feature"], "include_cards");
    assert_eq!(plan["record_payload"]["query"], "skill trigger");

    let db = Database::open(&home.path().join(".mempal/palace.db")).expect("open db");
    let events = db
        .list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
        .expect("list events");
    assert!(events.is_empty());
}

#[test]
fn test_cli_phase3_adoption_prepare_record_plain() {
    let home = setup_cli_home();
    let output = run_mempal(
        &home,
        &[
            "phase3",
            "adoption",
            "prepare-record",
            "--track",
            "card_context",
            "--signal",
            "used",
            "--feature",
            "include_cards",
        ],
    );
    assert!(
        output.status.success(),
        "prepare-record failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("writes=false"));
    assert!(stdout.contains("mempal phase3 adoption record"));
    assert!(stdout.contains("action=record"));
}

#[test]
fn test_cli_phase3_adoption_prepare_record_rejects_invalid_track() {
    let home = setup_cli_home();
    let output = run_mempal(
        &home,
        &[
            "phase3",
            "adoption",
            "prepare-record",
            "--track",
            "invalid",
            "--signal",
            "accepted",
            "--feature",
            "x",
        ],
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unsupported runtime adoption track"));
}

#[test]
fn test_cli_phase3_adoption_capture_card_context_dry_run() {
    let home = setup_cli_home();
    let output = run_mempal(
        &home,
        &[
            "phase3",
            "adoption",
            "capture",
            "--surface",
            "card-context",
            "--outcome",
            "accepted",
            "--card-id",
            "card_1",
            "--query",
            "skill trigger",
            "--note",
            "card helped",
            "--format",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "capture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("capture json");
    assert_eq!(report["writes"], false);
    assert_eq!(report["execute"], false);
    assert_eq!(report["surface"], "card-context");
    assert_eq!(report["outcome"], "accepted");
    assert_eq!(report["record_plan"]["writes"], false);
    assert_eq!(
        report["record_plan"]["record_payload"]["track"],
        "card_context"
    );
    assert_eq!(
        report["record_plan"]["record_payload"]["signal"],
        "accepted"
    );
    assert_eq!(
        report["record_plan"]["record_payload"]["feature"],
        "include_cards"
    );
    assert_eq!(report["record_quality"]["quality"], "ready");
    assert!(report["record_checked"].is_null());

    let db = Database::open(&home.path().join(".mempal/palace.db")).expect("open db");
    assert!(
        db.list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
            .expect("events")
            .is_empty()
    );
}

#[test]
fn test_cli_phase3_adoption_capture_execute_writes_ready_event() {
    let home = setup_cli_home();
    insert_test_card(&home, "card_1");
    let output = run_mempal(
        &home,
        &[
            "phase3",
            "adoption",
            "capture",
            "--surface",
            "card-context",
            "--outcome",
            "accepted",
            "--card-id",
            "card_1",
            "--query",
            "skill trigger",
            "--note",
            "card helped",
            "--execute",
            "--format",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "capture execute failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("capture json");
    assert_eq!(report["writes"], true);
    assert_eq!(report["execute"], true);
    assert_eq!(report["record_checked"]["writes"], true);
    assert_eq!(report["record_checked"]["blocked"], false);
    assert_eq!(
        report["record_checked"]["record_quality"]["quality"],
        "ready"
    );

    let db = Database::open(&home.path().join(".mempal/palace.db")).expect("open db");
    let events = db
        .list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
        .expect("events");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].feature, "include_cards");
}

#[test]
fn test_cli_phase3_adoption_capture_blocks_warning_by_default() {
    let home = setup_cli_home();
    let output = run_mempal(
        &home,
        &[
            "phase3",
            "adoption",
            "capture",
            "--surface",
            "card-context",
            "--outcome",
            "accepted",
            "--execute",
            "--format",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "capture warning should return blocked JSON: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("capture json");
    assert_eq!(report["writes"], false);
    assert_eq!(report["execute"], true);
    assert_eq!(report["record_quality"]["quality"], "warning");
    assert_eq!(report["record_checked"]["writes"], false);
    assert_eq!(report["record_checked"]["blocked"], true);

    let db = Database::open(&home.path().join(".mempal/palace.db")).expect("open db");
    assert!(
        db.list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
            .expect("events")
            .is_empty()
    );
}

#[test]
fn test_cli_phase3_adoption_capture_rejects_unknown_surface() {
    let home = setup_cli_home();
    let output = run_mempal(
        &home,
        &[
            "phase3",
            "adoption",
            "capture",
            "--surface",
            "unknown",
            "--outcome",
            "accepted",
        ],
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unsupported adoption capture surface"));
}

#[test]
fn test_cli_phase3_adoption_check_record_json_accepts_supported_event() {
    let home = setup_cli_home();
    let output = run_mempal(
        &home,
        &[
            "phase3",
            "adoption",
            "check-record",
            "--track",
            "card_context",
            "--signal",
            "accepted",
            "--feature",
            "include_cards",
            "--query",
            "skill trigger",
            "--card-id",
            "card_1",
            "--note",
            "card evidence helped",
            "--format",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "check-record failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("quality report json");
    assert_eq!(report["writes"], false);
    assert_eq!(report["valid"], true);
    assert_eq!(report["quality"], "ready");
    assert!(report["errors"].as_array().expect("errors").is_empty());

    let db = Database::open(&home.path().join(".mempal/palace.db")).expect("open db");
    let events = db
        .list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
        .expect("list events");
    assert!(events.is_empty());
}

#[test]
fn test_cli_phase3_adoption_check_record_json_warns_on_weak_evidence() {
    let home = setup_cli_home();
    let output = run_mempal(
        &home,
        &[
            "phase3",
            "adoption",
            "check-record",
            "--track",
            "card_context",
            "--signal",
            "accepted",
            "--feature",
            "include_cards",
            "--format",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "check-record failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("quality report json");
    assert_eq!(report["writes"], false);
    assert_eq!(report["valid"], true);
    assert_eq!(report["quality"], "warning");
    let warnings = report["warnings"].as_array().expect("warnings");
    assert!(warnings.iter().any(|warning| {
        warning
            .as_str()
            .expect("warning")
            .contains("concrete outcome context")
    }));
    assert!(
        warnings
            .iter()
            .any(|warning| warning.as_str().expect("warning").contains("card_id"))
    );

    let db = Database::open(&home.path().join(".mempal/palace.db")).expect("open db");
    let events = db
        .list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
        .expect("list events");
    assert!(events.is_empty());
}

#[test]
fn test_cli_phase3_adoption_check_record_rejects_empty_feature() {
    let home = setup_cli_home();
    let output = run_mempal(
        &home,
        &[
            "phase3",
            "adoption",
            "check-record",
            "--track",
            "card_context",
            "--signal",
            "accepted",
            "--feature",
            "   ",
            "--format",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "check-record should report invalid input without failing: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("quality report json");
    assert_eq!(report["writes"], false);
    assert_eq!(report["valid"], false);
    assert_eq!(report["quality"], "invalid");
    let errors = report["errors"].as_array().expect("errors");
    assert!(errors.iter().any(|error| {
        error
            .as_str()
            .expect("error")
            .contains("feature must not be empty")
    }));

    let db = Database::open(&home.path().join(".mempal/palace.db")).expect("open db");
    let events = db
        .list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
        .expect("list events");
    assert!(events.is_empty());
}

#[test]
fn test_cli_phase3_adoption_record_checked_writes_ready_event() {
    let home = setup_cli_home();
    let output = run_mempal(
        &home,
        &[
            "phase3",
            "adoption",
            "record-checked",
            "--track",
            "runtime_adoption",
            "--signal",
            "accepted",
            "--feature",
            "context_pack",
            "--query",
            "skill trigger",
            "--note",
            "context guidance helped",
            "--format",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "record-checked failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("checked record json");
    assert_eq!(report["writes"], true);
    assert_eq!(report["blocked"], false);
    assert_eq!(report["record_quality"]["quality"], "ready");
    assert!(report["event"]["id"].as_str().expect("event id").len() > 8);

    let db = Database::open(&home.path().join(".mempal/palace.db")).expect("open db");
    let events = db
        .list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
        .expect("list events");
    assert_eq!(events.len(), 1);
}

#[test]
fn test_cli_phase3_adoption_record_checked_blocks_warning_by_default() {
    let home = setup_cli_home();
    let output = run_mempal(
        &home,
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
            "--format",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "record-checked should return blocked JSON: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("checked record json");
    assert_eq!(report["writes"], false);
    assert_eq!(report["blocked"], true);
    assert_eq!(report["record_quality"]["quality"], "warning");
    assert!(report.get("event").is_none() || report["event"].is_null());

    let db = Database::open(&home.path().join(".mempal/palace.db")).expect("open db");
    let events = db
        .list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
        .expect("list events");
    assert!(events.is_empty());
}

#[test]
fn test_cli_phase3_adoption_record_checked_allow_warnings_writes_warning_event() {
    let home = setup_cli_home();
    let output = run_mempal(
        &home,
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
            "--allow-warnings",
            "--format",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "record-checked failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("checked record json");
    assert_eq!(report["writes"], true);
    assert_eq!(report["blocked"], false);
    assert_eq!(report["record_quality"]["quality"], "warning");
    assert!(report["event"]["id"].as_str().expect("event id").len() > 8);

    let db = Database::open(&home.path().join(".mempal/palace.db")).expect("open db");
    let events = db
        .list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
        .expect("list events");
    assert_eq!(events.len(), 1);
}

#[test]
fn test_cli_phase3_adoption_record_checked_blocks_invalid_even_with_allow_warnings() {
    let home = setup_cli_home();
    let output = run_mempal(
        &home,
        &[
            "phase3",
            "adoption",
            "record-checked",
            "--track",
            "card_context",
            "--signal",
            "accepted",
            "--feature",
            "   ",
            "--allow-warnings",
            "--format",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "record-checked should return blocked JSON: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("checked record json");
    assert_eq!(report["writes"], false);
    assert_eq!(report["blocked"], true);
    assert_eq!(report["record_quality"]["quality"], "invalid");
    assert!(report.get("event").is_none() || report["event"].is_null());

    let db = Database::open(&home.path().join(".mempal/palace.db")).expect("open db");
    let events = db
        .list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
        .expect("list events");
    assert!(events.is_empty());
}

#[test]
fn test_cli_phase3_adoption_review_json_summarizes_events() {
    let home = setup_cli_home();
    for (id, signal) in [
        ("review_accept_1", "accepted"),
        ("review_accept_2", "accepted"),
        ("review_reject_1", "rejected"),
    ] {
        let output = run_mempal(
            &home,
            &[
                "phase3",
                "adoption",
                "record",
                "--id",
                id,
                "--track",
                "card_context",
                "--signal",
                signal,
                "--feature",
                "include_cards",
            ],
        );
        assert!(
            output.status.success(),
            "record failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let output = run_mempal(
        &home,
        &[
            "phase3",
            "adoption",
            "review",
            "--track",
            "card_context",
            "--format",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "review failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("review json");
    assert_eq!(report["writes"], false);
    assert_eq!(report["total"], 3);
    assert_eq!(report["stats"]["accepted"], 2);
    assert_eq!(report["stats"]["rejected"], 1);
    assert_eq!(report["features"][0]["feature"], "include_cards");

    let db = Database::open(&home.path().join(".mempal/palace.db")).expect("open db");
    let events = db
        .list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
        .expect("list events");
    assert_eq!(events.len(), 3);
}

#[test]
fn test_cli_phase3_adoption_review_json_filters_signal() {
    let home = setup_cli_home();
    for (id, signal) in [
        ("review_accept_1", "accepted"),
        ("review_accept_2", "accepted"),
        ("review_reject_1", "rejected"),
    ] {
        let output = run_mempal(
            &home,
            &[
                "phase3",
                "adoption",
                "record",
                "--id",
                id,
                "--track",
                "card_context",
                "--signal",
                signal,
                "--feature",
                "include_cards",
            ],
        );
        assert!(
            output.status.success(),
            "record failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let output = run_mempal(
        &home,
        &[
            "phase3",
            "adoption",
            "review",
            "--track",
            "card_context",
            "--signal",
            "accepted",
            "--format",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "review failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("review json");
    assert_eq!(report["total"], 2);
    assert_eq!(report["stats"]["accepted"], 2);
    assert_eq!(report["stats"]["rejected"], 0);
}

#[test]
fn test_cli_phase3_adoption_review_json_no_evidence_read_only() {
    let home = setup_cli_home();
    let output = run_mempal(
        &home,
        &[
            "phase3",
            "adoption",
            "review",
            "--track",
            "evaluator",
            "--format",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "review failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("review json");
    assert_eq!(report["writes"], false);
    assert_eq!(report["total"], 0);
    assert_eq!(report["conclusion"], "no_evidence");

    let db = Database::open(&home.path().join(".mempal/palace.db")).expect("open db");
    let events = db
        .list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
        .expect("list events");
    assert!(events.is_empty());
}

#[test]
fn test_cli_phase3_gate_blocks_card_embeddings_without_miss_evidence() {
    let home = setup_cli_home();
    let gate = run_mempal(
        &home,
        &["phase3", "gate", "card-embeddings", "--format", "json"],
    );
    assert!(
        gate.status.success(),
        "gate failed: {}",
        String::from_utf8_lossy(&gate.stderr)
    );
    let gate_json: Value = serde_json::from_slice(&gate.stdout).expect("gate json");
    assert_eq!(gate_json["ready"], false);
    assert_eq!(gate_json["stats"]["misses"], 0);
}

#[test]
fn test_cli_phase3_evaluator_gate_exists_and_is_read_only() {
    let home = setup_cli_home();
    let gate = run_mempal(
        &home,
        &["phase3", "gate", "evaluator-api", "--format", "json"],
    );
    assert!(
        gate.status.success(),
        "gate failed: {}",
        String::from_utf8_lossy(&gate.stderr)
    );
    let gate_json: Value = serde_json::from_slice(&gate.stdout).expect("gate json");
    assert_eq!(gate_json["candidate"], "evaluator-api");
    assert_eq!(gate_json["ready"], false);
    assert_eq!(gate_json["required_track"], "evaluator");
}

#[test]
fn test_cli_phase3_evaluator_advise_supportive_read_only() {
    let home = setup_cli_home();
    let output = run_mempal(
        &home,
        &[
            "phase3",
            "evaluator",
            "advise",
            "--evaluator-id",
            "eval_policy",
            "--subject-kind",
            "dao_ren",
            "--subject-id",
            "k1",
            "--proposed-action",
            "promote",
            "--evidence-ref",
            "e1",
            "--evidence-ref",
            "e2",
            "--format",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "advise failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("advice json");
    assert_eq!(report["writes"], false);
    assert_eq!(report["lifecycle_authority"], false);
    assert_eq!(report["deterministic_gate_required"], true);
    assert_eq!(report["recommendation"], "advisory_support");
    assert_eq!(
        report["adoption_capture"]["record_payload"]["track"],
        "evaluator"
    );
    assert_eq!(
        report["adoption_capture"]["record_payload"]["feature"],
        "advisory_gate"
    );

    let db = Database::open(&home.path().join(".mempal/palace.db")).expect("open db");
    let events = db
        .list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
        .expect("list events");
    assert!(events.is_empty());
}

#[test]
fn test_cli_phase3_evaluator_advise_dao_tian_requires_human_review() {
    let home = setup_cli_home();
    let output = run_mempal(
        &home,
        &[
            "phase3",
            "evaluator",
            "advise",
            "--evaluator-id",
            "eval_policy",
            "--subject-kind",
            "dao_tian",
            "--subject-id",
            "k1",
            "--proposed-action",
            "canonical",
            "--evidence-ref",
            "e1",
            "--format",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "advise failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("advice json");
    assert_eq!(report["requires_human_review"], true);
    assert_eq!(report["recommendation"], "requires_human_review");
    let reasons = report["reasons"].as_array().expect("reasons");
    assert!(reasons.iter().any(|reason| {
        reason
            .as_str()
            .is_some_and(|value| value.contains("dao_tian canonicalization requires human review"))
    }));
}

#[test]
fn test_cli_phase3_evaluator_advise_needs_evidence_and_blocks_risk() {
    let home = setup_cli_home();
    let weak = run_mempal(
        &home,
        &[
            "phase3",
            "evaluator",
            "advise",
            "--evaluator-id",
            "eval_policy",
            "--subject-kind",
            "dao_ren",
            "--subject-id",
            "k1",
            "--proposed-action",
            "promote",
            "--format",
            "json",
        ],
    );
    assert!(
        weak.status.success(),
        "weak advise failed: {}",
        String::from_utf8_lossy(&weak.stderr)
    );
    let weak_report: Value = serde_json::from_slice(&weak.stdout).expect("weak advice json");
    assert_eq!(weak_report["recommendation"], "needs_evidence");

    let risky = run_mempal(
        &home,
        &[
            "phase3",
            "evaluator",
            "advise",
            "--evaluator-id",
            "eval_policy",
            "--subject-kind",
            "dao_ren",
            "--subject-id",
            "k1",
            "--proposed-action",
            "promote",
            "--evidence-ref",
            "e1",
            "--counterexample-ref",
            "c1",
            "--risk-note",
            "contradiction risk",
            "--format",
            "json",
        ],
    );
    assert!(
        risky.status.success(),
        "risky advise failed: {}",
        String::from_utf8_lossy(&risky.stderr)
    );
    let risky_report: Value = serde_json::from_slice(&risky.stdout).expect("risky advice json");
    assert_eq!(risky_report["recommendation"], "do_not_promote");
}

#[test]
fn test_cli_phase3_evaluator_advise_rejects_missing_evaluator() {
    let home = setup_cli_home();
    let output = run_mempal(
        &home,
        &[
            "phase3",
            "evaluator",
            "advise",
            "--subject-kind",
            "dao_ren",
            "--subject-id",
            "k1",
            "--proposed-action",
            "promote",
        ],
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("evaluator-id is required"));
}

#[test]
fn test_cli_phase3_adoption_record_rejects_invalid_track() {
    let home = setup_cli_home();
    let output = run_mempal(
        &home,
        &[
            "phase3",
            "adoption",
            "record",
            "--track",
            "invalid",
            "--signal",
            "accepted",
            "--feature",
            "x",
        ],
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unsupported runtime adoption track"));
}

#[test]
fn test_cli_phase3_research_validate_plan() {
    let home = setup_cli_home();
    let report_path = home.path().join("research-report.json");
    fs::write(
        &report_path,
        json!({
            "report_id": "research_001",
            "title": "Agent memory retrieval notes",
            "sources": [{"url": "https://example.invalid/report"}],
            "findings": [{"summary": "linked evidence retrieval needs adoption evidence"}],
            "candidate_insights": [{"statement": "measure before defaulting cards"}]
        })
        .to_string(),
    )
    .expect("write report");

    let output = run_mempal(
        &home,
        &[
            "phase3",
            "research-validate-plan",
            report_path.to_str().expect("report path"),
            "--format",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "validate-plan failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("plan json");
    assert_eq!(report["valid"], true);
    assert_eq!(report["source_count"], 1);
    assert_eq!(report["candidate_insight_count"], 1);
}

#[test]
fn test_cli_phase3_research_validate_plan_reports_missing_fields() {
    let home = setup_cli_home();
    let report_path = home.path().join("bad-research-report.json");
    fs::write(&report_path, "{}").expect("write bad report");

    let output = run_mempal(
        &home,
        &[
            "phase3",
            "research-validate-plan",
            report_path.to_str().expect("report path"),
        ],
    );
    assert!(
        output.status.success(),
        "validate-plan should report invalid input without failing: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("valid=false"));
    assert!(stdout.contains("error=report_id is required"));
    assert!(stdout.contains("error=sources must contain at least one item"));
}

#[test]
fn test_cli_phase3_research_ingest_plan_dry_run_json_no_write() {
    let home = setup_cli_home();
    let report_path = home.path().join("research-report.json");
    fs::write(
        &report_path,
        json!({
            "report_id": "research_p67_001",
            "title": "Agent self-evolution research",
            "sources": [{"id": "src_1", "url": "https://example.invalid/research"}],
            "findings": [{"summary": "Research findings must enter memory as evidence first."}],
            "candidate_insights": [{"statement": "Research output should be distilled only from evidence refs."}]
        })
        .to_string(),
    )
    .expect("write report");

    let output = run_mempal(
        &home,
        &[
            "phase3",
            "research-ingest-plan",
            report_path.to_str().expect("report path"),
            "--format",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "research-ingest-plan failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("ingest plan json");
    assert_eq!(report["valid"], true);
    assert_eq!(report["writes"], false);
    assert_eq!(report["planned_evidence_count"], 1);
    assert_eq!(report["candidate_insight_count"], 1);
    assert_eq!(
        report["evidence_drawers"]
            .as_array()
            .expect("drawers")
            .len(),
        1
    );
    assert_eq!(
        report["candidate_insights"]
            .as_array()
            .expect("insights")
            .len(),
        1
    );
    assert_eq!(
        report["candidate_insights"][0]["suggested_command"][0],
        "mempal"
    );

    let db = Database::open(&home.path().join(".mempal/palace.db")).expect("open db");
    assert_eq!(db.top_drawers(10).expect("drawers").len(), 0);
}

#[test]
fn test_cli_phase3_research_ingest_plan_execute_writes_research_evidence() {
    let home = setup_cli_home();
    let report_path = home.path().join("research-report.json");
    fs::write(
        &report_path,
        json!({
            "report_id": "research_p67_002",
            "title": "Research adapter evidence",
            "sources": [{"id": "src_1", "url": "https://example.invalid/a"}],
            "findings": [
                {"summary": "First finding becomes research evidence."},
                {"summary": "Second finding becomes research evidence."}
            ],
            "candidate_insights": []
        })
        .to_string(),
    )
    .expect("write report");

    let output = run_mempal(
        &home,
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
        "research-ingest-plan execute failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("ingest plan json");
    assert_eq!(report["valid"], true);
    assert_eq!(report["writes"], true);
    assert_eq!(report["created_count"], 2);
    assert_eq!(report["skipped_count"], 0);
    let drawers = report["evidence_drawers"].as_array().expect("drawers");
    assert_eq!(drawers.len(), 2);

    let db = Database::open(&home.path().join(".mempal/palace.db")).expect("open db");
    assert_eq!(db.top_drawers(10).expect("drawers").len(), 2);
    for drawer in drawers {
        let drawer_id = drawer["drawer_id"].as_str().expect("drawer id");
        let stored = db
            .get_drawer(drawer_id)
            .expect("load drawer")
            .expect("drawer exists");
        assert_eq!(stored.memory_kind, MemoryKind::Evidence);
        assert_eq!(stored.provenance, Some(Provenance::Research));
        assert!(stored.tier.is_none());
        assert!(stored.status.is_none());
    }

    let second = run_mempal(
        &home,
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
        second.status.success(),
        "second execute failed: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second_report: Value = serde_json::from_slice(&second.stdout).expect("second json");
    assert_eq!(second_report["created_count"], 0);
    assert_eq!(second_report["skipped_count"], 2);
    let db = Database::open(&home.path().join(".mempal/palace.db")).expect("open db");
    assert_eq!(db.top_drawers(10).expect("drawers").len(), 2);
}

#[test]
fn test_cli_phase3_research_ingest_plan_invalid_report_no_write() {
    let home = setup_cli_home();
    let report_path = home.path().join("bad-research-report.json");
    fs::write(&report_path, "{}").expect("write bad report");

    let output = run_mempal(
        &home,
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
        "invalid ingest plan should report invalid input without failing: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("invalid json");
    assert_eq!(report["valid"], false);
    assert_eq!(report["writes"], false);
    assert!(
        report["errors"]
            .as_array()
            .expect("errors")
            .iter()
            .any(|error| error.as_str().expect("error") == "report_id is required")
    );

    let db = Database::open(&home.path().join(".mempal/palace.db")).expect("open db");
    assert_eq!(db.top_drawers(10).expect("drawers").len(), 0);
}

#[test]
fn test_cli_phase3_research_ingest_plan_rejects_invalid_format() {
    let home = setup_cli_home();
    let report_path = home.path().join("research-report.json");
    fs::write(
        &report_path,
        json!({
            "report_id": "research_p67_003",
            "title": "Research adapter evidence",
            "sources": [{"url": "https://example.invalid/a"}],
            "findings": [{"summary": "Finding."}]
        })
        .to_string(),
    )
    .expect("write report");

    let output = run_mempal(
        &home,
        &[
            "phase3",
            "research-ingest-plan",
            report_path.to_str().expect("report path"),
            "--format",
            "yaml",
        ],
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unsupported phase3 research ingest format"));
}
