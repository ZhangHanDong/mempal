//! Integration tests for P8 cowork inbox push.
//!
//! Run with:
//!   cargo test --test cowork_inbox --no-default-features --features model2vec
//!
//! These tests exercise inbox behavior that must hold under real process
//! boundaries (concurrent drain, CLI-level graceful degrade, stdin-json
//! payload parsing). Unit coverage for push/drain/format lives inline in
//! src/cowork/inbox.rs.

use mempal::cowork::Tool;
use mempal::cowork::inbox::{InboxMessage, drain, push};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use tempfile::TempDir;

fn mempal_bin() -> String {
    env!("CARGO_BIN_EXE_mempal").to_string()
}

fn setup_repo(tmp: &TempDir, name: &str) -> PathBuf {
    let repo = tmp.path().join(name);
    fs::create_dir_all(repo.join(".git")).unwrap();
    repo
}

#[tokio::test]
async fn concurrent_drain_is_winner_takes_all_at_most_once() {
    let tmp = TempDir::new().unwrap();
    let mempal_home = Arc::new(tmp.path().to_path_buf());
    let repo = setup_repo(&tmp, "proj");
    let repo_arc = Arc::new(repo);

    for i in 0..3 {
        push(
            &mempal_home,
            Tool::Claude,
            Tool::Codex,
            &repo_arc,
            format!("concurrent-{i}"),
            "2026-04-15T02:00:00Z".into(),
        )
        .unwrap();
    }

    let home_a = Arc::clone(&mempal_home);
    let repo_a = Arc::clone(&repo_arc);
    let home_b = Arc::clone(&mempal_home);
    let repo_b = Arc::clone(&repo_arc);

    let task_a =
        tokio::task::spawn_blocking(move || drain(&home_a, Tool::Codex, &repo_a).unwrap());
    let task_b =
        tokio::task::spawn_blocking(move || drain(&home_b, Tool::Codex, &repo_b).unwrap());

    let (a, b) = tokio::join!(task_a, task_b);
    let a_msgs: Vec<InboxMessage> = a.unwrap();
    let b_msgs: Vec<InboxMessage> = b.unwrap();

    // Exactly one task won the whole batch; the other got nothing.
    let total_received = a_msgs.len() + b_msgs.len();
    assert_eq!(
        total_received, 3,
        "both tasks combined must see all 3 messages"
    );

    let winner_count = std::cmp::max(a_msgs.len(), b_msgs.len());
    let loser_count = std::cmp::min(a_msgs.len(), b_msgs.len());
    assert_eq!(winner_count, 3, "winner takes all 3");
    assert_eq!(loser_count, 0, "loser empty");

    // No duplicate delivery.
    let winner_contents: Vec<String> = if a_msgs.len() == 3 {
        a_msgs.iter().map(|m| m.content.clone()).collect()
    } else {
        b_msgs.iter().map(|m| m.content.clone()).collect()
    };
    assert_eq!(
        winner_contents,
        vec!["concurrent-0", "concurrent-1", "concurrent-2"]
    );
}

#[tokio::test]
async fn push_and_drain_have_no_palace_db_side_effects() {
    use mempal::core::db::Database;

    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("palace.db");

    let db = Database::open(&db_path).expect("open db");
    let drawers_before = db.drawer_count().expect("drawer count");
    let triples_before = db.triple_count().expect("triple count");
    let schema_before = db.schema_version().expect("schema version");
    assert_eq!(schema_before, 4, "baseline palace.db should be schema v4");
    drop(db);

    let mempal_home = tmp.path().join("home");
    let repo = setup_repo(&tmp, "proj");

    for i in 0..3 {
        push(
            &mempal_home,
            Tool::Claude,
            Tool::Codex,
            &repo,
            format!("msg-{i}"),
            "2026-04-15T03:00:00Z".into(),
        )
        .unwrap();
    }
    let _ = drain(&mempal_home, Tool::Codex, &repo).unwrap();
    let _ = drain(&mempal_home, Tool::Codex, &repo).unwrap();

    let db = Database::open(&db_path).expect("reopen db");
    assert_eq!(
        db.drawer_count().unwrap(),
        drawers_before,
        "drawer_count changed after push/drain"
    );
    assert_eq!(
        db.triple_count().unwrap(),
        triples_before,
        "triple_count changed after push/drain"
    );
    assert_eq!(
        db.schema_version().unwrap(),
        schema_before,
        "schema_version changed after push/drain"
    );
}

#[test]
fn cowork_drain_cli_graceful_degrade_when_mempal_home_missing() {
    let tmp = TempDir::new().unwrap();
    // HOME points to an empty dir with NO .mempal/ subdirectory.
    // mempal CLI will resolve mempal_home to tmp/.mempal, which doesn't exist.
    // Drain must gracefully return empty stdout + exit 0.
    let output = Command::new(mempal_bin())
        .args([
            "cowork-drain",
            "--target",
            "claude",
            "--cwd",
            "/tmp/fake-project",
        ])
        .env("HOME", tmp.path())
        .output()
        .expect("spawn");

    assert_eq!(output.status.code(), Some(0));
    assert!(
        output.stdout.is_empty(),
        "stdout should be empty on graceful degrade, got {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn cowork_drain_reads_cwd_from_stdin_json_codex_path() {
    let tmp = TempDir::new().unwrap();
    // HOME=tmp → mempal_home resolves to tmp/.mempal, seed inbox there.
    let mempal_home = tmp.path().join(".mempal");
    let repo = setup_repo(&tmp, "proj-delta");

    push(
        &mempal_home,
        Tool::Claude,
        Tool::Codex,
        &repo,
        "stdin json test".into(),
        "2026-04-15T04:00:00Z".into(),
    )
    .unwrap();

    let stdin_payload = format!(
        r#"{{"session_id":"s1","turn_id":"t1","transcript_path":null,"cwd":"{}","hook_event_name":"UserPromptSubmit","model":"gpt-5-codex","permission_mode":"workspace-write","prompt":"继续"}}"#,
        repo.display()
    );

    let mut child = Command::new(mempal_bin())
        .args([
            "cowork-drain",
            "--target",
            "codex",
            "--format",
            "codex-hook-json",
            "--cwd-source",
            "stdin-json",
        ])
        .env("HOME", tmp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    use std::io::Write;
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(stdin_payload.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();

    assert_eq!(output.status.code(), Some(0));
    let stdout_str = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout_str.contains("stdin json test"),
        "stdout should contain seeded message, got: {stdout_str}"
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout_str).unwrap();
    assert_eq!(
        parsed["hookSpecificOutput"]["hookEventName"],
        "UserPromptSubmit"
    );
}

#[test]
fn cowork_drain_stdin_json_malformed_payload_graceful_degrade() {
    let tmp = TempDir::new().unwrap();
    let bad_inputs = [
        "not json at all".to_string(),
        r#"{"session_id":"s","prompt":"继续"}"#.to_string(), // missing cwd
        r#"{"cwd":42}"#.to_string(),                          // wrong type
    ];
    for payload in &bad_inputs {
        let mut child = Command::new(mempal_bin())
            .args([
                "cowork-drain",
                "--target",
                "codex",
                "--format",
                "codex-hook-json",
                "--cwd-source",
                "stdin-json",
            ])
            .env("HOME", tmp.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        use std::io::Write;
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(payload.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();

        assert_eq!(
            output.status.code(),
            Some(0),
            "malformed payload {payload:?} must exit 0"
        );
        assert!(
            output.stdout.is_empty(),
            "stdout must be empty for malformed payload {payload:?}, got {:?}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}

#[test]
fn cowork_status_cli_lists_both_inboxes_without_draining() {
    let tmp = TempDir::new().unwrap();
    let mempal_home = tmp.path().join(".mempal");
    let repo = setup_repo(&tmp, "proj");

    push(
        &mempal_home,
        Tool::Codex,
        Tool::Claude,
        &repo,
        "for claude a".into(),
        "t".into(),
    )
    .unwrap();
    push(
        &mempal_home,
        Tool::Codex,
        Tool::Claude,
        &repo,
        "for claude b".into(),
        "t".into(),
    )
    .unwrap();
    push(
        &mempal_home,
        Tool::Claude,
        Tool::Codex,
        &repo,
        "for codex".into(),
        "t".into(),
    )
    .unwrap();

    let output = Command::new(mempal_bin())
        .args(["cowork-status", "--cwd", repo.to_str().unwrap()])
        .env("HOME", tmp.path())
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("claude inbox"), "{stdout}");
    assert!(stdout.contains("2 messages"), "{stdout}");
    assert!(stdout.contains("codex inbox"), "{stdout}");
    assert!(stdout.contains("1 message"), "{stdout}");

    // cowork-status must NOT drain
    let drained = drain(&mempal_home, Tool::Claude, &repo).unwrap();
    assert_eq!(drained.len(), 2, "cowork-status must not have drained");
}

#[test]
fn cowork_install_hooks_writes_claude_hook_script_with_exec_bit() {
    let tmp = TempDir::new().unwrap();
    let output = Command::new(mempal_bin())
        .args(["cowork-install-hooks"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));

    let script = tmp.path().join(".claude/hooks/user-prompt-submit.sh");
    assert!(script.exists(), "hook script not created");
    let content = fs::read_to_string(&script).unwrap();
    assert!(content.contains("mempal cowork-drain"));
    assert!(content.contains("--target claude"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&script).unwrap().permissions().mode();
        assert_ne!(mode & 0o100, 0, "owner execute bit must be set");
    }
}

#[test]
fn cowork_install_hooks_writes_correct_codex_hooks_json_shape() {
    let tmp = TempDir::new().unwrap();
    let fake_home = tmp.path().join("home");
    fs::create_dir_all(&fake_home).unwrap();
    // current_dir must also be writable for the Claude hook part
    let proj = tmp.path().join("proj");
    fs::create_dir_all(&proj).unwrap();

    let output = Command::new(mempal_bin())
        .args(["cowork-install-hooks", "--global-codex"])
        .current_dir(&proj)
        .env("HOME", &fake_home)
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let hooks_path = fake_home.join(".codex/hooks.json");
    assert!(hooks_path.exists(), "hooks.json not created");
    let content = fs::read_to_string(&hooks_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();

    // Nested shape verification
    assert!(parsed["hooks"].is_object());
    assert!(parsed["hooks"]["UserPromptSubmit"].is_array());
    assert!(
        parsed["hooks"]["UserPromptSubmit"]
            .as_array()
            .unwrap()
            .len()
            >= 1
    );

    let entry = &parsed["hooks"]["UserPromptSubmit"][0];
    assert!(entry["hooks"].is_array());
    let handler = &entry["hooks"][0];
    assert_eq!(handler["type"], "command");
    let cmd = handler["command"].as_str().unwrap();
    assert!(cmd.contains("mempal cowork-drain"));
    assert!(cmd.contains("--target codex"));
    assert!(cmd.contains("--format codex-hook-json"));
    assert!(cmd.contains("--cwd-source stdin-json"));
    assert!(!cmd.contains("$PWD"), "must not reference $PWD");
    assert!(
        entry.get("matcher").is_none() || entry["matcher"].is_null(),
        "matcher must not be present"
    );
}
