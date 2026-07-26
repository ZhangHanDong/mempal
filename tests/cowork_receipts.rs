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

#[test]
fn cowork_drain_stdout_failure_writes_no_drained_receipt() {
    // Codex re-review P1: `print!` is line-buffered and codex-hook-json has
    // no trailing newline, so with an unwritable stdout the output silently
    // stayed in the buffer and the message was still marked drained. Drain
    // must record `drained` only after the hook output actually flushed.
    let (home, repo) = tmp_home_and_repo();
    let mempal_home = home.path().join(".mempal");

    inbox::push_with_receipt(
        &mempal_home,
        Tool::Claude,
        Tool::Codex,
        &repo,
        "never injected".to_string(),
        "2026-07-27T02:00:00Z".to_string(),
    )
    .expect("push with receipt");

    // A pipe whose read end is already closed makes the child's stdout
    // flush fail deterministically with EPIPE. (A read-only file is NOT a
    // valid vector here: Rust std masks EBADF on stdout — handle_ebadf in
    // library/std/src/io/stdio.rs — so only EPIPE-class failures surface.)
    let (reader, writer) = std::io::pipe().expect("create pipe");
    drop(reader);
    let drain = Command::new(mempal_bin())
        .args([
            "cowork-drain",
            "--target",
            "codex",
            "--cwd",
            repo.to_str().expect("utf8"),
            "--format",
            "codex-hook-json",
        ])
        .env("HOME", home.path())
        .stdout(std::process::Stdio::from(writer))
        .output()
        .expect("run cowork-drain with broken-pipe stdout");
    // hook graceful-degrade contract: still exit 0, error on stderr
    assert!(drain.status.success());
    assert!(
        String::from_utf8_lossy(&drain.stderr).contains("cowork-drain"),
        "stderr should carry the write failure"
    );

    // the message was consumed by the drain rename, but the receipt must
    // NOT claim injection — the state is lost, not drained
    let states_out = Command::new(mempal_bin())
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
    let states: serde_json::Value =
        serde_json::from_slice(&states_out.stdout).expect("json output");
    let list = states.as_array().expect("array of states");
    assert_eq!(list.len(), 1);
    assert_eq!(
        list[0]["status"], "lost",
        "an un-flushed drain must not be recorded as drained: {states}"
    );
}

#[test]
fn cowork_drain_invalid_format_preserves_inbox_and_writes_no_receipt() {
    // Codex review P1: an invalid --format must be rejected BEFORE the
    // destructive drain rename — no message loss, no false drained receipt.
    let (home, repo) = tmp_home_and_repo();
    let mempal_home = home.path().join(".mempal");

    inbox::push_with_receipt(
        &mempal_home,
        Tool::Claude,
        Tool::Codex,
        &repo,
        "must survive typo".to_string(),
        "2026-07-26T05:00:00Z".to_string(),
    )
    .expect("push with receipt");

    let drain = Command::new(mempal_bin())
        .args([
            "cowork-drain",
            "--target",
            "codex",
            "--cwd",
            repo.to_str().expect("utf8"),
            "--format",
            "codex-hook-jsn",
        ])
        .env("HOME", home.path())
        .output()
        .expect("run cowork-drain with bad format");
    // hook graceful-degrade contract: still exit 0, error on stderr
    assert!(drain.status.success());
    assert!(
        String::from_utf8_lossy(&drain.stderr).contains("format"),
        "stderr should mention the format error"
    );

    // message must still be in the inbox
    let inbox_path = inbox::inbox_path(&mempal_home, Tool::Codex, &repo).expect("inbox path");
    assert!(
        inbox_path.exists(),
        "invalid format must not consume the inbox"
    );
    assert!(
        fs::read_to_string(&inbox_path)
            .expect("read inbox")
            .contains("must survive typo")
    );

    // and no drained receipt may exist
    let states_out = Command::new(mempal_bin())
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
    let states: serde_json::Value =
        serde_json::from_slice(&states_out.stdout).expect("json output");
    let list = states.as_array().expect("array of states");
    assert_eq!(list.len(), 1);
    assert_eq!(
        list[0]["status"], "pending",
        "no drained receipt may be recorded for a failed drain: {states}"
    );
}
