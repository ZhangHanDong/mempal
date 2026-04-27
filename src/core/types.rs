use super::anchor;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    Project,
    Conversation,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    Evidence,
    Knowledge,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryDomain {
    Project,
    Agent,
    Skill,
    Global,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnchorKind {
    Global,
    Repo,
    Worktree,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    Runtime,
    Research,
    Human,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeTier {
    Qi,
    Shu,
    DaoRen,
    DaoTian,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeStatus {
    Candidate,
    Promoted,
    Canonical,
    Demoted,
    Retired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerHints {
    pub intent_tags: Vec<String>,
    pub workflow_bias: Vec<String>,
    pub tool_needs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeEvidenceRole {
    Supporting,
    Verification,
    Counterexample,
    Teaching,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeEventType {
    Created,
    Promoted,
    Demoted,
    Retired,
    Linked,
    Unlinked,
    Updated,
    PublishedAnchor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeCard {
    pub id: String,
    pub statement: String,
    pub content: String,
    pub tier: KnowledgeTier,
    pub status: KnowledgeStatus,
    pub domain: MemoryDomain,
    pub field: String,
    pub anchor_kind: AnchorKind,
    pub anchor_id: String,
    pub parent_anchor_id: Option<String>,
    pub scope_constraints: Option<String>,
    pub trigger_hints: Option<TriggerHints>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KnowledgeCardFilter {
    pub tier: Option<KnowledgeTier>,
    pub status: Option<KnowledgeStatus>,
    pub domain: Option<MemoryDomain>,
    pub field: Option<String>,
    pub anchor_kind: Option<AnchorKind>,
    pub anchor_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeEvidenceLink {
    pub id: String,
    pub card_id: String,
    pub evidence_drawer_id: String,
    pub role: KnowledgeEvidenceRole,
    pub note: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KnowledgeCardEvent {
    pub id: String,
    pub card_id: String,
    pub event_type: KnowledgeEventType,
    pub from_status: Option<KnowledgeStatus>,
    pub to_status: Option<KnowledgeStatus>,
    pub reason: String,
    pub actor: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy)]
pub struct BootstrapIdentityParts<'a> {
    pub memory_kind: &'a MemoryKind,
    pub domain: &'a MemoryDomain,
    pub field: &'a str,
    pub anchor_kind: &'a AnchorKind,
    pub anchor_id: &'a str,
    pub parent_anchor_id: Option<&'a str>,
    pub provenance: Option<&'a Provenance>,
    pub statement: Option<&'a str>,
    pub tier: Option<&'a KnowledgeTier>,
    pub status: Option<&'a KnowledgeStatus>,
    pub supporting_refs: &'a [String],
    pub counterexample_refs: &'a [String],
    pub teaching_refs: &'a [String],
    pub verification_refs: &'a [String],
    pub scope_constraints: Option<&'a str>,
    pub trigger_hints: Option<&'a TriggerHints>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TunnelEndpoint {
    pub wing: String,
    pub room: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExplicitTunnel {
    pub id: String,
    pub left: TunnelEndpoint,
    pub right: TunnelEndpoint,
    pub label: String,
    pub created_at: String,
    pub created_by: Option<String>,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TunnelFollowResult {
    pub endpoint: TunnelEndpoint,
    pub via_tunnel_id: String,
    pub hop: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReindexSource {
    pub source_file: Option<String>,
    pub wing: String,
    pub room: Option<String>,
    pub drawer_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Drawer {
    pub id: String,
    pub content: String,
    pub wing: String,
    pub room: Option<String>,
    pub source_file: Option<String>,
    pub source_type: SourceType,
    pub added_at: String,
    pub chunk_index: Option<i64>,
    #[serde(default = "default_normalize_version")]
    pub normalize_version: u32,
    /// Importance ranking (0-5). Higher = more important for wake-up context.
    #[serde(default)]
    pub importance: i32,
    pub memory_kind: MemoryKind,
    pub domain: MemoryDomain,
    pub field: String,
    pub anchor_kind: AnchorKind,
    pub anchor_id: String,
    pub parent_anchor_id: Option<String>,
    pub provenance: Option<Provenance>,
    pub statement: Option<String>,
    pub tier: Option<KnowledgeTier>,
    pub status: Option<KnowledgeStatus>,
    #[serde(default)]
    pub supporting_refs: Vec<String>,
    #[serde(default)]
    pub counterexample_refs: Vec<String>,
    #[serde(default)]
    pub teaching_refs: Vec<String>,
    #[serde(default)]
    pub verification_refs: Vec<String>,
    pub scope_constraints: Option<String>,
    pub trigger_hints: Option<TriggerHints>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapEvidenceArgs {
    pub id: String,
    pub content: String,
    pub wing: String,
    pub room: Option<String>,
    pub source_file: Option<String>,
    pub source_type: SourceType,
    pub added_at: String,
    pub chunk_index: Option<i64>,
    pub importance: i32,
}

impl Drawer {
    pub fn new_bootstrap_evidence(args: BootstrapEvidenceArgs) -> Self {
        let defaults = anchor::bootstrap_defaults(&args.source_type);
        Self {
            id: args.id,
            content: args.content,
            wing: args.wing,
            room: args.room,
            source_file: args.source_file,
            source_type: args.source_type,
            added_at: args.added_at,
            chunk_index: args.chunk_index,
            normalize_version: default_normalize_version(),
            importance: args.importance,
            memory_kind: MemoryKind::Evidence,
            domain: MemoryDomain::Project,
            field: defaults.field,
            anchor_kind: defaults.anchor_kind,
            anchor_id: defaults.anchor_id,
            parent_anchor_id: defaults.parent_anchor_id,
            provenance: Some(defaults.provenance),
            statement: None,
            tier: None,
            status: None,
            supporting_refs: Vec::new(),
            counterexample_refs: Vec::new(),
            teaching_refs: Vec::new(),
            verification_refs: Vec::new(),
            scope_constraints: None,
            trigger_hints: None,
        }
    }
}

fn default_normalize_version() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Triple {
    pub id: String,
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub confidence: f64,
    pub source_drawer: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaxonomyEntry {
    pub wing: String,
    pub room: String,
    pub display_name: Option<String>,
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TripleStats {
    pub total: i64,
    pub active: i64,
    pub expired: i64,
    pub entities: i64,
    pub top_predicates: Vec<(String, i64)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteDecision {
    pub wing: Option<String>,
    pub room: Option<String>,
    pub confidence: f32,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NeighborChunk {
    pub drawer_id: String,
    pub content: String,
    pub chunk_index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkNeighbors {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prev: Option<NeighborChunk>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<NeighborChunk>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchResult {
    pub drawer_id: String,
    pub content: String,
    pub wing: String,
    pub room: Option<String>,
    pub source_file: String,
    pub memory_kind: MemoryKind,
    pub domain: MemoryDomain,
    pub field: String,
    pub statement: Option<String>,
    pub tier: Option<KnowledgeTier>,
    pub status: Option<KnowledgeStatus>,
    pub anchor_kind: AnchorKind,
    pub anchor_id: String,
    pub parent_anchor_id: Option<String>,
    pub similarity: f32,
    pub route: RouteDecision,
    #[serde(skip)]
    pub chunk_index: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub neighbors: Option<ChunkNeighbors>,
    /// Other wings that share this result's room (tunnel hints).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tunnel_hints: Vec<String>,
}
