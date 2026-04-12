//! Integration tests for P6 cowork peek-and-decide.
//!
//! Run with:
//!   cargo test --test cowork_peek --no-default-features --features model2vec
//!
//! These tests build a fake HOME dir with Claude/Codex fixture sessions and
//! verify the peek_partner orchestration end-to-end. They do NOT touch the
//! real ~/.claude or ~/.codex directories — `home_override` on PeekRequest
//! injects a tempdir as the resolved HOME.

use mempal::cowork::{PeekError, PeekRequest, Tool, peek_partner};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::TempDir;

/// Compute today's `(YYYY, MM, DD)` from `SystemTime::now()` so integration
/// tests always write Codex fixtures into a date directory that falls
/// inside `find_latest_session_for_cwd`'s 7-day scan window, regardless of
/// when the test is run.
fn today_ymd() -> (i64, u32, u32) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86400);
    // Inline civil_from_days (peek.rs's days_to_ymd is pub(crate), not
    // reachable from integration tests — duplicate the 12-line algorithm).
    let mut d = days;
    d += 719468;
    let era = if d >= 0 { d } else { d - 146096 } / 146097;
    let doe = (d - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    (year, m as u32, day as u32)
}

fn codex_day_dir(home: &Path, year: i64, month: u32, day: u32) -> PathBuf {
    home.join(format!(".codex/sessions/{year:04}/{month:02}/{day:02}"))
}

/// Build a fake HOME dir containing Claude and Codex fixture sessions for the
/// given cwd. Returns the TempDir guard (keep alive for the test) and the
/// HOME path to pass into `home_override`.
fn build_fake_home(cwd: &Path) -> (TempDir, PathBuf) {
    let tmp = TempDir::new().expect("tempdir");
    let home = tmp.path().to_path_buf();

    // Claude: ~/.claude/projects/<encoded>/session.jsonl
    let encoded = cwd.to_string_lossy().replace('/', "-");
    let claude_dir = home.join(".claude/projects").join(&encoded);
    fs::create_dir_all(&claude_dir).unwrap();
    let cwd_str = cwd.to_string_lossy();
    let claude_jsonl = format!(
        r#"{{"type":"permission-mode","permissionMode":"default"}}
{{"parentUuid":null,"isSidechain":false,"type":"user","message":{{"role":"user","content":"Claude user msg"}},"uuid":"u1","timestamp":"2026-04-13T10:00:00Z","cwd":"{cwd_str}"}}
{{"parentUuid":"u1","isSidechain":false,"type":"assistant","message":{{"role":"assistant","content":[{{"type":"text","text":"Claude reply"}}]}},"uuid":"a1","timestamp":"2026-04-13T10:00:05Z","cwd":"{cwd_str}"}}
"#
    );
    fs::write(claude_dir.join("session.jsonl"), claude_jsonl).unwrap();

    // Codex: ~/.codex/sessions/<today>/rollout-*.jsonl. Use today's actual
    // date so find_latest_session_for_cwd's 7-day scan window always
    // includes this fixture regardless of when the test runs.
    let (ty, tm, td) = today_ymd();
    let codex_dir = codex_day_dir(&home, ty, tm, td);
    fs::create_dir_all(&codex_dir).unwrap();
    let stamp = format!("{ty:04}-{tm:02}-{td:02}T12:00:00Z");
    let codex_jsonl = format!(
        r#"{{"timestamp":"{stamp}","type":"session_meta","payload":{{"id":"x","timestamp":"{stamp}","cwd":"{cwd_str}","originator":"codex-tui"}}}}
{{"timestamp":"{stamp}","type":"response_item","payload":{{"type":"message","role":"user","content":[{{"type":"input_text","text":"Codex user msg"}}]}}}}
{{"timestamp":"{stamp}","type":"response_item","payload":{{"type":"message","role":"assistant","content":[{{"type":"output_text","text":"Codex reply"}}]}}}}
"#
    );
    fs::write(
        codex_dir.join(format!("rollout-{ty:04}-{tm:02}-{td:02}T12-00-00-x.jsonl")),
        codex_jsonl,
    )
    .unwrap();

    (tmp, home)
}

#[test]
fn test_peek_partner_claude_reads_codex_session() {
    let cwd = PathBuf::from("/tmp/fake-project-1");
    let (_tmp, home) = build_fake_home(&cwd);

    let req = PeekRequest {
        tool: Tool::Codex,
        limit: 30,
        since: None,
        cwd,
        caller_tool: Some(Tool::Claude),
        home_override: Some(home),
    };
    let resp = peek_partner(req).expect("peek");

    assert_eq!(resp.partner_tool, Tool::Codex);
    assert_eq!(resp.messages.len(), 2);
    assert_eq!(resp.messages[0].text, "Codex user msg");
    assert_eq!(resp.messages[1].text, "Codex reply");
    assert!(resp.messages[0].at <= resp.messages[1].at);
    assert!(!resp.truncated);
    assert!(resp.session_path.is_some());
}

#[test]
fn test_peek_partner_auto_mode_infers_partner() {
    let cwd = PathBuf::from("/tmp/fake-project-2");
    let (_tmp, home) = build_fake_home(&cwd);

    let req = PeekRequest {
        tool: Tool::Auto,
        limit: 30,
        since: None,
        cwd,
        caller_tool: Some(Tool::Claude),
        home_override: Some(home),
    };
    let resp = peek_partner(req).expect("peek");

    assert_eq!(resp.partner_tool, Tool::Codex);
    assert_eq!(resp.messages.len(), 2);
}

#[test]
fn test_peek_partner_auto_mode_errors_without_client_info() {
    let cwd = PathBuf::from("/tmp/fake-project-3");
    let (_tmp, home) = build_fake_home(&cwd);

    let req = PeekRequest {
        tool: Tool::Auto,
        limit: 30,
        since: None,
        cwd,
        caller_tool: None,
        home_override: Some(home),
    };
    let err = peek_partner(req).unwrap_err();
    assert!(matches!(err, PeekError::CannotInferPartner));
}

#[test]
fn test_peek_partner_reports_inactive_session() {
    let cwd = PathBuf::from("/tmp/fake-project-4");
    let (_tmp, home) = build_fake_home(&cwd);

    // Backdate the Codex jsonl's mtime — the file still lives in today's
    // YYYY/MM/DD dir (inside the 7-day scan window), but its mtime is
    // decades old so partner_active should be false.
    let (ty, tm, td) = today_ymd();
    let codex_path = codex_day_dir(&home, ty, tm, td).join(format!(
        "rollout-{ty:04}-{tm:02}-{td:02}T12-00-00-x.jsonl"
    ));
    Command::new("touch")
        .arg("-t")
        .arg("198001010000")
        .arg(&codex_path)
        .status()
        .expect("touch");

    let req = PeekRequest {
        tool: Tool::Codex,
        limit: 30,
        since: None,
        cwd,
        caller_tool: Some(Tool::Claude),
        home_override: Some(home),
    };
    let resp = peek_partner(req).expect("peek");
    assert!(!resp.partner_active);
    assert!(!resp.messages.is_empty(), "still returns recent content");
}

#[test]
fn test_peek_partner_filters_by_project_cwd() {
    let cwd_a = PathBuf::from("/tmp/project-a-xyz");
    let (_tmp, home) = build_fake_home(&cwd_a);

    // Add a second Codex jsonl for a different cwd, in today's same date
    // directory (both inside the 7-day scan window). cwd filter must
    // still exclude it.
    let (ty, tm, td) = today_ymd();
    let other_dir = codex_day_dir(&home, ty, tm, td);
    fs::create_dir_all(&other_dir).unwrap();
    let stamp = format!("{ty:04}-{tm:02}-{td:02}T13:00:00Z");
    fs::write(
        other_dir.join(format!(
            "rollout-{ty:04}-{tm:02}-{td:02}T13-00-00-other.jsonl"
        )),
        format!(
            r#"{{"timestamp":"{stamp}","type":"session_meta","payload":{{"id":"other","timestamp":"{stamp}","cwd":"/tmp/project-b-xyz","originator":"codex-tui"}}}}
{{"timestamp":"{stamp}","type":"response_item","payload":{{"type":"message","role":"user","content":[{{"type":"input_text","text":"should not appear"}}]}}}}
"#
        ),
    )
    .unwrap();

    let req = PeekRequest {
        tool: Tool::Codex,
        limit: 30,
        since: None,
        cwd: cwd_a,
        caller_tool: Some(Tool::Claude),
        home_override: Some(home),
    };
    let resp = peek_partner(req).expect("peek");
    let path_str = resp.session_path.expect("path");
    // Returned session must be the project-a one (per cwd filter), NOT
    // the newer project-b rollout which sits in the same date dir.
    assert!(
        path_str.contains("-x.jsonl"),
        "expected project-a session (rollout-*-x.jsonl), got {path_str}"
    );
    assert!(
        !path_str.contains("other.jsonl"),
        "must not return project-b rollout, got {path_str}"
    );
    for m in &resp.messages {
        assert!(!m.text.contains("should not appear"));
    }
}

#[test]
fn test_peek_partner_has_no_mempal_side_effects() {
    // Invariant by construction: peek_partner never touches Database.
    // This test exercises it 3× to ensure no stateful leak across calls.
    let cwd = PathBuf::from("/tmp/fake-project-5");
    let (_tmp, home) = build_fake_home(&cwd);

    let req = PeekRequest {
        tool: Tool::Codex,
        limit: 30,
        since: None,
        cwd,
        caller_tool: Some(Tool::Claude),
        home_override: Some(home),
    };
    for _ in 0..3 {
        let _ = peek_partner(req.clone()).expect("peek");
    }
}

#[test]
fn test_peek_partner_returns_empty_when_no_session() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().to_path_buf();
    // Do not create any fixture files.

    let req = PeekRequest {
        tool: Tool::Claude,
        limit: 30,
        since: None,
        cwd: PathBuf::from("/tmp/no-session-project"),
        caller_tool: Some(Tool::Codex),
        home_override: Some(home),
    };
    let resp = peek_partner(req).expect("peek");
    assert_eq!(resp.messages.len(), 0);
    assert!(!resp.partner_active);
    assert!(resp.session_path.is_none());
}
