use crate::context::{ContextItem, ContextPack, ContextSection};
use crate::core::types::{
    AnchorKind, ChunkNeighbors, KnowledgeStatus, KnowledgeTier, MemoryDomain, MemoryKind,
    NeighborChunk, RouteDecision, SearchResult, TaxonomyEntry, TunnelEndpoint,
};
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

    /// Optional memory kind filter (`evidence` or `knowledge`).
    pub memory_kind: Option<String>,

    /// Optional domain filter (`project`, `agent`, `skill`, `global`).
    pub domain: Option<String>,

    /// Optional bootstrap field filter.
    pub field: Option<String>,

    /// Optional knowledge tier filter.
    pub tier: Option<String>,

    /// Optional knowledge status filter.
    pub status: Option<String>,

    /// Optional anchor kind filter (`global`, `repo`, `worktree`).
    pub anchor_kind: Option<String>,

    /// If true and top_k <= 10, include previous/next chunks from the same source.
    pub with_neighbors: Option<bool>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SearchResponse {
    pub results: Vec<SearchResultDto>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ContextRequest {
    pub query: String,
    pub field: Option<String>,
    pub domain: Option<String>,
    pub cwd: Option<String>,
    pub include_evidence: Option<bool>,
    pub max_items: Option<usize>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ContextResponse {
    pub query: String,
    pub domain: String,
    pub field: String,
    pub anchors: Vec<ContextAnchorDto>,
    pub sections: Vec<ContextSectionDto>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ContextAnchorDto {
    pub anchor_kind: String,
    pub anchor_id: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ContextSectionDto {
    pub name: String,
    pub items: Vec<ContextItemDto>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ContextItemDto {
    pub drawer_id: String,
    pub source_file: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    pub anchor_kind: String,
    pub anchor_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_anchor_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_hints: Option<TriggerHintsDto>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub neighbors: Option<ChunkNeighborsDto>,
    /// 3-4 letter entity codes derived from AAAK analysis.
    pub entities: Vec<String>,
    /// Topic keywords derived from AAAK analysis. May be empty.
    pub topics: Vec<String>,
    /// Classification flags derived from AAAK analysis. Always non-empty.
    pub flags: Vec<String>,
    /// Emotion tags derived from AAAK analysis. Always non-empty.
    pub emotions: Vec<String>,
    /// Importance derived from AAAK flags, normalized to the existing 2-4 scale.
    pub importance_stars: u8,
    pub memory_kind: String,
    pub domain: String,
    pub field: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub statement: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    pub anchor_kind: String,
    pub anchor_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_anchor_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ChunkNeighborsDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prev: Option<NeighborChunkDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<NeighborChunkDto>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct NeighborChunkDto {
    pub drawer_id: String,
    pub content: String,
    pub chunk_index: u32,
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

    /// If true, append this entry to one agent-diary drawer for the current
    /// UTC day. Requires wing="agent-diary" and an explicit room.
    pub diary_rollup: Option<bool>,

    /// Importance ranking (0-5). Higher values appear first in wake-up context.
    /// Default 0. Use 3-5 for key decisions, architecture choices, and lessons learned.
    pub importance: Option<i32>,

    pub memory_kind: Option<String>,
    pub domain: Option<String>,
    pub field: Option<String>,
    pub provenance: Option<String>,
    pub statement: Option<String>,
    pub tier: Option<String>,
    pub status: Option<String>,
    pub supporting_refs: Option<Vec<String>>,
    pub counterexample_refs: Option<Vec<String>>,
    pub teaching_refs: Option<Vec<String>>,
    pub verification_refs: Option<Vec<String>>,
    pub scope_constraints: Option<String>,
    pub trigger_hints: Option<TriggerHintsDto>,
    pub anchor_kind: Option<String>,
    pub anchor_id: Option<String>,
    pub parent_anchor_id: Option<String>,
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct TriggerHintsDto {
    pub intent_tags: Vec<String>,
    pub workflow_bias: Vec<String>,
    pub tool_needs: Vec<String>,
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
    /// Milliseconds spent waiting for the per-source ingest lock (P9-B).
    /// Omitted in dry-run and when lock was not acquired. When > 0, a
    /// concurrent ingest of the same content serialized with this call.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lock_wait_ms: Option<u64>,
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
    pub normalize_version_current: u32,
    pub stale_drawer_count: u64,
    pub drawer_count: i64,
    pub taxonomy_count: i64,
    pub db_size_bytes: u64,
    pub diary_rollup_days: u32,
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

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct TunnelsRequest {
    /// Action: "discover" (default), "list", "add", "delete", or "follow".
    pub action: Option<String>,
    pub left: Option<TunnelEndpointDto>,
    pub right: Option<TunnelEndpointDto>,
    pub from: Option<TunnelEndpointDto>,
    pub label: Option<String>,
    pub tunnel_id: Option<String>,
    pub wing: Option<String>,
    /// Filter for list: "passive", "explicit", or "all" (default).
    pub kind: Option<String>,
    /// Follow depth. Must be 1 or 2. Defaults to 1.
    pub max_hops: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TunnelEndpointDto {
    pub wing: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct TunnelsResponse {
    pub tunnels: Vec<TunnelDto>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct TunnelDto {
    pub tunnel_id: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left: Option<TunnelEndpointDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right: Option<TunnelEndpointDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via_tunnel_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hop: Option<u8>,
}

impl From<TunnelEndpointDto> for TunnelEndpoint {
    fn from(value: TunnelEndpointDto) -> Self {
        Self {
            wing: value.wing,
            room: value.room,
        }
    }
}

impl From<&TunnelEndpoint> for TunnelEndpointDto {
    fn from(value: &TunnelEndpoint) -> Self {
        Self {
            wing: value.wing.clone(),
            room: value.room.clone(),
        }
    }
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

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct CoworkPushRequest {
    /// The message content to deliver. Maximum 8 KB. Short status updates,
    /// decision summaries, or drawer_id pointers. Do NOT push search results
    /// or large reasoning blocks — see Rule 10 in MEMORY_PROTOCOL.
    pub content: String,

    /// Target agent: "claude" or "codex". OMIT to infer partner from MCP
    /// client identity (Claude → Codex, Codex → Claude). Self-push is rejected.
    #[serde(default)]
    pub target_tool: Option<String>,

    /// Absolute filesystem path of the project cwd this push is scoped to.
    /// Internally normalized to git repo root via `project_identity()` so
    /// subdirectory callers land on the same inbox as repo-root callers.
    pub cwd: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CoworkPushResponse {
    pub target_tool: String,
    pub inbox_path: String,
    pub pushed_at: String,
    pub inbox_size_after: u64,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct FactCheckRequest {
    /// Text to check for contradictions against KG triples + known entities.
    pub text: String,
    /// Optional wing filter for known-entity scope. OMIT unless you have
    /// already seen the exact wing name via mempal_status.
    pub wing: Option<String>,
    /// Optional room filter within a wing. OMIT unless explicitly named.
    pub room: Option<String>,
    /// Optional RFC3339 timestamp for the `now` cutoff used by
    /// StaleFact detection. OMIT to use current UTC time.
    pub now: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FactCheckResponse {
    pub issues: Vec<crate::factcheck::FactIssue>,
    pub checked_entities: Vec<String>,
    pub kg_triples_scanned: usize,
}

impl SearchResultDto {
    pub fn with_signals_from_result(value: SearchResult) -> Self {
        let signals = crate::aaak::analyze(&value.content);

        Self {
            drawer_id: value.drawer_id,
            content: value.content,
            wing: value.wing,
            room: value.room,
            source_file: value.source_file,
            similarity: value.similarity,
            route: value.route.into(),
            tunnel_hints: value.tunnel_hints,
            neighbors: value.neighbors.map(ChunkNeighborsDto::from),
            entities: signals.entities,
            topics: signals.topics,
            flags: signals.flags,
            emotions: signals.emotions,
            importance_stars: signals.importance_stars,
            memory_kind: memory_kind_slug(&value.memory_kind).to_string(),
            domain: domain_slug(&value.domain).to_string(),
            field: value.field,
            statement: value.statement,
            tier: value
                .tier
                .as_ref()
                .map(knowledge_tier_slug)
                .map(str::to_string),
            status: value
                .status
                .as_ref()
                .map(knowledge_status_slug)
                .map(str::to_string),
            anchor_kind: anchor_kind_slug(&value.anchor_kind).to_string(),
            anchor_id: value.anchor_id,
            parent_anchor_id: value.parent_anchor_id,
        }
    }
}

impl From<ContextPack> for ContextResponse {
    fn from(value: ContextPack) -> Self {
        Self {
            query: value.query,
            domain: domain_slug(&value.domain).to_string(),
            field: value.field,
            anchors: value
                .anchors
                .into_iter()
                .map(|anchor| ContextAnchorDto {
                    anchor_kind: anchor_kind_slug(&anchor.anchor_kind).to_string(),
                    anchor_id: anchor.anchor_id,
                })
                .collect(),
            sections: value
                .sections
                .into_iter()
                .map(ContextSectionDto::from)
                .collect(),
        }
    }
}

impl From<ContextSection> for ContextSectionDto {
    fn from(value: ContextSection) -> Self {
        Self {
            name: value.name,
            items: value.items.into_iter().map(ContextItemDto::from).collect(),
        }
    }
}

impl From<ContextItem> for ContextItemDto {
    fn from(value: ContextItem) -> Self {
        Self {
            drawer_id: value.drawer_id,
            source_file: value.source_file,
            text: value.text,
            tier: value
                .tier
                .as_ref()
                .map(knowledge_tier_slug)
                .map(str::to_string),
            status: value
                .status
                .as_ref()
                .map(knowledge_status_slug)
                .map(str::to_string),
            anchor_kind: anchor_kind_slug(&value.anchor_kind).to_string(),
            anchor_id: value.anchor_id,
            parent_anchor_id: value.parent_anchor_id,
            trigger_hints: value.trigger_hints.map(TriggerHintsDto::from),
        }
    }
}

impl From<crate::core::types::TriggerHints> for TriggerHintsDto {
    fn from(value: crate::core::types::TriggerHints) -> Self {
        Self {
            intent_tags: value.intent_tags,
            workflow_bias: value.workflow_bias,
            tool_needs: value.tool_needs,
        }
    }
}

impl From<ChunkNeighbors> for ChunkNeighborsDto {
    fn from(value: ChunkNeighbors) -> Self {
        Self {
            prev: value.prev.map(NeighborChunkDto::from),
            next: value.next.map(NeighborChunkDto::from),
        }
    }
}

impl From<NeighborChunk> for NeighborChunkDto {
    fn from(value: NeighborChunk) -> Self {
        Self {
            drawer_id: value.drawer_id,
            content: value.content,
            chunk_index: value.chunk_index,
        }
    }
}

fn memory_kind_slug(value: &MemoryKind) -> &'static str {
    match value {
        MemoryKind::Evidence => "evidence",
        MemoryKind::Knowledge => "knowledge",
    }
}

fn domain_slug(value: &MemoryDomain) -> &'static str {
    match value {
        MemoryDomain::Project => "project",
        MemoryDomain::Agent => "agent",
        MemoryDomain::Skill => "skill",
        MemoryDomain::Global => "global",
    }
}

fn knowledge_tier_slug(value: &KnowledgeTier) -> &'static str {
    match value {
        KnowledgeTier::Qi => "qi",
        KnowledgeTier::Shu => "shu",
        KnowledgeTier::DaoRen => "dao_ren",
        KnowledgeTier::DaoTian => "dao_tian",
    }
}

fn knowledge_status_slug(value: &KnowledgeStatus) -> &'static str {
    match value {
        KnowledgeStatus::Candidate => "candidate",
        KnowledgeStatus::Promoted => "promoted",
        KnowledgeStatus::Canonical => "canonical",
        KnowledgeStatus::Demoted => "demoted",
        KnowledgeStatus::Retired => "retired",
    }
}

fn anchor_kind_slug(value: &AnchorKind) -> &'static str {
    match value {
        AnchorKind::Global => "global",
        AnchorKind::Repo => "repo",
        AnchorKind::Worktree => "worktree",
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

#[cfg(test)]
mod tests {
    use crate::core::types::{
        AnchorKind, KnowledgeStatus, KnowledgeTier, MemoryDomain, MemoryKind, RouteDecision,
        SearchResult,
    };

    use super::SearchResultDto;

    fn sample_result(content: &str) -> SearchResult {
        SearchResult {
            drawer_id: "drawer-1".to_string(),
            content: content.to_string(),
            wing: "mempal".to_string(),
            room: Some("signals".to_string()),
            source_file: "/tmp/signals.md".to_string(),
            memory_kind: MemoryKind::Knowledge,
            domain: MemoryDomain::Project,
            field: "bootstrap".to_string(),
            statement: Some("normalized statement".to_string()),
            tier: Some(KnowledgeTier::Shu),
            status: Some(KnowledgeStatus::Promoted),
            anchor_kind: AnchorKind::Repo,
            anchor_id: "repo://signals".to_string(),
            parent_anchor_id: None,
            similarity: 0.91,
            route: RouteDecision {
                wing: Some("mempal".to_string()),
                room: Some("signals".to_string()),
                confidence: 0.88,
                reason: "unit test".to_string(),
            },
            chunk_index: Some(0),
            neighbors: None,
            tunnel_hints: vec!["docs".to_string()],
        }
    }

    #[test]
    fn test_with_signals_preserves_raw_content_and_citations() {
        let original = "We decided to use Arc<Mutex<>> for state because shared ownership mattered";
        let dto = SearchResultDto::with_signals_from_result(sample_result(original));

        assert_eq!(dto.content, original);
        assert!(!dto.content.starts_with("V1|"));
        assert!(!dto.content.contains('★'));
        assert_eq!(dto.drawer_id, "drawer-1");
        assert_eq!(dto.source_file, "/tmp/signals.md");
        assert_eq!(dto.tunnel_hints, vec!["docs".to_string()]);
        assert_eq!(dto.memory_kind, "knowledge");
        assert_eq!(dto.tier.as_deref(), Some("shu"));
        assert!(dto.flags.contains(&"DECISION".to_string()));
        assert!(dto.importance_stars >= 2);
        assert!(!dto.entities.is_empty());
    }

    #[test]
    fn test_with_signals_applies_empty_content_sentinels() {
        let dto = SearchResultDto::with_signals_from_result(sample_result(""));

        assert_eq!(dto.entities, vec!["UNK".to_string()]);
        assert_eq!(dto.flags, vec!["CORE".to_string()]);
        assert_eq!(dto.emotions, vec!["determ".to_string()]);
        assert!(dto.topics.is_empty());
        assert_eq!(dto.importance_stars, 2);
    }
}
