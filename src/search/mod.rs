#![warn(clippy::all)]

use crate::core::{
    db::Database,
    types::{RouteDecision, SearchResult},
    utils::source_file_or_synthetic,
};
use crate::embed::{EmbedError, Embedder};
use thiserror::Error;

use crate::search::filter::build_filter_clause;

pub mod filter;
pub mod rerank;
pub mod route;

pub type Result<T> = std::result::Result<T, SearchError>;

#[derive(Debug, Error)]
pub enum SearchError {
    #[error("failed to embed search query")]
    EmbedQuery(#[source] EmbedError),
    #[error("embedder returned no query vector")]
    MissingQueryVector,
    #[error("failed to count candidate drawers")]
    CountCandidateDrawers(#[source] rusqlite::Error),
    #[error("failed to count total drawers")]
    CountTotalDrawers(#[source] rusqlite::Error),
    #[error("failed to serialize query vector")]
    SerializeQueryVector(#[source] serde_json::Error),
    #[error("top_k does not fit into i64")]
    InvalidTopK,
    #[error("failed to prepare search statement")]
    PrepareSearch(#[source] rusqlite::Error),
    #[error("failed to execute search query")]
    ExecuteSearch(#[source] rusqlite::Error),
    #[error("failed to collect search rows")]
    CollectSearchRows(#[source] rusqlite::Error),
    #[error("failed to load taxonomy entries")]
    LoadTaxonomy(#[source] crate::core::db::DbError),
    #[error("failed to run keyword search")]
    KeywordSearch(#[source] crate::core::db::DbError),
}

pub async fn search<E: Embedder + ?Sized>(
    db: &Database,
    embedder: &E,
    query: &str,
    wing: Option<&str>,
    room: Option<&str>,
    top_k: usize,
) -> Result<Vec<SearchResult>> {
    if top_k == 0 {
        return Ok(Vec::new());
    }

    let route = resolve_route(db, query, wing, room)?;

    let embeddings = embedder
        .embed(&[query])
        .await
        .map_err(SearchError::EmbedQuery)?;
    let query_vector = embeddings
        .into_iter()
        .next()
        .ok_or(SearchError::MissingQueryVector)?;

    search_with_vector(db, query, &query_vector, route, top_k)
}

pub fn search_with_vector(
    db: &Database,
    query: &str,
    query_vector: &[f32],
    route: RouteDecision,
    top_k: usize,
) -> Result<Vec<SearchResult>> {
    if top_k == 0 {
        return Ok(Vec::new());
    }

    // Hybrid search: vector + BM25, merged via RRF
    let vector_results = search_by_vector(db, query_vector, route.clone(), top_k)?;

    let fts_ids = db
        .search_fts(query, route.wing.as_deref(), route.room.as_deref(), top_k)
        .map_err(SearchError::KeywordSearch)?;

    let mut results = if fts_ids.is_empty() {
        vector_results
    } else {
        rrf_merge(vector_results, &fts_ids, &route, db, top_k)
    };

    // Inject tunnel hints: for each result, check if its room exists in other wings
    inject_tunnel_hints(db, &mut results);

    Ok(results)
}

/// For each search result, check if its room appears in other wings (tunnel).
/// If so, add the other wing names as tunnel_hints.
fn inject_tunnel_hints(db: &Database, results: &mut [SearchResult]) {
    let tunnels = match db.find_tunnels() {
        Ok(t) => t,
        Err(_) => return,
    };
    if tunnels.is_empty() {
        return;
    }

    // Build room → other-wings map
    let tunnel_map: std::collections::HashMap<&str, &[String]> = tunnels
        .iter()
        .map(|(room, wings)| (room.as_str(), wings.as_slice()))
        .collect();

    for result in results.iter_mut() {
        if let Some(room) = result.room.as_deref() {
            if let Some(wings) = tunnel_map.get(room) {
                result.tunnel_hints = wings
                    .iter()
                    .filter(|w| *w != &result.wing)
                    .cloned()
                    .collect();
            }
        }
    }
}

/// Reciprocal Rank Fusion: merge vector and BM25 ranked lists.
/// RRF score = sum(1 / (k + rank)) across both lists, with k=60.
fn rrf_merge(
    vector_results: Vec<SearchResult>,
    fts_ids: &[(String, f64)],
    route: &RouteDecision,
    db: &Database,
    top_k: usize,
) -> Vec<SearchResult> {
    use std::collections::HashMap;

    const RRF_K: f64 = 60.0;

    let mut scores: HashMap<String, f64> = HashMap::new();
    let mut result_map: HashMap<String, SearchResult> = HashMap::new();

    // Score vector results
    for (rank, result) in vector_results.into_iter().enumerate() {
        let score = 1.0 / (RRF_K + rank as f64 + 1.0);
        scores.insert(result.drawer_id.clone(), score);
        result_map.insert(result.drawer_id.clone(), result);
    }

    // Score FTS results and merge
    for (rank, (id, _bm25_score)) in fts_ids.iter().enumerate() {
        let score = 1.0 / (RRF_K + rank as f64 + 1.0);
        *scores.entry(id.clone()).or_default() += score;

        // If this ID wasn't in vector results, load the drawer
        if !result_map.contains_key(id) {
            if let Ok(Some(drawer)) = db.get_drawer(id) {
                result_map.insert(
                    id.clone(),
                    SearchResult {
                        drawer_id: drawer.id,
                        content: drawer.content,
                        wing: drawer.wing,
                        room: drawer.room,
                        source_file: source_file_or_synthetic(id, drawer.source_file.as_deref()),
                        similarity: 0.0, // will be overwritten below
                        route: route.clone(),
                        tunnel_hints: vec![],
                    },
                );
            }
        }
    }

    // Sort by RRF score descending, fill in similarity field
    let mut merged: Vec<SearchResult> = scores
        .into_iter()
        .filter_map(|(id, rrf_score)| {
            let mut result = result_map.remove(&id)?;
            result.similarity = rrf_score as f32;
            Some(result)
        })
        .collect();
    merged.sort_by(|a, b| {
        b.similarity
            .partial_cmp(&a.similarity)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    merged.truncate(top_k);
    merged
}

pub fn search_by_vector(
    db: &Database,
    query_vector: &[f32],
    route: RouteDecision,
    top_k: usize,
) -> Result<Vec<SearchResult>> {
    if top_k == 0 {
        return Ok(Vec::new());
    }

    let applied_wing = route.wing.as_deref();
    let applied_room = route.room.as_deref();

    let count_sql = format!(
        "SELECT COUNT(*) FROM drawers d {}",
        build_filter_clause("d", 1, 2)
    );
    let candidate_count: i64 = db
        .conn()
        .query_row(&count_sql, (applied_wing, applied_room), |row| row.get(0))
        .map_err(SearchError::CountCandidateDrawers)?;
    if candidate_count == 0 {
        return Ok(Vec::new());
    }
    let total_count: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM drawers WHERE deleted_at IS NULL",
            [],
            |row| row.get(0),
        )
        .map_err(SearchError::CountTotalDrawers)?;

    // sqlite-vec vec0 MATCH ... k = ? has a hard upper bound of 4096.
    // Clamp to stay under the limit; BM25 side of the hybrid rerank still
    // covers candidates that fall outside the KNN top-4096 by vector distance.
    const SQLITE_VEC_KNN_MAX: i64 = 4096;
    let knn_k: i64 = total_count.min(SQLITE_VEC_KNN_MAX);

    let query_json =
        serde_json::to_string(query_vector).map_err(SearchError::SerializeQueryVector)?;
    let top_k = i64::try_from(top_k).map_err(|_| SearchError::InvalidTopK)?;

    let search_sql = format!(
        r#"
        WITH matches AS (
            SELECT id, distance
            FROM drawer_vectors
            WHERE embedding MATCH vec_f32(?1)
              AND k = ?2
        )
        SELECT d.id, d.content, d.wing, d.room, d.source_file, matches.distance
        FROM matches
        JOIN drawers d ON d.id = matches.id
        {}
        ORDER BY matches.distance ASC
        LIMIT ?5
        "#,
        build_filter_clause("d", 3, 4)
    );

    let mut statement = db
        .conn()
        .prepare(&search_sql)
        .map_err(SearchError::PrepareSearch)?;
    let results = statement
        .query_map(
            (
                query_json.as_str(),
                knn_k,
                applied_wing,
                applied_room,
                top_k,
            ),
            |row| {
                let distance: f64 = row.get(5)?;
                let drawer_id: String = row.get(0)?;
                let source_file = row.get::<_, Option<String>>(4)?;
                Ok(SearchResult {
                    drawer_id: drawer_id.clone(),
                    content: row.get(1)?,
                    wing: row.get(2)?,
                    room: row.get(3)?,
                    source_file: source_file_or_synthetic(&drawer_id, source_file.as_deref()),
                    similarity: (1.0_f64 - distance) as f32,
                    route: route.clone(),
                    tunnel_hints: vec![],
                })
            },
        )
        .map_err(SearchError::ExecuteSearch)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(SearchError::CollectSearchRows)?;

    Ok(results)
}

pub fn resolve_route(
    db: &Database,
    query: &str,
    wing: Option<&str>,
    room: Option<&str>,
) -> Result<RouteDecision> {
    if wing.is_some() || room.is_some() {
        let scope = match (wing, room) {
            (Some(wing), Some(room)) => format!("{wing}/{room}"),
            (Some(wing), None) => wing.to_string(),
            (None, Some(room)) => format!("room={room}"),
            (None, None) => "global".to_string(),
        };
        return Ok(RouteDecision {
            wing: wing.map(ToOwned::to_owned),
            room: room.map(ToOwned::to_owned),
            confidence: 1.0,
            reason: format!("explicit filters provided: {scope}"),
        });
    }

    let taxonomy = db.taxonomy_entries().map_err(SearchError::LoadTaxonomy)?;
    let route = route::route_query(query, &taxonomy);
    if route.confidence >= 0.5 {
        return Ok(route);
    }

    Ok(RouteDecision {
        wing: None,
        room: None,
        confidence: route.confidence,
        reason: route.reason,
    })
}
