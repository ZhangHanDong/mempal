#![warn(clippy::all)]

use mempal_core::{
    db::Database,
    types::{RouteDecision, SearchResult},
    utils::source_file_or_synthetic,
};
use mempal_embed::{EmbedError, Embedder};
use thiserror::Error;

use crate::filter::build_filter_clause;

pub mod filter;
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
    LoadTaxonomy(#[source] mempal_core::db::DbError),
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

    search_by_vector(db, &query_vector, route, top_k)
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
                total_count,
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
