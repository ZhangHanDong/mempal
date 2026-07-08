//! Reranker trait for optional cross-encoder reranking of search results.
//!
//! Default: `NoopReranker` — passes results through unchanged.
//! Future: ONNX cross-encoder reranker, API-based reranker.

use crate::core::types::SearchResult;

/// Reranks a list of search results given the original query.
/// Implementations should reorder (and optionally re-score) the results
/// based on cross-encoder or other fine-grained relevance signals.
pub trait Reranker: Send + Sync {
    fn rerank(&self, query: &str, results: Vec<SearchResult>) -> Vec<SearchResult>;
}

/// Default reranker that does nothing — preserves the RRF-merged order.
pub struct NoopReranker;

impl Reranker for NoopReranker {
    fn rerank(&self, _query: &str, results: Vec<SearchResult>) -> Vec<SearchResult> {
        results
    }
}
