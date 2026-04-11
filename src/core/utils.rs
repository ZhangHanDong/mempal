use std::collections::BTreeSet;
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use super::types::TaxonomyEntry;

pub const DEFAULT_ROOM: &str = "default";

pub fn build_drawer_id(wing: &str, room: Option<&str>, content: &str) -> String {
    let room = room.unwrap_or(DEFAULT_ROOM);
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let digest = format!("{:x}", hasher.finalize());

    format!(
        "drawer_{}_{}_{}",
        sanitize_component(wing),
        sanitize_component(room),
        &digest[..8]
    )
}

pub fn build_triple_id(subject: &str, predicate: &str, object: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(subject.as_bytes());
    hasher.update([0]);
    hasher.update(predicate.as_bytes());
    hasher.update([0]);
    hasher.update(object.as_bytes());
    let digest = format!("{:x}", hasher.finalize());

    format!(
        "triple_{}_{}_{}",
        sanitize_component_prefix(subject, 8),
        sanitize_component_prefix(predicate, 8),
        &digest[..8]
    )
}

pub fn current_timestamp() -> String {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs().to_string(),
        Err(_) => "0".to_string(),
    }
}

pub fn synthetic_source_file(drawer_id: &str) -> String {
    format!("mempal://drawer/{drawer_id}")
}

pub fn source_file_or_synthetic(drawer_id: &str, source_file: Option<&str>) -> String {
    source_file
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| synthetic_source_file(drawer_id))
}

pub fn route_room_from_taxonomy(content: &str, wing: &str, taxonomy: &[TaxonomyEntry]) -> String {
    let normalized_content = content.to_lowercase();
    let content_terms = content_terms(&normalized_content);

    taxonomy
        .iter()
        .filter(|entry| entry.wing == wing)
        .filter_map(|entry| {
            let matched_keywords = matched_keywords(&normalized_content, &content_terms, entry);
            (!matched_keywords.is_empty()).then_some((entry, matched_keywords))
        })
        .max_by(|(left_entry, left_matches), (right_entry, right_matches)| {
            left_matches
                .len()
                .cmp(&right_matches.len())
                .then_with(|| {
                    left_matches
                        .iter()
                        .map(String::len)
                        .sum::<usize>()
                        .cmp(&right_matches.iter().map(String::len).sum::<usize>())
                })
                .then_with(|| left_entry.keywords.len().cmp(&right_entry.keywords.len()))
        })
        .map(|(entry, _)| {
            if entry.room.trim().is_empty() {
                DEFAULT_ROOM.to_string()
            } else {
                entry.room.clone()
            }
        })
        .unwrap_or_else(|| DEFAULT_ROOM.to_string())
}

fn sanitize_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn sanitize_component_prefix(value: &str, max_len: usize) -> String {
    let sanitized = sanitize_component(value);
    let prefix: String = sanitized.chars().take(max_len).collect();
    if prefix.is_empty() {
        "x".to_string()
    } else {
        prefix
    }
}

fn matched_keywords(
    normalized_content: &str,
    content_terms: &BTreeSet<String>,
    entry: &TaxonomyEntry,
) -> Vec<String> {
    entry
        .keywords
        .iter()
        .map(|keyword| keyword.trim().to_lowercase())
        .filter(|keyword| {
            !keyword.is_empty()
                && (content_terms.contains(keyword)
                    || normalized_content.contains(keyword.as_str()))
        })
        .collect()
}

fn content_terms(content: &str) -> BTreeSet<String> {
    content
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|term| !term.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}
