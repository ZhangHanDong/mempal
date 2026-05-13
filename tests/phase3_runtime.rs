use std::fs;
use std::process::Command;

use mempal::core::db::Database;
use mempal::core::types::{
    MemoryKind, Provenance, RuntimeAdoptionEvent, RuntimeAdoptionFilter, RuntimeAdoptionSignal,
    RuntimeAdoptionTrack,
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
