use std::fs;
use std::process::Command;

use mempal::core::db::Database;
use mempal::core::types::{
    RuntimeAdoptionEvent, RuntimeAdoptionFilter, RuntimeAdoptionSignal, RuntimeAdoptionTrack,
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
