use mempal_core::types::{RouteDecision, SearchResult, TaxonomyEntry};
use rmcp::schemars::{self, JsonSchema};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SearchRequest {
    /// Natural-language query. Use the user's actual question verbatim
    /// when possible — the embedding model handles paraphrase and translation.
    pub query: String,

    /// Optional wing filter. OMIT (leave null) unless you already know the
    /// EXACT wing name from a prior mempal_status call or the user named it
    /// explicitly. Wing filtering is a strict equality match, so guessing a
    /// wing name (e.g. "engineering", "backend") will silently return zero
    /// results. When in doubt, leave this field unset for a global search
    /// across all wings.
    pub wing: Option<String>,

    /// Optional room filter within a wing. Same rule as wing: OMIT unless you
    /// have seen the exact room name in a prior mempal_status call. Guessing
    /// returns zero results.
    pub room: Option<String>,

    /// Maximum number of results to return. Defaults to 10 when omitted.
    pub top_k: Option<usize>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SearchResponse {
    pub results: Vec<SearchResultDto>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SearchResultDto {
    pub drawer_id: String,
    pub content: String,
    pub wing: String,
    pub room: Option<String>,
    pub source_file: String,
    pub similarity: f32,
    pub route: RouteDecisionDto,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct RouteDecisionDto {
    pub wing: Option<String>,
    pub room: Option<String>,
    pub confidence: f32,
    pub reason: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct IngestRequest {
    pub content: String,
    pub wing: String,
    pub room: Option<String>,
    pub source: Option<String>,

    /// If true, return the drawer_id that WOULD be created without actually
    /// writing to the database. Use this to preview before committing.
    pub dry_run: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DeleteRequest {
    /// The drawer_id to soft-delete. The drawer is marked with a deleted_at
    /// timestamp but not physically removed. Use `mempal purge` CLI to
    /// permanently remove soft-deleted drawers.
    pub drawer_id: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct DeleteResponse {
    pub drawer_id: String,
    pub deleted: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct IngestResponse {
    pub drawer_id: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct StatusResponse {
    pub schema_version: u32,
    pub drawer_count: i64,
    pub taxonomy_count: i64,
    pub db_size_bytes: u64,
    pub scopes: Vec<ScopeCount>,
    pub aaak_spec: String,
    pub memory_protocol: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ScopeCount {
    pub wing: String,
    pub room: Option<String>,
    pub drawer_count: i64,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct TaxonomyRequest {
    pub action: String,
    pub wing: Option<String>,
    pub room: Option<String>,
    pub keywords: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct TaxonomyResponse {
    pub action: String,
    pub entries: Vec<TaxonomyEntryDto>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct TaxonomyEntryDto {
    pub wing: String,
    pub room: String,
    pub display_name: Option<String>,
    pub keywords: Vec<String>,
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
