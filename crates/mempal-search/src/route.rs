use std::collections::BTreeSet;

use mempal_core::types::{RouteDecision, TaxonomyEntry};

pub fn route_query(query: &str, taxonomy: &[TaxonomyEntry]) -> RouteDecision {
    let normalized_query = query.to_lowercase();
    let query_terms = query_terms(&normalized_query);

    let Some((entry, matched_keywords)) = taxonomy
        .iter()
        .filter_map(|entry| {
            let matched_keywords = matched_keywords(&normalized_query, &query_terms, entry);
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
    else {
        return RouteDecision {
            wing: None,
            room: None,
            confidence: 0.0,
            reason: "global search: no taxonomy keyword match".to_string(),
        };
    };

    let room = (!entry.room.is_empty()).then_some(entry.room.clone());
    let confidence = match matched_keywords.len() {
        0 => 0.0,
        1 => 0.6,
        2 => 0.8,
        _ => 1.0,
    };
    let reason = format!(
        "taxonomy match: {} / {} via keywords [{}]",
        entry.wing,
        room.as_deref().unwrap_or("default"),
        matched_keywords.join(", ")
    );

    RouteDecision {
        wing: Some(entry.wing.clone()),
        room,
        confidence,
        reason,
    }
}

fn matched_keywords(
    normalized_query: &str,
    query_terms: &BTreeSet<String>,
    entry: &TaxonomyEntry,
) -> Vec<String> {
    entry
        .keywords
        .iter()
        .map(|keyword| keyword.trim().to_lowercase())
        .filter(|keyword| {
            !keyword.is_empty()
                && (query_terms.contains(keyword) || normalized_query.contains(keyword.as_str()))
        })
        .collect()
}

fn query_terms(query: &str) -> BTreeSet<String> {
    query
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|term| !term.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}
