//! Name candidate extraction + Levenshtein-based similar-name detection.

use std::collections::HashSet;

use crate::core::db::{Database, DbError};

use super::FactIssue;

/// Levenshtein distance on bytes (OK for ASCII names; CJK is out of scope
/// per P9-A spec). Two-row buffer, O(min(a, b)) space.
pub fn edit_distance(a: &str, b: &str) -> usize {
    let (a, b) = if a.len() < b.len() { (b, a) } else { (a, b) };
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();

    if b.is_empty() {
        return a.len();
    }

    let mut prev: Vec<usize> = (0..=b_bytes.len()).collect();
    let mut curr = vec![0usize; b_bytes.len() + 1];

    for (i, ac) in a_bytes.iter().enumerate() {
        curr[0] = i + 1;
        for (j, bc) in b_bytes.iter().enumerate() {
            let cost = if ac == bc { 0 } else { 1 };
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b_bytes.len()]
}

/// Extract candidate entity names from text by reusing AAAK's entity
/// extractor (capitalized proper nouns, stop-list filtered). Preserves
/// first-seen order.
pub fn candidates_from_text(text: &str) -> Vec<String> {
    crate::aaak::codec::extract_entities(text)
        .into_iter()
        .filter(|s| s.len() >= 3)
        .collect()
}

/// Build the known-entity set from KG subjects/objects plus recent
/// drawers in the optional scope (capped at 50 drawers to bound cost).
pub fn query_known_entities(
    db: &Database,
    scope: Option<(&str, Option<&str>)>,
) -> Result<Vec<String>, DbError> {
    let mut seen: HashSet<String> = HashSet::new();

    // Path 1: KG subjects + objects (capitalized object heuristic to
    // reduce noise — KG objects can be anything).
    let triples = db.query_triples(None, None, None, false)?;
    for t in triples {
        if is_name_like(&t.subject) {
            seen.insert(t.subject);
        }
        if is_name_like(&t.object) {
            seen.insert(t.object);
        }
    }

    // Path 2: recent drawers in scope (cap 50). Reuse AAAK extraction
    // so the name-distribution matches search result signals.
    let drawers = db.top_drawers(50)?;
    for drawer in drawers {
        if let Some((wing, room)) = scope {
            if drawer.wing != wing {
                continue;
            }
            if let Some(r) = room {
                if drawer.room.as_deref() != Some(r) {
                    continue;
                }
            }
        }
        for entity in crate::aaak::codec::extract_entities(&drawer.content) {
            if entity.len() >= 3 {
                seen.insert(entity);
            }
        }
    }

    Ok(seen.into_iter().collect())
}

fn is_name_like(value: &str) -> bool {
    value.len() >= 3
        && value.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && value.chars().all(|c| c.is_ascii_alphabetic())
}

/// For each `mentioned ∈ text_names`, emit `SimilarNameConflict` if the
/// closest known entity is within edit distance 2 and not identical.
pub fn detect_similar_name_conflicts(text_names: &[String], known: &[String]) -> Vec<FactIssue> {
    let mut issues = Vec::new();

    for mentioned in text_names {
        let mut best: Option<(&String, usize)> = None;
        for k in known {
            if k == mentioned {
                // Exact match: not a conflict. Short-circuit.
                best = None;
                break;
            }
            let d = edit_distance(mentioned, k);
            if d <= 2 && best.map(|(_, bd)| d < bd).unwrap_or(true) {
                best = Some((k, d));
            }
        }
        if let Some((known_entity, edit)) = best {
            issues.push(FactIssue::SimilarNameConflict {
                mentioned: mentioned.clone(),
                known_entity: known_entity.clone(),
                edit_distance: edit,
            });
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edit_distance_equal_zero() {
        assert_eq!(edit_distance("Alice", "Alice"), 0);
        assert_eq!(edit_distance("", ""), 0);
    }

    #[test]
    fn test_edit_distance_one_substitution() {
        assert_eq!(edit_distance("Bob", "Bop"), 1);
    }

    #[test]
    fn test_edit_distance_insertion_plus_deletion() {
        // "Bob" -> "Bobby" requires inserting "by" (2 edits).
        assert_eq!(edit_distance("Bob", "Bobby"), 2);
    }

    #[test]
    fn test_edit_distance_empty_vs_nonempty() {
        assert_eq!(edit_distance("", "Alice"), 5);
        assert_eq!(edit_distance("Alice", ""), 5);
    }

    #[test]
    fn test_similar_name_detects_bob_vs_bobby() {
        let text = vec!["Bobby".to_string()];
        let known = vec!["Bob".to_string(), "Alice".to_string()];
        let issues = detect_similar_name_conflicts(&text, &known);
        assert_eq!(issues.len(), 1);
        match &issues[0] {
            FactIssue::SimilarNameConflict {
                mentioned,
                known_entity,
                edit_distance,
            } => {
                assert_eq!(mentioned, "Bobby");
                assert_eq!(known_entity, "Bob");
                assert_eq!(*edit_distance, 2);
            }
            other => panic!("unexpected issue: {other:?}"),
        }
    }

    #[test]
    fn test_similar_name_ignores_identical() {
        let text = vec!["Alice".to_string()];
        let known = vec!["Alice".to_string(), "Alicia".to_string()];
        let issues = detect_similar_name_conflicts(&text, &known);
        // Exact match wins — no conflict reported even though Alicia is
        // edit distance 2.
        assert!(issues.is_empty());
    }

    #[test]
    fn test_is_name_like_requires_capital_alpha() {
        assert!(is_name_like("Alice"));
        assert!(!is_name_like("alice"));
        assert!(!is_name_like("Al")); // too short
        assert!(!is_name_like("Acme Inc")); // space not allowed
        assert!(!is_name_like("Co3")); // digit
    }
}
