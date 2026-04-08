use axum::{
    Json, Router,
    extract::{Query, State},
    http::{
        HeaderValue, Method, StatusCode,
        header::CONTENT_TYPE,
    },
    response::{IntoResponse, Response},
    routing::{get, post},
};
use mempal_core::{
    db::Database,
    types::{Drawer, RouteDecision, SearchResult, SourceType, TaxonomyEntry},
};
use mempal_search::{filter::build_filter_clause, route::route_query};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::state::ApiState;

pub const DEFAULT_REST_ADDR: &str = "127.0.0.1:3080";

pub async fn serve(listener: tokio::net::TcpListener, state: ApiState) -> std::io::Result<()> {
    axum::serve(listener, router(state)).await
}

pub fn router(state: ApiState) -> Router {
    Router::new()
        .route("/api/search", get(search_handler))
        .route("/api/ingest", post(ingest_handler))
        .route("/api/taxonomy", get(taxonomy_handler))
        .route("/api/status", get(status_handler))
        .with_state(state)
        .layer(cors_layer())
}

fn cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(|origin: &HeaderValue, _| {
            is_local_origin(origin)
        }))
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([CONTENT_TYPE])
}

fn is_local_origin(origin: &HeaderValue) -> bool {
    origin
        .to_str()
        .map(|value| {
            value.starts_with("http://localhost")
                || value.starts_with("https://localhost")
                || value.starts_with("http://127.0.0.1")
                || value.starts_with("https://127.0.0.1")
        })
        .unwrap_or(false)
}

#[derive(Debug, Deserialize)]
struct SearchQuery {
    q: String,
    wing: Option<String>,
    room: Option<String>,
    top_k: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct IngestRequest {
    content: String,
    wing: String,
    room: Option<String>,
    source: Option<String>,
}

#[derive(Debug, Serialize)]
struct IngestResponse {
    drawer_id: String,
}

#[derive(Debug, Serialize)]
struct StatusResponse {
    drawer_count: i64,
    taxonomy_count: i64,
    db_size_bytes: u64,
    wings: Vec<ScopeCount>,
}

#[derive(Debug, Serialize)]
struct ScopeCount {
    wing: String,
    room: Option<String>,
    drawer_count: i64,
}

#[derive(Debug, Serialize)]
struct SearchResultDto {
    drawer_id: String,
    content: String,
    wing: String,
    room: Option<String>,
    source_file: Option<String>,
    similarity: f32,
    route: RouteDecisionDto,
}

#[derive(Debug, Serialize)]
struct RouteDecisionDto {
    wing: Option<String>,
    room: Option<String>,
    confidence: f32,
    reason: String,
}

#[derive(Debug, Serialize)]
struct TaxonomyEntryDto {
    wing: String,
    room: String,
    display_name: Option<String>,
    keywords: Vec<String>,
}

async fn search_handler(
    State(state): State<ApiState>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResultDto>>, ApiError> {
    let embedder = state.embedder_factory.build().await.map_err(internal_error)?;
    let query_vector = embedder
        .embed(&[query.q.as_str()])
        .await
        .map_err(internal_error)?
        .into_iter()
        .next()
        .ok_or_else(|| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "embedder returned no vector"))?;
    let db = Database::open(&state.db_path).map_err(internal_error)?;
    let route = resolve_route(&db, &query.q, query.wing.as_deref(), query.room.as_deref())?;
    let results = run_search(&db, &query_vector, route, query.top_k.unwrap_or(10))?;

    Ok(Json(results.into_iter().map(SearchResultDto::from).collect()))
}

async fn ingest_handler(
    State(state): State<ApiState>,
    Json(request): Json<IngestRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let embedder = state.embedder_factory.build().await.map_err(internal_error)?;
    let vector = embedder
        .embed(&[request.content.as_str()])
        .await
        .map_err(internal_error)?
        .into_iter()
        .next()
        .ok_or_else(|| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "embedder returned no vector"))?;
    let db = Database::open(&state.db_path).map_err(internal_error)?;
    let drawer_id = build_drawer_id(&request.wing, request.room.as_deref(), &request.content);

    if !drawer_exists(&db, &drawer_id)? {
        db.insert_drawer(&Drawer {
            id: drawer_id.clone(),
            content: request.content,
            wing: request.wing,
            room: request.room,
            source_file: request.source,
            source_type: SourceType::Manual,
            added_at: current_timestamp(),
            chunk_index: Some(0),
        })
        .map_err(internal_error)?;
        insert_vector(&db, &drawer_id, &vector)?;
    }

    Ok((StatusCode::CREATED, Json(IngestResponse { drawer_id })))
}

async fn taxonomy_handler(
    State(state): State<ApiState>,
) -> Result<Json<Vec<TaxonomyEntryDto>>, ApiError> {
    let db = Database::open(&state.db_path).map_err(internal_error)?;
    let entries = db
        .taxonomy_entries()
        .map_err(internal_error)?
        .into_iter()
        .map(TaxonomyEntryDto::from)
        .collect();
    Ok(Json(entries))
}

async fn status_handler(State(state): State<ApiState>) -> Result<Json<StatusResponse>, ApiError> {
    let db = Database::open(&state.db_path).map_err(internal_error)?;
    let drawer_count = query_count(&db, "drawers")?;
    let taxonomy_count = query_count(&db, "taxonomy")?;
    let db_size_bytes = db.database_size_bytes().map_err(internal_error)?;
    let wings = scope_counts(&db)?;

    Ok(Json(StatusResponse {
        drawer_count,
        taxonomy_count,
        db_size_bytes,
        wings,
    }))
}

fn run_search(
    db: &Database,
    query_vector: &[f32],
    route: RouteDecision,
    top_k: usize,
) -> Result<Vec<SearchResult>, ApiError> {
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
        .map_err(internal_error)?;
    if candidate_count == 0 {
        return Ok(Vec::new());
    }

    let total_count: i64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM drawers", [], |row| row.get(0))
        .map_err(internal_error)?;
    let query_json = serde_json::to_string(query_vector).map_err(internal_error)?;
    let top_k = i64::try_from(top_k).map_err(internal_error)?;

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

    let mut statement = db.conn().prepare(&search_sql).map_err(internal_error)?;
    statement
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
        .map_err(internal_error)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(internal_error)
}

fn resolve_route(
    db: &Database,
    query: &str,
    wing: Option<&str>,
    room: Option<&str>,
) -> Result<RouteDecision, ApiError> {
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

    let taxonomy = db.taxonomy_entries().map_err(internal_error)?;
    let route = route_query(query, &taxonomy);
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

fn query_count(db: &Database, table: &str) -> Result<i64, ApiError> {
    db.conn()
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0))
        .map_err(internal_error)
}

fn scope_counts(db: &Database) -> Result<Vec<ScopeCount>, ApiError> {
    let mut statement = db
        .conn()
        .prepare(
            r#"
            SELECT wing, room, COUNT(*)
            FROM drawers
            GROUP BY wing, room
            ORDER BY wing, room
            "#,
        )
        .map_err(internal_error)?;
    statement
        .query_map([], |row| {
            Ok(ScopeCount {
                wing: row.get(0)?,
                room: row.get(1)?,
                drawer_count: row.get(2)?,
            })
        })
        .map_err(internal_error)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(internal_error)
}

fn drawer_exists(db: &Database, drawer_id: &str) -> Result<bool, ApiError> {
    let exists: i64 = db
        .conn()
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM drawers WHERE id = ?1)",
            [drawer_id],
            |row| row.get(0),
        )
        .map_err(internal_error)?;
    Ok(exists == 1)
}

fn insert_vector(db: &Database, drawer_id: &str, vector: &[f32]) -> Result<(), ApiError> {
    let vector_json = serde_json::to_string(vector).map_err(internal_error)?;
    db.conn()
        .execute(
            "INSERT INTO drawer_vectors (id, embedding) VALUES (?1, vec_f32(?2))",
            (drawer_id, vector_json.as_str()),
        )
        .map_err(internal_error)?;
    Ok(())
}

fn build_drawer_id(wing: &str, room: Option<&str>, content: &str) -> String {
    let room = room.unwrap_or("default");
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

fn current_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs().to_string(),
        Err(_) => "0".to_string(),
    }
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({
                "error": self.message,
            })),
        )
            .into_response()
    }
}

fn internal_error(error: impl std::fmt::Display) -> ApiError {
    ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}

impl From<SearchResult> for SearchResultDto {
    fn from(value: SearchResult) -> Self {
        Self {
            drawer_id: value.drawer_id,
            content: value.content,
            wing: value.wing,
            room: value.room,
            source_file: value.source_file,
            similarity: value.similarity,
            route: value.route.into(),
        }
    }
}

impl From<RouteDecision> for RouteDecisionDto {
    fn from(value: RouteDecision) -> Self {
        Self {
            wing: value.wing,
            room: value.room,
            confidence: value.confidence,
            reason: value.reason,
        }
    }
}

impl From<TaxonomyEntry> for TaxonomyEntryDto {
    fn from(value: TaxonomyEntry) -> Self {
        Self {
            wing: value.wing,
            room: value.room,
            display_name: value.display_name,
            keywords: value.keywords,
        }
    }
}
