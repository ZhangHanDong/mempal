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

    let task_a = tokio::task::spawn_blocking(move || drain(&home_a, Tool::Codex, &repo_a).unwrap());
    let task_b = tokio::task::spawn_blocking(move || drain(&home_b, Tool::Codex, &repo_b).unwrap());

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
fn cowork_drain_cli_rejects_auto_target() {
    // Guard for Codex review finding 1: `mempal cowork-drain --target auto`
    // used to parse via `Tool::from_str_ci` which silently accepted "auto".
    // Spec line 39 limits target to `claude|codex`. CLI still exits 0 per
    // graceful-degrade contract (stdout empty, error → stderr).
    let tmp = TempDir::new().unwrap();
    let output = Command::new(mempal_bin())
        .args(["cowork-drain", "--target", "auto", "--cwd", "/tmp/whatever"])
        .env("HOME", tmp.path())
        .output()
        .expect("spawn");

    // Graceful degrade: exit 0, stdout empty, error on stderr.
    assert_eq!(output.status.code(), Some(0));
    assert!(
        output.stdout.is_empty(),
        "stdout must stay empty on invalid target, got {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid target") && stderr.contains("expected claude|codex"),
        "stderr should reject `auto`, got: {stderr}"
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
        r#"{"cwd":42}"#.to_string(),                         // wrong type
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
fn cowork_install_hooks_registers_claude_user_prompt_submit_in_settings_json() {
    // The bug that this test exists to prevent: Claude Code does NOT
    // auto-discover `.claude/hooks/*.sh` by filename convention. A hook
    // must be registered under `hooks.UserPromptSubmit` in
    // `.claude/settings.json` (type="command", command=<script path>) or
    // it silently never fires. Without this guard, E2E would ship broken
    // while 99/99 automated tests passed — exactly what happened once.
    let tmp = TempDir::new().unwrap();
    let output = Command::new(mempal_bin())
        .args(["cowork-install-hooks"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let settings_path = tmp.path().join(".claude/settings.json");
    assert!(
        settings_path.exists(),
        ".claude/settings.json must be created"
    );
    let content = fs::read_to_string(&settings_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();

    let arr = parsed["hooks"]["UserPromptSubmit"].as_array().unwrap();
    assert_eq!(
        arr.len(),
        1,
        "fresh install should produce exactly one UserPromptSubmit entry"
    );
    let handler = &arr[0]["hooks"][0];
    assert_eq!(handler["type"], "command");
    assert_eq!(
        handler["command"],
        "bash .claude/hooks/user-prompt-submit.sh"
    );
    // Claude Code UserPromptSubmit hooks must NOT carry a matcher (fires
    // on every submit, no tool involvement).
    assert!(arr[0].get("matcher").is_none() || arr[0]["matcher"].is_null());
}

#[test]
fn cowork_install_hooks_preserves_existing_unrelated_settings_json_hooks() {
    // Seeds .claude/settings.json with an existing PostToolUse hook and
    // an unrelated UserPromptSubmit entry, then runs install-hooks. The
    // existing entries must survive; only the UserPromptSubmit array gets
    // the canonical drain command appended.
    let tmp = TempDir::new().unwrap();
    let claude_dir = tmp.path().join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    let seed = serde_json::json!({
        "hooks": {
            "PostToolUse": [
                {
                    "matcher": "Bash",
                    "hooks": [{
                        "type": "command",
                        "command": "echo post-tool-use guard"
                    }]
                }
            ],
            "UserPromptSubmit": [
                {
                    "hooks": [{
                        "type": "command",
                        "command": "echo unrelated prompt hook"
                    }]
                }
            ]
        }
    });
    let settings_path = claude_dir.join("settings.json");
    fs::write(&settings_path, serde_json::to_string_pretty(&seed).unwrap()).unwrap();

    let output = Command::new(mempal_bin())
        .args(["cowork-install-hooks"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));

    let content = fs::read_to_string(&settings_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();

    // PostToolUse untouched.
    let post = parsed["hooks"]["PostToolUse"].as_array().unwrap();
    assert_eq!(post.len(), 1);
    assert_eq!(post[0]["matcher"], "Bash");
    assert_eq!(post[0]["hooks"][0]["command"], "echo post-tool-use guard");

    // UserPromptSubmit: unrelated hook survives, canonical appended.
    let ups = parsed["hooks"]["UserPromptSubmit"].as_array().unwrap();
    assert_eq!(ups.len(), 2);
    let commands: Vec<&str> = ups
        .iter()
        .flat_map(|entry| entry["hooks"].as_array().unwrap().iter())
        .map(|h| h["command"].as_str().unwrap())
        .collect();
    assert!(commands.contains(&"echo unrelated prompt hook"));
    assert!(commands.contains(&"bash .claude/hooks/user-prompt-submit.sh"));
}

#[test]
fn cowork_install_hooks_heals_stale_claude_settings_entry() {
    // Pre-existing stale drain entry in .claude/settings.json (e.g., old
    // mempal version inlined the command directly or used a different
    // script path). Re-running install-hooks must self-heal: remove the
    // stale entry, append canonical, and preserve unrelated hooks.
    let tmp = TempDir::new().unwrap();
    let claude_dir = tmp.path().join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    let seed = serde_json::json!({
        "hooks": {
            "UserPromptSubmit": [
                {
                    "hooks": [{
                        "type": "command",
                        // Stale: old-style inline command that bypassed the .sh file.
                        "command": "mempal cowork-drain --target claude --cwd \"$PWD\""
                    }]
                },
                {
                    // Stale with matcher — should also be healed.
                    "hooks": [{
                        "type": "command",
                        "command": "bash /some/old/path/user-prompt-submit.sh"
                    }]
                },
                {
                    // Unrelated — must survive.
                    "hooks": [{
                        "type": "command",
                        "command": "echo unrelated survives"
                    }]
                }
            ]
        }
    });
    let settings_path = claude_dir.join("settings.json");
    fs::write(&settings_path, serde_json::to_string_pretty(&seed).unwrap()).unwrap();

    let output = Command::new(mempal_bin())
        .args(["cowork-install-hooks"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("healed stale Claude Code drain hook"),
        "install-hooks output must mention healing, got: {stdout}"
    );

    let content = fs::read_to_string(&settings_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    let ups = parsed["hooks"]["UserPromptSubmit"].as_array().unwrap();

    let commands: Vec<&str> = ups
        .iter()
        .flat_map(|entry| entry["hooks"].as_array().unwrap().iter())
        .map(|h| h["command"].as_str().unwrap())
        .collect();

    // Both stale entries removed.
    assert!(
        !commands
            .iter()
            .any(|c| c.contains("cowork-drain --target claude --cwd \"$PWD\"")),
        "stale inline command must be removed, got: {commands:?}"
    );
    assert!(
        !commands
            .iter()
            .any(|c| c.contains("/some/old/path/user-prompt-submit.sh")),
        "stale path command must be removed, got: {commands:?}"
    );
    // Canonical present exactly once.
    let canonical_count = commands
        .iter()
        .filter(|c| **c == "bash .claude/hooks/user-prompt-submit.sh")
        .count();
    assert_eq!(canonical_count, 1, "canonical must appear exactly once");
    // Unrelated preserved.
    assert!(
        commands.contains(&"echo unrelated survives"),
        "unrelated hook must be preserved, got: {commands:?}"
    );
}

#[test]
fn cowork_install_hooks_is_idempotent_for_claude_settings_json() {
    // Running install-hooks 3 times on the same repo must leave
    // .claude/settings.json with exactly one canonical drain entry and
    // print "already registered (no-op)" on the 2nd and 3rd runs.
    let tmp = TempDir::new().unwrap();

    for i in 0..3 {
        let output = Command::new(mempal_bin())
            .args(["cowork-install-hooks"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0));
        let stdout = String::from_utf8_lossy(&output.stdout);
        if i == 0 {
            assert!(
                stdout.contains("registered Claude Code hook"),
                "first run should register, got: {stdout}"
            );
        } else {
            assert!(
                stdout.contains("already registered"),
                "run #{i} should be no-op, got: {stdout}"
            );
        }
    }

    let settings_path = tmp.path().join(".claude/settings.json");
    let content = fs::read_to_string(&settings_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    let ups = parsed["hooks"]["UserPromptSubmit"].as_array().unwrap();
    let canonical_count = ups
        .iter()
        .flat_map(|entry| entry["hooks"].as_array().unwrap().iter())
        .filter(|h| h["command"].as_str() == Some("bash .claude/hooks/user-prompt-submit.sh"))
        .count();
    assert_eq!(
        canonical_count, 1,
        "expected exactly 1 canonical entry after 3 invocations"
    );
}

#[test]
fn cowork_install_hooks_warns_when_codex_hooks_feature_flag_missing() {
    // Spec finding #3 from E2E run: Codex's hooks runtime is gated behind
    // the `codex_hooks` feature flag. In shipped codex-cli (<= 0.120.0) the
    // flag is "under development" and OFF by default, so ~/.codex/hooks.json
    // is silently ignored. install-hooks --global-codex must surface this
    // clearly — otherwise users see "✓ merged Codex hook" and assume the
    // pipeline is live when it isn't.
    let tmp = TempDir::new().unwrap();
    let fake_home = tmp.path().join("home");
    fs::create_dir_all(fake_home.join(".codex")).unwrap();
    // Seed a config.toml WITHOUT any features section — simulates the
    // default state of a fresh codex-cli install.
    fs::write(
        fake_home.join(".codex/config.toml"),
        "model = \"gpt-5-codex\"\n",
    )
    .unwrap();
    let proj = tmp.path().join("proj");
    fs::create_dir_all(&proj).unwrap();

    let output = Command::new(mempal_bin())
        .args(["cowork-install-hooks", "--global-codex"])
        .current_dir(&proj)
        .env("HOME", &fake_home)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);

    // The warning text must be specific enough that a user can act on it.
    assert!(
        stdout.contains("codex_hooks` feature is currently disabled"),
        "install-hooks must warn about disabled codex_hooks feature, got stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("codex features enable codex_hooks"),
        "warning must tell user the exact command to activate, got stdout:\n{stdout}"
    );
}

#[test]
fn cowork_install_hooks_no_warning_when_codex_hooks_feature_enabled() {
    // Mirror test: when config.toml already has `codex_hooks = true`
    // (either inline `features.codex_hooks = true` or under `[features]`),
    // install-hooks must NOT print the warning.
    for toml_content in [
        "model = \"gpt-5-codex\"\n[features]\ncodex_hooks = true\n",
        "model = \"gpt-5-codex\"\nfeatures.codex_hooks = true\n",
    ] {
        let tmp = TempDir::new().unwrap();
        let fake_home = tmp.path().join("home");
        fs::create_dir_all(fake_home.join(".codex")).unwrap();
        fs::write(fake_home.join(".codex/config.toml"), toml_content).unwrap();
        let proj = tmp.path().join("proj");
        fs::create_dir_all(&proj).unwrap();

        let output = Command::new(mempal_bin())
            .args(["cowork-install-hooks", "--global-codex"])
            .current_dir(&proj)
            .env("HOME", &fake_home)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0));
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            !stdout.contains("codex_hooks` feature is currently disabled"),
            "install-hooks must NOT warn when feature already enabled (toml: {toml_content:?}), got stdout:\n{stdout}"
        );
    }
}

#[test]
fn cowork_install_hooks_warns_when_codex_hooks_feature_explicitly_false() {
    // Edge case: user explicitly set codex_hooks = false (e.g., they
    // tried it, had issues, disabled it). install-hooks must still warn
    // — the installed hook is non-functional until they flip it back.
    let tmp = TempDir::new().unwrap();
    let fake_home = tmp.path().join("home");
    fs::create_dir_all(fake_home.join(".codex")).unwrap();
    fs::write(
        fake_home.join(".codex/config.toml"),
        "[features]\ncodex_hooks = false\n",
    )
    .unwrap();
    let proj = tmp.path().join("proj");
    fs::create_dir_all(&proj).unwrap();

    let output = Command::new(mempal_bin())
        .args(["cowork-install-hooks", "--global-codex"])
        .current_dir(&proj)
        .env("HOME", &fake_home)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("codex_hooks` feature is currently disabled"),
        "install-hooks must warn when feature is explicitly false, got stdout:\n{stdout}"
    );
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
        !parsed["hooks"]["UserPromptSubmit"]
            .as_array()
            .unwrap()
            .is_empty()
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

#[test]
fn cowork_install_hooks_is_idempotent_for_global_codex() {
    // Running `cowork-install-hooks --global-codex` multiple times must NOT
    // append duplicate entries to ~/.codex/hooks.json. Otherwise, each user
    // turn would trigger the drain hook N times (one per invocation).
    let tmp = TempDir::new().unwrap();
    let fake_home = tmp.path().join("home");
    fs::create_dir_all(&fake_home).unwrap();
    let proj = tmp.path().join("proj");
    fs::create_dir_all(&proj).unwrap();

    // Run install-hooks --global-codex THREE times.
    for _ in 0..3 {
        let output = Command::new(mempal_bin())
            .args(["cowork-install-hooks", "--global-codex"])
            .current_dir(&proj)
            .env("HOME", &fake_home)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0));
    }

    let hooks_path = fake_home.join(".codex/hooks.json");
    let content = fs::read_to_string(&hooks_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();

    // After 3 invocations, UserPromptSubmit array must still have exactly 1
    // mempal cowork-drain entry.
    let arr = parsed["hooks"]["UserPromptSubmit"].as_array().unwrap();
    let mempal_entries = arr
        .iter()
        .filter(|entry| {
            entry
                .get("hooks")
                .and_then(|h| h.as_array())
                .map(|inner| {
                    inner.iter().any(|h| {
                        h.get("command")
                            .and_then(|c| c.as_str())
                            .map(|cmd| cmd.contains("mempal cowork-drain"))
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false)
        })
        .count();
    assert_eq!(
        mempal_entries, 1,
        "install-hooks must be idempotent; expected exactly 1 mempal drain \
         entry after 3 invocations, got {mempal_entries}"
    );
}

#[test]
fn cowork_install_hooks_heals_stale_codex_drain_entry() {
    // Guard for Codex review finding 2: the earlier idempotency fix used a
    // loose substring match, so a pre-existing stale `mempal cowork-drain`
    // handler (wrong target/format, old flag set from a previous mempal
    // version) would be silently left in place — re-install would no-op
    // instead of self-healing. Spec line 48 pins the canonical command.
    //
    // This test seeds ~/.codex/hooks.json with a stale entry, runs
    // install-hooks --global-codex, and asserts the final state has
    // exactly one mempal drain entry and it equals the pinned canonical
    // command.
    let tmp = TempDir::new().unwrap();
    let fake_home = tmp.path().join("home");
    fs::create_dir_all(fake_home.join(".codex")).unwrap();
    let proj = tmp.path().join("proj");
    fs::create_dir_all(&proj).unwrap();

    // Seed a hooks.json where the existing mempal-drain handler has wrong
    // flags (missing --cwd-source, wrong format) — this is what a user
    // with an older mempal would have.
    let seed = serde_json::json!({
        "hooks": {
            "UserPromptSubmit": [
                {
                    "hooks": [{
                        "type": "command",
                        "command": "mempal cowork-drain --target codex --format plain",
                        "statusMessage": "stale mempal"
                    }]
                },
                {
                    // An unrelated hook that must survive untouched.
                    "hooks": [{
                        "type": "command",
                        "command": "echo unrelated",
                        "statusMessage": "unrelated"
                    }]
                }
            ]
        }
    });
    let hooks_path = fake_home.join(".codex/hooks.json");
    fs::write(&hooks_path, serde_json::to_string_pretty(&seed).unwrap()).unwrap();

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

    let content = fs::read_to_string(&hooks_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    let arr = parsed["hooks"]["UserPromptSubmit"].as_array().unwrap();

    let canonical =
        "mempal cowork-drain --target codex --format codex-hook-json --cwd-source stdin-json";

    // Collect all drain-related commands still present.
    let drain_cmds: Vec<&str> = arr
        .iter()
        .flat_map(|entry| {
            entry
                .get("hooks")
                .and_then(|h| h.as_array())
                .map(|inner| inner.iter())
                .into_iter()
                .flatten()
        })
        .filter_map(|h| h.get("command").and_then(|c| c.as_str()))
        .filter(|c| c.contains("mempal cowork-drain"))
        .collect();

    assert_eq!(
        drain_cmds.len(),
        1,
        "expected exactly 1 mempal drain entry after self-heal, got {drain_cmds:?}"
    );
    assert_eq!(
        drain_cmds[0], canonical,
        "stale entry must be replaced by canonical command"
    );

    // Unrelated hook must survive — install-hooks must only touch mempal
    // drain entries, not wipe the whole UserPromptSubmit array.
    let survived_unrelated = arr.iter().any(|entry| {
        entry
            .get("hooks")
            .and_then(|h| h.as_array())
            .map(|inner| {
                inner.iter().any(|h| {
                    h.get("command")
                        .and_then(|c| c.as_str())
                        .map(|cmd| cmd == "echo unrelated")
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false)
    });
    assert!(
        survived_unrelated,
        "unrelated UserPromptSubmit hook must be preserved"
    );
}
