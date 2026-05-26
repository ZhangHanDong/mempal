//! Integration tests for P84 multi-agent cowork bus.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn mempal_bin() -> String {
    env!("CARGO_BIN_EXE_mempal").to_string()
}

fn setup_repo(tmp: &TempDir, name: &str) -> PathBuf {
    let repo = tmp.path().join(name);
    fs::create_dir_all(repo.join(".git")).expect("create fake git repo");
    repo
}

fn run_mempal(home: &TempDir, args: &[&str]) -> std::process::Output {
    Command::new(mempal_bin())
        .args(args)
        .env("HOME", home.path())
        .output()
        .expect("run mempal")
}

fn run_mempal_with_env(
    home: &TempDir,
    args: &[&str],
    envs: &[(&str, String)],
) -> std::process::Output {
    let mut command = Command::new(mempal_bin());
    command.args(args).env("HOME", home.path());
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().expect("run mempal")
}

fn install_fake_tmux(home: &TempDir, exit_code: i32) -> (String, PathBuf) {
    let bin_dir = home.path().join("fake-bin");
    fs::create_dir_all(&bin_dir).expect("create fake bin");
    let log_path = home.path().join("tmux.log");
    let script = format!("#!/bin/sh\nprintf '%s\\n' \"$@\" >> \"$TMUX_LOG\"\nexit {exit_code}\n");
    let tmux_path = bin_dir.join("tmux");
    fs::write(&tmux_path, script).expect("write fake tmux");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&tmux_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&tmux_path, perms).unwrap();
    }
    let old_path = std::env::var("PATH").unwrap_or_default();
    let path = format!("{}:{old_path}", bin_dir.display());
    (path, log_path)
}

fn install_fake_tmux_capture(home: &TempDir) -> (String, PathBuf) {
    let bin_dir = home.path().join("fake-capture-bin");
    fs::create_dir_all(&bin_dir).expect("create fake bin");
    let log_path = home.path().join("tmux-capture.log");
    let script = "#!/bin/sh\nprintf '%s\\n' \"$@\" >> \"$TMUX_LOG\"\nif [ \"$1\" = \"capture-pane\" ]; then printf 'pane line 1\\npane line 2\\n'; fi\nexit 0\n";
    let tmux_path = bin_dir.join("tmux");
    fs::write(&tmux_path, script).expect("write fake tmux");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&tmux_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&tmux_path, perms).unwrap();
    }
    let old_path = std::env::var("PATH").unwrap_or_default();
    let path = format!("{}:{old_path}", bin_dir.display());
    (path, log_path)
}

fn stdout(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn assert_success(output: &std::process::Output) {
    assert!(
        output.status.success(),
        "command failed\nstdout:\n{}\nstderr:\n{}",
        stdout(output),
        stderr(output)
    );
}

fn message_id_from_send(output: &std::process::Output) -> String {
    stdout(output)
        .split_whitespace()
        .find_map(|part| part.strip_prefix("message_id="))
        .expect("message_id in send output")
        .to_string()
}

fn register(home: &TempDir, repo: &Path, agent_id: &str, tool: &str) {
    let output = run_mempal(
        home,
        &[
            "cowork-register",
            "--agent-id",
            agent_id,
            "--tool",
            tool,
            "--cwd",
            repo.to_str().unwrap(),
        ],
    );
    assert_success(&output);
}

fn bus_project_dir(home: &TempDir, repo: &Path) -> PathBuf {
    let encoded = repo.to_string_lossy().replace('/', "-");
    home.path().join(".mempal/cowork-bus").join(encoded)
}

fn bus_events_path(home: &TempDir, repo: &Path) -> PathBuf {
    bus_project_dir(home, repo).join("events.jsonl")
}

fn sessions_path(home: &TempDir, repo: &Path) -> PathBuf {
    bus_project_dir(home, repo).join("sessions.json")
}

fn palace_db_path(home: &TempDir) -> PathBuf {
    home.path().join(".mempal/palace.db")
}

fn event_lines(home: &TempDir, repo: &Path) -> Vec<String> {
    fs::read_to_string(bus_events_path(home, repo))
        .expect("events file")
        .lines()
        .map(str::to_string)
        .collect()
}

fn failed_event_id(home: &TempDir, repo: &Path) -> String {
    let output = run_mempal(
        home,
        &[
            "cowork-events",
            "--cwd",
            repo.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert_success(&output);
    let parsed: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("events json");
    parsed
        .as_array()
        .expect("events array")
        .iter()
        .find(|event| event["status"] == "failed")
        .and_then(|event| event["event_id"].as_str())
        .expect("failed event id")
        .to_string()
}

#[test]
fn test_cli_cowork_register_and_agents_list() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "multi-agent-proj");

    register(&home, &repo, "claude-main", "claude");
    register(&home, &repo, "codex-a", "codex");
    register(&home, &repo, "codex-b", "codex");

    let output = run_mempal(&home, &["cowork-agents", "--cwd", repo.to_str().unwrap()]);
    assert_success(&output);
    let out = stdout(&output);
    assert!(out.contains("claude-main"), "{out}");
    assert!(out.contains("codex-a"), "{out}");
    assert!(out.contains("codex-b"), "{out}");
    assert!(out.contains("tool=claude"), "{out}");
    assert!(out.contains("tool=codex"), "{out}");
    assert!(out.contains("transport=inbox"), "{out}");

    let project_dir = bus_project_dir(&home, &repo);
    assert!(
        project_dir.join("agents.json").exists(),
        "registry must be under cowork-bus/<encoded_project>"
    );
    assert!(
        !home.path().join(".mempal/palace.db").exists(),
        "multi-agent bus commands must not create palace.db"
    );
}

#[test]
fn test_cli_cowork_heartbeat_updates_presence() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "presence-proj");
    register(&home, &repo, "codex-a", "codex");

    let heartbeat = run_mempal(
        &home,
        &[
            "cowork-heartbeat",
            "--agent-id",
            "codex-a",
            "--cwd",
            repo.to_str().unwrap(),
            "--seen-at",
            "2026-05-25T00:00:00Z",
        ],
    );
    assert_success(&heartbeat);
    assert!(stdout(&heartbeat).contains("last_seen_at=2026-05-25T00:00:00Z"));

    let agents = run_mempal(
        &home,
        &[
            "cowork-agents",
            "--cwd",
            repo.to_str().unwrap(),
            "--now",
            "2026-05-25T00:05:00Z",
        ],
    );
    assert_success(&agents);
    let out = stdout(&agents);
    assert!(out.contains("codex-a"), "{out}");
    assert!(out.contains("presence=online"), "{out}");
    assert!(out.contains("last_seen_at=2026-05-25T00:00:00Z"), "{out}");
}

#[test]
fn test_cli_cowork_agents_marks_stale_presence() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "presence-stale-proj");
    register(&home, &repo, "codex-a", "codex");
    let heartbeat = run_mempal(
        &home,
        &[
            "cowork-heartbeat",
            "--agent-id",
            "codex-a",
            "--cwd",
            repo.to_str().unwrap(),
            "--seen-at",
            "2026-05-25T00:00:00Z",
        ],
    );
    assert_success(&heartbeat);

    let agents = run_mempal(
        &home,
        &[
            "cowork-agents",
            "--cwd",
            repo.to_str().unwrap(),
            "--now",
            "2026-05-25T00:11:00Z",
        ],
    );
    assert_success(&agents);
    assert!(stdout(&agents).contains("presence=stale"));
}

#[test]
fn test_cli_cowork_heartbeat_rejects_unknown_agent() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "presence-missing-proj");

    let heartbeat = run_mempal(
        &home,
        &[
            "cowork-heartbeat",
            "--agent-id",
            "codex-missing",
            "--cwd",
            repo.to_str().unwrap(),
        ],
    );
    assert!(!heartbeat.status.success());
    assert!(stderr(&heartbeat).contains("unknown agent"));
}

#[test]
fn test_cli_cowork_send_drains_only_target_agent() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "multi-agent-proj");
    register(&home, &repo, "claude-main", "claude");
    register(&home, &repo, "codex-a", "codex");
    register(&home, &repo, "codex-b", "codex");

    let send = run_mempal(
        &home,
        &[
            "cowork-send",
            "--from",
            "claude-main",
            "--to",
            "codex-a",
            "--cwd",
            repo.to_str().unwrap(),
            "--message",
            "review P84 bus routing",
        ],
    );
    assert_success(&send);

    let drain_a = run_mempal(
        &home,
        &[
            "cowork-agent-drain",
            "--agent-id",
            "codex-a",
            "--cwd",
            repo.to_str().unwrap(),
        ],
    );
    assert_success(&drain_a);
    assert!(stdout(&drain_a).contains("review P84 bus routing"));

    let drain_b = run_mempal(
        &home,
        &[
            "cowork-agent-drain",
            "--agent-id",
            "codex-b",
            "--cwd",
            repo.to_str().unwrap(),
        ],
    );
    assert_success(&drain_b);
    assert_eq!(stdout(&drain_b), "");

    let drain_a_again = run_mempal(
        &home,
        &[
            "cowork-agent-drain",
            "--agent-id",
            "codex-a",
            "--cwd",
            repo.to_str().unwrap(),
        ],
    );
    assert_success(&drain_a_again);
    assert_eq!(stdout(&drain_a_again), "");
}

#[test]
fn test_cli_cowork_delivery_status_pending() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "delivery-pending-proj");
    register(&home, &repo, "claude-main", "claude");
    register(&home, &repo, "codex-a", "codex");

    let send = run_mempal(
        &home,
        &[
            "cowork-send",
            "--from",
            "claude-main",
            "--to",
            "codex-a",
            "--cwd",
            repo.to_str().unwrap(),
            "--message",
            "pending delivery",
        ],
    );
    assert_success(&send);
    let message_id = message_id_from_send(&send);
    assert!(message_id.starts_with("evt-"), "{message_id}");

    let deliveries = run_mempal(
        &home,
        &["cowork-deliveries", "--cwd", repo.to_str().unwrap()],
    );
    assert_success(&deliveries);
    let out = stdout(&deliveries);
    assert!(out.contains(&message_id), "{out}");
    assert!(out.contains("status=pending"), "{out}");
    assert!(out.contains("target=codex-a"), "{out}");
}

#[test]
fn test_cli_cowork_delivery_status_drained() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "delivery-drained-proj");
    register(&home, &repo, "claude-main", "claude");
    register(&home, &repo, "codex-a", "codex");

    let send = run_mempal(
        &home,
        &[
            "cowork-send",
            "--from",
            "claude-main",
            "--to",
            "codex-a",
            "--cwd",
            repo.to_str().unwrap(),
            "--message",
            "drain changes status",
        ],
    );
    assert_success(&send);
    let message_id = message_id_from_send(&send);
    let drain = run_mempal(
        &home,
        &[
            "cowork-agent-drain",
            "--agent-id",
            "codex-a",
            "--cwd",
            repo.to_str().unwrap(),
        ],
    );
    assert_success(&drain);

    let deliveries = run_mempal(
        &home,
        &[
            "cowork-deliveries",
            "--cwd",
            repo.to_str().unwrap(),
            "--agent-id",
            "codex-a",
        ],
    );
    assert_success(&deliveries);
    let out = stdout(&deliveries);
    assert!(out.contains(&message_id), "{out}");
    assert!(out.contains("status=drained"), "{out}");
}

#[test]
fn test_cli_cowork_ack_marks_delivery_acked() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "delivery-acked-proj");
    register(&home, &repo, "claude-main", "claude");
    register(&home, &repo, "codex-a", "codex");

    let send = run_mempal(
        &home,
        &[
            "cowork-send",
            "--from",
            "claude-main",
            "--to",
            "codex-a",
            "--cwd",
            repo.to_str().unwrap(),
            "--message",
            "ack without drain",
        ],
    );
    assert_success(&send);
    let message_id = message_id_from_send(&send);

    let ack = run_mempal(
        &home,
        &[
            "cowork-ack",
            "--agent-id",
            "codex-a",
            "--cwd",
            repo.to_str().unwrap(),
            "--message-id",
            &message_id,
        ],
    );
    assert_success(&ack);
    assert!(stdout(&ack).contains("status=acked"));

    let deliveries = run_mempal(
        &home,
        &[
            "cowork-deliveries",
            "--cwd",
            repo.to_str().unwrap(),
            "--agent-id",
            "codex-a",
        ],
    );
    assert_success(&deliveries);
    assert!(stdout(&deliveries).contains("status=acked"));

    let drain = run_mempal(
        &home,
        &[
            "cowork-agent-drain",
            "--agent-id",
            "codex-a",
            "--cwd",
            repo.to_str().unwrap(),
        ],
    );
    assert_success(&drain);
    assert!(stdout(&drain).contains("ack without drain"));
}

#[test]
fn test_cli_cowork_send_with_thread_metadata() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "thread-proj");
    register(&home, &repo, "claude-main", "claude");
    register(&home, &repo, "codex-a", "codex");

    let send = run_mempal(
        &home,
        &[
            "cowork-send",
            "--from",
            "claude-main",
            "--to",
            "codex-a",
            "--cwd",
            repo.to_str().unwrap(),
            "--message",
            "threaded message",
            "--thread-id",
            "p90-review",
        ],
    );
    assert_success(&send);

    let drain = run_mempal(
        &home,
        &[
            "cowork-agent-drain",
            "--agent-id",
            "codex-a",
            "--cwd",
            repo.to_str().unwrap(),
        ],
    );
    assert_success(&drain);
    assert!(stdout(&drain).contains("thread=p90-review"));

    let events = run_mempal(&home, &["cowork-events", "--cwd", repo.to_str().unwrap()]);
    assert_success(&events);
    assert!(stdout(&events).contains("thread_id=p90-review"));
}

#[test]
fn test_cli_cowork_channel_send_fans_out() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "channel-proj");
    register(&home, &repo, "claude-main", "claude");
    register(&home, &repo, "codex-a", "codex");
    register(&home, &repo, "codex-b", "codex");

    let set = run_mempal(
        &home,
        &[
            "cowork-channel-set",
            "--channel",
            "review",
            "--agent",
            "codex-a",
            "--agent",
            "codex-b",
            "--cwd",
            repo.to_str().unwrap(),
        ],
    );
    assert_success(&set);

    let send = run_mempal(
        &home,
        &[
            "cowork-channel-send",
            "--from",
            "claude-main",
            "--channel",
            "review",
            "--cwd",
            repo.to_str().unwrap(),
            "--message",
            "review channel update",
            "--thread-id",
            "p90-review",
        ],
    );
    assert_success(&send);

    for agent_id in ["codex-a", "codex-b"] {
        let drain = run_mempal(
            &home,
            &[
                "cowork-agent-drain",
                "--agent-id",
                agent_id,
                "--cwd",
                repo.to_str().unwrap(),
            ],
        );
        assert_success(&drain);
        let out = stdout(&drain);
        assert!(out.contains("review channel update"), "{out}");
        assert!(out.contains("thread=p90-review"), "{out}");
        assert!(out.contains("channel=review"), "{out}");
    }

    let deliveries = run_mempal(
        &home,
        &[
            "cowork-deliveries",
            "--cwd",
            repo.to_str().unwrap(),
            "--agent-id",
            "codex-a",
        ],
    );
    assert_success(&deliveries);
    assert!(stdout(&deliveries).contains("channel=review"));
}

#[test]
fn test_cli_cowork_channel_set_replaces_members() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "channel-replace-proj");
    register(&home, &repo, "claude-main", "claude");
    register(&home, &repo, "codex-a", "codex");
    register(&home, &repo, "codex-b", "codex");

    let set_initial = run_mempal(
        &home,
        &[
            "cowork-channel-set",
            "--channel",
            "review",
            "--agent",
            "codex-a",
            "--agent",
            "codex-b",
            "--cwd",
            repo.to_str().unwrap(),
        ],
    );
    assert_success(&set_initial);
    let set_replacement = run_mempal(
        &home,
        &[
            "cowork-channel-set",
            "--channel",
            "review",
            "--agent",
            "codex-b",
            "--cwd",
            repo.to_str().unwrap(),
        ],
    );
    assert_success(&set_replacement);

    let send = run_mempal(
        &home,
        &[
            "cowork-channel-send",
            "--from",
            "claude-main",
            "--channel",
            "review",
            "--cwd",
            repo.to_str().unwrap(),
            "--message",
            "only b should see this",
        ],
    );
    assert_success(&send);

    let drain_a = run_mempal(
        &home,
        &[
            "cowork-agent-drain",
            "--agent-id",
            "codex-a",
            "--cwd",
            repo.to_str().unwrap(),
        ],
    );
    assert_success(&drain_a);
    assert_eq!(stdout(&drain_a), "");

    let drain_b = run_mempal(
        &home,
        &[
            "cowork-agent-drain",
            "--agent-id",
            "codex-b",
            "--cwd",
            repo.to_str().unwrap(),
        ],
    );
    assert_success(&drain_b);
    assert!(stdout(&drain_b).contains("only b should see this"));
}

#[test]
fn test_cli_cowork_channel_send_rejects_unknown_channel() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "channel-missing-proj");
    register(&home, &repo, "claude-main", "claude");

    let send = run_mempal(
        &home,
        &[
            "cowork-channel-send",
            "--from",
            "claude-main",
            "--channel",
            "missing",
            "--cwd",
            repo.to_str().unwrap(),
            "--message",
            "nope",
        ],
    );
    assert!(!send.status.success());
    assert!(stderr(&send).contains("unknown channel"));
}

#[test]
fn test_cli_cowork_events_records_register_send_drain() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "events-proj");
    register(&home, &repo, "claude-main", "claude");
    register(&home, &repo, "codex-a", "codex");

    let send = run_mempal(
        &home,
        &[
            "cowork-send",
            "--from",
            "claude-main",
            "--to",
            "codex-a",
            "--cwd",
            repo.to_str().unwrap(),
            "--message",
            "record this handoff",
        ],
    );
    assert_success(&send);

    let drain = run_mempal(
        &home,
        &[
            "cowork-agent-drain",
            "--agent-id",
            "codex-a",
            "--cwd",
            repo.to_str().unwrap(),
        ],
    );
    assert_success(&drain);

    let events = run_mempal(&home, &["cowork-events", "--cwd", repo.to_str().unwrap()]);
    assert_success(&events);
    let out = stdout(&events);
    assert!(out.contains("evt-"), "{out}");
    assert!(out.contains("type=register"), "{out}");
    assert!(out.contains("type=send"), "{out}");
    assert!(out.contains("type=drain"), "{out}");
    assert!(out.contains("status=registered"), "{out}");
    assert!(out.contains("status=delivered"), "{out}");
    assert!(out.contains("status=drained"), "{out}");
    assert!(out.contains("actor=claude-main"), "{out}");
    assert!(out.contains("targets=codex-a"), "{out}");
    assert!(
        bus_events_path(&home, &repo).exists(),
        "events.jsonl must exist under the project bus directory"
    );
}

#[test]
fn test_cli_cowork_events_json_is_machine_readable() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "events-json-proj");
    register(&home, &repo, "claude-main", "claude");

    let events = run_mempal(
        &home,
        &[
            "cowork-events",
            "--cwd",
            repo.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert_success(&events);
    assert_eq!(stderr(&events), "");
    let parsed: serde_json::Value = serde_json::from_str(&stdout(&events)).expect("json events");
    let array = parsed.as_array().expect("events array");
    assert_eq!(array.len(), 1);
    assert_eq!(array[0]["event_type"], "register");
    assert_eq!(array[0]["status"], "registered");
}

#[test]
fn test_cli_cowork_events_file_output_is_append_only() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "events-append-proj");
    register(&home, &repo, "claude-main", "claude");
    register(&home, &repo, "codex-a", "codex");
    let before = event_lines(&home, &repo);

    let send = run_mempal(
        &home,
        &[
            "cowork-send",
            "--from",
            "claude-main",
            "--to",
            "codex-a",
            "--cwd",
            repo.to_str().unwrap(),
            "--message",
            "append one event",
        ],
    );
    assert_success(&send);

    let after = event_lines(&home, &repo);
    assert_eq!(after.len(), before.len() + 1);
    assert_eq!(&after[..before.len()], before.as_slice());
    assert!(after.last().unwrap().contains("\"event_type\":\"send\""));
}

#[test]
fn test_cli_cowork_events_limit_returns_latest() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "events-limit-proj");
    register(&home, &repo, "claude-main", "claude");
    register(&home, &repo, "codex-a", "codex");

    let send = run_mempal(
        &home,
        &[
            "cowork-send",
            "--from",
            "claude-main",
            "--to",
            "codex-a",
            "--cwd",
            repo.to_str().unwrap(),
            "--message",
            "latest one",
        ],
    );
    assert_success(&send);
    let drain = run_mempal(
        &home,
        &[
            "cowork-agent-drain",
            "--agent-id",
            "codex-a",
            "--cwd",
            repo.to_str().unwrap(),
        ],
    );
    assert_success(&drain);

    let events = run_mempal(
        &home,
        &[
            "cowork-events",
            "--cwd",
            repo.to_str().unwrap(),
            "--limit",
            "2",
        ],
    );
    assert_success(&events);
    let out = stdout(&events);
    let lines: Vec<_> = out.lines().collect();
    assert_eq!(lines.len(), 2, "{out}");
    assert!(out.contains("type=send"), "{out}");
    assert!(out.contains("type=drain"), "{out}");
    assert!(!out.contains("type=register"), "{out}");
}

#[test]
fn test_cli_cowork_broadcast_fans_out_to_each_agent() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "multi-agent-proj");
    register(&home, &repo, "claude-main", "claude");
    register(&home, &repo, "codex-a", "codex");
    register(&home, &repo, "codex-b", "codex");

    let broadcast = run_mempal(
        &home,
        &[
            "cowork-broadcast",
            "--from",
            "claude-main",
            "--to",
            "codex-a",
            "--to",
            "codex-b",
            "--cwd",
            repo.to_str().unwrap(),
            "--message",
            "main changed, pull before continuing",
        ],
    );
    assert_success(&broadcast);

    for agent_id in ["codex-a", "codex-b"] {
        let drain = run_mempal(
            &home,
            &[
                "cowork-agent-drain",
                "--agent-id",
                agent_id,
                "--cwd",
                repo.to_str().unwrap(),
            ],
        );
        assert_success(&drain);
        assert!(stdout(&drain).contains("main changed, pull before continuing"));
    }
}

#[test]
fn test_cli_cowork_bus_rejects_invalid_addressing() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "multi-agent-proj");
    register(&home, &repo, "codex-a", "codex");

    let bad_id = run_mempal(
        &home,
        &[
            "cowork-register",
            "--agent-id",
            "bad/id",
            "--tool",
            "codex",
            "--cwd",
            repo.to_str().unwrap(),
        ],
    );
    assert!(!bad_id.status.success());
    assert!(stderr(&bad_id).contains("invalid agent id"));

    let self_send = run_mempal(
        &home,
        &[
            "cowork-send",
            "--from",
            "codex-a",
            "--to",
            "codex-a",
            "--cwd",
            repo.to_str().unwrap(),
            "--message",
            "self",
        ],
    );
    assert!(!self_send.status.success());
    assert!(stderr(&self_send).contains("cannot send to self"));

    let missing_target = run_mempal(
        &home,
        &[
            "cowork-send",
            "--from",
            "codex-a",
            "--to",
            "codex-missing",
            "--cwd",
            repo.to_str().unwrap(),
            "--message",
            "missing",
        ],
    );
    assert!(!missing_target.status.success());
    assert!(stderr(&missing_target).contains("unknown target agent"));
}

#[test]
fn test_legacy_cowork_status_still_lists_tool_inboxes() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "legacy-proj");

    let output = run_mempal(&home, &["cowork-status", "--cwd", repo.to_str().unwrap()]);
    assert_success(&output);
    let out = stdout(&output);
    assert!(out.contains("claude inbox"), "{out}");
    assert!(out.contains("codex inbox"), "{out}");
    assert!(
        !home.path().join(".mempal/cowork-bus").exists(),
        "legacy status must not require bus registry"
    );
}

#[test]
fn test_cli_cowork_send_to_tmux_transport_invokes_tmux() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "tmux-proj");
    let (path, log_path) = install_fake_tmux(&home, 0);
    register(&home, &repo, "claude-main", "claude");

    let register_tmux = run_mempal(
        &home,
        &[
            "cowork-register",
            "--agent-id",
            "codex-a",
            "--tool",
            "codex",
            "--cwd",
            repo.to_str().unwrap(),
            "--transport",
            "tmux",
            "--tmux-target",
            "mempal:0.1",
        ],
    );
    assert_success(&register_tmux);

    let send = run_mempal_with_env(
        &home,
        &[
            "cowork-send",
            "--from",
            "claude-main",
            "--to",
            "codex-a",
            "--cwd",
            repo.to_str().unwrap(),
            "--message",
            "tmux hello",
        ],
        &[("PATH", path), ("TMUX_LOG", log_path.display().to_string())],
    );
    assert_success(&send);

    let log = fs::read_to_string(&log_path).expect("tmux log");
    assert!(log.contains("send-keys"), "{log}");
    assert!(log.contains("-t"), "{log}");
    assert!(log.contains("mempal:0.1"), "{log}");
    assert!(
        log.contains("[mempal bus from claude-main to codex-a] tmux hello"),
        "{log}"
    );
    assert!(log.contains("Enter"), "{log}");

    let drain = run_mempal(
        &home,
        &[
            "cowork-agent-drain",
            "--agent-id",
            "codex-a",
            "--cwd",
            repo.to_str().unwrap(),
        ],
    );
    assert_success(&drain);
    assert_eq!(stdout(&drain), "");
}

#[test]
fn test_cli_cowork_register_tmux_requires_target() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "tmux-proj");

    let output = run_mempal(
        &home,
        &[
            "cowork-register",
            "--agent-id",
            "codex-a",
            "--tool",
            "codex",
            "--cwd",
            repo.to_str().unwrap(),
            "--transport",
            "tmux",
        ],
    );
    assert!(!output.status.success());
    assert!(stderr(&output).contains("tmux_target is required"));
}

#[test]
fn test_cli_cowork_tmux_failure_does_not_write_inbox() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "tmux-proj");
    let (path, log_path) = install_fake_tmux(&home, 42);
    register(&home, &repo, "claude-main", "claude");
    let register_tmux = run_mempal(
        &home,
        &[
            "cowork-register",
            "--agent-id",
            "codex-a",
            "--tool",
            "codex",
            "--cwd",
            repo.to_str().unwrap(),
            "--transport",
            "tmux",
            "--tmux-target",
            "mempal:0.1",
        ],
    );
    assert_success(&register_tmux);

    let send = run_mempal_with_env(
        &home,
        &[
            "cowork-send",
            "--from",
            "claude-main",
            "--to",
            "codex-a",
            "--cwd",
            repo.to_str().unwrap(),
            "--message",
            "tmux should fail",
        ],
        &[("PATH", path), ("TMUX_LOG", log_path.display().to_string())],
    );
    assert!(!send.status.success());
    assert!(stderr(&send).contains("tmux delivery failed"));

    let drain = run_mempal(
        &home,
        &[
            "cowork-agent-drain",
            "--agent-id",
            "codex-a",
            "--cwd",
            repo.to_str().unwrap(),
        ],
    );
    assert_success(&drain);
    assert_eq!(stdout(&drain), "");
}

#[test]
fn test_cli_cowork_tmux_peek_captures_pane() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "tmux-peek-proj");
    let (path, log_path) = install_fake_tmux_capture(&home);
    let register_tmux = run_mempal(
        &home,
        &[
            "cowork-register",
            "--agent-id",
            "codex-a",
            "--tool",
            "codex",
            "--cwd",
            repo.to_str().unwrap(),
            "--transport",
            "tmux",
            "--tmux-target",
            "mempal:0.1",
        ],
    );
    assert_success(&register_tmux);

    let peek = run_mempal_with_env(
        &home,
        &[
            "cowork-tmux-peek",
            "--agent-id",
            "codex-a",
            "--cwd",
            repo.to_str().unwrap(),
            "--lines",
            "20",
        ],
        &[("PATH", path), ("TMUX_LOG", log_path.display().to_string())],
    );
    assert_success(&peek);
    assert_eq!(stderr(&peek), "");
    assert!(stdout(&peek).contains("pane line 1"));
    let log = fs::read_to_string(&log_path).expect("tmux log");
    assert!(log.contains("capture-pane"), "{log}");
    assert!(log.contains("-t"), "{log}");
    assert!(log.contains("mempal:0.1"), "{log}");
    assert!(log.contains("-p"), "{log}");
    assert!(log.contains("-S"), "{log}");
    assert!(log.contains("-20"), "{log}");
}

#[test]
fn test_cli_cowork_tmux_peek_rejects_inbox_agent() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "tmux-peek-inbox-proj");
    register(&home, &repo, "codex-a", "codex");

    let peek = run_mempal(
        &home,
        &[
            "cowork-tmux-peek",
            "--agent-id",
            "codex-a",
            "--cwd",
            repo.to_str().unwrap(),
        ],
    );
    assert!(!peek.status.success());
    assert!(stderr(&peek).contains("not registered with transport=tmux"));
}

#[test]
fn test_cli_cowork_tmux_peek_has_no_bus_side_effects() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "tmux-peek-side-effect-proj");
    let (path, log_path) = install_fake_tmux_capture(&home);
    let register_tmux = run_mempal(
        &home,
        &[
            "cowork-register",
            "--agent-id",
            "codex-a",
            "--tool",
            "codex",
            "--cwd",
            repo.to_str().unwrap(),
            "--transport",
            "tmux",
            "--tmux-target",
            "mempal:0.1",
        ],
    );
    assert_success(&register_tmux);
    let before = fs::read_to_string(bus_events_path(&home, &repo)).expect("events before");

    let peek = run_mempal_with_env(
        &home,
        &[
            "cowork-tmux-peek",
            "--agent-id",
            "codex-a",
            "--cwd",
            repo.to_str().unwrap(),
        ],
        &[("PATH", path), ("TMUX_LOG", log_path.display().to_string())],
    );
    assert_success(&peek);
    let after = fs::read_to_string(bus_events_path(&home, &repo)).expect("events after");
    assert_eq!(after, before);
    assert!(!home.path().join(".mempal/palace.db").exists());
}

#[test]
fn test_cli_cowork_tmux_peek_does_not_write_file_output() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "tmux-peek-no-file-proj");
    let (path, log_path) = install_fake_tmux_capture(&home);
    let register_tmux = run_mempal(
        &home,
        &[
            "cowork-register",
            "--agent-id",
            "codex-a",
            "--tool",
            "codex",
            "--cwd",
            repo.to_str().unwrap(),
            "--transport",
            "tmux",
            "--tmux-target",
            "mempal:0.1",
        ],
    );
    assert_success(&register_tmux);
    let before_files = fs::read_dir(bus_project_dir(&home, &repo))
        .expect("bus dir")
        .count();

    let peek = run_mempal_with_env(
        &home,
        &[
            "cowork-tmux-peek",
            "--agent-id",
            "codex-a",
            "--cwd",
            repo.to_str().unwrap(),
        ],
        &[("PATH", path), ("TMUX_LOG", log_path.display().to_string())],
    );
    assert_success(&peek);
    let after_files = fs::read_dir(bus_project_dir(&home, &repo))
        .expect("bus dir")
        .count();
    assert_eq!(after_files, before_files);
}

#[test]
fn test_cli_cowork_events_records_tmux_failure() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "events-tmux-proj");
    let (path, log_path) = install_fake_tmux(&home, 42);
    register(&home, &repo, "claude-main", "claude");
    let register_tmux = run_mempal(
        &home,
        &[
            "cowork-register",
            "--agent-id",
            "codex-a",
            "--tool",
            "codex",
            "--cwd",
            repo.to_str().unwrap(),
            "--transport",
            "tmux",
            "--tmux-target",
            "mempal:0.1",
        ],
    );
    assert_success(&register_tmux);

    let send = run_mempal_with_env(
        &home,
        &[
            "cowork-send",
            "--from",
            "claude-main",
            "--to",
            "codex-a",
            "--cwd",
            repo.to_str().unwrap(),
            "--message",
            "tmux should fail and log",
        ],
        &[("PATH", path), ("TMUX_LOG", log_path.display().to_string())],
    );
    assert!(!send.status.success());
    assert!(stderr(&send).contains("tmux delivery failed"));

    let events = run_mempal(&home, &["cowork-events", "--cwd", repo.to_str().unwrap()]);
    assert_success(&events);
    let out = stdout(&events);
    assert!(out.contains("type=send"), "{out}");
    assert!(out.contains("status=failed"), "{out}");
    assert!(out.contains("transport=tmux"), "{out}");
    assert!(out.contains("targets=codex-a"), "{out}");

    let drain = run_mempal(
        &home,
        &[
            "cowork-agent-drain",
            "--agent-id",
            "codex-a",
            "--cwd",
            repo.to_str().unwrap(),
        ],
    );
    assert_success(&drain);
    assert_eq!(stdout(&drain), "");
}

#[test]
fn test_cli_cowork_failed_delivery_cannot_be_acked() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "delivery-failed-proj");
    let (path, log_path) = install_fake_tmux(&home, 42);
    register(&home, &repo, "claude-main", "claude");
    let register_tmux = run_mempal(
        &home,
        &[
            "cowork-register",
            "--agent-id",
            "codex-a",
            "--tool",
            "codex",
            "--cwd",
            repo.to_str().unwrap(),
            "--transport",
            "tmux",
            "--tmux-target",
            "mempal:0.1",
        ],
    );
    assert_success(&register_tmux);

    let send = run_mempal_with_env(
        &home,
        &[
            "cowork-send",
            "--from",
            "claude-main",
            "--to",
            "codex-a",
            "--cwd",
            repo.to_str().unwrap(),
            "--message",
            "failed status",
        ],
        &[("PATH", path), ("TMUX_LOG", log_path.display().to_string())],
    );
    assert!(!send.status.success());
    let message_id = failed_event_id(&home, &repo);

    let deliveries = run_mempal(
        &home,
        &[
            "cowork-deliveries",
            "--cwd",
            repo.to_str().unwrap(),
            "--agent-id",
            "codex-a",
        ],
    );
    assert_success(&deliveries);
    let out = stdout(&deliveries);
    assert!(out.contains(&message_id), "{out}");
    assert!(out.contains("status=failed"), "{out}");

    let ack = run_mempal(
        &home,
        &[
            "cowork-ack",
            "--agent-id",
            "codex-a",
            "--cwd",
            repo.to_str().unwrap(),
            "--message-id",
            &message_id,
        ],
    );
    assert!(!ack.status.success());
    assert!(stderr(&ack).contains("cannot ack failed delivery"));
}

#[test]
fn test_cli_cowork_runbook_plain() {
    let home = TempDir::new().expect("home");
    let output = run_mempal(&home, &["cowork-runbook", "--format", "plain"]);
    assert_success(&output);
    let out = stdout(&output);
    assert!(out.contains("cowork-register"), "{out}");
    assert!(out.contains("cowork-channel-send"), "{out}");
    assert!(out.contains("cowork-tmux-peek"), "{out}");
    assert_eq!(stderr(&output), "");
}

#[test]
fn test_cli_cowork_runbook_json() {
    let home = TempDir::new().expect("home");
    let output = run_mempal(&home, &["cowork-runbook", "--format", "json"]);
    assert_success(&output);
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("runbook json");
    assert!(
        value["title"]
            .as_str()
            .unwrap_or_default()
            .contains("Multi-Agent Cowork Runbook")
    );
    assert!(
        value["content"]
            .as_str()
            .unwrap_or_default()
            .contains("cowork-deliveries")
    );
}

#[test]
fn test_cli_cowork_runbook_rejects_invalid_format() {
    let home = TempDir::new().expect("home");
    let output = run_mempal(&home, &["cowork-runbook", "--format", "yaml"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("unknown format"));
    assert!(!home.path().join(".mempal").exists());
}

#[test]
fn test_cli_cowork_doctor_empty_registry() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "doctor-empty");
    let output = run_mempal(&home, &["cowork-doctor", "--cwd", repo.to_str().unwrap()]);
    assert_success(&output);
    let out = stdout(&output);
    assert!(out.contains("status=warning"), "{out}");
    assert!(out.contains("no registered agents"), "{out}");
    assert!(!bus_events_path(&home, &repo).exists());
}

#[test]
fn test_cli_cowork_doctor_reports_stale_and_pending() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "doctor-stale");
    register(&home, &repo, "claude-main", "claude");
    register(&home, &repo, "codex-a", "codex");
    let heartbeat = run_mempal(
        &home,
        &[
            "cowork-heartbeat",
            "--agent-id",
            "codex-a",
            "--cwd",
            repo.to_str().unwrap(),
            "--seen-at",
            "2026-05-25T00:00:00Z",
        ],
    );
    assert_success(&heartbeat);
    let send = run_mempal(
        &home,
        &[
            "cowork-send",
            "--from",
            "claude-main",
            "--to",
            "codex-a",
            "--cwd",
            repo.to_str().unwrap(),
            "--message",
            "pending doctor",
        ],
    );
    assert_success(&send);

    let output = run_mempal(
        &home,
        &[
            "cowork-doctor",
            "--cwd",
            repo.to_str().unwrap(),
            "--now",
            "2026-05-25T00:20:00Z",
        ],
    );
    assert_success(&output);
    let out = stdout(&output);
    assert!(out.contains("stale_agents=1"), "{out}");
    assert!(out.contains("pending_deliveries=1"), "{out}");
    assert!(out.contains("status=warning"), "{out}");
}

#[test]
fn test_cli_cowork_doctor_json_tmux_probe() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "doctor-tmux");
    let (path, log_path) = install_fake_tmux(&home, 0);
    let register_tmux = run_mempal(
        &home,
        &[
            "cowork-register",
            "--agent-id",
            "codex-a",
            "--tool",
            "codex",
            "--cwd",
            repo.to_str().unwrap(),
            "--transport",
            "tmux",
            "--tmux-target",
            "mempal:0.1",
        ],
    );
    assert_success(&register_tmux);
    let output = run_mempal_with_env(
        &home,
        &[
            "cowork-doctor",
            "--cwd",
            repo.to_str().unwrap(),
            "--probe-tmux",
            "--format",
            "json",
        ],
        &[("PATH", path), ("TMUX_LOG", log_path.display().to_string())],
    );
    assert_success(&output);
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("doctor json");
    assert_eq!(value["tmux"][0]["status"], "ok");
    let log = fs::read_to_string(log_path).expect("tmux log");
    assert!(log.contains("has-session"), "{log}");
    assert!(log.contains("-t"), "{log}");
    assert!(log.contains("mempal:0.1"), "{log}");
}

#[test]
fn test_cli_cowork_session_create_and_list() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "session-create");
    register(&home, &repo, "claude-main", "claude");
    register(&home, &repo, "codex-a", "codex");
    let create = run_mempal(
        &home,
        &[
            "cowork-session-create",
            "--cwd",
            repo.to_str().unwrap(),
            "--session-id",
            "review-1",
            "--title",
            "Review 1",
            "--agent",
            "claude-main",
            "--agent",
            "codex-a",
        ],
    );
    assert_success(&create);
    let list = run_mempal(&home, &["cowork-sessions", "--cwd", repo.to_str().unwrap()]);
    assert_success(&list);
    assert!(stdout(&list).contains("review-1"));
    assert!(sessions_path(&home, &repo).exists());
}

#[test]
fn test_cli_cowork_session_rejects_unknown_agent() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "session-reject");
    register(&home, &repo, "claude-main", "claude");
    let create = run_mempal(
        &home,
        &[
            "cowork-session-create",
            "--cwd",
            repo.to_str().unwrap(),
            "--session-id",
            "review-1",
            "--title",
            "Review 1",
            "--agent",
            "claude-main",
            "--agent",
            "codex-missing",
        ],
    );
    assert!(!create.status.success());
    assert!(stderr(&create).contains("unknown agent"));
    assert!(!sessions_path(&home, &repo).exists());
}

#[test]
fn test_cli_cowork_session_status_update() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "session-status");
    register(&home, &repo, "claude-main", "claude");
    register(&home, &repo, "codex-a", "codex");
    let create = run_mempal(
        &home,
        &[
            "cowork-session-create",
            "--cwd",
            repo.to_str().unwrap(),
            "--session-id",
            "review-1",
            "--title",
            "Review 1",
            "--agent",
            "claude-main",
            "--agent",
            "codex-a",
        ],
    );
    assert_success(&create);
    let update = run_mempal(
        &home,
        &[
            "cowork-session-status",
            "--cwd",
            repo.to_str().unwrap(),
            "--session-id",
            "review-1",
            "--status",
            "paused",
        ],
    );
    assert_success(&update);
    let list = run_mempal(
        &home,
        &[
            "cowork-sessions",
            "--cwd",
            repo.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert_success(&list);
    let value: serde_json::Value = serde_json::from_slice(&list.stdout).expect("sessions json");
    assert_eq!(value[0]["status"], "paused");
    let events = run_mempal(&home, &["cowork-events", "--cwd", repo.to_str().unwrap()]);
    assert_success(&events);
    assert!(stdout(&events).contains("session_status"));
}

#[test]
fn test_cli_cowork_session_close_no_capture() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "session-close");
    register(&home, &repo, "claude-main", "claude");
    register(&home, &repo, "codex-a", "codex");
    assert_success(&run_mempal(
        &home,
        &[
            "cowork-session-create",
            "--cwd",
            repo.to_str().unwrap(),
            "--session-id",
            "review-1",
            "--title",
            "Review 1",
            "--agent",
            "claude-main",
            "--agent",
            "codex-a",
        ],
    ));

    let close = run_mempal(
        &home,
        &[
            "cowork-session-close",
            "--cwd",
            repo.to_str().unwrap(),
            "--session-id",
            "review-1",
        ],
    );
    assert_success(&close);
    let list = run_mempal(
        &home,
        &[
            "cowork-sessions",
            "--cwd",
            repo.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert_success(&list);
    let value: serde_json::Value = serde_json::from_slice(&list.stdout).expect("sessions json");
    assert_eq!(value[0]["status"], "closed");
    assert!(!palace_db_path(&home).exists());
}

#[test]
fn test_cli_cowork_session_close_capture_execute() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "session-close-capture");
    register(&home, &repo, "claude-main", "claude");
    register(&home, &repo, "codex-a", "codex");
    assert_success(&run_mempal(
        &home,
        &[
            "cowork-session-create",
            "--cwd",
            repo.to_str().unwrap(),
            "--session-id",
            "review-1",
            "--title",
            "Review 1",
            "--agent",
            "claude-main",
            "--agent",
            "codex-a",
        ],
    ));

    let close = run_mempal(
        &home,
        &[
            "cowork-session-close",
            "--cwd",
            repo.to_str().unwrap(),
            "--session-id",
            "review-1",
            "--capture",
            "--execute",
            "--format",
            "json",
        ],
    );
    assert_success(&close);
    let value: serde_json::Value = serde_json::from_slice(&close.stdout).expect("close json");
    assert_eq!(value["session"]["status"], "closed");
    assert_eq!(value["capture"]["writes"], true);
    let drawer_id = value["capture"]["drawer_id"].as_str().expect("drawer id");
    let db = mempal::core::db::Database::open(&palace_db_path(&home)).expect("open db");
    let drawer = db
        .get_drawer(drawer_id)
        .expect("get drawer")
        .expect("drawer exists");
    assert_eq!(drawer.wing, "cowork-capture");
}

#[test]
fn test_cli_cowork_handoff_plain() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "handoff-plain");
    register(&home, &repo, "claude-main", "claude");
    register(&home, &repo, "codex-a", "codex");
    assert_success(&run_mempal(
        &home,
        &[
            "cowork-session-create",
            "--cwd",
            repo.to_str().unwrap(),
            "--session-id",
            "review-1",
            "--title",
            "Review 1",
            "--agent",
            "claude-main",
            "--agent",
            "codex-a",
        ],
    ));
    assert_success(&run_mempal(
        &home,
        &[
            "cowork-send",
            "--from",
            "claude-main",
            "--to",
            "codex-a",
            "--cwd",
            repo.to_str().unwrap(),
            "--message",
            "handoff pending",
        ],
    ));
    let handoff = run_mempal(&home, &["cowork-handoff", "--cwd", repo.to_str().unwrap()]);
    assert_success(&handoff);
    let out = stdout(&handoff);
    assert!(out.contains("Active sessions"), "{out}");
    assert!(out.contains("review-1"), "{out}");
    assert!(out.contains("Pending deliveries"), "{out}");
}

#[test]
fn test_cli_cowork_handoff_filters_thread() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "handoff-thread");
    register(&home, &repo, "claude-main", "claude");
    register(&home, &repo, "codex-a", "codex");
    for (thread, message) in [("thread-a", "message a"), ("thread-b", "message b")] {
        assert_success(&run_mempal(
            &home,
            &[
                "cowork-send",
                "--from",
                "claude-main",
                "--to",
                "codex-a",
                "--cwd",
                repo.to_str().unwrap(),
                "--thread-id",
                thread,
                "--message",
                message,
            ],
        ));
    }
    let handoff = run_mempal(
        &home,
        &[
            "cowork-handoff",
            "--cwd",
            repo.to_str().unwrap(),
            "--thread-id",
            "thread-a",
            "--format",
            "json",
        ],
    );
    assert_success(&handoff);
    let out = stdout(&handoff);
    assert!(out.contains("thread-a"), "{out}");
    assert!(!out.contains("thread-b"), "{out}");
}

#[test]
fn test_cli_cowork_handoff_rejects_invalid_format() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "handoff-invalid");
    register(&home, &repo, "claude-main", "claude");
    let before = fs::read_to_string(bus_events_path(&home, &repo)).expect("events");
    let handoff = run_mempal(
        &home,
        &[
            "cowork-handoff",
            "--cwd",
            repo.to_str().unwrap(),
            "--format",
            "yaml",
        ],
    );
    assert!(!handoff.status.success());
    let after = fs::read_to_string(bus_events_path(&home, &repo)).expect("events");
    assert_eq!(before, after);
}

#[test]
fn test_cli_cowork_capture_dry_run_no_write() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "capture-dry-run");
    register(&home, &repo, "claude-main", "claude");
    let capture = run_mempal(
        &home,
        &[
            "cowork-capture",
            "--cwd",
            repo.to_str().unwrap(),
            "--summary-source",
            "handoff",
            "--format",
            "json",
        ],
    );
    assert_success(&capture);
    let value: serde_json::Value = serde_json::from_slice(&capture.stdout).expect("capture json");
    assert_eq!(value["writes"], false);
    assert!(!palace_db_path(&home).exists());
}

#[test]
fn test_cli_cowork_capture_execute_writes_evidence() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "capture-execute");
    register(&home, &repo, "claude-main", "claude");
    let capture = run_mempal(
        &home,
        &[
            "cowork-capture",
            "--cwd",
            repo.to_str().unwrap(),
            "--summary-source",
            "handoff",
            "--execute",
            "--format",
            "json",
        ],
    );
    assert_success(&capture);
    let value: serde_json::Value = serde_json::from_slice(&capture.stdout).expect("capture json");
    assert_eq!(value["writes"], true);
    let drawer_id = value["drawer_id"].as_str().expect("drawer id");
    let db = mempal::core::db::Database::open(&palace_db_path(&home)).expect("open db");
    let drawer = db
        .get_drawer(drawer_id)
        .expect("get drawer")
        .expect("drawer exists");
    assert_eq!(drawer.wing, "cowork-capture");
    assert!(drawer.content.contains("Cowork Handoff Capture"));
}

#[test]
fn test_cli_cowork_capture_rejects_unknown_source() {
    let home = TempDir::new().expect("home");
    let repo = setup_repo(&home, "capture-unknown");
    let capture = run_mempal(
        &home,
        &[
            "cowork-capture",
            "--cwd",
            repo.to_str().unwrap(),
            "--summary-source",
            "tmux",
        ],
    );
    assert!(!capture.status.success());
    assert!(stderr(&capture).contains("unsupported cowork capture summary source"));
}

#[test]
fn test_cli_maintenance_runbook_plain() {
    let home = TempDir::new().expect("home");
    let output = run_mempal(&home, &["maintenance-runbook", "--format", "plain"]);
    assert_success(&output);
    let out = stdout(&output);
    assert!(out.contains("research-ingest-plan"), "{out}");
    assert!(out.contains("knowledge distill"), "{out}");
    assert!(out.contains("cowork-capture"), "{out}");
}

#[test]
fn test_cli_maintenance_runbook_json() {
    let home = TempDir::new().expect("home");
    let output = run_mempal(&home, &["maintenance-runbook", "--format", "json"]);
    assert_success(&output);
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("maintenance json");
    assert!(
        value["title"]
            .as_str()
            .unwrap_or_default()
            .contains("Maintenance Runbook")
    );
    assert!(
        value["content"]
            .as_str()
            .unwrap_or_default()
            .contains("runtime adoption")
    );
}

#[test]
fn test_cli_maintenance_runbook_rejects_invalid_format() {
    let home = TempDir::new().expect("home");
    let output = run_mempal(&home, &["maintenance-runbook", "--format", "yaml"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("unknown format"));
    assert!(!home.path().join(".mempal").exists());
}
