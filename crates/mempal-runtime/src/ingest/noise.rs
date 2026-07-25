use serde_json::Value;

const SYSTEM_REMINDER_TAG: &str = "system-reminder";

/// Codex runtime preamble wrappers injected as user/developer messages
/// (issue #10): AGENTS.md instructions, environment context, plugin
/// recommendations, and turn-abort markers.
const CODEX_WRAPPER_TAGS: &[&str] = &[
    "INSTRUCTIONS",
    "user_instructions",
    "environment_context",
    "recommended_plugins",
    "turn_aborted",
];

struct MarkerPair {
    open: String,
    close: String,
}

impl MarkerPair {
    fn for_tag(tag: &str) -> Self {
        Self {
            open: format!("<{tag}>"),
            close: format!("</{tag}>"),
        }
    }
}

pub fn strip_claude_jsonl_noise(content: &str) -> String {
    let markers = [MarkerPair::for_tag(SYSTEM_REMINDER_TAG)];
    let without_system_reminders = strip_marker_blocks(content, &markers);
    strip_noise_lines(&without_system_reminders, true)
}

pub fn strip_codex_rollout_noise(content: &str) -> String {
    let markers: Vec<MarkerPair> = CODEX_WRAPPER_TAGS
        .iter()
        .map(|tag| MarkerPair::for_tag(tag))
        .collect();
    let without_wrappers = strip_marker_blocks(content, &markers);
    strip_noise_lines(&without_wrappers, false)
}

fn strip_marker_blocks(content: &str, markers: &[MarkerPair]) -> String {
    let mut output = String::with_capacity(content.len());
    let mut in_code_block = false;
    let mut skipping: Option<usize> = None;

    for line in content.split_inclusive('\n') {
        let line_without_newline = line.strip_suffix('\n').unwrap_or(line);
        if skipping.is_none() && is_code_fence(line_without_newline) {
            in_code_block = !in_code_block;
            output.push_str(line);
            continue;
        }
        if in_code_block {
            output.push_str(line);
            continue;
        }

        output.push_str(&strip_marker_blocks_from_line(line, markers, &mut skipping));
    }

    output
}

fn strip_marker_blocks_from_line(
    line: &str,
    markers: &[MarkerPair],
    skipping: &mut Option<usize>,
) -> String {
    let mut output = String::new();
    let mut remaining = line;

    loop {
        if let Some(active) = *skipping {
            let close = markers[active].close.as_str();
            let Some(end) = remaining.find(close) else {
                return output;
            };
            remaining = &remaining[end + close.len()..];
            *skipping = None;
        }

        let earliest_open = markers
            .iter()
            .enumerate()
            .filter_map(|(index, marker)| {
                remaining
                    .find(marker.open.as_str())
                    .map(|position| (position, index))
            })
            .min();
        let Some((start, index)) = earliest_open else {
            output.push_str(remaining);
            return output;
        };
        output.push_str(&remaining[..start]);
        let marker = &markers[index];
        let after_open = &remaining[start + marker.open.len()..];
        if let Some(end) = after_open.find(marker.close.as_str()) {
            remaining = &after_open[end + marker.close.len()..];
        } else {
            *skipping = Some(index);
            return output;
        }
    }
}

fn strip_noise_lines(content: &str, claude_rules: bool) -> String {
    let mut output = String::with_capacity(content.len());
    let mut in_code_block = false;
    let mut skipping_banner = false;

    for line in content.split_inclusive('\n') {
        let line_without_newline = line.strip_suffix('\n').unwrap_or(line);
        let trimmed = line_without_newline.trim();

        if is_code_fence(line_without_newline) {
            in_code_block = !in_code_block;
            output.push_str(line);
            continue;
        }
        if in_code_block {
            output.push_str(line);
            continue;
        }

        if skipping_banner {
            if trimmed.is_empty() {
                skipping_banner = false;
            }
            continue;
        }

        if claude_rules && is_skill_banner_start(trimmed) {
            skipping_banner = true;
            continue;
        }
        if claude_rules && (is_command_name_line(trimmed) || is_tool_use_id_array_line(trimmed)) {
            continue;
        }
        if is_codex_session_marker(trimmed) {
            continue;
        }

        output.push_str(line);
    }

    output
}

fn is_code_fence(line: &str) -> bool {
    line.trim_start().starts_with("```")
}

fn is_skill_banner_start(trimmed: &str) -> bool {
    trimmed.starts_with("=== DORA SKILLS LOADED ===")
        || trimmed.starts_with("=== RUST SKILLS Loaded ===")
}

fn is_command_name_line(trimmed: &str) -> bool {
    trimmed.starts_with("<command-name>") && trimmed.ends_with("</command-name>")
}

fn is_tool_use_id_array_line(trimmed: &str) -> bool {
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(trimmed) else {
        return false;
    };
    !items.is_empty()
        && items
            .iter()
            .all(|item| item.get("type").and_then(Value::as_str) == Some("tool_use_id"))
}

fn is_codex_session_marker(trimmed: &str) -> bool {
    trimmed.starts_with("[session ")
        && (trimmed.ends_with(" started]") || trimmed.ends_with(" ended]"))
}
