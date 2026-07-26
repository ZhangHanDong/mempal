//! P116 CLI integration: `mempal cowork-receipts` + `cowork-drain --hook-runtime`.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

use mempal::cowork::Tool;
use mempal::cowork::inbox;

fn mempal_bin() -> String {
    env!("CARGO_BIN_EXE_mempal").to_string()
}

fn tmp_home_and_repo() -> (TempDir, PathBuf) {
    let tmp = TempDir::new().expect("tempdir");
    let repo = tmp.path().join("proj");
    fs::create_dir_all(repo.join(".git")).expect("create repo");
    (tmp, repo)
}

#[test]
fn cowork_receipts_tracks_pending_then_drained() {
    let (home, repo) = tmp_home_and_repo();
    let mempal_home = home.path().join(".mempal");

    let outcome = inbox::push_with_receipt(
        &mempal_home,
        Tool::Claude,
        Tool::Codex,
        &repo,
        "receipt cli test".to_string(),
        "2026-07-26T02:00:00Z".to_string(),
    )
    .expect("push with receipt");

    // 1) pending state visible in json format
    let output = Command::new(mempal_bin())
        .args([
            "cowork-receipts",
            "--cwd",
            repo.to_str().expect("utf8"),
            "--format",
            "json",
        ])
        .env("HOME", home.path())
        .output()
        .expect("run cowork-receipts");
    assert!(
        output.status.success(),
        "cowork-receipts failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let states: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json output");
    let list = states.as_array().expect("array of states");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["message_id"], outcome.message_id);
    assert_eq!(list[0]["status"], "pending");

    // 2) drain with --hook-runtime records injected_as + hook_runtime
    let drain = Command::new(mempal_bin())
        .args([
            "cowork-drain",
            "--target",
            "codex",
            "--cwd",
            repo.to_str().expect("utf8"),
            "--format",
            "plain",
            "--hook-runtime",
            "test UserPromptSubmit",
        ])
        .env("HOME", home.path())
        .output()
        .expect("run cowork-drain");
    assert!(
        drain.status.success(),
        "cowork-drain failed: {}",
        String::from_utf8_lossy(&drain.stderr)
    );
    assert!(
        String::from_utf8_lossy(&drain.stdout).contains("receipt cli test"),
        "drained content missing from hook output"
    );

    // 3) receipts now show drained with metadata
    let output = Command::new(mempal_bin())
        .args([
            "cowork-receipts",
            "--cwd",
            repo.to_str().expect("utf8"),
            "--format",
            "json",
        ])
        .env("HOME", home.path())
        .output()
        .expect("run cowork-receipts after drain");
    assert!(output.status.success());
    let states: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json output");
    let list = states.as_array().expect("array of states");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["status"], "drained");
    assert_eq!(list[0]["injected_as"], "plain");
    assert_eq!(list[0]["hook_runtime"], "test UserPromptSubmit");

    // 4) plain format mentions the state
    let plain = Command::new(mempal_bin())
        .args(["cowork-receipts", "--cwd", repo.to_str().expect("utf8")])
        .env("HOME", home.path())
        .output()
        .expect("run cowork-receipts plain");
    assert!(plain.status.success());
    let text = String::from_utf8_lossy(&plain.stdout);
    assert!(
        text.contains("drained"),
        "plain output missing state: {text}"
    );
}
