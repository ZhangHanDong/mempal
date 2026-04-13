//! Claude Code session reader.

use crate::cowork::peek::{PeekError, PeekMessage, parse_rfc3339};
use serde_json::Value;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Build the Claude Code project encoding from a cwd: `/` → `-`.
///
/// Claude Code stores session files under
/// `~/.claude/projects/<encoded>/<session-uuid>.jsonl` where `<encoded>`
/// is the cwd with every `/` replaced by `-`.
pub fn encode_cwd(cwd: &Path) -> String {
    cwd.to_string_lossy().replace('/', "-")
}

/// Build the Claude Code project directory for a given cwd and HOME.
pub fn claude_project_dir(home: &Path, cwd: &Path) -> PathBuf {
    home.join(".claude/projects").join(encode_cwd(cwd))
}

/// Find the latest (by mtime) `.jsonl` in the Claude project directory.
/// Returns `None` if the directory doesn't exist or has no jsonl files.
pub fn latest_session_file(project_dir: &Path) -> Option<(PathBuf, SystemTime)> {
    let entries = fs::read_dir(project_dir).ok()?;
    entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "jsonl").unwrap_or(false))
        .filter_map(|e| {
            let mtime = e.metadata().ok()?.modified().ok()?;
            Some((e.path(), mtime))
        })
        .max_by_key(|(_, m)| *m)
}

/// Parse a Claude Code session jsonl. Returns `(messages, truncated)` where
/// `messages` is the tail `limit` user+assistant text messages in ascending
/// chronological order, and `truncated` is true iff more than `limit`
/// candidates existed in the file.
///
/// Single-pass: read the whole file once, accumulate candidates, take tail.
pub fn parse_jsonl_messages(
    path: &Path,
    since: Option<&str>,
    limit: usize,
) -> Result<(Vec<PeekMessage>, bool), PeekError> {
    // Pre-parse the `since` cutoff once into epoch seconds. Compare in
    // instant semantics, not lexicographic string order — string compare is
    // broken across timezone offsets (e.g. "10:00+08:00" vs "02:00Z" look
    // unequal but are the same instant).
    let since_cutoff: Option<i64> = match since {
        Some(raw) => Some(parse_rfc3339(raw).ok_or_else(|| {
            PeekError::Parse(format!("invalid `since` RFC3339 timestamp: {raw}"))
        })?),
        None => None,
    };

    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut all: Vec<PeekMessage> = Vec::new();

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let val: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Some(msg) = extract_message(&val) {
            if let Some(cutoff) = since_cutoff {
                // Messages whose timestamp can't be parsed are kept (safer
                // default than silently dropping them).
                if let Some(msg_ts) = parse_rfc3339(&msg.at) {
                    if msg_ts <= cutoff {
                        continue;
                    }
                }
            }
            all.push(msg);
        }
    }

    let total = all.len();
    let truncated = total > limit;
    let start = total.saturating_sub(limit);
    let tail = all.split_off(start);
    Ok((tail, truncated))
}

/// Extract a PeekMessage from one Claude jsonl entry if it's a user/assistant
/// text message. Returns `None` for:
///   * top-level `type` other than "user" / "assistant"
///   * entries with `isMeta: true` (command caveats, shell echoes)
///   * messages whose content has no extractable text (only tool_use / only tool_result)
fn extract_message(val: &Value) -> Option<PeekMessage> {
    let top_type = val.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if top_type != "user" && top_type != "assistant" {
        return None;
    }

    if val.get("isMeta").and_then(|v| v.as_bool()).unwrap_or(false) {
        return None;
    }

    let message = val.get("message")?;
    let role = message.get("role").and_then(|v| v.as_str())?;
    if role != "user" && role != "assistant" {
        return None;
    }

    // `message.content` can be either a plain string or an array of content blocks.
    let content = message.get("content")?;
    let text = match content {
        Value::String(s) => s.trim().to_string(),
        Value::Array(blocks) => {
            let parts: Vec<String> = blocks
                .iter()
                .filter_map(|b| {
                    let block_type = b.get("type").and_then(|v| v.as_str())?;
                    if block_type == "text" {
                        b.get("text")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                    } else {
                        None
                    }
                })
                .collect();
            parts.join("\n")
        }
        _ => return None,
    };

    if text.is_empty() {
        return None;
    }

    let at = val
        .get("timestamp")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Some(PeekMessage {
        role: role.to_string(),
        at,
        text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn reads_plain_text_and_structured_content() {
        let fixture = Path::new("tests/fixtures/cowork/claude/session.jsonl");
        let (messages, truncated) = parse_jsonl_messages(fixture, None, 30).expect("parse");
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].text, "Hello from user turn 1");
        assert_eq!(messages[1].role, "assistant");
        assert_eq!(messages[1].text, "Hello from assistant turn 1");
        assert_eq!(messages[3].text, "Second assistant reply");
        assert!(messages[0].at <= messages[3].at);
        assert!(!truncated);
    }

    #[test]
    fn filters_tool_use_blocks_and_is_meta_entries() {
        let fixture = Path::new("tests/fixtures/cowork/claude/session_with_tools.jsonl");
        let (messages, _) = parse_jsonl_messages(fixture, None, 30).expect("parse");
        // Expected: u1 ("User turn with tool"), a1 ("Let me check"),
        //           a2 ("Here is the listing"), u4 ("Follow-up question")
        // Skipped:  u2 (only tool_result — no text block), u3 (isMeta:true)
        assert_eq!(messages.len(), 4);
        for m in &messages {
            assert!(m.role == "user" || m.role == "assistant");
            assert!(!m.text.is_empty());
            assert!(!m.text.contains("tool_use"));
            assert!(!m.text.contains("tool_result"));
        }
        assert_eq!(messages[0].text, "User turn with tool");
        assert_eq!(messages[1].text, "Let me check");
        assert_eq!(messages[2].text, "Here is the listing");
        assert_eq!(messages[3].text, "Follow-up question");
    }

    #[test]
    fn honors_limit_by_taking_tail_and_sets_truncated() {
        let fixture = Path::new("tests/fixtures/cowork/claude/session.jsonl");
        let (messages, truncated) = parse_jsonl_messages(fixture, None, 2).expect("parse");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].text, "Second user message");
        assert_eq!(messages[1].text, "Second assistant reply");
        assert!(truncated);
    }

    #[test]
    fn since_filter_compares_instants_not_strings() {
        // Cross-timezone regression: `since` with a +08:00 offset represents
        // UTC 02:00:00. Messages at UTC 02:30:00Z and UTC 05:00:00Z are both
        // strictly newer than that instant, so they MUST be returned. The
        // message at UTC 01:00:00Z is older and must be dropped.
        //
        // With a naive lexicographic string compare, `"...01:00:00Z"`,
        // `"...02:30:00Z"`, `"...05:00:00Z"` are all textually less than
        // `"...10:00:00+08:00"`, so the broken implementation drops all 3.
        let fixture = Path::new("tests/fixtures/cowork/claude_since/session.jsonl");
        let since = Some("2026-04-13T10:00:00+08:00");
        let (messages, _) = parse_jsonl_messages(fixture, since, 30).expect("parse");
        assert_eq!(
            messages.len(),
            2,
            "expected 2 messages strictly newer than the +08:00 cutoff \
             (UTC 02:00), got {}: {:?}",
            messages.len(),
            messages.iter().map(|m| &m.text).collect::<Vec<_>>()
        );
        assert!(messages.iter().any(|m| m.text.contains("02:30")));
        assert!(messages.iter().any(|m| m.text.contains("05:00")));
        assert!(!messages.iter().any(|m| m.text.contains("01:00")));
    }

    #[test]
    fn encoded_cwd_replaces_slashes_with_dashes() {
        assert_eq!(encode_cwd(Path::new("/Users/foo/bar")), "-Users-foo-bar");
        assert_eq!(encode_cwd(Path::new("/a")), "-a");
    }
}
