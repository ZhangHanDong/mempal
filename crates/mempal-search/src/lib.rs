#![warn(clippy::all)]

use anyhow::{Context, Result};
use mempal_core::{
    db::Database,
    types::{RouteDecision, SearchResult},
};
use mempal_embed::Embedder;

use crate::filter::build_filter_clause;

pub mod filter;
pub mod route;

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
    let applied_wing = route.wing.as_deref();
    let applied_room = route.room.as_deref();

    let count_sql = format!(
        "SELECT COUNT(*) FROM drawers d {}",
        build_filter_clause("d", 1, 2)
    );
    let candidate_count: i64 = db
        .conn()
        .query_row(&count_sql, (applied_wing, applied_room), |row| row.get(0))
        .context("failed to count candidate drawers")?;
    if candidate_count == 0 {
        return Ok(Vec::new());
    }
    let total_count: i64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM drawers", [], |row| row.get(0))
        .context("failed to count total drawers")?;

    let embeddings = embedder
        .embed(&[query])
        .await
        .context("failed to embed search query")?;
    let query_vector = embeddings
        .into_iter()
        .next()
        .context("embedder returned no query vector")?;
    let query_json =
        serde_json::to_string(&query_vector).context("failed to serialize query vector")?;
    let top_k = i64::try_from(top_k).context("top_k does not fit into i64")?;

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
        .context("failed to prepare search statement")?;
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
                Ok(SearchResult {
                    drawer_id: row.get(0)?,
                    content: row.get(1)?,
                    wing: row.get(2)?,
                    room: row.get(3)?,
                    source_file: row.get(4)?,
                    similarity: (1.0_f64 - distance) as f32,
                    route: route.clone(),
                })
            },
        )
        .context("failed to execute search query")?
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("failed to collect search rows")?;

    Ok(results)
}

fn resolve_route(
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

    let taxonomy = db
        .taxonomy_entries()
        .context("failed to load taxonomy entries")?;
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
