#![warn(clippy::all)]

use crate::core::{
    db::Database,
    types::{
        AnchorKind, KnowledgeStatus, KnowledgeTier, MemoryDomain, MemoryKind, RouteDecision,
        SearchResult,
    },
    utils::source_file_or_synthetic,
};
use crate::embed::{EmbedError, Embedder};
use mempal_search_core::{DEFAULT_RRF_K, build_fts_match_query, reciprocal_rank_fusion};
use thiserror::Error;

use crate::search::filter::build_filter_clause;

pub mod filter;
pub mod rerank;
pub mod route;

pub type Result<T> = std::result::Result<T, SearchError>;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchFilters {
    pub memory_kind: Option<String>,
    pub domain: Option<String>,
    pub field: Option<String>,
    pub tier: Option<String>,
    pub status: Option<String>,
    pub anchor_kind: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchOptions {
    pub filters: SearchFilters,
    pub with_neighbors: bool,
}

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
    #[error("failed to load neighbor chunks")]
    LoadNeighbors(#[source] crate::core::db::DbError),
}

pub async fn search<E: Embedder + ?Sized>(
    db: &Database,
    embedder: &E,
    query: &str,
    wing: Option<&str>,
    room: Option<&str>,
    top_k: usize,
) -> Result<Vec<SearchResult>> {
    search_with_options(
        db,
        embedder,
        query,
        wing,
        room,
        SearchOptions::default(),
        top_k,
    )
    .await
}

pub async fn search_with_filters<E: Embedder + ?Sized>(
    db: &Database,
    embedder: &E,
    query: &str,
    wing: Option<&str>,
    room: Option<&str>,
    filters: &SearchFilters,
    top_k: usize,
) -> Result<Vec<SearchResult>> {
    search_with_options(
        db,
        embedder,
        query,
        wing,
        room,
        SearchOptions {
            filters: filters.clone(),
            with_neighbors: false,
        },
        top_k,
    )
    .await
}

pub async fn search_with_options<E: Embedder + ?Sized>(
    db: &Database,
    embedder: &E,
    query: &str,
    wing: Option<&str>,
    room: Option<&str>,
    options: SearchOptions,
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

    search_with_vector_options(db, query, &query_vector, route, options, top_k)
}

pub fn search_with_vector(
    db: &Database,
    query: &str,
    query_vector: &[f32],
    route: RouteDecision,
    top_k: usize,
) -> Result<Vec<SearchResult>> {
    search_with_vector_options(
        db,
        query,
        query_vector,
        route,
        SearchOptions::default(),
        top_k,
    )
}

pub fn search_with_vector_and_filters(
    db: &Database,
    query: &str,
    query_vector: &[f32],
    route: RouteDecision,
    filters: &SearchFilters,
    top_k: usize,
) -> Result<Vec<SearchResult>> {
    search_with_vector_options(
        db,
        query,
        query_vector,
        route,
        SearchOptions {
            filters: filters.clone(),
            with_neighbors: false,
        },
        top_k,
    )
}

pub fn search_with_vector_options(
    db: &Database,
    query: &str,
    query_vector: &[f32],
    route: RouteDecision,
    options: SearchOptions,
    top_k: usize,
) -> Result<Vec<SearchResult>> {
    if top_k == 0 {
        return Ok(Vec::new());
    }

    // Hybrid search: vector + BM25, merged via RRF
    let vector_results =
        search_by_vector_with_filters(db, query_vector, route.clone(), &options.filters, top_k)?;

    let fts_results = search_fts_with_filters(db, query, &route, &options.filters, top_k)?;

    let mut results = if fts_results.is_empty() {
        vector_results
    } else {
        rrf_merge(vector_results, fts_results, top_k)
    };

    // Inject tunnel hints: for each result, check if its room exists in other wings
    inject_tunnel_hints(db, &mut results);
    if options.with_neighbors && top_k <= 10 {
        inject_chunk_neighbors(db, &mut results)?;
    }

    Ok(results)
}

fn inject_chunk_neighbors(db: &Database, results: &mut [SearchResult]) -> Result<()> {
    for result in results {
        let Some(chunk_index) = result.chunk_index else {
            continue;
        };
        let neighbors = db
            .neighbor_chunks(
                &result.source_file,
                &result.wing,
                result.room.as_deref(),
                chunk_index,
            )
            .map_err(SearchError::LoadNeighbors)?;
        if neighbors.prev.is_some() || neighbors.next.is_some() {
            result.neighbors = Some(neighbors);
        }
    }

    Ok(())
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

        if let Ok(explicit_hints) = db.explicit_tunnel_hints(&result.wing, result.room.as_deref()) {
            result.tunnel_hints.extend(explicit_hints);
        }
        result.tunnel_hints.sort();
        result.tunnel_hints.dedup();
    }
}

fn rrf_merge(
    vector_results: Vec<SearchResult>,
    fts_results: Vec<SearchResult>,
    top_k: usize,
) -> Vec<SearchResult> {
    let vector_hits = vector_results
        .into_iter()
        .map(|result| (result.drawer_id.clone(), result))
        .collect::<Vec<_>>();
    let fts_hits = fts_results
        .into_iter()
        .map(|result| (result.drawer_id.clone(), result))
        .collect::<Vec<_>>();

    reciprocal_rank_fusion(vec![vector_hits, fts_hits], top_k, DEFAULT_RRF_K)
        .into_iter()
        .map(|hit| {
            let mut result = hit.item;
            result.similarity = hit.score as f32;
            result
        })
        .collect()
}

pub fn search_by_vector(
    db: &Database,
    query_vector: &[f32],
    route: RouteDecision,
    top_k: usize,
) -> Result<Vec<SearchResult>> {
    search_by_vector_with_filters(db, query_vector, route, &SearchFilters::default(), top_k)
}

fn search_by_vector_with_filters(
    db: &Database,
    query_vector: &[f32],
    route: RouteDecision,
    filters: &SearchFilters,
    top_k: usize,
) -> Result<Vec<SearchResult>> {
    if top_k == 0 {
        return Ok(Vec::new());
    }

    let applied_wing = route.wing.as_deref();
    let applied_room = route.room.as_deref();
    let memory_kind = filters.memory_kind.as_deref();
    let domain = filters.domain.as_deref();
    let field = filters.field.as_deref();
    let tier = filters.tier.as_deref();
    let status = filters.status.as_deref();
    let anchor_kind = filters.anchor_kind.as_deref();

    let count_sql = format!(
        "SELECT COUNT(*) FROM drawers d {}",
        build_filter_clause("d", 1)
    );
    let candidate_count: i64 = db
        .conn()
        .query_row(
            &count_sql,
            (
                applied_wing,
                applied_room,
                memory_kind,
                domain,
                field,
                tier,
                status,
                anchor_kind,
            ),
            |row| row.get(0),
        )
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
        SELECT d.id, d.content, d.wing, d.room, d.source_file,
               d.memory_kind, d.domain, d.field, d.statement, d.tier, d.status,
               d.anchor_kind, d.anchor_id, d.parent_anchor_id, d.chunk_index, matches.distance
        FROM matches
        JOIN drawers d ON d.id = matches.id
        {}
        ORDER BY matches.distance ASC
        LIMIT ?11
        "#,
        build_filter_clause("d", 3)
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
                memory_kind,
                domain,
                field,
                tier,
                status,
                anchor_kind,
                top_k,
            ),
            |row| {
                let distance: f64 = row.get(15)?;
                map_search_result_row(row, &route, (1.0_f64 - distance) as f32)
            },
        )
        .map_err(SearchError::ExecuteSearch)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(SearchError::CollectSearchRows)?;

    Ok(results)
}

fn search_fts_with_filters(
    db: &Database,
    query: &str,
    route: &RouteDecision,
    filters: &SearchFilters,
    limit: usize,
) -> Result<Vec<SearchResult>> {
    let Some(match_query) = build_fts_match_query(query) else {
        return Ok(Vec::new());
    };
    let limit = i64::try_from(limit).map_err(|_| SearchError::InvalidTopK)?;
    let sql = format!(
        r#"
        SELECT d.id, d.content, d.wing, d.room, d.source_file,
               d.memory_kind, d.domain, d.field, d.statement, d.tier, d.status,
               d.anchor_kind, d.anchor_id, d.parent_anchor_id, d.chunk_index, fts.rank
        FROM drawers_fts fts
        JOIN drawers d ON d.rowid = fts.rowid
        {}
          AND drawers_fts MATCH ?1
        ORDER BY fts.rank
        LIMIT ?10
        "#,
        build_filter_clause("d", 2)
    );
    let mut statement = db
        .conn()
        .prepare(&sql)
        .map_err(SearchError::PrepareSearch)?;
    statement
        .query_map(
            (
                match_query.as_str(),
                route.wing.as_deref(),
                route.room.as_deref(),
                filters.memory_kind.as_deref(),
                filters.domain.as_deref(),
                filters.field.as_deref(),
                filters.tier.as_deref(),
                filters.status.as_deref(),
                filters.anchor_kind.as_deref(),
                limit,
            ),
            |row| map_search_result_row(row, route, 0.0),
        )
        .map_err(SearchError::ExecuteSearch)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(SearchError::CollectSearchRows)
}

fn map_search_result_row(
    row: &rusqlite::Row<'_>,
    route: &RouteDecision,
    similarity: f32,
) -> rusqlite::Result<SearchResult> {
    let drawer_id: String = row.get(0)?;
    let source_file = row.get::<_, Option<String>>(4)?;
    Ok(SearchResult {
        drawer_id: drawer_id.clone(),
        content: row.get(1)?,
        wing: row.get(2)?,
        room: row.get(3)?,
        source_file: source_file_or_synthetic(&drawer_id, source_file.as_deref()),
        memory_kind: memory_kind_from_str(&row.get::<_, String>(5)?)?,
        domain: memory_domain_from_str(&row.get::<_, String>(6)?)?,
        field: row.get(7)?,
        statement: row.get(8)?,
        tier: row
            .get::<_, Option<String>>(9)?
            .map(|value| knowledge_tier_from_str(&value))
            .transpose()?,
        status: row
            .get::<_, Option<String>>(10)?
            .map(|value| knowledge_status_from_str(&value))
            .transpose()?,
        anchor_kind: anchor_kind_from_str(&row.get::<_, String>(11)?)?,
        anchor_id: row.get(12)?,
        parent_anchor_id: row.get(13)?,
        similarity,
        route: route.clone(),
        chunk_index: row.get(14)?,
        neighbors: None,
        tunnel_hints: vec![],
    })
}

fn invalid_enum_value(kind: &'static str, value: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("invalid {kind}: {value}"),
        )),
    )
}

fn memory_kind_from_str(value: &str) -> rusqlite::Result<MemoryKind> {
    match value {
        "evidence" => Ok(MemoryKind::Evidence),
        "knowledge" => Ok(MemoryKind::Knowledge),
        _ => Err(invalid_enum_value("memory_kind", value.to_string())),
    }
}

fn memory_domain_from_str(value: &str) -> rusqlite::Result<MemoryDomain> {
    match value {
        "project" => Ok(MemoryDomain::Project),
        "agent" => Ok(MemoryDomain::Agent),
        "skill" => Ok(MemoryDomain::Skill),
        "global" => Ok(MemoryDomain::Global),
        _ => Err(invalid_enum_value("domain", value.to_string())),
    }
}

fn knowledge_tier_from_str(value: &str) -> rusqlite::Result<KnowledgeTier> {
    match value {
        "qi" => Ok(KnowledgeTier::Qi),
        "shu" => Ok(KnowledgeTier::Shu),
        "dao_ren" => Ok(KnowledgeTier::DaoRen),
        "dao_tian" => Ok(KnowledgeTier::DaoTian),
        _ => Err(invalid_enum_value("tier", value.to_string())),
    }
}

fn knowledge_status_from_str(value: &str) -> rusqlite::Result<KnowledgeStatus> {
    match value {
        "candidate" => Ok(KnowledgeStatus::Candidate),
        "promoted" => Ok(KnowledgeStatus::Promoted),
        "canonical" => Ok(KnowledgeStatus::Canonical),
        "demoted" => Ok(KnowledgeStatus::Demoted),
        "retired" => Ok(KnowledgeStatus::Retired),
        _ => Err(invalid_enum_value("status", value.to_string())),
    }
}

fn anchor_kind_from_str(value: &str) -> rusqlite::Result<AnchorKind> {
    match value {
        "global" => Ok(AnchorKind::Global),
        "repo" => Ok(AnchorKind::Repo),
        "worktree" => Ok(AnchorKind::Worktree),
        _ => Err(invalid_enum_value("anchor_kind", value.to_string())),
    }
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
