use serde_json::Value;

const SYSTEM_REMINDER_OPEN: &str = "<system-reminder>";
const SYSTEM_REMINDER_CLOSE: &str = "</system-reminder>";

pub fn strip_claude_jsonl_noise(content: &str) -> String {
    let without_system_reminders = strip_system_reminders(content);
    strip_noise_lines(&without_system_reminders, true)
}

pub fn strip_codex_rollout_noise(content: &str) -> String {
    strip_noise_lines(content, false)
}

fn strip_system_reminders(content: &str) -> String {
    let mut output = String::with_capacity(content.len());
    let mut in_code_block = false;
    let mut skipping_reminder = false;

    for line in content.split_inclusive('\n') {
        let line_without_newline = line.strip_suffix('\n').unwrap_or(line);
        if !skipping_reminder && is_code_fence(line_without_newline) {
            in_code_block = !in_code_block;
            output.push_str(line);
            continue;
        }
        if in_code_block {
            output.push_str(line);
            continue;
        }

        output.push_str(&strip_system_reminders_from_line(
            line,
            &mut skipping_reminder,
        ));
    }

    output
}

fn strip_system_reminders_from_line(line: &str, skipping_reminder: &mut bool) -> String {
    let mut output = String::new();
    let mut remaining = line;

    loop {
        if *skipping_reminder {
            let Some(end) = remaining.find(SYSTEM_REMINDER_CLOSE) else {
                return output;
            };
            remaining = &remaining[end + SYSTEM_REMINDER_CLOSE.len()..];
            *skipping_reminder = false;
        }

        let Some(start) = remaining.find(SYSTEM_REMINDER_OPEN) else {
            output.push_str(remaining);
            return output;
        };
        output.push_str(&remaining[..start]);
        let after_open = &remaining[start + SYSTEM_REMINDER_OPEN.len()..];
        if let Some(end) = after_open.find(SYSTEM_REMINDER_CLOSE) {
            remaining = &after_open[end + SYSTEM_REMINDER_CLOSE.len()..];
        } else {
            *skipping_reminder = true;
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
