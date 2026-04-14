//! Bidirectional cowork inbox for P8 cowork-push protocol.
//!
//! File-based ephemeral message queue between Claude Code and Codex
//! agents working in the same project (git root). Push appends a jsonl
//! entry; drain atomically renames + reads + deletes the file.
//!
//! Design: docs/specs/2026-04-14-p8-cowork-inbox-push.md
//! Spec:   specs/p8-cowork-inbox-push.spec.md

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::peek::Tool;

pub const MAX_MESSAGE_SIZE: usize = 8 * 1024;
pub const MAX_PENDING_MESSAGES: usize = 16;
pub const MAX_TOTAL_INBOX_BYTES: u64 = 32 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum InboxError {
    #[error("message content exceeds {MAX_MESSAGE_SIZE} bytes: got {0} bytes")]
    MessageTooLarge(usize),
    #[error("invalid cwd path (contains `..` or is not absolute): {0}")]
    InvalidCwd(String),
    #[error("cannot push to self (both caller and target resolve to {0:?})")]
    SelfPush(Tool),
    #[error(
        "inbox full: {current_count} messages / {current_bytes} bytes pending \
         (limits: {MAX_PENDING_MESSAGES} messages, {MAX_TOTAL_INBOX_BYTES} bytes) — \
         partner must drain first"
    )]
    InboxFull {
        current_count: usize,
        current_bytes: u64,
    },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboxMessage {
    pub pushed_at: String,
    pub from: String,
    pub content: String,
}

/// Resolve ~/.mempal using the HOME env var. Matches the existing
/// `expand_home` pattern at src/main.rs:949-957. Used by both the CLI
/// subcommands (cowork-drain / cowork-status / cowork-install-hooks)
/// and the MCP server handler (mempal_cowork_push).
///
/// No `dirs` crate dependency — P8 explicitly promises zero new runtime deps.
pub fn mempal_home() -> PathBuf {
    match std::env::var_os("HOME") {
        Some(home) => PathBuf::from(home).join(".mempal"),
        None => PathBuf::from(".mempal"),
    }
}

/// Resolve the given cwd to a canonical "project identity" path. Walks the
/// directory tree looking for a `.git` entry (git repo root); falls back to
/// the raw cwd if no `.git` ancestor is found.
///
/// This normalizes the "Claude in repo root, Codex in src/cowork" scenario —
/// both resolve to the same project identity, so push and drain see the same
/// inbox file.
pub fn project_identity(cwd: &Path) -> PathBuf {
    let mut current = cwd.to_path_buf();
    loop {
        if current.join(".git").exists() {
            return current;
        }
        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => return cwd.to_path_buf(),
        }
    }
}

/// Encode an already-normalized project identity path into the dashed
/// filename format. Input should be the OUTPUT of `project_identity`, not
/// a raw cwd. Rejects non-absolute paths and paths containing `..`.
pub fn encode_project_identity(identity: &Path) -> Result<String, InboxError> {
    let s = identity.to_string_lossy();
    if !identity.is_absolute() || s.contains("..") {
        return Err(InboxError::InvalidCwd(s.to_string()));
    }
    Ok(s.replace('/', "-"))
}

/// Return `<mempal_home>/cowork-inbox/<target>/<encoded_project_identity>.jsonl`.
pub fn inbox_path(
    mempal_home: &Path,
    target: Tool,
    cwd: &Path,
) -> Result<PathBuf, InboxError> {
    let identity = project_identity(cwd);
    let encoded = encode_project_identity(&identity)?;
    Ok(mempal_home
        .join("cowork-inbox")
        .join(target.dir_name())
        .join(format!("{encoded}.jsonl")))
}

/// Append a message to the target agent's inbox. Enforces self-push rejection,
/// size cap, and PROSPECTIVE backpressure (checks post-append state, not
/// pre-append state — ensures MAX_TOTAL_INBOX_BYTES is a real upper bound).
///
/// Returns `(inbox_path, total_bytes_after_append)`.
pub fn push(
    mempal_home: &Path,
    caller: Tool,
    target: Tool,
    cwd: &Path,
    content: String,
    pushed_at: String,
) -> Result<(PathBuf, u64), InboxError> {
    use std::fs;
    use std::io::Write;

    if caller == target {
        return Err(InboxError::SelfPush(caller));
    }
    if content.len() > MAX_MESSAGE_SIZE {
        return Err(InboxError::MessageTooLarge(content.len()));
    }

    let path = inbox_path(mempal_home, target, cwd)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let (existing_count, existing_bytes) = if path.exists() {
        let content_bytes = fs::read_to_string(&path).unwrap_or_default();
        let line_count = content_bytes
            .lines()
            .filter(|l| !l.trim().is_empty())
            .count();
        (line_count, content_bytes.len() as u64)
    } else {
        (0, 0)
    };

    let msg = InboxMessage {
        pushed_at,
        from: caller.dir_name().to_string(),
        content,
    };
    let line = serde_json::to_string(&msg)?;
    // writeln! appends exactly 1 byte for `\n`
    let new_line_bytes = (line.len() as u64) + 1;
    let prospective_count = existing_count + 1;
    let prospective_bytes = existing_bytes.saturating_add(new_line_bytes);
    if prospective_count > MAX_PENDING_MESSAGES || prospective_bytes > MAX_TOTAL_INBOX_BYTES {
        return Err(InboxError::InboxFull {
            current_count: existing_count,
            current_bytes: existing_bytes,
        });
    }

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    writeln!(file, "{line}")?;
    file.flush()?;

    let size = fs::metadata(&path)?.len();
    Ok((path, size))
}

/// Drain all messages from this (target, project_identity) inbox.
///
/// **At-most-once, winner-takes-all.** Two concurrent drain calls race on
/// `fs::rename(path → path.draining)`. POSIX guarantees this rename is atomic:
/// exactly one caller wins and proceeds to read+delete; the loser sees
/// `ErrorKind::NotFound` and returns an empty Vec. **Crash window**: a winner
/// crashing after rename but before delete leaves an orphaned `.draining`
/// file whose content is lost. This is an accepted tradeoff; P8 does not
/// implement crash recovery.
pub fn drain(
    mempal_home: &Path,
    target: Tool,
    cwd: &Path,
) -> Result<Vec<InboxMessage>, InboxError> {
    use std::fs;

    let path = inbox_path(mempal_home, target, cwd)?;
    let draining = path.with_extension("draining");

    match fs::rename(&path, &draining) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Vec::new());
        }
        Err(e) => return Err(e.into()),
    }

    let content = fs::read_to_string(&draining)?;
    let mut messages = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Skip malformed lines rather than failing the whole drain.
        if let Ok(msg) = serde_json::from_str::<InboxMessage>(trimmed) {
            messages.push(msg);
        }
    }

    // Best-effort cleanup; content is already in `messages`.
    let _ = fs::remove_file(&draining);
    Ok(messages)
}

/// Format drained messages as plain text for prepend-to-prompt hooks.
pub fn format_plain(from: Tool, messages: &[InboxMessage]) -> String {
    if messages.is_empty() {
        return String::new();
    }
    let mut out = format!(
        "[Partner inbox from {} ({} message{} since last check):]\n",
        from.dir_name(),
        messages.len(),
        if messages.len() == 1 { "" } else { "s" }
    );
    for msg in messages {
        out.push_str(&format!("- {}: {}\n", msg.pushed_at, msg.content));
    }
    out.push_str("[End partner inbox]\n");
    out
}

/// Format drained messages as Codex native hook JSON envelope.
/// Returns empty string when no messages.
pub fn format_codex_hook_json(
    from: Tool,
    messages: &[InboxMessage],
) -> Result<String, InboxError> {
    if messages.is_empty() {
        return Ok(String::new());
    }
    let plain = format_plain(from, messages);
    let envelope = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "UserPromptSubmit",
            "additionalContext": plain
        }
    });
    Ok(envelope.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn project_identity_walks_to_git_root_from_subdir() {
        let tmp = TempDir::new().unwrap();
        let repo_root = tmp.path().join("project-gamma");
        let subdir = repo_root.join("src").join("cowork");
        fs::create_dir_all(&subdir).unwrap();
        fs::create_dir_all(repo_root.join(".git")).unwrap();

        assert_eq!(project_identity(&subdir), repo_root);
        assert_eq!(project_identity(&repo_root), repo_root);
    }

    #[test]
    fn project_identity_falls_back_to_raw_cwd_without_git() {
        let tmp = TempDir::new().unwrap();
        let plain = tmp.path().join("no-git-dir");
        fs::create_dir_all(&plain).unwrap();

        assert_eq!(project_identity(&plain), plain);
    }

    #[test]
    fn encode_project_identity_rejects_relative_path() {
        let result = encode_project_identity(Path::new("relative/path"));
        assert!(matches!(result, Err(InboxError::InvalidCwd(_))));
    }

    #[test]
    fn encode_project_identity_rejects_parent_traversal() {
        let result = encode_project_identity(Path::new("/tmp/../etc"));
        assert!(matches!(result, Err(InboxError::InvalidCwd(_))));
    }

    #[test]
    fn encode_project_identity_replaces_slashes_with_dashes() {
        let encoded = encode_project_identity(
            Path::new("/Users/zhangalex/Work/Projects/AI/mempal"),
        )
        .unwrap();
        assert_eq!(encoded, "-Users-zhangalex-Work-Projects-AI-mempal");
    }

    #[test]
    fn mempal_home_resolves_from_home_env_var() {
        // `mempal_home()` reads `$HOME` at call time. This test verifies the
        // shape — `$HOME/.mempal` — without mutating the process env.
        let home = std::env::var("HOME").unwrap_or_default();
        if home.is_empty() {
            return;
        }
        let resolved = mempal_home();
        assert_eq!(resolved, PathBuf::from(home).join(".mempal"));
    }

    #[test]
    fn inbox_path_composes_home_target_and_encoded_identity() {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("proj");
        fs::create_dir_all(repo.join(".git")).unwrap();

        let path = inbox_path(tmp.path(), Tool::Codex, &repo).unwrap();
        assert!(path.starts_with(tmp.path().join("cowork-inbox").join("codex")));
        assert!(path.to_string_lossy().ends_with(".jsonl"));
        let encoded_name = path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(encoded_name.contains("proj"));
    }

    fn rfc3339() -> String {
        "2026-04-15T00:00:00Z".to_string()
    }

    fn tmpdir_with_git() -> (TempDir, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("proj");
        fs::create_dir_all(repo.join(".git")).unwrap();
        (tmp, repo)
    }

    #[test]
    fn push_rejects_content_over_max_size() {
        let tmp_home = TempDir::new().unwrap();
        let (_tmp_repo, repo) = tmpdir_with_git();
        let oversize = "x".repeat(MAX_MESSAGE_SIZE + 1);
        let err = push(
            tmp_home.path(),
            Tool::Claude,
            Tool::Codex,
            &repo,
            oversize,
            rfc3339(),
        )
        .unwrap_err();
        assert!(matches!(err, InboxError::MessageTooLarge(n) if n == MAX_MESSAGE_SIZE + 1));
    }

    #[test]
    fn push_rejects_cwd_with_parent_traversal() {
        let tmp = TempDir::new().unwrap();
        let weird = Path::new("/tmp/../etc");
        let err = push(
            tmp.path(),
            Tool::Claude,
            Tool::Codex,
            weird,
            "x".into(),
            rfc3339(),
        )
        .unwrap_err();
        assert!(matches!(err, InboxError::InvalidCwd(_)));
    }

    #[test]
    fn push_rejects_self_push() {
        let tmp_home = TempDir::new().unwrap();
        let (_t, repo) = tmpdir_with_git();
        let err = push(
            tmp_home.path(),
            Tool::Codex,
            Tool::Codex,
            &repo,
            "x".into(),
            rfc3339(),
        )
        .unwrap_err();
        assert!(matches!(err, InboxError::SelfPush(Tool::Codex)));
    }

    #[test]
    fn push_rejects_when_prospective_count_would_exceed_limit() {
        let tmp_home = TempDir::new().unwrap();
        let (_t, repo) = tmpdir_with_git();
        for _ in 0..MAX_PENDING_MESSAGES {
            push(
                tmp_home.path(),
                Tool::Claude,
                Tool::Codex,
                &repo,
                "a".into(),
                rfc3339(),
            )
            .unwrap();
        }
        let err = push(
            tmp_home.path(),
            Tool::Claude,
            Tool::Codex,
            &repo,
            "a".into(),
            rfc3339(),
        )
        .unwrap_err();
        assert!(matches!(
            err,
            InboxError::InboxFull { current_count: 16, .. }
        ));
    }

    #[test]
    fn push_rejects_when_prospective_bytes_would_cross_limit() {
        // Spec S16' requires precise precondition: existing_bytes == 32700,
        // existing_count == 10. Land there using serde probe technique.
        let tmp_home = TempDir::new().unwrap();
        let (_t, repo) = tmpdir_with_git();

        const TARGET_BYTES: u64 = 32_700;
        const TARGET_COUNT: usize = 10;
        let bytes_per_push = (TARGET_BYTES / TARGET_COUNT as u64) as usize;

        let probe = InboxMessage {
            pushed_at: rfc3339(),
            from: Tool::Claude.dir_name().to_string(),
            content: String::new(),
        };
        let empty_line_bytes = serde_json::to_string(&probe).unwrap().len() + 1;
        assert!(
            bytes_per_push > empty_line_bytes,
            "bytes_per_push ({bytes_per_push}) must exceed empty_line_bytes ({empty_line_bytes})"
        );
        let content_per_push = "a".repeat(bytes_per_push - empty_line_bytes);

        for _ in 0..TARGET_COUNT {
            push(
                tmp_home.path(),
                Tool::Claude,
                Tool::Codex,
                &repo,
                content_per_push.clone(),
                rfc3339(),
            )
            .unwrap();
        }

        let inbox = inbox_path(tmp_home.path(), Tool::Codex, &repo).unwrap();
        let current_bytes = fs::metadata(&inbox).unwrap().len();
        let current_count = fs::read_to_string(&inbox)
            .unwrap()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .count();
        assert_eq!(current_bytes, TARGET_BYTES, "precondition: current_bytes == 32700");
        assert_eq!(current_count, TARGET_COUNT, "precondition: current_count == 10");

        let would_cross = "y".repeat(200);
        let err = push(
            tmp_home.path(),
            Tool::Claude,
            Tool::Codex,
            &repo,
            would_cross,
            rfc3339(),
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                InboxError::InboxFull {
                    current_count: 10,
                    current_bytes: 32_700,
                }
            ),
            "expected InboxFull with 10/32700 preconditions, got: {err:?}"
        );

        let after = fs::metadata(&inbox).unwrap().len();
        assert_eq!(after, TARGET_BYTES);
    }

    #[test]
    fn push_accepts_when_prospective_bytes_exactly_at_limit_and_rejects_one_more() {
        // Bracket the boundary: exact hit accepts, one byte over rejects.
        // This pair is what actually proves `>` vs `>=`.
        //
        // A single push is capped at MAX_MESSAGE_SIZE = 8 KB, so we cannot
        // fill a 32 KB inbox in one go. Seed with big pushes first to get
        // the inbox within one message slot of the limit, then compute the
        // exact content length for the final push.
        let tmp_home = TempDir::new().unwrap();
        let (_t, repo) = tmpdir_with_git();

        let probe = InboxMessage {
            pushed_at: rfc3339(),
            from: Tool::Claude.dir_name().to_string(),
            content: String::new(),
        };
        let probe_empty_line_bytes = serde_json::to_string(&probe).unwrap().len() as u64 + 1;

        // Seed until the remaining budget is within one MAX_MESSAGE_SIZE slot.
        // Each seed push adds `probe_empty_line_bytes + seed_content.len()`
        // bytes. Use seeds of (MAX_MESSAGE_SIZE - 100) so the size check
        // never fails, and loop until we're close to the limit.
        let seed_content_len = MAX_MESSAGE_SIZE - 100;
        let seed_content = "s".repeat(seed_content_len);
        let inbox_preview = inbox_path(tmp_home.path(), Tool::Codex, &repo).unwrap();
        loop {
            let current = if inbox_preview.exists() {
                fs::metadata(&inbox_preview).unwrap().len()
            } else {
                0
            };
            let remaining_after_maybe_seed =
                MAX_TOTAL_INBOX_BYTES - current - probe_empty_line_bytes - (seed_content_len as u64);
            // If pushing another seed would leave remaining <= MAX_MESSAGE_SIZE
            // + overhead, we're ready for the final exact push. Stop seeding.
            if remaining_after_maybe_seed < (MAX_MESSAGE_SIZE as u64) {
                break;
            }
            push(
                tmp_home.path(),
                Tool::Claude,
                Tool::Codex,
                &repo,
                seed_content.clone(),
                rfc3339(),
            )
            .unwrap();
        }
        // Do one more seed push to actually land within one slot of the limit.
        push(
            tmp_home.path(),
            Tool::Claude,
            Tool::Codex,
            &repo,
            seed_content.clone(),
            rfc3339(),
        )
        .unwrap();

        let inbox = inbox_path(tmp_home.path(), Tool::Codex, &repo).unwrap();
        let current_bytes = fs::metadata(&inbox).unwrap().len();
        let remaining = MAX_TOTAL_INBOX_BYTES - current_bytes;
        // remaining should be > probe_empty_line_bytes (room for at least one more line)
        // AND content portion should fit in MAX_MESSAGE_SIZE.
        let exact_content_len = (remaining - probe_empty_line_bytes) as usize;
        assert!(
            exact_content_len <= MAX_MESSAGE_SIZE,
            "seed math is wrong: exact_content_len {exact_content_len} > MAX_MESSAGE_SIZE {MAX_MESSAGE_SIZE}"
        );
        let exact_content = "a".repeat(exact_content_len);

        push(
            tmp_home.path(),
            Tool::Claude,
            Tool::Codex,
            &repo,
            exact_content,
            rfc3339(),
        )
        .unwrap();

        let final_bytes = fs::metadata(&inbox).unwrap().len();
        assert_eq!(
            final_bytes, MAX_TOTAL_INBOX_BYTES,
            "inbox MUST land exactly on the 32 KB boundary"
        );

        // One byte over → reject. This is the `>` vs `>=` discriminator.
        let err = push(
            tmp_home.path(),
            Tool::Claude,
            Tool::Codex,
            &repo,
            "x".into(),
            rfc3339(),
        )
        .unwrap_err();
        assert!(
            matches!(err, InboxError::InboxFull { .. }),
            "one byte over the limit MUST be rejected, got: {err:?}"
        );

        let after_rejected = fs::metadata(&inbox).unwrap().len();
        assert_eq!(after_rejected, MAX_TOTAL_INBOX_BYTES);
    }

    #[test]
    fn drain_round_trip_preserves_content_bytes() {
        let tmp_home = TempDir::new().unwrap();
        let (_t, repo) = tmpdir_with_git();
        let content = "hello from claude, P8 test #1".to_string();
        push(
            tmp_home.path(),
            Tool::Claude,
            Tool::Codex,
            &repo,
            content.clone(),
            rfc3339(),
        )
        .unwrap();

        let messages = drain(tmp_home.path(), Tool::Codex, &repo).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, content);
        assert_eq!(messages[0].from, "claude");
    }

    #[test]
    fn drain_preserves_unicode_bytes_round_trip() {
        let tmp_home = TempDir::new().unwrap();
        let (_t, repo) = tmpdir_with_git();
        let content = "决策：采用 Arc<Mutex<>> 🔒 because 'shared ownership' 需要".to_string();
        push(
            tmp_home.path(),
            Tool::Claude,
            Tool::Codex,
            &repo,
            content.clone(),
            rfc3339(),
        )
        .unwrap();

        let messages = drain(tmp_home.path(), Tool::Codex, &repo).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, content);
    }

    #[test]
    fn drain_empty_inbox_returns_empty_vec() {
        let tmp_home = TempDir::new().unwrap();
        let (_t, repo) = tmpdir_with_git();
        let messages = drain(tmp_home.path(), Tool::Claude, &repo).unwrap();
        assert!(messages.is_empty());
    }

    #[test]
    fn drain_nonexistent_inbox_dir_returns_empty_vec() {
        let tmp_home = TempDir::new().unwrap();
        let (_t, repo) = tmpdir_with_git();
        let messages = drain(tmp_home.path(), Tool::Codex, &repo).unwrap();
        assert!(messages.is_empty());
    }

    #[test]
    fn drain_preserves_fifo_order() {
        let tmp_home = TempDir::new().unwrap();
        let (_t, repo) = tmpdir_with_git();
        for i in 0..3 {
            push(
                tmp_home.path(),
                Tool::Claude,
                Tool::Codex,
                &repo,
                format!("message-{i}"),
                rfc3339(),
            )
            .unwrap();
        }

        let messages = drain(tmp_home.path(), Tool::Codex, &repo).unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].content, "message-0");
        assert_eq!(messages[1].content, "message-1");
        assert_eq!(messages[2].content, "message-2");
    }

    #[test]
    fn drain_is_one_shot_file_disappears() {
        let tmp_home = TempDir::new().unwrap();
        let (_t, repo) = tmpdir_with_git();
        push(
            tmp_home.path(),
            Tool::Claude,
            Tool::Codex,
            &repo,
            "one".into(),
            rfc3339(),
        )
        .unwrap();

        let first = drain(tmp_home.path(), Tool::Codex, &repo).unwrap();
        assert_eq!(first.len(), 1);

        let second = drain(tmp_home.path(), Tool::Codex, &repo).unwrap();
        assert!(second.is_empty());

        let path = inbox_path(tmp_home.path(), Tool::Codex, &repo).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn drain_is_isolated_per_distinct_project() {
        let tmp_home = TempDir::new().unwrap();
        let proj_a = tmp_home.path().join("alpha");
        let proj_b = tmp_home.path().join("beta");
        fs::create_dir_all(proj_a.join(".git")).unwrap();
        fs::create_dir_all(proj_b.join(".git")).unwrap();

        push(
            tmp_home.path(),
            Tool::Claude,
            Tool::Codex,
            &proj_a,
            "for alpha".into(),
            rfc3339(),
        )
        .unwrap();

        let drained = drain(tmp_home.path(), Tool::Codex, &proj_b).unwrap();
        assert!(drained.is_empty(), "proj-b drain must not see proj-a messages");

        let path_a = inbox_path(tmp_home.path(), Tool::Codex, &proj_a).unwrap();
        assert!(path_a.exists(), "proj-a inbox still present");
    }

    #[test]
    fn format_plain_empty_messages_returns_empty_string() {
        let out = format_plain(Tool::Codex, &[]);
        assert!(out.is_empty());
    }

    #[test]
    fn format_plain_includes_count_and_message_lines() {
        let msgs = vec![
            InboxMessage {
                pushed_at: "2026-04-15T01:00:00Z".into(),
                from: "codex".into(),
                content: "first".into(),
            },
            InboxMessage {
                pushed_at: "2026-04-15T01:01:00Z".into(),
                from: "codex".into(),
                content: "second".into(),
            },
        ];
        let out = format_plain(Tool::Codex, &msgs);
        assert!(out.contains("Partner inbox from codex"));
        assert!(out.contains("2 messages"));
        assert!(out.contains("first"));
        assert!(out.contains("second"));
        assert!(out.contains("[End partner inbox]"));
    }

    #[test]
    fn format_codex_hook_json_wraps_plain_in_correct_envelope() {
        let msgs = vec![InboxMessage {
            pushed_at: "2026-04-15T01:00:00Z".into(),
            from: "claude".into(),
            content: "test\nwith\nnewlines and \"quotes\"".into(),
        }];
        let out = format_codex_hook_json(Tool::Claude, &msgs).unwrap();

        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            parsed["hookSpecificOutput"]["hookEventName"],
            "UserPromptSubmit"
        );

        let ac = parsed["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap();
        let expected_plain = format_plain(Tool::Claude, &msgs);
        assert_eq!(ac, expected_plain);

        assert!(ac.contains("test\nwith\nnewlines"));
        assert!(ac.contains("\"quotes\""));
    }

    #[test]
    fn format_codex_hook_json_empty_returns_empty_string() {
        let out = format_codex_hook_json(Tool::Claude, &[]).unwrap();
        assert!(out.is_empty());
    }
}
