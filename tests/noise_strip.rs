use mempal::core::db::Database;
use mempal::core::types::{BootstrapEvidenceArgs, Drawer, SourceType};
use mempal::embed::Embedder;
use mempal::ingest::noise::{strip_claude_jsonl_noise, strip_codex_rollout_noise};
use mempal::ingest::normalize::CURRENT_NORMALIZE_VERSION;
use mempal::ingest::reindex::{ReindexMode, ReindexOptions, reindex_sources};
use mempal::ingest::{IngestOptions, ingest_file_with_options};
use std::process::Command;
use tempfile::TempDir;

struct StubEmbedder;

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

#[test]
fn test_claude_jsonl_strips_system_reminder() {
    let content = "hello <system-reminder>mcp info</system-reminder> world";

    let stripped = strip_claude_jsonl_noise(content);

    assert_eq!(stripped, "hello  world");
}

#[test]
fn test_code_block_preserved_verbatim() {
    let code = "```rust\nfn main() {}\n```";
    let content = format!("{code}\n<system-reminder>x</system-reminder>");

    let stripped = strip_claude_jsonl_noise(&content);

    assert!(stripped.contains(code));
    assert!(!stripped.contains("system-reminder"));
}

#[test]
fn test_user_message_angle_brackets_preserved() {
    let content = r#"user: "I prefer Vec<T> over [T]""#;

    let stripped = strip_claude_jsonl_noise(content);

    assert_eq!(stripped.as_bytes(), content.as_bytes());
    assert!(stripped.contains("Vec<T>"));
    assert!(stripped.contains("[T]"));
}

#[test]
fn test_codex_rollout_session_markers_stripped() {
    let content = "[session 12345 started]\nwork\n[session 12345 ended]";

    let stripped = strip_codex_rollout_noise(content);

    assert_eq!(stripped, "work\n");
}

#[test]
fn test_strip_no_match_returns_identity() {
    let content = "plain text no markers";

    let stripped = strip_claude_jsonl_noise(content);

    assert_eq!(stripped.as_bytes(), content.as_bytes());
}

#[test]
fn test_strip_preserves_unicode_bytes() {
    let content = "决策 🎯 <system-reminder>x</system-reminder> 完成 ✅";

    let stripped = strip_claude_jsonl_noise(content);

    assert!(stripped.contains("决策 🎯"));
    assert!(stripped.contains(" 完成 ✅"));
    assert!(!stripped.contains("system-reminder"));
}

#[tokio::test]
async fn test_plain_markdown_not_stripped() {
    let tmp = TempDir::new().expect("tempdir");
    let db = Database::open(&tmp.path().join("palace.db")).expect("open db");
    let source = tmp.path().join("doc.md");
    std::fs::write(
        &source,
        "# Title\n<system-reminder>fake</system-reminder>\nbody",
    )
    .expect("write markdown");

    ingest_file_with_options(
        &db,
        &StubEmbedder,
        &source,
        "mempal",
        IngestOptions {
            room: Some("noise"),
            source_root: source.parent(),
            dry_run: false,
            source_file_override: None,
            replace_existing_source: false,
            no_strip_noise: false,
            ..IngestOptions::default()
        },
    )
    .await
    .expect("ingest markdown");

    let content = only_active_content(&db);
    assert!(content.contains("<system-reminder>fake</system-reminder>"));
}

#[tokio::test]
async fn test_ingest_outcome_reports_stripped_bytes() {
    let tmp = TempDir::new().expect("tempdir");
    let db = Database::open(&tmp.path().join("palace.db")).expect("open db");
    let source = tmp.path().join("claude.jsonl");
    let reminder = "x".repeat(2048);
    let message = format!("before <system-reminder>{reminder}</system-reminder> after");
    std::fs::write(
        &source,
        serde_json::json!({
            "type": "assistant",
            "message": message
        })
        .to_string(),
    )
    .expect("write claude jsonl");

    let stats = ingest_file_with_options(
        &db,
        &StubEmbedder,
        &source,
        "mempal",
        IngestOptions {
            room: Some("noise"),
            source_root: source.parent(),
            dry_run: false,
            source_file_override: None,
            replace_existing_source: false,
            no_strip_noise: false,
            ..IngestOptions::default()
        },
    )
    .await
    .expect("ingest claude jsonl");

    let stripped = stats
        .noise_bytes_stripped
        .expect("strip metric should be present");
    assert!(
        (2050..=2100).contains(&stripped),
        "unexpected stripped bytes: {stripped}"
    );
    let content = only_active_content(&db);
    assert!(!content.contains("system-reminder"));
    assert!(content.contains("before  after"));
}

#[tokio::test]
async fn test_normalize_version_bump_triggers_reindex_opportunity() {
    let tmp = TempDir::new().expect("tempdir");
    let db = Database::open(&tmp.path().join("palace.db")).expect("open db");
    let source = tmp.path().join("claude.jsonl");
    std::fs::write(
        &source,
        serde_json::json!({
            "type": "assistant",
            "message": "old <system-reminder>noise</system-reminder> clean"
        })
        .to_string(),
    )
    .expect("write claude jsonl");
    insert_stale_drawer(&db, &source.to_string_lossy());

    let report = reindex_sources(
        &db,
        &StubEmbedder,
        ReindexOptions {
            mode: ReindexMode::Stale,
            dry_run: false,
        },
    )
    .await
    .expect("reindex stale source");

    assert_eq!(report.processed_sources, 1);
    let content = only_active_content(&db);
    assert!(!content.contains("system-reminder"));
    let version: u32 = db
        .conn()
        .query_row(
            "SELECT normalize_version FROM drawers WHERE deleted_at IS NULL",
            [],
            |row| row.get(0),
        )
        .expect("read normalize_version");
    assert_eq!(version, CURRENT_NORMALIZE_VERSION);
    // P115: PR #7 (response_item preference + shared block-text extraction)
    // plus the codex wrapper strip changed normalize output; version 3 lets
    // `reindex --stale` find sources normalized under the old rules.
    assert_eq!(CURRENT_NORMALIZE_VERSION, 3);
}

#[test]
fn test_codex_runtime_preamble_wrappers_stripped() {
    let content = "<user_instructions>\nAGENTS.md boilerplate\n</user_instructions>\nreal request\n<environment_context>\n<cwd>/tmp/x</cwd>\n</environment_context>\nmore work";

    let stripped = strip_codex_rollout_noise(content);

    assert!(!stripped.contains("AGENTS.md boilerplate"), "{stripped}");
    assert!(!stripped.contains("user_instructions"), "{stripped}");
    assert!(!stripped.contains("environment_context"), "{stripped}");
    assert!(!stripped.contains("<cwd>"), "{stripped}");
    assert!(stripped.contains("real request"), "{stripped}");
    assert!(stripped.contains("more work"), "{stripped}");
}

#[test]
fn test_codex_all_wrapper_tags_stripped() {
    for tag in [
        "INSTRUCTIONS",
        "user_instructions",
        "environment_context",
        "recommended_plugins",
        "turn_aborted",
    ] {
        let content = format!("before\n<{tag}>\nboilerplate payload\n</{tag}>\nafter");

        let stripped = strip_codex_rollout_noise(&content);

        assert!(
            !stripped.contains("boilerplate payload") && !stripped.contains(tag),
            "wrapper <{tag}> not stripped: {stripped}"
        );
        assert!(stripped.contains("before"), "{stripped}");
        assert!(stripped.contains("after"), "{stripped}");
    }
}

#[test]
fn test_codex_wrapper_only_message_strips_to_blank() {
    let content = "<recommended_plugins>\nplugin list here\n</recommended_plugins>";

    let stripped = strip_codex_rollout_noise(content);

    assert!(
        stripped.trim().is_empty(),
        "wrapper-only content should strip to blank: {stripped:?}"
    );
}

#[test]
fn test_codex_wrapper_inside_code_fence_preserved() {
    let content = "```xml\n<environment_context>sample</environment_context>\n```\ndone";

    let stripped = strip_codex_rollout_noise(content);

    assert!(
        stripped.contains("<environment_context>sample</environment_context>"),
        "{stripped}"
    );
    assert!(stripped.contains("done"), "{stripped}");
}

#[test]
fn test_cli_no_strip_noise_flag() {
    let tmp = TempDir::new().expect("tempdir");
    write_cli_config(tmp.path(), &tmp.path().join("palace.db"));
    let source = tmp.path().join("claude.jsonl");
    std::fs::write(
        &source,
        serde_json::json!({
            "type": "assistant",
            "message": "before <system-reminder>noise</system-reminder> after"
        })
        .to_string(),
    )
    .expect("write claude jsonl");

    let stripped_output = Command::new(mempal_bin())
        .args([
            "ingest",
            source.to_str().expect("utf8 path"),
            "--wing",
            "mempal",
            "--dry-run",
        ])
        .env("HOME", tmp.path())
        .output()
        .expect("run default ingest");
    assert!(
        stripped_output.status.success(),
        "default ingest failed: {}",
        String::from_utf8_lossy(&stripped_output.stderr)
    );
    let stripped_stdout = String::from_utf8_lossy(&stripped_output.stdout);
    assert!(
        metric_value(&stripped_stdout, "noise_bytes_stripped") > 0,
        "expected default strip metric > 0, stdout: {stripped_stdout}"
    );

    let no_strip_output = Command::new(mempal_bin())
        .args([
            "ingest",
            source.to_str().expect("utf8 path"),
            "--wing",
            "mempal",
            "--dry-run",
            "--no-strip-noise",
        ])
        .env("HOME", tmp.path())
        .output()
        .expect("run no-strip ingest");
    assert!(
        no_strip_output.status.success(),
        "no-strip ingest failed: {}",
        String::from_utf8_lossy(&no_strip_output.stderr)
    );
    let no_strip_stdout = String::from_utf8_lossy(&no_strip_output.stdout);
    assert!(
        no_strip_stdout.contains("noise_bytes_stripped=0"),
        "expected no-strip metric to be zero, stdout: {no_strip_stdout}"
    );
}

fn insert_stale_drawer(db: &Database, source_file: &str) {
    let mut drawer = Drawer::new_bootstrap_evidence(BootstrapEvidenceArgs {
        id: "drawer_stale_noise".to_string(),
        content: "old <system-reminder>noise</system-reminder> clean".to_string(),
        wing: "mempal".to_string(),
        room: Some("noise".to_string()),
        source_file: Some(source_file.to_string()),
        source_type: SourceType::Conversation,
        added_at: "1710000000".to_string(),
        chunk_index: Some(0),
        importance: 0,
    });
    drawer.normalize_version = 1;
    db.insert_drawer(&drawer).expect("insert stale drawer");
}

fn only_active_content(db: &Database) -> String {
    db.conn()
        .query_row(
            "SELECT content FROM drawers WHERE deleted_at IS NULL",
            [],
            |row| row.get(0),
        )
        .expect("read only active content")
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

fn metric_value(stdout: &str, metric: &str) -> u64 {
    stdout
        .split_ascii_whitespace()
        .find_map(|part| part.strip_prefix(&format!("{metric}=")))
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}
