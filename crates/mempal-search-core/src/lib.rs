#![warn(clippy::all)]

use std::collections::HashMap;

pub const DEFAULT_RRF_K: f64 = 60.0;

#[derive(Debug, Clone, PartialEq)]
pub struct RankedHit<T> {
    pub key: String,
    pub item: T,
    pub score: f64,
}

pub fn reciprocal_rank_fusion<T>(
    ranked_lists: Vec<Vec<(String, T)>>,
    limit: usize,
    k: f64,
) -> Vec<RankedHit<T>> {
    if limit == 0 {
        return Vec::new();
    }

    let mut scores: HashMap<String, f64> = HashMap::new();
    let mut items: HashMap<String, T> = HashMap::new();

    for ranked_list in ranked_lists {
        for (rank, (key, item)) in ranked_list.into_iter().enumerate() {
            let score = 1.0 / (k + rank as f64 + 1.0);
            *scores.entry(key.clone()).or_default() += score;
            items.entry(key).or_insert(item);
        }
    }

    let mut merged = scores
        .into_iter()
        .filter_map(|(key, score)| {
            let item = items.remove(&key)?;
            Some(RankedHit { key, item, score })
        })
        .collect::<Vec<_>>();

    merged.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.key.cmp(&b.key))
    });
    merged.truncate(limit);
    merged
}

pub fn build_fts_match_query(query: &str) -> Option<String> {
    let terms = query
        .split_whitespace()
        .map(str::trim)
        .filter(|term| !term.is_empty())
        .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
        .collect::<Vec<_>>();

    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" AND "))
    }
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_RRF_K, build_fts_match_query, reciprocal_rank_fusion};

    #[test]
    fn rrf_merges_ranked_lists_by_shared_keys() {
        let fused = reciprocal_rank_fusion(
            vec![
                vec![("a".to_string(), "vector-a"), ("b".to_string(), "vector-b")],
                vec![("b".to_string(), "fts-b"), ("c".to_string(), "fts-c")],
            ],
            3,
            DEFAULT_RRF_K,
        );

        assert_eq!(
            fused.iter().map(|hit| hit.key.as_str()).collect::<Vec<_>>(),
            vec!["b", "a", "c"]
        );
        assert_eq!(fused[0].item, "vector-b");
    }

    #[test]
    fn fts_query_quotes_terms_and_escapes_quotes() {
        assert_eq!(
            build_fts_match_query(r#"alpha "beta""#).as_deref(),
            Some(r#""alpha" AND """beta""""#)
        );
        assert_eq!(build_fts_match_query("  ").as_deref(), None);
    }
}
