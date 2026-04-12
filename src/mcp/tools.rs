use crate::core::types::{RouteDecision, SearchResult, TaxonomyEntry};
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
    /// Other wings sharing this room (tunnel cross-references).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tunnel_hints: Vec<String>,
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

    /// Importance ranking (0-5). Higher values appear first in wake-up context.
    /// Default 0. Use 3-5 for key decisions, architecture choices, and lessons learned.
    pub importance: Option<i32>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duplicate_warning: Option<DuplicateWarning>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct DuplicateWarning {
    pub similar_drawer_id: String,
    pub similarity: f32,
    pub preview: String,
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

// --- Knowledge Graph ---

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct KgRequest {
    /// Action: "add", "query", or "invalidate".
    pub action: String,
    pub subject: Option<String>,
    pub predicate: Option<String>,
    pub object: Option<String>,
    /// Triple ID (required for invalidate).
    pub triple_id: Option<String>,
    /// Only return currently-valid triples (default true).
    pub active_only: Option<bool>,
    /// Link to the source drawer that evidences this triple.
    pub source_drawer: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct KgResponse {
    pub action: String,
    pub triples: Vec<TripleDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<KgStatsDto>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct KgStatsDto {
    pub total: i64,
    pub active: i64,
    pub expired: i64,
    pub entities: i64,
    pub top_predicates: Vec<(String, i64)>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct TripleDto {
    pub id: String,
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub confidence: f64,
    pub source_drawer: Option<String>,
}

// --- Tunnels ---

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct TunnelsResponse {
    pub tunnels: Vec<TunnelDto>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct TunnelDto {
    pub room: String,
    pub wings: Vec<String>,
}

// --- Cowork peek ---

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct PeekPartnerRequest {
    /// Which agent tool's session to read. "auto" uses MCP ClientInfo.name
    /// to infer the partner (Claude ↔ Codex); "claude" or "codex" bypasses
    /// inference. If you explicitly name your own tool the call is rejected
    /// to prevent self-peek.
    pub tool: String,

    /// Maximum number of user+assistant messages to return. Default 30.
    pub limit: Option<usize>,

    /// Optional RFC3339 timestamp cutoff — only messages strictly newer than
    /// this are returned.
    pub since: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct PeekPartnerResponse {
    pub partner_tool: String,
    pub session_path: Option<String>,
    pub session_mtime: Option<String>,
    pub partner_active: bool,
    pub messages: Vec<PeekMessageDto>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct PeekMessageDto {
    pub role: String,
    pub at: String,
    pub text: String,
}

impl From<crate::cowork::PeekMessage> for PeekMessageDto {
    fn from(m: crate::cowork::PeekMessage) -> Self {
        Self {
            role: m.role,
            at: m.at,
            text: m.text,
        }
    }
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
            tunnel_hints: value.tunnel_hints,
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
