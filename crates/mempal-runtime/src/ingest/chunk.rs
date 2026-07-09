pub fn chunk_text(text: &str, window: usize, overlap: usize) -> Vec<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() || window == 0 {
        return Vec::new();
    }

    debug_assert!(
        overlap < window,
        "chunk overlap ({overlap}) must be less than window ({window})"
    );
    let overlap = overlap.min(window.saturating_sub(1));

    let chars = trimmed.chars().collect::<Vec<_>>();
    let mut chunks = Vec::new();
    let mut start = 0usize;

    while start < chars.len() {
        let mut end = usize::min(start + window, chars.len());

        if end < chars.len()
            && let Some(split) = chars[start..end]
                .iter()
                .rposition(|ch| matches!(ch, '\n' | ' ' | '\t'))
            && split > window / 2
        {
            end = start + split + 1;
        }

        let chunk = chars[start..end]
            .iter()
            .collect::<String>()
            .trim()
            .to_string();
        if !chunk.is_empty() {
            chunks.push(chunk);
        }

        if end == chars.len() {
            break;
        }

        let next_start = end.saturating_sub(overlap);
        start = if next_start <= start { end } else { next_start };
    }

    chunks
}

pub fn chunk_conversation(transcript: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = Vec::new();

    for line in transcript.lines() {
        let is_user_turn = line.starts_with("> ");
        if is_user_turn && !current.is_empty() {
            chunks.push(current.join("\n"));
            current.clear();
        }

        if !line.trim().is_empty() || !current.is_empty() {
            current.push(line.to_string());
        }
    }

    if !current.is_empty() {
        chunks.push(current.join("\n"));
    }

    chunks
}
