//! Public analysis layer extracted from AaakCodec internals.
//!
//! Provides structured signal extraction without producing an AAAK document.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// Structured signals derived from text using the existing AAAK heuristics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AaakSignals {
    pub entities: Vec<String>,
    pub topics: Vec<String>,
    pub flags: Vec<String>,
    pub emotions: Vec<String>,
    pub importance_stars: u8,
}

/// Analyze text and return structured AAAK-derived signals.
pub fn analyze(text: &str) -> AaakSignals {
    let normalized = super::codec::normalize_whitespace(text);

    let mut entity_codes = Vec::new();
    let mut seen = BTreeSet::new();
    for entity in super::codec::extract_entities(&normalized) {
        let code = super::codec::default_entity_code(&entity);
        if seen.insert(code.clone()) {
            entity_codes.push(code);
        }
    }
    if entity_codes.is_empty() {
        entity_codes.push("UNK".to_string());
    }

    let flags = super::codec::detect_flags(&normalized);
    let importance_stars = super::codec::infer_weight(&flags);

    AaakSignals {
        entities: entity_codes,
        topics: super::codec::extract_topics(&normalized),
        flags,
        emotions: super::codec::detect_emotions(&normalized),
        importance_stars,
    }
}

#[cfg(test)]
mod tests {
    use super::analyze;

    #[test]
    fn test_importance_stars_defaults_to_2_for_uncategorized_text() {
        let signals = analyze("weather update today");
        assert_eq!(signals.importance_stars, 2);
        assert!(signals.flags.contains(&"CORE".to_string()));
        assert!(!signals.flags.contains(&"DECISION".to_string()));
    }

    #[test]
    fn test_empty_content_yields_sentinel_defaults() {
        let signals = analyze("");
        assert_eq!(signals.entities, vec!["UNK".to_string()]);
        assert_eq!(signals.flags, vec!["CORE".to_string()]);
        assert_eq!(signals.emotions, vec!["determ".to_string()]);
        assert!(signals.topics.is_empty());
        assert_eq!(signals.importance_stars, 2);
    }

    #[test]
    fn test_whitespace_content_matches_empty_sentinel_behavior() {
        let signals = analyze("   \t\n  ");
        assert_eq!(signals.entities, vec!["UNK".to_string()]);
        assert_eq!(signals.flags, vec!["CORE".to_string()]);
        assert_eq!(signals.emotions, vec!["determ".to_string()]);
        assert!(signals.topics.is_empty());
        assert_eq!(signals.importance_stars, 2);
    }

    #[test]
    fn test_analyze_entities_never_empty_after_code_mapping() {
        let cases = [
            "",
            "   \t\n  ",
            "12345",
            "just a boring sentence",
            "Decision: use Arc<Mutex<>>",
        ];

        for text in cases {
            let signals = analyze(text);
            assert!(
                !signals.entities.is_empty(),
                "entities empty for input: {text:?}"
            );

            let first = &signals.entities[0];
            let chars = first.chars().count();
            assert!(
                (3..=4).contains(&chars),
                "first entity {first:?} should be 3-4 chars"
            );
            assert!(
                first.chars().all(|ch: char| ch.is_ascii_uppercase()),
                "first entity {first:?} should be uppercase ASCII"
            );
        }
    }
}
