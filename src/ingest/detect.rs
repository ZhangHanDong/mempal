use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    ClaudeJsonl,
    ChatGptJson,
    CodexJsonl,
    SlackJson,
    PlainText,
}

pub fn detect_format(content: &str) -> Format {
    if is_claude_jsonl(content) {
        return Format::ClaudeJsonl;
    }

    if is_codex_jsonl(content) {
        return Format::CodexJsonl;
    }

    if is_slack_json(content) {
        return Format::SlackJson;
    }

    if is_chatgpt_json(content) {
        return Format::ChatGptJson;
    }

    Format::PlainText
}

fn is_codex_jsonl(content: &str) -> bool {
    let mut has_session_meta = false;
    let mut has_activity = false;

    for line in content.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return false;
        };
        match value.get("type").and_then(Value::as_str) {
            Some("session_meta") => has_session_meta = true,
            Some("event_msg" | "response_item" | "turn_context" | "compacted") => {
                has_activity = true
            }
            Some(_) => {} // tolerate newer rollout record types
            None => return false,
        }
    }

    has_session_meta && has_activity
}

fn is_slack_json(content: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(content) else {
        return false;
    };
    let Some(arr) = value.as_array() else {
        return false;
    };
    // Slack messages have "type": "message" and "user"/"username" + "text"
    arr.iter().take(5).any(|msg| {
        msg.get("type").and_then(Value::as_str) == Some("message")
            && (msg.get("user").is_some() || msg.get("username").is_some())
            && msg.get("text").is_some()
    })
}

fn is_claude_jsonl(content: &str) -> bool {
    let mut saw_line = false;

    for line in content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return false;
        };

        if value.get("type").and_then(Value::as_str).is_none() {
            return false;
        }
        if extract_message_text(&value).is_none() {
            return false;
        }

        saw_line = true;
    }

    saw_line
}

fn is_chatgpt_json(content: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(content) else {
        return false;
    };

    matches!(value, Value::Array(_))
        || value.get("messages").is_some()
        || value.get("mapping").is_some()
}

pub(crate) fn extract_message_text(value: &Value) -> Option<String> {
    value
        .get("message")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| value.get("content").and_then(extract_content_text))
}

pub(crate) fn extract_content_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Array(items) => Some(
            items
                .iter()
                .filter_map(|item| {
                    item.as_str().map(ToOwned::to_owned).or_else(|| {
                        item.get("text")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned)
                    })
                })
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        Value::Object(map) => map
            .get("parts")
            .and_then(Value::as_array)
            .map(|parts| {
                parts
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .filter(|text| !text.is_empty()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{Format, detect_format};

    #[test]
    fn detects_current_codex_rollout_with_turn_context_and_compacted() {
        let content = r#"{"timestamp":"2026-04-19T10:37:36.000Z","type":"session_meta","payload":{"cwd":"/tmp/project"}}
{"timestamp":"2026-04-19T10:37:36.100Z","type":"turn_context","payload":{"cwd":"/tmp/project"}}
{"timestamp":"2026-04-19T10:37:36.200Z","type":"response_item","payload":{"type":"message","role":"developer","content":[{"type":"input_text","text":"instructions"}]}}
{"timestamp":"2026-04-19T10:37:36.300Z","type":"compacted","payload":{"summary":"trimmed"}}
{"timestamp":"2026-04-19T10:37:36.400Z","type":"event_msg","payload":{"type":"token_count"}}"#;

        assert_eq!(detect_format(content), Format::CodexJsonl);
    }
}
