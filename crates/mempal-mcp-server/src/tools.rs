use crate::adoption_analytics::{RuntimeAdoptionAnalyticsGroup, RuntimeAdoptionAnalyticsReport};
use crate::brief::{
    BriefCard, BriefCitation, BriefEvidence, BriefEvidenceCitation, BriefFact, BriefSummary,
    BriefUncertainty, BriefUnresolvedItem, CognitiveBrief,
};
use crate::context::{ContextItem, ContextPack, ContextSection, DistillSuggestion};
use crate::core::phase3::{
    Phase3ReadinessReport, ResearchCandidateInsightPlan, ResearchEvidenceDrawerPlan,
    ResearchIngestPlanReport, RuntimeAdoptionCheckedRecordReport, RuntimeAdoptionGuidance,
    RuntimeAdoptionInstrumentationMode, RuntimeAdoptionInstrumentationPolicy,
    RuntimeAdoptionRecordPlan, RuntimeAdoptionRecordQualityReport, RuntimeAdoptionReviewFilters,
    RuntimeAdoptionReviewReport, RuntimeAdoptionSignalCounts, RuntimeAdoptionSignalGuidance,
    RuntimeAdoptionTrackGuidance,
};
use crate::core::types::{
    AnchorKind, ChunkNeighbors, KnowledgeCard, KnowledgeCardEvent, KnowledgeStatus, KnowledgeTier,
    MemoryDomain, MemoryKind, NeighborChunk, RouteDecision, RuntimeAdoptionEvent,
    RuntimeAdoptionSignal, RuntimeAdoptionTrack, SearchResult, TaxonomyEntry, TunnelEndpoint,
};
use crate::doctor::{DoctorDbReport, DoctorInstallReport, DoctorReport};
use crate::field_taxonomy::FieldTaxonomyEntry;
use crate::knowledge_anchor::PublishAnchorOutcome;
use crate::knowledge_card_lifecycle::{
    DemoteCardOutcome, KnowledgeCardGateReport, PromoteCardOutcome,
};
use crate::knowledge_card_retrieval::{RetrievedEvidenceCitation, RetrievedKnowledgeCard};
use crate::knowledge_distill::DistillOutcome;
use crate::knowledge_gate::{GateReport, PromotionPolicyEntry};
use crate::knowledge_lifecycle::{DemoteOutcome, PromoteOutcome};
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
    #[schemars(with = "Option<i64>")]
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
    pub include_cards: Option<bool>,
    #[schemars(with = "Option<i64>")]
    pub max_items: Option<usize>,
    /// Maximum number of `dao_tian` items to include. Defaults to 1; 0 disables
    /// the `dao_tian` section while preserving lower-tier context.
    #[schemars(with = "Option<i64>")]
    pub dao_tian_limit: Option<usize>,
    /// P106: include the read-only `distill_suggestions` signal. Defaults to
    /// true; set false to omit it. Never changes the assembled sections.
    pub include_distill_suggestions: Option<bool>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ContextResponse {
    pub query: String,
    pub domain: String,
    pub field: String,
    pub anchors: Vec<ContextAnchorDto>,
    pub sections: Vec<ContextSectionDto>,
    /// P106: read-only signal flagging fields with dense evidence but no
    /// promoted knowledge yet. Empty when disabled or nothing qualifies.
    pub distill_suggestions: Vec<DistillSuggestionDto>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct DistillSuggestionDto {
    pub field: String,
    #[schemars(with = "i64")]
    pub evidence_count: usize,
    pub sample_evidence_drawer_ids: Vec<String>,
    pub suggested_tier: String,
}

impl From<DistillSuggestion> for DistillSuggestionDto {
    fn from(value: DistillSuggestion) -> Self {
        Self {
            field: value.field,
            evidence_count: value.evidence_count,
            sample_evidence_drawer_ids: value.sample_evidence_drawer_ids,
            suggested_tier: value.suggested_tier,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default, JsonSchema)]
pub struct DoctorRequest {}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct DoctorResponse {
    pub current_version: String,
    #[schemars(with = "i64")]
    pub supported_schema_version: u32,
    pub db: DoctorDbDto,
    pub install: DoctorInstallDto,
    pub warnings: Vec<String>,
    pub recommendations: Vec<String>,
    pub mcp: DoctorMcpDto,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct DoctorDbDto {
    pub path: String,
    pub exists: bool,
    #[schemars(with = "Option<i64>")]
    pub schema_version: Option<u32>,
    pub compatible: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct DoctorInstallDto {
    pub current_exe: Option<String>,
    pub path_mempal: Option<String>,
    pub path_matches_current_exe: Option<bool>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct DoctorMcpDto {
    pub required_tools: Vec<DoctorToolDto>,
    pub phase3_actions: Vec<String>,
    pub cowork_bus_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct DoctorToolDto {
    pub name: String,
    pub advertised: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct BriefMcpRequest {
    pub query: String,
    pub field: Option<String>,
    pub domain: Option<String>,
    pub cwd: Option<String>,
    #[schemars(with = "Option<i64>")]
    pub max_items: Option<usize>,
    #[schemars(with = "Option<i64>")]
    pub dao_tian_limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct BriefMcpResponse {
    pub query: String,
    pub domain: String,
    pub field: String,
    pub summary: BriefSummaryDto,
    pub key_facts: Vec<BriefFactDto>,
    pub evidence: Vec<BriefEvidenceDto>,
    pub cards: Vec<BriefCardDto>,
    pub entities: Vec<String>,
    pub unresolved_items: Vec<BriefUnresolvedItemDto>,
    pub uncertainty: Vec<BriefUncertaintyDto>,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct BriefSummaryDto {
    pub narrative: String,
    #[schemars(with = "i64")]
    pub key_fact_count: usize,
    #[schemars(with = "i64")]
    pub evidence_count: usize,
    #[schemars(with = "i64")]
    pub card_count: usize,
    #[schemars(with = "i64")]
    pub unresolved_count: usize,
    #[schemars(with = "i64")]
    pub uncertainty_count: usize,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct BriefFactDto {
    pub text: String,
    pub section: String,
    pub citation: BriefCitationDto,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct BriefEvidenceDto {
    pub text: String,
    pub citation: BriefCitationDto,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct BriefCardDto {
    pub card_id: String,
    pub text: String,
    pub citation: BriefCitationDto,
    pub evidence_citations: Vec<BriefEvidenceCitationDto>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct BriefUnresolvedItemDto {
    pub text: String,
    pub citation: BriefCitationDto,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct BriefUncertaintyDto {
    pub kind: String,
    pub message: String,
    pub citations: Vec<BriefCitationDto>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct BriefCitationDto {
    pub drawer_id: String,
    pub source_file: String,
    pub anchor_kind: String,
    pub anchor_id: String,
    pub card_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct BriefEvidenceCitationDto {
    pub evidence_drawer_id: String,
    pub role: String,
    pub source_file: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct KnowledgeGateRequest {
    pub drawer_id: String,
    pub target_status: Option<String>,
    pub reviewer: Option<String>,
    pub allow_counterexamples: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct KnowledgeDistillRequest {
    pub statement: String,
    pub content: String,
    pub tier: String,
    pub supporting_refs: Vec<String>,
    pub counterexample_refs: Option<Vec<String>>,
    pub teaching_refs: Option<Vec<String>>,
    pub domain: Option<String>,
    pub field: Option<String>,
    pub wing: Option<String>,
    pub room: Option<String>,
    pub scope_constraints: Option<String>,
    pub trigger_hints: Option<TriggerHintsDto>,
    pub cwd: Option<String>,
    pub importance: Option<i32>,
    pub dry_run: Option<bool>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct KnowledgeDistillResponse {
    pub drawer_id: String,
    pub created: bool,
    pub dry_run: bool,
}

impl From<DistillOutcome> for KnowledgeDistillResponse {
    fn from(outcome: DistillOutcome) -> Self {
        Self {
            drawer_id: outcome.drawer_id,
            created: outcome.created,
            dry_run: outcome.dry_run,
        }
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct KnowledgePromoteRequest {
    pub drawer_id: String,
    pub status: String,
    pub verification_refs: Vec<String>,
    pub reason: String,
    pub reviewer: Option<String>,
    pub allow_counterexamples: Option<bool>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct KnowledgePromoteResponse {
    pub drawer_id: String,
    pub old_status: String,
    pub new_status: String,
    pub verification_refs: Vec<String>,
    pub gate: Option<KnowledgeGateResponse>,
}

impl From<PromoteOutcome> for KnowledgePromoteResponse {
    fn from(outcome: PromoteOutcome) -> Self {
        Self {
            drawer_id: outcome.drawer_id,
            old_status: outcome.old_status,
            new_status: outcome.new_status,
            verification_refs: outcome.verification_refs,
            gate: outcome.gate.map(KnowledgeGateResponse::from),
        }
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct KnowledgeDemoteRequest {
    pub drawer_id: String,
    pub status: String,
    pub evidence_refs: Vec<String>,
    pub reason: String,
    pub reason_type: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct KnowledgeDemoteResponse {
    pub drawer_id: String,
    pub old_status: String,
    pub new_status: String,
    pub counterexample_refs: Vec<String>,
}

impl From<DemoteOutcome> for KnowledgeDemoteResponse {
    fn from(outcome: DemoteOutcome) -> Self {
        Self {
            drawer_id: outcome.drawer_id,
            old_status: outcome.old_status,
            new_status: outcome.new_status,
            counterexample_refs: outcome.counterexample_refs,
        }
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct KnowledgePublishAnchorRequest {
    pub drawer_id: String,
    pub to: String,
    pub target_anchor_id: Option<String>,
    pub cwd: Option<String>,
    pub reason: String,
    pub reviewer: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct KnowledgePublishAnchorResponse {
    pub drawer_id: String,
    pub old_anchor_kind: String,
    pub old_anchor_id: String,
    pub old_parent_anchor_id: Option<String>,
    pub new_anchor_kind: String,
    pub new_anchor_id: String,
    pub new_parent_anchor_id: Option<String>,
}

impl From<PublishAnchorOutcome> for KnowledgePublishAnchorResponse {
    fn from(outcome: PublishAnchorOutcome) -> Self {
        Self {
            drawer_id: outcome.drawer_id,
            old_anchor_kind: outcome.old_anchor_kind,
            old_anchor_id: outcome.old_anchor_id,
            old_parent_anchor_id: outcome.old_parent_anchor_id,
            new_anchor_kind: outcome.new_anchor_kind,
            new_anchor_id: outcome.new_anchor_id,
            new_parent_anchor_id: outcome.new_parent_anchor_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct KnowledgeGateResponse {
    pub drawer_id: String,
    pub tier: String,
    pub status: String,
    pub target_status: String,
    pub allowed: bool,
    pub reasons: Vec<String>,
    pub requirements: KnowledgeGateRequirementsDto,
    pub evidence_counts: KnowledgeGateEvidenceCountsDto,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct KnowledgeGateRequirementsDto {
    #[schemars(with = "i64")]
    pub min_supporting_refs: usize,
    #[schemars(with = "i64")]
    pub min_verification_refs: usize,
    #[schemars(with = "i64")]
    pub min_teaching_refs: usize,
    pub reviewer_required: bool,
    pub counterexamples_block: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct KnowledgeGateEvidenceCountsDto {
    #[schemars(with = "i64")]
    pub supporting: usize,
    #[schemars(with = "i64")]
    pub counterexample: usize,
    #[schemars(with = "i64")]
    pub teaching: usize,
    #[schemars(with = "i64")]
    pub verification: usize,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct KnowledgePolicyResponse {
    pub entries: Vec<KnowledgePolicyEntryDto>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct KnowledgePolicyEntryDto {
    pub tier: String,
    pub target_status: String,
    pub requirements: KnowledgeGateRequirementsDto,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct KnowledgeCardsRequest {
    pub action: String,
    pub query: Option<String>,
    pub card_id: Option<String>,
    pub target_status: Option<String>,
    pub reviewer: Option<String>,
    pub allow_counterexamples: Option<bool>,
    pub verification_refs: Option<Vec<String>>,
    pub evidence_refs: Option<Vec<String>>,
    pub reason: Option<String>,
    pub reason_type: Option<String>,
    pub enforce_gate: Option<bool>,
    pub tier: Option<String>,
    pub status: Option<String>,
    pub domain: Option<String>,
    pub field: Option<String>,
    pub anchor_kind: Option<String>,
    pub anchor_id: Option<String>,
    pub cwd: Option<String>,
    #[schemars(with = "Option<i64>")]
    pub top_k: Option<usize>,
    #[schemars(with = "Option<i64>")]
    pub evidence_top_k: Option<usize>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct KnowledgeCardsResponse {
    pub cards: Vec<KnowledgeCardDto>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub retrieved: Vec<RetrievedKnowledgeCardDto>,
    pub events: Vec<KnowledgeCardEventDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gate: Option<KnowledgeCardGateDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub promote: Option<KnowledgeCardPromoteDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub demote: Option<KnowledgeCardDemoteDto>,
}

/// Emit a concrete object JSON Schema for free-form JSON fields.
///
/// `serde_json::Value` makes schemars emit a boolean `true` schema for the
/// field. Strict MCP clients (e.g. Claude Code's Zod-based validator) reject a
/// boolean property schema and then fail to load the *entire* tool list. Emit a
/// well-formed `{"type": "object"}` schema instead so the tool advertises
/// cleanly while still accepting arbitrary JSON objects.
fn free_form_object_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "object",
        "additionalProperties": true
    })
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct Phase3Request {
    pub action: String,
    pub id: Option<String>,
    pub surface: Option<String>,
    pub outcome: Option<String>,
    pub subject_kind: Option<String>,
    pub subject_id: Option<String>,
    pub proposed_action: Option<String>,
    pub evidence_refs: Option<Vec<String>>,
    pub counterexample_refs: Option<Vec<String>>,
    pub risk_notes: Option<Vec<String>>,
    pub rollback_criteria: Option<Vec<String>>,
    pub track: Option<String>,
    pub signal: Option<String>,
    pub feature: Option<String>,
    pub query: Option<String>,
    pub context_hash: Option<String>,
    pub card_id: Option<String>,
    pub evaluator_id: Option<String>,
    pub research_report_id: Option<String>,
    pub note: Option<String>,
    #[schemars(schema_with = "free_form_object_schema")]
    pub metadata: Option<serde_json::Value>,
    #[schemars(with = "Option<i64>")]
    pub limit: Option<usize>,
    pub candidate: Option<String>,
    #[schemars(schema_with = "free_form_object_schema")]
    pub report: Option<serde_json::Value>,
    pub execute: Option<bool>,
    pub allow_warnings: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, JsonSchema)]
pub struct Phase3Response {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guidance: Option<RuntimeAdoptionGuidanceDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instrumentation_policy: Option<RuntimeAdoptionInstrumentationPolicyDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record_plan: Option<RuntimeAdoptionRecordPlanDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record_quality: Option<RuntimeAdoptionRecordQualityDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record_checked: Option<RuntimeAdoptionCheckedRecordDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review_report: Option<RuntimeAdoptionReviewReportDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readiness_report: Option<Phase3ReadinessReportDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event: Option<RuntimeAdoptionEventDto>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub events: Vec<RuntimeAdoptionEventDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<RuntimeAdoptionStatsDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analytics: Option<RuntimeAdoptionAnalyticsDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gate: Option<Phase3GateDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub research_plan: Option<ResearchAdapterPlanDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub research_ingest_plan: Option<ResearchIngestPlanDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evaluator_advice: Option<EvaluatorAdviceDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_proposal: Option<CardContextDefaultProposalDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rollback_control: Option<CardContextRollbackControlDto>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct RuntimeAdoptionAnalyticsDto {
    pub writes: bool,
    #[schemars(with = "i64")]
    pub total_events: usize,
    pub groups: Vec<RuntimeAdoptionAnalyticsGroupDto>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct RuntimeAdoptionAnalyticsGroupDto {
    pub track: String,
    pub feature: String,
    #[schemars(with = "i64")]
    pub total: usize,
    #[schemars(with = "i64")]
    pub used: usize,
    #[schemars(with = "i64")]
    pub accepted: usize,
    #[schemars(with = "i64")]
    pub rejected: usize,
    #[schemars(with = "i64")]
    pub misses: usize,
    #[schemars(with = "i64")]
    pub rollbacks: usize,
    #[schemars(with = "i64")]
    pub contradictions: usize,
    #[schemars(with = "i64")]
    pub neutral: usize,
    pub recommendation: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct RuntimeAdoptionGuidanceDto {
    #[schemars(with = "i64")]
    pub version: u32,
    pub recording_rule: String,
    pub required_fields: Vec<String>,
    pub optional_fields: Vec<String>,
    pub signals: Vec<RuntimeAdoptionSignalGuidanceDto>,
    pub tracks: Vec<RuntimeAdoptionTrackGuidanceDto>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct RuntimeAdoptionSignalGuidanceDto {
    pub signal: String,
    pub when: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct RuntimeAdoptionTrackGuidanceDto {
    pub track: String,
    pub when: String,
    pub feature_examples: Vec<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct RuntimeAdoptionInstrumentationPolicyDto {
    #[schemars(with = "i64")]
    pub version: u32,
    pub writes: bool,
    pub default_mode: String,
    pub allowed_modes: Vec<RuntimeAdoptionInstrumentationModeDto>,
    pub forbidden_modes: Vec<String>,
    pub requirements: Vec<String>,
    pub rollback_requirements: Vec<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct RuntimeAdoptionInstrumentationModeDto {
    pub mode: String,
    pub description: String,
    pub requires_execute: bool,
    pub requires_checked_capture: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct RuntimeAdoptionRecordPlanDto {
    pub writes: bool,
    pub record_command: Vec<String>,
    pub record_payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct RuntimeAdoptionRecordQualityDto {
    pub writes: bool,
    pub valid: bool,
    pub quality: String,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct RuntimeAdoptionCheckedRecordDto {
    pub writes: bool,
    pub blocked: bool,
    pub record_quality: RuntimeAdoptionRecordQualityDto,
    pub event: Option<RuntimeAdoptionEventDto>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct RuntimeAdoptionReviewReportDto {
    pub writes: bool,
    pub filters: RuntimeAdoptionReviewFiltersDto,
    #[schemars(with = "i64")]
    pub total: usize,
    pub stats: RuntimeAdoptionSignalCountsDto,
    pub features: Vec<RuntimeAdoptionFeatureReviewDto>,
    pub conclusion: String,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct RuntimeAdoptionReviewFiltersDto {
    pub track: Option<String>,
    pub feature: Option<String>,
    pub signal: Option<String>,
    #[schemars(with = "i64")]
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct RuntimeAdoptionSignalCountsDto {
    #[schemars(with = "i64")]
    pub total: usize,
    #[schemars(with = "i64")]
    pub used: usize,
    #[schemars(with = "i64")]
    pub accepted: usize,
    #[schemars(with = "i64")]
    pub rejected: usize,
    #[schemars(with = "i64")]
    pub misses: usize,
    #[schemars(with = "i64")]
    pub rollbacks: usize,
    #[schemars(with = "i64")]
    pub contradictions: usize,
    #[schemars(with = "i64")]
    pub neutral: usize,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct RuntimeAdoptionFeatureReviewDto {
    pub feature: String,
    pub stats: RuntimeAdoptionSignalCountsDto,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Phase3ReadinessReportDto {
    pub writes: bool,
    pub candidate: String,
    pub ready: bool,
    pub decision: String,
    pub required_track: String,
    pub required_feature: String,
    pub review: RuntimeAdoptionReviewReportDto,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct EvaluatorAdviceDto {
    pub writes: bool,
    pub evaluator_id: String,
    pub subject_kind: String,
    pub subject_id: String,
    pub proposed_action: String,
    pub recommendation: String,
    pub lifecycle_authority: bool,
    pub deterministic_gate_required: bool,
    pub requires_human_review: bool,
    pub evidence_refs: Vec<String>,
    pub counterexample_refs: Vec<String>,
    pub risk_notes: Vec<String>,
    pub reasons: Vec<String>,
    pub adoption_capture: RuntimeAdoptionRecordPlanDto,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CardContextDefaultProposalDto {
    pub writes: bool,
    pub candidate: String,
    pub proposal_ready: bool,
    pub decision: String,
    pub readiness: Phase3ReadinessReportDto,
    pub rollback_criteria: Vec<String>,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CardContextRollbackControlDto {
    pub writes: bool,
    pub candidate: String,
    pub execute: bool,
    pub rollback_required: bool,
    pub applied: bool,
    pub include_cards_default_before: bool,
    pub include_cards_default_after: bool,
    pub review: RuntimeAdoptionReviewReportDto,
    pub reasons: Vec<String>,
}

impl From<crate::core::phase3::CardContextDefaultProposalReport> for CardContextDefaultProposalDto {
    fn from(report: crate::core::phase3::CardContextDefaultProposalReport) -> Self {
        Self {
            writes: report.writes,
            candidate: report.candidate,
            proposal_ready: report.proposal_ready,
            decision: report.decision,
            readiness: report.readiness.into(),
            rollback_criteria: report.rollback_criteria,
            reasons: report.reasons,
        }
    }
}

impl From<crate::core::phase3::CardContextRollbackControlReport> for CardContextRollbackControlDto {
    fn from(report: crate::core::phase3::CardContextRollbackControlReport) -> Self {
        Self {
            writes: report.writes,
            candidate: report.candidate,
            execute: report.execute,
            rollback_required: report.rollback_required,
            applied: report.applied,
            include_cards_default_before: report.include_cards_default_before,
            include_cards_default_after: report.include_cards_default_after,
            review: report.review.into(),
            reasons: report.reasons,
        }
    }
}

impl From<crate::core::phase3::EvaluatorAdviceReport> for EvaluatorAdviceDto {
    fn from(report: crate::core::phase3::EvaluatorAdviceReport) -> Self {
        Self {
            writes: report.writes,
            evaluator_id: report.evaluator_id,
            subject_kind: report.subject_kind,
            subject_id: report.subject_id,
            proposed_action: report.proposed_action,
            recommendation: report.recommendation,
            lifecycle_authority: report.lifecycle_authority,
            deterministic_gate_required: report.deterministic_gate_required,
            requires_human_review: report.requires_human_review,
            evidence_refs: report.evidence_refs,
            counterexample_refs: report.counterexample_refs,
            risk_notes: report.risk_notes,
            reasons: report.reasons,
            adoption_capture: report.adoption_capture.into(),
        }
    }
}

impl From<RuntimeAdoptionRecordPlan> for RuntimeAdoptionRecordPlanDto {
    fn from(plan: RuntimeAdoptionRecordPlan) -> Self {
        Self {
            writes: plan.writes,
            record_command: plan.record_command,
            record_payload: plan.record_payload,
        }
    }
}

impl From<RuntimeAdoptionRecordQualityReport> for RuntimeAdoptionRecordQualityDto {
    fn from(report: RuntimeAdoptionRecordQualityReport) -> Self {
        Self {
            writes: report.writes,
            valid: report.valid,
            quality: report.quality,
            errors: report.errors,
            warnings: report.warnings,
        }
    }
}

impl From<RuntimeAdoptionCheckedRecordReport> for RuntimeAdoptionCheckedRecordDto {
    fn from(report: RuntimeAdoptionCheckedRecordReport) -> Self {
        Self {
            writes: report.writes,
            blocked: report.blocked,
            record_quality: report.record_quality.into(),
            event: report.event.map(RuntimeAdoptionEventDto::from),
        }
    }
}

impl From<RuntimeAdoptionReviewReport> for RuntimeAdoptionReviewReportDto {
    fn from(report: RuntimeAdoptionReviewReport) -> Self {
        Self {
            writes: report.writes,
            filters: report.filters.into(),
            total: report.total,
            stats: report.stats.into(),
            features: report
                .features
                .into_iter()
                .map(|feature| RuntimeAdoptionFeatureReviewDto {
                    feature: feature.feature,
                    stats: feature.stats.into(),
                })
                .collect(),
            conclusion: report.conclusion,
            reasons: report.reasons,
        }
    }
}

impl From<Phase3ReadinessReport> for Phase3ReadinessReportDto {
    fn from(report: Phase3ReadinessReport) -> Self {
        Self {
            writes: report.writes,
            candidate: report.candidate,
            ready: report.ready,
            decision: report.decision,
            required_track: report.required_track,
            required_feature: report.required_feature,
            review: report.review.into(),
            reasons: report.reasons,
        }
    }
}

impl From<RuntimeAdoptionReviewFilters> for RuntimeAdoptionReviewFiltersDto {
    fn from(filters: RuntimeAdoptionReviewFilters) -> Self {
        Self {
            track: filters.track,
            feature: filters.feature,
            signal: filters.signal,
            limit: filters.limit,
        }
    }
}

impl From<RuntimeAdoptionSignalCounts> for RuntimeAdoptionSignalCountsDto {
    fn from(stats: RuntimeAdoptionSignalCounts) -> Self {
        Self {
            total: stats.total,
            used: stats.used,
            accepted: stats.accepted,
            rejected: stats.rejected,
            misses: stats.misses,
            rollbacks: stats.rollbacks,
            contradictions: stats.contradictions,
            neutral: stats.neutral,
        }
    }
}

impl From<RuntimeAdoptionGuidance> for RuntimeAdoptionGuidanceDto {
    fn from(guidance: RuntimeAdoptionGuidance) -> Self {
        Self {
            version: guidance.version,
            recording_rule: guidance.recording_rule,
            required_fields: guidance.required_fields,
            optional_fields: guidance.optional_fields,
            signals: guidance.signals.into_iter().map(Into::into).collect(),
            tracks: guidance.tracks.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<RuntimeAdoptionSignalGuidance> for RuntimeAdoptionSignalGuidanceDto {
    fn from(guidance: RuntimeAdoptionSignalGuidance) -> Self {
        Self {
            signal: guidance.signal,
            when: guidance.when,
        }
    }
}

impl From<RuntimeAdoptionTrackGuidance> for RuntimeAdoptionTrackGuidanceDto {
    fn from(guidance: RuntimeAdoptionTrackGuidance) -> Self {
        Self {
            track: guidance.track,
            when: guidance.when,
            feature_examples: guidance.feature_examples,
        }
    }
}

impl From<RuntimeAdoptionInstrumentationPolicy> for RuntimeAdoptionInstrumentationPolicyDto {
    fn from(policy: RuntimeAdoptionInstrumentationPolicy) -> Self {
        Self {
            version: policy.version,
            writes: policy.writes,
            default_mode: policy.default_mode,
            allowed_modes: policy.allowed_modes.into_iter().map(Into::into).collect(),
            forbidden_modes: policy.forbidden_modes,
            requirements: policy.requirements,
            rollback_requirements: policy.rollback_requirements,
        }
    }
}

impl From<RuntimeAdoptionInstrumentationMode> for RuntimeAdoptionInstrumentationModeDto {
    fn from(mode: RuntimeAdoptionInstrumentationMode) -> Self {
        Self {
            mode: mode.mode,
            description: mode.description,
            requires_execute: mode.requires_execute,
            requires_checked_capture: mode.requires_checked_capture,
        }
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct RuntimeAdoptionEventDto {
    pub id: String,
    pub track: String,
    pub signal: String,
    pub feature: String,
    pub query: Option<String>,
    pub context_hash: Option<String>,
    pub card_id: Option<String>,
    pub evaluator_id: Option<String>,
    pub research_report_id: Option<String>,
    pub note: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct RuntimeAdoptionStatsDto {
    #[schemars(with = "i64")]
    pub total: usize,
    #[schemars(with = "i64")]
    pub used: usize,
    #[schemars(with = "i64")]
    pub accepted: usize,
    #[schemars(with = "i64")]
    pub rejected: usize,
    #[schemars(with = "i64")]
    pub misses: usize,
    #[schemars(with = "i64")]
    pub rollbacks: usize,
    #[schemars(with = "i64")]
    pub contradictions: usize,
    #[schemars(with = "i64")]
    pub neutral: usize,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Phase3GateDto {
    pub candidate: String,
    pub ready: bool,
    pub required_track: String,
    pub stats: RuntimeAdoptionStatsDto,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ResearchAdapterPlanDto {
    pub valid: bool,
    pub report_id: String,
    pub title: String,
    #[schemars(with = "i64")]
    pub source_count: usize,
    #[schemars(with = "i64")]
    pub finding_count: usize,
    #[schemars(with = "i64")]
    pub candidate_insight_count: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ResearchIngestPlanDto {
    pub valid: bool,
    pub writes: bool,
    pub report_id: String,
    pub title: String,
    #[schemars(with = "i64")]
    pub source_count: usize,
    #[schemars(with = "i64")]
    pub finding_count: usize,
    #[schemars(with = "i64")]
    pub candidate_insight_count: usize,
    #[schemars(with = "i64")]
    pub planned_evidence_count: usize,
    #[schemars(with = "i64")]
    pub created_count: usize,
    #[schemars(with = "i64")]
    pub skipped_count: usize,
    pub errors: Vec<String>,
    pub evidence_drawers: Vec<ResearchEvidenceDrawerPlanDto>,
    pub candidate_insights: Vec<ResearchCandidateInsightPlanDto>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ResearchEvidenceDrawerPlanDto {
    pub drawer_id: String,
    #[schemars(with = "i64")]
    pub finding_index: usize,
    pub source_file: String,
    pub created: bool,
    pub skipped: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ResearchCandidateInsightPlanDto {
    pub statement: String,
    pub supporting_refs: Vec<String>,
    pub suggested_command: Vec<String>,
}

impl From<ResearchIngestPlanReport> for ResearchIngestPlanDto {
    fn from(report: ResearchIngestPlanReport) -> Self {
        Self {
            valid: report.valid,
            writes: report.writes,
            report_id: report.report_id,
            title: report.title,
            source_count: report.source_count,
            finding_count: report.finding_count,
            candidate_insight_count: report.candidate_insight_count,
            planned_evidence_count: report.planned_evidence_count,
            created_count: report.created_count,
            skipped_count: report.skipped_count,
            errors: report.errors,
            evidence_drawers: report
                .evidence_drawers
                .into_iter()
                .map(ResearchEvidenceDrawerPlanDto::from)
                .collect(),
            candidate_insights: report
                .candidate_insights
                .into_iter()
                .map(ResearchCandidateInsightPlanDto::from)
                .collect(),
        }
    }
}

impl From<ResearchEvidenceDrawerPlan> for ResearchEvidenceDrawerPlanDto {
    fn from(plan: ResearchEvidenceDrawerPlan) -> Self {
        Self {
            drawer_id: plan.drawer_id,
            finding_index: plan.finding_index,
            source_file: plan.source_file,
            created: plan.created,
            skipped: plan.skipped,
        }
    }
}

impl From<ResearchCandidateInsightPlan> for ResearchCandidateInsightPlanDto {
    fn from(plan: ResearchCandidateInsightPlan) -> Self {
        Self {
            statement: plan.statement,
            supporting_refs: plan.supporting_refs,
            suggested_command: plan.suggested_command,
        }
    }
}

impl From<RuntimeAdoptionAnalyticsReport> for RuntimeAdoptionAnalyticsDto {
    fn from(report: RuntimeAdoptionAnalyticsReport) -> Self {
        Self {
            writes: report.writes,
            total_events: report.total_events,
            groups: report.groups.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<RuntimeAdoptionAnalyticsGroup> for RuntimeAdoptionAnalyticsGroupDto {
    fn from(group: RuntimeAdoptionAnalyticsGroup) -> Self {
        Self {
            track: group.track,
            feature: group.feature,
            total: group.total,
            used: group.used,
            accepted: group.accepted,
            rejected: group.rejected,
            misses: group.misses,
            rollbacks: group.rollbacks,
            contradictions: group.contradictions,
            neutral: group.neutral,
            recommendation: group.recommendation,
        }
    }
}

impl From<RuntimeAdoptionEvent> for RuntimeAdoptionEventDto {
    fn from(event: RuntimeAdoptionEvent) -> Self {
        Self {
            id: event.id,
            track: runtime_adoption_track_slug(&event.track).to_string(),
            signal: runtime_adoption_signal_slug(&event.signal).to_string(),
            feature: event.feature,
            query: event.query,
            context_hash: event.context_hash,
            card_id: event.card_id,
            evaluator_id: event.evaluator_id,
            research_report_id: event.research_report_id,
            note: event.note,
            metadata: event.metadata,
            created_at: event.created_at,
        }
    }
}

fn runtime_adoption_track_slug(track: &RuntimeAdoptionTrack) -> &'static str {
    match track {
        RuntimeAdoptionTrack::RuntimeAdoption => "runtime_adoption",
        RuntimeAdoptionTrack::CardContext => "card_context",
        RuntimeAdoptionTrack::CardEmbedding => "card_embedding",
        RuntimeAdoptionTrack::Evaluator => "evaluator",
        RuntimeAdoptionTrack::ResearchAdapter => "research_adapter",
    }
}

fn runtime_adoption_signal_slug(signal: &RuntimeAdoptionSignal) -> &'static str {
    match signal {
        RuntimeAdoptionSignal::Used => "used",
        RuntimeAdoptionSignal::Accepted => "accepted",
        RuntimeAdoptionSignal::Rejected => "rejected",
        RuntimeAdoptionSignal::Miss => "miss",
        RuntimeAdoptionSignal::Rollback => "rollback",
        RuntimeAdoptionSignal::Contradiction => "contradiction",
        RuntimeAdoptionSignal::Neutral => "neutral",
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct KnowledgeCardDto {
    pub id: String,
    pub statement: String,
    pub content: String,
    pub tier: String,
    pub status: String,
    pub domain: String,
    pub field: String,
    pub anchor_kind: String,
    pub anchor_id: String,
    pub parent_anchor_id: Option<String>,
    pub scope_constraints: Option<String>,
    pub trigger_hints: Option<TriggerHintsDto>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct RetrievedKnowledgeCardDto {
    pub card: KnowledgeCardDto,
    pub evidence_citations: Vec<RetrievedEvidenceCitationDto>,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct RetrievedEvidenceCitationDto {
    pub evidence_drawer_id: String,
    pub role: String,
    pub source_file: String,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct KnowledgeCardEventDto {
    pub id: String,
    pub card_id: String,
    pub event_type: String,
    pub from_status: Option<String>,
    pub to_status: Option<String>,
    pub reason: String,
    pub actor: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct KnowledgeCardGateDto {
    pub card_id: String,
    pub tier: String,
    pub status: String,
    pub target_status: String,
    pub allowed: bool,
    pub reasons: Vec<String>,
    pub requirements: KnowledgeGateRequirementsDto,
    pub evidence_counts: KnowledgeGateEvidenceCountsDto,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct KnowledgeCardPromoteDto {
    pub card_id: String,
    pub old_status: String,
    pub new_status: String,
    pub verification_refs: Vec<String>,
    pub gate: Option<KnowledgeCardGateDto>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct KnowledgeCardDemoteDto {
    pub card_id: String,
    pub old_status: String,
    pub new_status: String,
    pub counterexample_refs: Vec<String>,
}

impl From<Vec<PromotionPolicyEntry>> for KnowledgePolicyResponse {
    fn from(entries: Vec<PromotionPolicyEntry>) -> Self {
        Self {
            entries: entries
                .into_iter()
                .map(|entry| KnowledgePolicyEntryDto {
                    tier: entry.tier,
                    target_status: entry.target_status,
                    requirements: KnowledgeGateRequirementsDto {
                        min_supporting_refs: entry.requirements.min_supporting_refs,
                        min_verification_refs: entry.requirements.min_verification_refs,
                        min_teaching_refs: entry.requirements.min_teaching_refs,
                        reviewer_required: entry.requirements.reviewer_required,
                        counterexamples_block: entry.requirements.counterexamples_block,
                    },
                })
                .collect(),
        }
    }
}

impl From<GateReport> for KnowledgeGateResponse {
    fn from(report: GateReport) -> Self {
        Self {
            drawer_id: report.drawer_id,
            tier: report.tier,
            status: report.status,
            target_status: report.target_status,
            allowed: report.allowed,
            reasons: report.reasons,
            requirements: KnowledgeGateRequirementsDto {
                min_supporting_refs: report.requirements.min_supporting_refs,
                min_verification_refs: report.requirements.min_verification_refs,
                min_teaching_refs: report.requirements.min_teaching_refs,
                reviewer_required: report.requirements.reviewer_required,
                counterexamples_block: report.requirements.counterexamples_block,
            },
            evidence_counts: KnowledgeGateEvidenceCountsDto {
                supporting: report.evidence_counts.supporting,
                counterexample: report.evidence_counts.counterexample,
                teaching: report.evidence_counts.teaching,
                verification: report.evidence_counts.verification,
            },
        }
    }
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
    pub card_id: Option<String>,
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
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub evidence_citations: Vec<ContextEvidenceCitationDto>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ContextEvidenceCitationDto {
    pub evidence_drawer_id: String,
    pub role: String,
    pub source_file: String,
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
    #[schemars(with = "i64")]
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
    #[schemars(with = "i64")]
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

    /// Memory layer. Defaults to "evidence" (a raw, verbatim record) when
    /// omitted. Set "knowledge" ONLY for a fully-formed typed knowledge drawer,
    /// in which case statement, tier, status, and supporting_refs are ALL
    /// required and provenance is forbidden. To turn existing evidence drawers
    /// into a governed rule, prefer mempal_knowledge_distill over a hand-built
    /// knowledge ingest.
    pub memory_kind: Option<String>,
    pub domain: Option<String>,
    pub field: Option<String>,
    pub provenance: Option<String>,
    /// Knowledge-only field — NOT for an evidence drawer. A default (evidence)
    /// ingest REJECTS this and the other knowledge-only fields (tier, status,
    /// supporting_refs, counterexample_refs, teaching_refs, verification_refs,
    /// scope_constraints, trigger_hints). Only valid with
    /// memory_kind="knowledge". To distill evidence into a rule, use
    /// mempal_knowledge_distill.
    pub statement: Option<String>,
    /// Knowledge-only (see `statement`). Rejected on an evidence drawer; use
    /// memory_kind="knowledge" or mempal_knowledge_distill.
    pub tier: Option<String>,
    /// Knowledge-only (see `statement`). Rejected on an evidence drawer; use
    /// memory_kind="knowledge" or mempal_knowledge_distill.
    pub status: Option<String>,
    /// Knowledge-only (see `statement`). Rejected on an evidence drawer; use
    /// memory_kind="knowledge" or mempal_knowledge_distill.
    pub supporting_refs: Option<Vec<String>>,
    /// Knowledge-only (see `statement`). Rejected on an evidence drawer; use
    /// memory_kind="knowledge" or mempal_knowledge_distill.
    pub counterexample_refs: Option<Vec<String>>,
    /// Knowledge-only (see `statement`). Rejected on an evidence drawer; use
    /// memory_kind="knowledge" or mempal_knowledge_distill.
    pub teaching_refs: Option<Vec<String>>,
    /// Knowledge-only (see `statement`). Rejected on an evidence drawer; use
    /// memory_kind="knowledge" or mempal_knowledge_distill.
    pub verification_refs: Option<Vec<String>>,
    /// Knowledge-only (see `statement`). Rejected on an evidence drawer; use
    /// memory_kind="knowledge" or mempal_knowledge_distill.
    pub scope_constraints: Option<String>,
    /// Knowledge-only (see `statement`). Rejected on an evidence drawer; use
    /// memory_kind="knowledge" or mempal_knowledge_distill.
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
    #[schemars(with = "Option<i64>")]
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
    pub schema_version: i32,
    pub normalize_version_current: i32,
    pub stale_drawer_count: i64,
    pub drawer_count: i64,
    pub taxonomy_count: i64,
    pub db_size_bytes: i64,
    pub diary_rollup_days: i32,
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

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct FieldTaxonomyResponse {
    pub entries: Vec<FieldTaxonomyEntryDto>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct FieldTaxonomyEntryDto {
    pub field: String,
    pub domains: Vec<String>,
    pub description: String,
    pub examples: Vec<String>,
}

impl From<FieldTaxonomyEntry> for FieldTaxonomyEntryDto {
    fn from(value: FieldTaxonomyEntry) -> Self {
        Self {
            field: value.field.to_string(),
            domains: value
                .domains
                .iter()
                .map(|domain| (*domain).to_string())
                .collect(),
            description: value.description.to_string(),
            examples: value
                .examples
                .iter()
                .map(|example| (*example).to_string())
                .collect(),
        }
    }
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
    #[schemars(with = "Option<i64>")]
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
    #[schemars(with = "Option<i64>")]
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
    #[schemars(with = "Option<i64>")]
    pub limit: Option<usize>,

    /// Optional RFC3339 timestamp cutoff — only messages strictly newer than
    /// this are returned.
    pub since: Option<String>,

    /// Optional project directory whose partner session to read. Use this to
    /// peek a partner working in another project; when omitted, mempal reads the
    /// partner session for the project this MCP server runs in.
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SessionPeekRequest {
    /// Concrete agent tool whose local session should be read. Must be
    /// "claude" or "codex"; "auto" is intentionally rejected because this is
    /// an explicit session reader, not partner inference.
    pub tool: String,

    /// Maximum number of user+assistant messages to return. Default 30.
    #[schemars(with = "Option<i64>")]
    pub limit: Option<usize>,

    /// Optional RFC3339 timestamp cutoff — only messages strictly newer than
    /// this are returned.
    pub since: Option<String>,

    /// Required project directory whose session should be read. Use this for
    /// same-tool or cross-project reads such as Codex reading another Codex
    /// session for a different cwd.
    pub cwd: String,
}

/// P109: response for `mempal_projects`.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ProjectsResponse {
    pub projects: Vec<crate::projects::ProjectSummary>,
}

/// P109: request for `mempal_resume`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ResumeRequest {
    /// Project name or fragment; matches a wing or a worktree-path basename.
    pub query: String,
    /// Recent evidence drawers to include (default 5).
    #[schemars(with = "Option<i64>")]
    pub evidence_limit: Option<usize>,
    /// In-flight candidate knowledge drawers to include (default 5).
    #[schemars(with = "Option<i64>")]
    pub candidate_limit: Option<usize>,
}

/// P109: flat response for `mempal_resume` (object-rooted for MCP output schema).
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ResumeResponse {
    /// One of `resolved`, `ambiguous`, `not_found`.
    pub resolution: String,
    pub query: String,
    /// The resume pack when `resolution == "resolved"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pack: Option<crate::projects::ResumePack>,
    /// Candidate projects when `resolution == "ambiguous"`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<crate::projects::ProjectSummary>,
    /// Available wings when `resolution == "not_found"`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub available: Vec<String>,
}

impl From<crate::projects::ResumeResolution> for ResumeResponse {
    fn from(resolution: crate::projects::ResumeResolution) -> Self {
        use crate::projects::ResumeResolution;
        match resolution {
            ResumeResolution::Resolved(pack) => ResumeResponse {
                resolution: "resolved".to_string(),
                query: pack.wing.clone(),
                pack: Some(*pack),
                candidates: Vec::new(),
                available: Vec::new(),
            },
            ResumeResolution::Ambiguous { query, candidates } => ResumeResponse {
                resolution: "ambiguous".to_string(),
                query,
                pack: None,
                candidates,
                available: Vec::new(),
            },
            ResumeResolution::NotFound { query, available } => ResumeResponse {
                resolution: "not_found".to_string(),
                query,
                pack: None,
                candidates: Vec::new(),
                available,
            },
        }
    }
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
pub struct SessionPeekResponse {
    pub tool: String,
    pub session_path: Option<String>,
    pub session_mtime: Option<String>,
    pub active: bool,
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
    #[schemars(with = "i64")]
    pub inbox_size_after: u64,
    /// P116 delivery receipt handle; track it via `mempal cowork-receipts`.
    pub message_id: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct CoworkBusRequest {
    /// Action to execute. P85+ uses one MCP tool so agents do not need many
    /// separate tool names for the same multi-agent bus.
    pub action: String,

    /// Absolute filesystem path of the project cwd. Internally normalized to
    /// git repo root so all agents in one project share the same bus registry.
    pub cwd: String,

    /// Concrete target/current agent id for register/drain actions.
    #[serde(default)]
    pub agent_id: Option<String>,

    /// Tool family/name for register action, e.g. "claude" or "codex".
    #[serde(default)]
    pub tool: Option<String>,

    /// Transport metadata for register action. Defaults to "inbox".
    #[serde(default)]
    pub transport: Option<String>,

    /// Optional tmux target for `transport=tmux`.
    #[serde(default)]
    pub tmux_target: Option<String>,

    /// Concrete source agent id for send/broadcast actions.
    #[serde(default)]
    pub from: Option<String>,

    /// Concrete target agent ids for send/broadcast actions.
    #[serde(default)]
    pub to: Vec<String>,

    /// Concrete agent ids for channel_set action.
    #[serde(default)]
    pub agents: Vec<String>,

    /// Message body for send/broadcast actions.
    #[serde(default)]
    pub message: Option<String>,

    /// Optional thread id metadata for send/broadcast/channel_send actions.
    #[serde(default)]
    pub thread_id: Option<String>,

    /// Optional channel name for send/broadcast metadata and channel actions.
    #[serde(default)]
    pub channel: Option<String>,

    /// Delivery event id for action "ack".
    #[serde(default)]
    pub message_id: Option<String>,

    /// Optional max number of latest events for action "events".
    #[serde(default)]
    #[schemars(with = "Option<i64>")]
    pub limit: Option<usize>,

    /// Optional RFC3339 timestamp for deterministic list presence checks.
    #[serde(default)]
    pub now: Option<String>,

    /// Optional RFC3339 timestamp for action "heartbeat".
    #[serde(default)]
    pub seen_at: Option<String>,

    /// Optional line count for action "tmux_peek".
    #[serde(default)]
    #[schemars(with = "Option<i64>")]
    pub lines: Option<usize>,

    /// Optional tmux reachability probe for action "doctor".
    #[serde(default)]
    pub probe_tmux: Option<bool>,

    /// Session id for session and handoff/capture actions.
    #[serde(default)]
    pub session_id: Option<String>,

    /// Session title for action "session_create".
    #[serde(default)]
    pub title: Option<String>,

    /// Session goal for action "session_create".
    #[serde(default)]
    pub goal: Option<String>,

    /// Session status for action "session_status".
    #[serde(default)]
    pub status: Option<String>,

    /// Capture summary source for action "capture".
    #[serde(default)]
    pub summary_source: Option<String>,

    /// Evidence wing for action "capture".
    #[serde(default)]
    pub wing: Option<String>,

    /// Evidence room for action "capture".
    #[serde(default)]
    pub room: Option<String>,

    /// Optional note for action "capture".
    #[serde(default)]
    pub note: Option<String>,

    /// Explicit capture switch for action "session_close".
    #[serde(default)]
    pub capture: Option<bool>,

    /// Explicit write switch for action "capture" and "session_close".
    #[serde(default)]
    pub execute: Option<bool>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CoworkBusResponse {
    pub action: String,
    pub agents: Vec<CoworkBusAgentDto>,
    pub delivered: Vec<CoworkBusDeliveryDto>,
    pub messages: Vec<CoworkBusMessageDto>,
    pub events: Vec<CoworkBusEventDto>,
    pub deliveries: Vec<CoworkBusDeliveryStatusDto>,
    pub channels: Vec<CoworkBusChannelDto>,
    pub tmux_peek: Option<CoworkBusTmuxPeekDto>,
    pub doctor: Option<CoworkBusDoctorDto>,
    pub sessions: Vec<CoworkBusSessionDto>,
    pub handoff: Option<CoworkBusHandoffDto>,
    pub capture: Option<CoworkBusCaptureDto>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CoworkBusAgentDto {
    pub agent_id: String,
    pub tool: String,
    pub transport: String,
    pub tmux_target: Option<String>,
    pub registered_at: String,
    pub updated_at: String,
    pub last_seen_at: Option<String>,
    pub presence: String,
    #[schemars(with = "i64")]
    pub pending_count: usize,
    #[schemars(with = "i64")]
    pub pending_bytes: u64,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CoworkBusDeliveryDto {
    pub message_id: String,
    pub target_agent_id: String,
    pub transport: String,
    pub inbox_path: Option<String>,
    #[schemars(with = "Option<i64>")]
    pub inbox_size_after: Option<u64>,
    pub tmux_target: Option<String>,
    pub thread_id: Option<String>,
    pub channel: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CoworkBusMessageDto {
    pub pushed_at: String,
    pub from: String,
    pub content: String,
    pub thread_id: Option<String>,
    pub channel: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CoworkBusEventDto {
    pub event_id: String,
    pub occurred_at: String,
    pub event_type: String,
    pub status: String,
    pub actor_agent_id: Option<String>,
    pub target_agent_ids: Vec<String>,
    pub transport: Option<String>,
    pub message_preview: Option<String>,
    pub thread_id: Option<String>,
    pub channel: Option<String>,
    pub details: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CoworkBusDeliveryStatusDto {
    pub message_id: String,
    pub event_type: String,
    pub status: String,
    pub from: String,
    pub target_agent_id: String,
    pub transport: String,
    pub message_preview: Option<String>,
    pub thread_id: Option<String>,
    pub channel: Option<String>,
    pub delivered_at: String,
    pub updated_at: String,
    pub acked_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CoworkBusChannelDto {
    pub channel: String,
    pub agents: Vec<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CoworkBusTmuxPeekDto {
    pub agent_id: String,
    pub tmux_target: String,
    #[schemars(with = "i64")]
    pub lines: usize,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CoworkBusDoctorDto {
    pub status: String,
    #[schemars(with = "i64")]
    pub agent_count: usize,
    #[schemars(with = "i64")]
    pub channel_count: usize,
    #[schemars(with = "i64")]
    pub session_count: usize,
    #[schemars(with = "i64")]
    pub stale_agents: usize,
    #[schemars(with = "i64")]
    pub never_seen_agents: usize,
    #[schemars(with = "i64")]
    pub pending_deliveries: usize,
    pub warnings: Vec<String>,
    pub tmux: Vec<CoworkBusTmuxProbeDto>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CoworkBusTmuxProbeDto {
    pub agent_id: String,
    pub tmux_target: String,
    pub status: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CoworkBusSessionDto {
    pub session_id: String,
    pub title: String,
    pub goal: Option<String>,
    pub agents: Vec<String>,
    pub channels: Vec<String>,
    pub thread_id: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CoworkBusHandoffDto {
    pub filters: CoworkBusHandoffFiltersDto,
    pub sessions: Vec<CoworkBusSessionDto>,
    pub agents: Vec<CoworkBusHandoffAgentDto>,
    pub pending_deliveries: Vec<CoworkBusDeliveryStatusDto>,
    pub recent_events: Vec<CoworkBusEventDto>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CoworkBusHandoffFiltersDto {
    pub thread_id: Option<String>,
    pub channel: Option<String>,
    pub session_id: Option<String>,
    #[schemars(with = "i64")]
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CoworkBusHandoffAgentDto {
    pub agent_id: String,
    pub tool: String,
    pub presence: String,
    #[schemars(with = "i64")]
    pub pending_count: usize,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CoworkBusCaptureDto {
    pub writes: bool,
    pub drawer_id: Option<String>,
    pub wing: String,
    pub room: Option<String>,
    pub source: String,
    pub content: String,
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
    #[schemars(with = "i64")]
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
            distill_suggestions: value
                .distill_suggestions
                .into_iter()
                .map(DistillSuggestionDto::from)
                .collect(),
        }
    }
}

impl DoctorResponse {
    pub fn from_report(report: DoctorReport, mcp: DoctorMcpDto) -> Self {
        Self {
            current_version: report.current_version,
            supported_schema_version: report.supported_schema_version,
            db: report.db.into(),
            install: report.install.into(),
            warnings: report.warnings,
            recommendations: report.recommendations,
            mcp,
        }
    }
}

impl From<DoctorDbReport> for DoctorDbDto {
    fn from(report: DoctorDbReport) -> Self {
        Self {
            path: report.path,
            exists: report.exists,
            schema_version: report.schema_version,
            compatible: report.compatible,
            error: report.error,
        }
    }
}

impl From<DoctorInstallReport> for DoctorInstallDto {
    fn from(report: DoctorInstallReport) -> Self {
        Self {
            current_exe: report.current_exe,
            path_mempal: report.path_mempal,
            path_matches_current_exe: report.path_matches_current_exe,
        }
    }
}

impl From<CognitiveBrief> for BriefMcpResponse {
    fn from(brief: CognitiveBrief) -> Self {
        Self {
            query: brief.query,
            domain: domain_slug(&brief.domain).to_string(),
            field: brief.field,
            summary: brief.summary.into(),
            key_facts: brief.key_facts.into_iter().map(Into::into).collect(),
            evidence: brief.evidence.into_iter().map(Into::into).collect(),
            cards: brief.cards.into_iter().map(Into::into).collect(),
            entities: brief.entities,
            unresolved_items: brief.unresolved_items.into_iter().map(Into::into).collect(),
            uncertainty: brief.uncertainty.into_iter().map(Into::into).collect(),
            next_actions: brief.next_actions,
        }
    }
}

impl From<BriefSummary> for BriefSummaryDto {
    fn from(summary: BriefSummary) -> Self {
        Self {
            narrative: summary.narrative,
            key_fact_count: summary.key_fact_count,
            evidence_count: summary.evidence_count,
            card_count: summary.card_count,
            unresolved_count: summary.unresolved_count,
            uncertainty_count: summary.uncertainty_count,
        }
    }
}

impl From<BriefFact> for BriefFactDto {
    fn from(fact: BriefFact) -> Self {
        Self {
            text: fact.text,
            section: fact.section,
            citation: fact.citation.into(),
        }
    }
}

impl From<BriefEvidence> for BriefEvidenceDto {
    fn from(evidence: BriefEvidence) -> Self {
        Self {
            text: evidence.text,
            citation: evidence.citation.into(),
        }
    }
}

impl From<BriefCard> for BriefCardDto {
    fn from(card: BriefCard) -> Self {
        Self {
            card_id: card.card_id,
            text: card.text,
            citation: card.citation.into(),
            evidence_citations: card
                .evidence_citations
                .into_iter()
                .map(Into::into)
                .collect(),
        }
    }
}

impl From<BriefUnresolvedItem> for BriefUnresolvedItemDto {
    fn from(item: BriefUnresolvedItem) -> Self {
        Self {
            text: item.text,
            citation: item.citation.into(),
        }
    }
}

impl From<BriefUncertainty> for BriefUncertaintyDto {
    fn from(uncertainty: BriefUncertainty) -> Self {
        Self {
            kind: uncertainty.kind,
            message: uncertainty.message,
            citations: uncertainty.citations.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<BriefCitation> for BriefCitationDto {
    fn from(citation: BriefCitation) -> Self {
        Self {
            drawer_id: citation.drawer_id,
            source_file: citation.source_file,
            anchor_kind: anchor_kind_slug(&citation.anchor_kind).to_string(),
            anchor_id: citation.anchor_id,
            card_id: citation.card_id,
        }
    }
}

impl From<BriefEvidenceCitation> for BriefEvidenceCitationDto {
    fn from(citation: BriefEvidenceCitation) -> Self {
        Self {
            evidence_drawer_id: citation.evidence_drawer_id,
            role: knowledge_evidence_role_slug(&citation.role).to_string(),
            source_file: citation.source_file,
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
            card_id: value.card_id,
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
            evidence_citations: value
                .evidence_citations
                .into_iter()
                .map(|citation| ContextEvidenceCitationDto {
                    evidence_drawer_id: citation.evidence_drawer_id,
                    role: knowledge_evidence_role_slug(&citation.role).to_string(),
                    source_file: citation.source_file,
                })
                .collect(),
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

impl From<KnowledgeCard> for KnowledgeCardDto {
    fn from(value: KnowledgeCard) -> Self {
        Self {
            id: value.id,
            statement: value.statement,
            content: value.content,
            tier: knowledge_tier_slug(&value.tier).to_string(),
            status: knowledge_status_slug(&value.status).to_string(),
            domain: domain_slug(&value.domain).to_string(),
            field: value.field,
            anchor_kind: anchor_kind_slug(&value.anchor_kind).to_string(),
            anchor_id: value.anchor_id,
            parent_anchor_id: value.parent_anchor_id,
            scope_constraints: value.scope_constraints,
            trigger_hints: value.trigger_hints.map(TriggerHintsDto::from),
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

impl From<RetrievedKnowledgeCard> for RetrievedKnowledgeCardDto {
    fn from(value: RetrievedKnowledgeCard) -> Self {
        Self {
            card: KnowledgeCardDto::from(value.card),
            evidence_citations: value
                .evidence_citations
                .into_iter()
                .map(RetrievedEvidenceCitationDto::from)
                .collect(),
            score: value.score,
        }
    }
}

impl From<RetrievedEvidenceCitation> for RetrievedEvidenceCitationDto {
    fn from(value: RetrievedEvidenceCitation) -> Self {
        Self {
            evidence_drawer_id: value.evidence_drawer_id,
            role: knowledge_evidence_role_slug(&value.role).to_string(),
            source_file: value.source_file,
            score: value.score,
        }
    }
}

impl From<KnowledgeCardEvent> for KnowledgeCardEventDto {
    fn from(value: KnowledgeCardEvent) -> Self {
        Self {
            id: value.id,
            card_id: value.card_id,
            event_type: knowledge_event_type_slug(&value.event_type).to_string(),
            from_status: value
                .from_status
                .as_ref()
                .map(knowledge_status_slug)
                .map(str::to_string),
            to_status: value
                .to_status
                .as_ref()
                .map(knowledge_status_slug)
                .map(str::to_string),
            reason: value.reason,
            actor: value.actor,
            metadata: value.metadata,
            created_at: value.created_at,
        }
    }
}

impl From<KnowledgeCardGateReport> for KnowledgeCardGateDto {
    fn from(value: KnowledgeCardGateReport) -> Self {
        Self {
            card_id: value.card_id,
            tier: value.tier,
            status: value.status,
            target_status: value.target_status,
            allowed: value.allowed,
            reasons: value.reasons,
            requirements: KnowledgeGateRequirementsDto {
                min_supporting_refs: value.requirements.min_supporting_refs,
                min_verification_refs: value.requirements.min_verification_refs,
                min_teaching_refs: value.requirements.min_teaching_refs,
                reviewer_required: value.requirements.reviewer_required,
                counterexamples_block: value.requirements.counterexamples_block,
            },
            evidence_counts: KnowledgeGateEvidenceCountsDto {
                supporting: value.evidence_counts.supporting,
                counterexample: value.evidence_counts.counterexample,
                teaching: value.evidence_counts.teaching,
                verification: value.evidence_counts.verification,
            },
        }
    }
}

impl From<PromoteCardOutcome> for KnowledgeCardPromoteDto {
    fn from(value: PromoteCardOutcome) -> Self {
        Self {
            card_id: value.card_id,
            old_status: value.old_status,
            new_status: value.new_status,
            verification_refs: value.verification_refs,
            gate: value.gate.map(KnowledgeCardGateDto::from),
        }
    }
}

impl From<DemoteCardOutcome> for KnowledgeCardDemoteDto {
    fn from(value: DemoteCardOutcome) -> Self {
        Self {
            card_id: value.card_id,
            old_status: value.old_status,
            new_status: value.new_status,
            counterexample_refs: value.counterexample_refs,
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

fn knowledge_evidence_role_slug(value: &crate::core::types::KnowledgeEvidenceRole) -> &'static str {
    match value {
        crate::core::types::KnowledgeEvidenceRole::Supporting => "supporting",
        crate::core::types::KnowledgeEvidenceRole::Verification => "verification",
        crate::core::types::KnowledgeEvidenceRole::Counterexample => "counterexample",
        crate::core::types::KnowledgeEvidenceRole::Teaching => "teaching",
    }
}

fn anchor_kind_slug(value: &AnchorKind) -> &'static str {
    match value {
        AnchorKind::Global => "global",
        AnchorKind::Repo => "repo",
        AnchorKind::Worktree => "worktree",
    }
}

fn knowledge_event_type_slug(value: &crate::core::types::KnowledgeEventType) -> &'static str {
    match value {
        crate::core::types::KnowledgeEventType::Created => "created",
        crate::core::types::KnowledgeEventType::Promoted => "promoted",
        crate::core::types::KnowledgeEventType::Demoted => "demoted",
        crate::core::types::KnowledgeEventType::Retired => "retired",
        crate::core::types::KnowledgeEventType::Linked => "linked",
        crate::core::types::KnowledgeEventType::Unlinked => "unlinked",
        crate::core::types::KnowledgeEventType::Updated => "updated",
        crate::core::types::KnowledgeEventType::PublishedAnchor => "published_anchor",
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

    use super::{RouteDecisionDto, SearchResponse, SearchResultDto, StatusResponse};

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

    #[test]
    fn test_search_result_serializes_empty_tunnel_hints() {
        let mut result = sample_result("No tunnel hints here.");
        result.tunnel_hints.clear();
        let dto = SearchResultDto::with_signals_from_result(result);
        let json = serde_json::to_value(&dto).expect("serialize dto");

        assert_eq!(json["tunnel_hints"], serde_json::json!([]));
    }

    /// 固化搜索结果 schema 必须要求返回 tunnel_hints 字段。
    #[test]
    fn test_search_result_tunnel_hints_is_schema_required() {
        // 按客户端看到的 SearchResponse schema 定位 results 数组元素定义。
        let schema = serde_json::to_value(rmcp::schemars::schema_for!(SearchResponse))
            .expect("serialize search response schema");
        let item_ref = schema
            .pointer("/properties/results/items/$ref")
            .and_then(serde_json::Value::as_str)
            .expect("results item schema ref");
        let item_schema = schema
            .pointer(item_ref.strip_prefix('#').expect("local schema ref"))
            .expect("search result item schema");
        let required = item_schema["required"]
            .as_array()
            .expect("search result required properties");

        assert!(
            required
                .iter()
                .any(|property| property.as_str() == Some("tunnel_hints")),
            "search result schema must require tunnel_hints: {item_schema}"
        );
    }

    /// 模拟 Ajv 校验，确保 schema 要求的搜索结果字段始终被序列化。
    #[test]
    fn test_search_response_required_props_always_serialized() {
        // 覆盖所有 Vec 为空且所有 Option 为 None 的最小搜索结果。
        let dto = SearchResultDto {
            drawer_id: String::new(),
            content: String::new(),
            wing: String::new(),
            room: None,
            source_file: String::new(),
            similarity: 0.0,
            route: RouteDecisionDto {
                wing: None,
                room: None,
                confidence: 0.0,
                reason: String::new(),
            },
            tunnel_hints: Vec::new(),
            neighbors: None,
            entities: Vec::new(),
            topics: Vec::new(),
            flags: Vec::new(),
            emotions: Vec::new(),
            importance_stars: 0,
            memory_kind: String::new(),
            domain: String::new(),
            field: String::new(),
            statement: None,
            tier: None,
            status: None,
            anchor_kind: String::new(),
            anchor_id: String::new(),
            parent_anchor_id: None,
        };
        // 读取客户端契约中的 required 列表，并与实际 structured content 对照。
        let schema = serde_json::to_value(rmcp::schemars::schema_for!(SearchResponse))
            .expect("serialize search response schema");
        let item_ref = schema
            .pointer("/properties/results/items/$ref")
            .and_then(serde_json::Value::as_str)
            .expect("results item schema ref");
        let item_schema = schema
            .pointer(item_ref.strip_prefix('#').expect("local schema ref"))
            .expect("search result item schema");
        let required = item_schema["required"]
            .as_array()
            .expect("search result required properties");
        let json = serde_json::to_value(&dto).expect("serialize search result");
        let object = json.as_object().expect("search result object");

        for property in required {
            // 每个 required 属性都必须出现在实际 JSON 对象中，即使值为空。
            let property_name = property.as_str().expect("required property name");
            assert!(
                object.contains_key(property_name),
                "required property `{property_name}` missing from serialized search result: {json}"
            );
        }
    }

    #[test]
    fn test_status_response_schema_avoids_unsigned_integer_formats() {
        let schema = rmcp::schemars::schema_for!(StatusResponse);
        let json = serde_json::to_string(&schema).expect("serialize schema");

        assert!(!json.contains("uint32"), "{json}");
        assert!(!json.contains("uint64"), "{json}");
    }
}
