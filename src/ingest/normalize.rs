use serde_json::Value;
use thiserror::Error;

use super::detect::{Format, extract_content_text, extract_message_text};

pub type Result<T> = std::result::Result<T, NormalizeError>;

#[derive(Debug, Error)]
pub enum NormalizeError {
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("unsupported ChatGPT JSON shape")]
    UnsupportedChatGptShape,
}

pub fn normalize_content(content: &str, format: Format) -> Result<String> {
    match format {
        Format::PlainText => Ok(content.trim().to_string()),
        Format::ClaudeJsonl => normalize_claude_jsonl(content),
        Format::ChatGptJson => normalize_chatgpt_json(content),
        Format::CodexJsonl => normalize_codex_jsonl(content),
        Format::SlackJson => normalize_slack_json(content),
    }
}

fn normalize_claude_jsonl(content: &str) -> Result<String> {
    let mut lines = Vec::new();

    for raw_line in content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let value: Value = serde_json::from_str(raw_line)?;
        let role = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("assistant");
        let message = extract_message_text(&value).unwrap_or_default();

        if message.trim().is_empty() {
            continue;
        }

        if matches!(role, "human" | "user") {
            lines.push(format!("> {}", message.trim()));
        } else {
            lines.push(message.trim().to_string());
        }
    }

    Ok(lines.join("\n"))
}

fn normalize_chatgpt_json(content: &str) -> Result<String> {
    let value: Value = serde_json::from_str(content)?;

    if let Some(messages) = value.as_array() {
        return normalize_chatgpt_messages(messages);
    }

    if let Some(messages) = value.get("messages").and_then(Value::as_array) {
        return normalize_chatgpt_messages(messages);
    }

    if let Some(mapping) = value.get("mapping").and_then(Value::as_object) {
        let mut ordered = Vec::new();
        if let Some(root_id) = find_root_node(mapping) {
            collect_messages_dfs(mapping, &root_id, &mut ordered);
        }

        return Ok(render_transcript(ordered));
    }

    Err(NormalizeError::UnsupportedChatGptShape)
}

fn normalize_chatgpt_messages(messages: &[Value]) -> Result<String> {
    let transcript = render_transcript(messages.iter().filter_map(|message| {
        let role = message.get("role").and_then(Value::as_str)?;
        let content = message.get("content").and_then(extract_content_text)?;
        Some((role.to_string(), content))
    }));

    Ok(transcript)
}

fn find_root_node(mapping: &serde_json::Map<String, Value>) -> Option<String> {
    mapping
        .iter()
        .find(|(_, node)| {
            node.get("parent")
                .is_none_or(|p| p.is_null() || p.as_str() == Some(""))
        })
        .map(|(id, _)| id.clone())
}

fn collect_messages_dfs(
    mapping: &serde_json::Map<String, Value>,
    node_id: &str,
    result: &mut Vec<(String, String)>,
) {
    let Some(node) = mapping.get(node_id) else {
        return;
    };

    if let Some(message) = node.get("message") {
        let role = message
            .get("author")
            .and_then(|author| author.get("role"))
            .and_then(Value::as_str);
        let content = message.get("content").and_then(extract_content_text);
        if let (Some(role), Some(content)) = (role, content) {
            result.push((role.to_string(), content));
        }
    }

    if let Some(children) = node.get("children").and_then(Value::as_array) {
        for child in children {
            if let Some(child_id) = child.as_str() {
                collect_messages_dfs(mapping, child_id, result);
            }
        }
    }
}

fn normalize_codex_jsonl(content: &str) -> Result<String> {
    let mut pairs: Vec<(String, String)> = Vec::new();

    for line in content.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let value: Value = serde_json::from_str(line)?;
        if value.get("type").and_then(Value::as_str) != Some("event_msg") {
            continue;
        }
        let Some(payload) = value.get("payload") else {
            continue;
        };
        let msg_type = payload.get("type").and_then(Value::as_str).unwrap_or("");
        let message = payload
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        if message.is_empty() {
            continue;
        }
        match msg_type {
            "user_message" => pairs.push(("user".to_string(), message.to_string())),
            "agent_message" => pairs.push(("assistant".to_string(), message.to_string())),
            _ => {}
        }
    }

    Ok(render_transcript(pairs))
}

fn normalize_slack_json(content: &str) -> Result<String> {
    let value: Value = serde_json::from_str(content)?;
    let messages = value
        .as_array()
        .ok_or(NormalizeError::UnsupportedChatGptShape)?;

    let mut speakers: Vec<String> = Vec::new();
    let mut pairs: Vec<(String, String)> = Vec::new();

    for msg in messages {
        if msg.get("type").and_then(Value::as_str) != Some("message") {
            continue;
        }
        let speaker = msg
            .get("user")
            .or_else(|| msg.get("username"))
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let text = msg.get("text").and_then(Value::as_str).unwrap_or("").trim();
        if text.is_empty() {
            continue;
        }

        // First speaker = user, second = assistant
        if !speakers.contains(&speaker) {
            speakers.push(speaker.clone());
        }
        let role = if speakers.first() == Some(&speaker) {
            "user"
        } else {
            "assistant"
        };
        pairs.push((role.to_string(), text.to_string()));
    }

    Ok(render_transcript(pairs))
}

fn render_transcript(items: impl IntoIterator<Item = (String, String)>) -> String {
    let mut lines = Vec::new();

    for (role, content) in items {
        if content.trim().is_empty() {
            continue;
        }

        if matches!(role.as_str(), "user" | "human") {
            lines.push(format!("> {}", content.trim()));
        } else {
            lines.push(content.trim().to_string());
        }
    }

    lines.join("\n")
}
