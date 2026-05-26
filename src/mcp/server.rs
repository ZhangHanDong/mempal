use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::adoption_analytics::build_runtime_adoption_analytics;
use crate::brief::brief_from_context;
use crate::context::assemble_context_with_vector;
use crate::core::{
    anchor::{self, DerivedAnchor},
    db::Database,
    phase3::{
        EvaluatorAdviceInput, RuntimeAdoptionCaptureInput, RuntimeAdoptionCheckedRecordReport,
        RuntimeAdoptionRecordPlanInput, RuntimeAdoptionReviewFilters,
        build_research_ingest_plan_from_value, capture_runtime_adoption_record_input,
        card_context_default_proposal, card_context_default_readiness,
        card_context_rollback_control, check_runtime_adoption_record, evaluator_advice,
        prepare_runtime_adoption_capture, prepare_runtime_adoption_record,
        review_runtime_adoption_events, runtime_adoption_guidance,
        runtime_adoption_instrumentation_policy, should_write_checked_record,
    },
    types::{
        AnchorKind, BootstrapIdentityParts, Drawer, ExplicitTunnel, KnowledgeCardFilter,
        KnowledgeStatus, KnowledgeTier, MemoryDomain, MemoryKind, Provenance, RuntimeAdoptionEvent,
        RuntimeAdoptionFilter, RuntimeAdoptionSignal, RuntimeAdoptionTrack, SourceType,
        TriggerHints, Triple,
    },
    utils::{
        build_bootstrap_drawer_id_from_parts, build_triple_id, current_timestamp,
        knowledge_source_file, source_file_or_synthetic,
    },
};
use crate::cowork::{
    AgentRecord, AgentStatus, BusError, DeliveryReport, InboxMessage, PeekError,
    PeekRequest as CoworkPeekRequest, Tool, peek_partner,
};
use crate::doctor::{COWORK_BUS_ACTIONS, PHASE3_ACTIONS, REQUIRED_MCP_TOOLS, build_doctor_report};
use crate::embed::EmbedderFactory;
use crate::field_taxonomy::field_taxonomy;
use crate::ingest::{
    IngestError,
    diary::{
        DIARY_ROLLUP_WING, DiaryRollupOptions, commit_prepared_diary_rollup,
        diary_rollup_drawer_id, prepare_diary_rollup,
    },
    normalize::CURRENT_NORMALIZE_VERSION,
};
use crate::knowledge_anchor::{PublishAnchorRequest as CorePublishAnchorRequest, publish_anchor};
use crate::knowledge_card_lifecycle::{
    DemoteCardRequest as CoreDemoteCardRequest, PromoteCardRequest as CorePromoteCardRequest,
    demote_card, evaluate_card_gate_by_id, promote_card,
};
use crate::knowledge_card_retrieval::{
    KnowledgeCardRetrievalRequest as CoreCardRetrievalRequest, retrieve_knowledge_cards_with_vector,
};
use crate::knowledge_distill::{
    DistillPlan, DistillRequest as CoreDistillRequest, commit_distill, prepare_distill,
};
use crate::knowledge_gate::{evaluate_gate_by_id, promotion_policy};
use crate::knowledge_lifecycle::{
    DemoteRequest as CoreDemoteRequest, PromoteRequest as CorePromoteRequest, demote_knowledge,
    promote_knowledge,
};
use crate::search::{SearchFilters, SearchOptions, resolve_route, search_with_vector_options};
use anyhow::Context;
use rmcp::{
    ErrorData, Json, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
};
use serde_json::Value;

use super::tools::{
    BriefMcpRequest, BriefMcpResponse, ContextRequest, ContextResponse, CoworkBusAgentDto,
    CoworkBusCaptureDto, CoworkBusChannelDto, CoworkBusDeliveryDto, CoworkBusDeliveryStatusDto,
    CoworkBusDoctorDto, CoworkBusEventDto, CoworkBusHandoffAgentDto, CoworkBusHandoffDto,
    CoworkBusHandoffFiltersDto, CoworkBusMessageDto, CoworkBusRequest, CoworkBusResponse,
    CoworkBusSessionDto, CoworkBusTmuxPeekDto, CoworkBusTmuxProbeDto, CoworkPushRequest,
    CoworkPushResponse, DeleteRequest, DeleteResponse, DoctorMcpDto, DoctorRequest, DoctorResponse,
    DoctorToolDto, DuplicateWarning, FactCheckRequest, FactCheckResponse, FieldTaxonomyEntryDto,
    FieldTaxonomyResponse, IngestRequest, IngestResponse, KgRequest, KgResponse, KgStatsDto,
    KnowledgeCardDto, KnowledgeCardEventDto, KnowledgeCardsRequest, KnowledgeCardsResponse,
    KnowledgeDemoteRequest, KnowledgeDemoteResponse, KnowledgeDistillRequest,
    KnowledgeDistillResponse, KnowledgeGateRequest, KnowledgeGateResponse, KnowledgePolicyResponse,
    KnowledgePromoteRequest, KnowledgePromoteResponse, KnowledgePublishAnchorRequest,
    KnowledgePublishAnchorResponse, PeekMessageDto, PeekPartnerRequest, PeekPartnerResponse,
    Phase3GateDto, Phase3Request, Phase3Response, ResearchAdapterPlanDto, ResearchIngestPlanDto,
    RetrievedKnowledgeCardDto, RuntimeAdoptionEventDto, RuntimeAdoptionStatsDto, ScopeCount,
    SearchRequest, SearchResponse, SearchResultDto, StatusResponse, TaxonomyEntryDto,
    TaxonomyRequest, TaxonomyResponse, TriggerHintsDto, TripleDto, TunnelDto, TunnelEndpointDto,
    TunnelsRequest, TunnelsResponse,
};

#[derive(Clone)]
pub struct MempalMcpServer {
    db_path: PathBuf,
    config: crate::core::config::Config,
    embedder_factory: Arc<dyn EmbedderFactory>,
    tool_router: ToolRouter<Self>,
    /// Captured via `initialize` override so `auto` peek mode can infer the
    /// partner from the calling MCP client's self-reported name.
    client_name: Arc<Mutex<Option<String>>>,
}

impl MempalMcpServer {
    pub fn new(db_path: PathBuf, config: crate::core::config::Config) -> Self {
        let embedder_factory =
            Arc::new(crate::embed::ConfiguredEmbedderFactory::new(config.clone()));
        Self {
            db_path,
            config,
            embedder_factory,
            tool_router: Self::tool_router(),
            client_name: Arc::new(Mutex::new(None)),
        }
    }

    pub fn new_with_factory(db_path: PathBuf, embedder_factory: Arc<dyn EmbedderFactory>) -> Self {
        Self::new_with_config_and_factory(
            db_path,
            crate::core::config::Config::default(),
            embedder_factory,
        )
    }

    pub fn new_with_config_and_factory(
        db_path: PathBuf,
        config: crate::core::config::Config,
        embedder_factory: Arc<dyn EmbedderFactory>,
    ) -> Self {
        Self {
            db_path,
            config,
            embedder_factory,
            tool_router: Self::tool_router(),
            client_name: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn serve_stdio(
        self,
    ) -> anyhow::Result<rmcp::service::RunningService<rmcp::RoleServer, Self>> {
        self.serve(rmcp::transport::stdio())
            .await
            .context("failed to initialize MCP stdio transport")
    }

    fn open_db(&self) -> std::result::Result<Database, ErrorData> {
        Database::open(&self.db_path).map_err(|error| {
            ErrorData::internal_error(format!("failed to open database: {error}"), None)
        })
    }

    pub async fn ingest_json_for_test(
        &self,
        value: Value,
    ) -> std::result::Result<IngestResponse, ErrorData> {
        let request = serde_json::from_value(value)
            .map_err(|error| ErrorData::invalid_params(error.to_string(), None))?;
        self.mempal_ingest(Parameters(request))
            .await
            .map(|response| response.0)
    }

    pub async fn search_json_for_test(
        &self,
        value: Value,
    ) -> std::result::Result<SearchResponse, ErrorData> {
        let request = serde_json::from_value(value)
            .map_err(|error| ErrorData::invalid_params(error.to_string(), None))?;
        self.mempal_search(Parameters(request))
            .await
            .map(|response| response.0)
    }

    pub async fn context_json_for_test(
        &self,
        value: Value,
    ) -> std::result::Result<ContextResponse, ErrorData> {
        let request = serde_json::from_value(value)
            .map_err(|error| ErrorData::invalid_params(error.to_string(), None))?;
        self.mempal_context(Parameters(request))
            .await
            .map(|response| response.0)
    }

    pub async fn doctor_json_for_test(
        &self,
        value: Value,
    ) -> std::result::Result<DoctorResponse, ErrorData> {
        let request = serde_json::from_value(value)
            .map_err(|error| ErrorData::invalid_params(error.to_string(), None))?;
        self.mempal_doctor(Parameters(request))
            .await
            .map(|response| response.0)
    }

    pub async fn brief_json_for_test(
        &self,
        value: Value,
    ) -> std::result::Result<BriefMcpResponse, ErrorData> {
        let request = serde_json::from_value(value)
            .map_err(|error| ErrorData::invalid_params(error.to_string(), None))?;
        self.mempal_brief(Parameters(request))
            .await
            .map(|response| response.0)
    }

    pub async fn knowledge_gate_json_for_test(
        &self,
        value: Value,
    ) -> std::result::Result<KnowledgeGateResponse, ErrorData> {
        let request = serde_json::from_value(value)
            .map_err(|error| ErrorData::invalid_params(error.to_string(), None))?;
        self.mempal_knowledge_gate(Parameters(request))
            .await
            .map(|response| response.0)
    }

    pub async fn knowledge_distill_json_for_test(
        &self,
        value: Value,
    ) -> std::result::Result<KnowledgeDistillResponse, ErrorData> {
        let request = serde_json::from_value(value)
            .map_err(|error| ErrorData::invalid_params(error.to_string(), None))?;
        self.mempal_knowledge_distill(Parameters(request))
            .await
            .map(|response| response.0)
    }

    pub async fn knowledge_promote_json_for_test(
        &self,
        value: Value,
    ) -> std::result::Result<KnowledgePromoteResponse, ErrorData> {
        let request = serde_json::from_value(value)
            .map_err(|error| ErrorData::invalid_params(error.to_string(), None))?;
        self.mempal_knowledge_promote(Parameters(request))
            .await
            .map(|response| response.0)
    }

    pub async fn knowledge_demote_json_for_test(
        &self,
        value: Value,
    ) -> std::result::Result<KnowledgeDemoteResponse, ErrorData> {
        let request = serde_json::from_value(value)
            .map_err(|error| ErrorData::invalid_params(error.to_string(), None))?;
        self.mempal_knowledge_demote(Parameters(request))
            .await
            .map(|response| response.0)
    }

    pub async fn knowledge_publish_anchor_json_for_test(
        &self,
        value: Value,
    ) -> std::result::Result<KnowledgePublishAnchorResponse, ErrorData> {
        let request = serde_json::from_value(value)
            .map_err(|error| ErrorData::invalid_params(error.to_string(), None))?;
        self.mempal_knowledge_publish_anchor(Parameters(request))
            .await
            .map(|response| response.0)
    }

    pub async fn tunnels_json_for_test(
        &self,
        value: Value,
    ) -> std::result::Result<TunnelsResponse, ErrorData> {
        let request = serde_json::from_value(value)
            .map_err(|error| ErrorData::invalid_params(error.to_string(), None))?;
        self.mempal_tunnels(Parameters(request))
            .await
            .map(|response| response.0)
    }

    pub async fn status_json_for_test(&self) -> std::result::Result<StatusResponse, ErrorData> {
        self.mempal_status().await.map(|response| response.0)
    }

    pub async fn knowledge_policy_json_for_test(
        &self,
    ) -> std::result::Result<KnowledgePolicyResponse, ErrorData> {
        self.mempal_knowledge_policy()
            .await
            .map(|response| response.0)
    }

    pub async fn knowledge_cards_json_for_test(
        &self,
        value: Value,
    ) -> std::result::Result<KnowledgeCardsResponse, ErrorData> {
        let request = serde_json::from_value(value)
            .map_err(|error| ErrorData::invalid_params(error.to_string(), None))?;
        self.mempal_knowledge_cards(Parameters(request))
            .await
            .map(|response| response.0)
    }

    pub async fn phase3_json_for_test(
        &self,
        value: Value,
    ) -> std::result::Result<Phase3Response, ErrorData> {
        let request = serde_json::from_value(value)
            .map_err(|error| ErrorData::invalid_params(error.to_string(), None))?;
        self.mempal_phase3(Parameters(request))
            .await
            .map(|response| response.0)
    }

    pub async fn field_taxonomy_json_for_test(
        &self,
    ) -> std::result::Result<FieldTaxonomyResponse, ErrorData> {
        self.mempal_field_taxonomy()
            .await
            .map(|response| response.0)
    }
}

#[derive(Debug)]
struct ValidatedIngestMetadata {
    memory_kind: MemoryKind,
    domain: MemoryDomain,
    field: String,
    anchor_kind: AnchorKind,
    anchor_id: String,
    parent_anchor_id: Option<String>,
    provenance: Option<Provenance>,
    statement: Option<String>,
    tier: Option<KnowledgeTier>,
    status: Option<KnowledgeStatus>,
    supporting_refs: Vec<String>,
    counterexample_refs: Vec<String>,
    teaching_refs: Vec<String>,
    verification_refs: Vec<String>,
    scope_constraints: Option<String>,
    trigger_hints: Option<TriggerHints>,
}

impl ValidatedIngestMetadata {
    fn identity_parts(&self) -> BootstrapIdentityParts<'_> {
        BootstrapIdentityParts {
            memory_kind: &self.memory_kind,
            domain: &self.domain,
            field: &self.field,
            anchor_kind: &self.anchor_kind,
            anchor_id: &self.anchor_id,
            parent_anchor_id: self.parent_anchor_id.as_deref(),
            provenance: self.provenance.as_ref(),
            statement: self.statement.as_deref(),
            tier: self.tier.as_ref(),
            status: self.status.as_ref(),
            supporting_refs: &self.supporting_refs,
            counterexample_refs: &self.counterexample_refs,
            teaching_refs: &self.teaching_refs,
            verification_refs: &self.verification_refs,
            scope_constraints: self.scope_constraints.as_deref(),
            trigger_hints: self.trigger_hints.as_ref(),
        }
    }
}

fn validate_ingest_request(
    request: &IngestRequest,
    source_type: &SourceType,
) -> std::result::Result<ValidatedIngestMetadata, ErrorData> {
    let memory_kind =
        parse_memory_kind(request.memory_kind.as_deref())?.unwrap_or(MemoryKind::Evidence);
    let domain = parse_domain(request.domain.as_deref())?.unwrap_or(MemoryDomain::Project);
    let field = trim_to_option(request.field.as_deref())
        .unwrap_or(anchor::DEFAULT_FIELD)
        .to_string();
    let statement = trim_to_owned(request.statement.as_deref());
    let tier = parse_tier(request.tier.as_deref())?;
    let status = parse_status(request.status.as_deref())?;
    let provenance = parse_provenance(request.provenance.as_deref())?;
    let supporting_refs = normalize_refs(request.supporting_refs.as_deref());
    let counterexample_refs = normalize_refs(request.counterexample_refs.as_deref());
    let teaching_refs = normalize_refs(request.teaching_refs.as_deref());
    let verification_refs = normalize_refs(request.verification_refs.as_deref());
    let scope_constraints = trim_to_owned(request.scope_constraints.as_deref());
    let trigger_hints = request.trigger_hints.as_ref().map(trigger_hints_from_dto);

    let derived_anchor = validate_anchor_metadata(request, &domain, source_type)?;

    match memory_kind {
        MemoryKind::Evidence => {
            if statement.is_some()
                || tier.is_some()
                || status.is_some()
                || !supporting_refs.is_empty()
                || !counterexample_refs.is_empty()
                || !teaching_refs.is_empty()
                || !verification_refs.is_empty()
                || scope_constraints.is_some()
                || trigger_hints.is_some()
            {
                return Err(ErrorData::invalid_params(
                    "evidence drawer does not allow knowledge-only fields",
                    None,
                ));
            }

            Ok(ValidatedIngestMetadata {
                memory_kind,
                domain,
                field,
                anchor_kind: derived_anchor.anchor_kind,
                anchor_id: derived_anchor.anchor_id,
                parent_anchor_id: derived_anchor.parent_anchor_id,
                provenance: Some(
                    provenance.unwrap_or_else(|| anchor::bootstrap_provenance(source_type)),
                ),
                statement: None,
                tier: None,
                status: None,
                supporting_refs: Vec::new(),
                counterexample_refs: Vec::new(),
                teaching_refs: Vec::new(),
                verification_refs: Vec::new(),
                scope_constraints: None,
                trigger_hints: None,
            })
        }
        MemoryKind::Knowledge => {
            if provenance.is_some() {
                return Err(ErrorData::invalid_params(
                    "knowledge drawer does not allow provenance",
                    None,
                ));
            }

            let statement = statement.ok_or_else(|| {
                ErrorData::invalid_params(
                    "knowledge drawer requires statement and supporting_refs",
                    None,
                )
            })?;
            let tier = tier.ok_or_else(|| {
                ErrorData::invalid_params(
                    "knowledge drawer requires tier, status, statement, and supporting_refs",
                    None,
                )
            })?;
            let status = status.ok_or_else(|| {
                ErrorData::invalid_params(
                    "knowledge drawer requires tier, status, statement, and supporting_refs",
                    None,
                )
            })?;

            if supporting_refs.is_empty() {
                return Err(ErrorData::invalid_params(
                    "knowledge drawer requires statement and supporting_refs",
                    None,
                ));
            }
            validate_drawer_refs("supporting_refs", &supporting_refs)?;
            validate_drawer_refs("counterexample_refs", &counterexample_refs)?;
            validate_drawer_refs("teaching_refs", &teaching_refs)?;
            validate_drawer_refs("verification_refs", &verification_refs)?;

            validate_tier_status(&tier, &status)?;

            Ok(ValidatedIngestMetadata {
                memory_kind,
                domain,
                field,
                anchor_kind: derived_anchor.anchor_kind,
                anchor_id: derived_anchor.anchor_id,
                parent_anchor_id: derived_anchor.parent_anchor_id,
                provenance: None,
                statement: Some(statement),
                tier: Some(tier),
                status: Some(status),
                supporting_refs,
                counterexample_refs,
                teaching_refs,
                verification_refs,
                scope_constraints,
                trigger_hints,
            })
        }
    }
}

fn validate_anchor_metadata(
    request: &IngestRequest,
    domain: &MemoryDomain,
    source_type: &SourceType,
) -> std::result::Result<DerivedAnchor, ErrorData> {
    let explicit_kind = trim_to_option(request.anchor_kind.as_deref());
    let explicit_id = trim_to_option(request.anchor_id.as_deref());

    let anchor = match (explicit_kind, explicit_id) {
        (Some(kind), Some(anchor_id)) => {
            let anchor_kind = parse_anchor_kind(Some(kind))?.expect("explicit kind");
            anchor::validate_explicit_anchor(&anchor_kind, anchor_id).map_err(anchor_error)?;
            DerivedAnchor {
                anchor_kind,
                anchor_id: anchor_id.to_string(),
                parent_anchor_id: trim_to_owned(request.parent_anchor_id.as_deref()),
            }
        }
        (Some(_), None) | (None, Some(_)) => {
            return Err(ErrorData::invalid_params(
                "anchor_kind and anchor_id must be provided together",
                None,
            ));
        }
        (None, None) => {
            if let Some(cwd) = trim_to_option(request.cwd.as_deref()) {
                anchor::derive_anchor_from_cwd(Some(Path::new(cwd))).map_err(anchor_error)?
            } else {
                let defaults = anchor::bootstrap_defaults(source_type);
                DerivedAnchor {
                    anchor_kind: defaults.anchor_kind,
                    anchor_id: defaults.anchor_id,
                    parent_anchor_id: defaults.parent_anchor_id,
                }
            }
        }
    };

    anchor::validate_anchor_domain(domain, &anchor.anchor_kind)
        .map_err(|message| ErrorData::invalid_params(message.to_string(), None))?;
    Ok(anchor)
}

fn validate_tier_status(
    tier: &KnowledgeTier,
    status: &KnowledgeStatus,
) -> std::result::Result<(), ErrorData> {
    let allowed = match tier {
        KnowledgeTier::DaoTian => &[KnowledgeStatus::Canonical, KnowledgeStatus::Demoted][..],
        KnowledgeTier::DaoRen => &[
            KnowledgeStatus::Candidate,
            KnowledgeStatus::Promoted,
            KnowledgeStatus::Demoted,
            KnowledgeStatus::Retired,
        ][..],
        KnowledgeTier::Shu => &[
            KnowledgeStatus::Promoted,
            KnowledgeStatus::Demoted,
            KnowledgeStatus::Retired,
        ][..],
        KnowledgeTier::Qi => &[
            KnowledgeStatus::Candidate,
            KnowledgeStatus::Promoted,
            KnowledgeStatus::Demoted,
            KnowledgeStatus::Retired,
        ][..],
    };

    if allowed.contains(status) {
        return Ok(());
    }

    let message = match tier {
        KnowledgeTier::DaoTian => "dao_tian only allows canonical or demoted",
        KnowledgeTier::DaoRen => "dao_ren only allows candidate, promoted, demoted, or retired",
        KnowledgeTier::Shu => "shu only allows promoted, demoted, or retired",
        KnowledgeTier::Qi => "qi only allows candidate, promoted, demoted, or retired",
    };
    Err(ErrorData::invalid_params(message, None))
}

fn parse_memory_kind(value: Option<&str>) -> std::result::Result<Option<MemoryKind>, ErrorData> {
    parse_enum(value, "memory_kind", |normalized| match normalized {
        "evidence" => Some(MemoryKind::Evidence),
        "knowledge" => Some(MemoryKind::Knowledge),
        _ => None,
    })
}

fn parse_domain(value: Option<&str>) -> std::result::Result<Option<MemoryDomain>, ErrorData> {
    parse_enum(value, "domain", |normalized| match normalized {
        "project" => Some(MemoryDomain::Project),
        "agent" => Some(MemoryDomain::Agent),
        "skill" => Some(MemoryDomain::Skill),
        "global" => Some(MemoryDomain::Global),
        _ => None,
    })
}

fn parse_anchor_kind(value: Option<&str>) -> std::result::Result<Option<AnchorKind>, ErrorData> {
    parse_enum(value, "anchor_kind", |normalized| match normalized {
        "global" => Some(AnchorKind::Global),
        "repo" => Some(AnchorKind::Repo),
        "worktree" => Some(AnchorKind::Worktree),
        _ => None,
    })
}

fn parse_provenance(value: Option<&str>) -> std::result::Result<Option<Provenance>, ErrorData> {
    parse_enum(value, "provenance", |normalized| match normalized {
        "runtime" => Some(Provenance::Runtime),
        "research" => Some(Provenance::Research),
        "human" => Some(Provenance::Human),
        _ => None,
    })
}

fn parse_tier(value: Option<&str>) -> std::result::Result<Option<KnowledgeTier>, ErrorData> {
    parse_enum(value, "tier", |normalized| match normalized {
        "qi" => Some(KnowledgeTier::Qi),
        "shu" => Some(KnowledgeTier::Shu),
        "dao_ren" => Some(KnowledgeTier::DaoRen),
        "dao_tian" => Some(KnowledgeTier::DaoTian),
        _ => None,
    })
}

fn parse_status(value: Option<&str>) -> std::result::Result<Option<KnowledgeStatus>, ErrorData> {
    parse_enum(value, "status", |normalized| match normalized {
        "candidate" => Some(KnowledgeStatus::Candidate),
        "promoted" => Some(KnowledgeStatus::Promoted),
        "canonical" => Some(KnowledgeStatus::Canonical),
        "demoted" => Some(KnowledgeStatus::Demoted),
        "retired" => Some(KnowledgeStatus::Retired),
        _ => None,
    })
}

fn parse_enum<T, F>(
    value: Option<&str>,
    field: &'static str,
    parser: F,
) -> std::result::Result<Option<T>, ErrorData>
where
    F: Fn(&str) -> Option<T>,
{
    let Some(value) = trim_to_option(value) else {
        return Ok(None);
    };

    parser(value)
        .map(Some)
        .ok_or_else(|| ErrorData::invalid_params(format!("invalid {field}: {value}"), None))
}

fn normalize_refs(values: Option<&[String]>) -> Vec<String> {
    values
        .unwrap_or(&[])
        .iter()
        .filter_map(|value| trim_to_owned(Some(value.as_str())))
        .collect()
}

fn validate_drawer_refs(field: &str, values: &[String]) -> std::result::Result<(), ErrorData> {
    if values.iter().all(|value| looks_like_drawer_id(value)) {
        Ok(())
    } else {
        Err(ErrorData::invalid_params(
            format!("{field} must contain drawer ids"),
            None,
        ))
    }
}

fn looks_like_drawer_id(value: &str) -> bool {
    value.starts_with("drawer_")
        && value.len() > "drawer_".len()
        && value
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
}

fn trigger_hints_from_dto(dto: &TriggerHintsDto) -> TriggerHints {
    TriggerHints {
        intent_tags: normalize_refs(Some(&dto.intent_tags)),
        workflow_bias: normalize_refs(Some(&dto.workflow_bias)),
        tool_needs: normalize_refs(Some(&dto.tool_needs)),
    }
}

fn trim_to_option(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn trim_to_owned(value: Option<&str>) -> Option<String> {
    trim_to_option(value).map(ToOwned::to_owned)
}

fn required_string<'a>(
    value: Option<&'a str>,
    field: &'static str,
) -> std::result::Result<&'a str, ErrorData> {
    trim_to_option(value)
        .ok_or_else(|| ErrorData::invalid_params(format!("{field} is required"), None))
}

fn parse_runtime_adoption_track_opt(
    value: Option<&str>,
) -> std::result::Result<Option<RuntimeAdoptionTrack>, ErrorData> {
    parse_enum(value, "track", |normalized| match normalized {
        "runtime_adoption" => Some(RuntimeAdoptionTrack::RuntimeAdoption),
        "card_context" => Some(RuntimeAdoptionTrack::CardContext),
        "card_embedding" => Some(RuntimeAdoptionTrack::CardEmbedding),
        "evaluator" => Some(RuntimeAdoptionTrack::Evaluator),
        "research_adapter" => Some(RuntimeAdoptionTrack::ResearchAdapter),
        _ => None,
    })
}

fn parse_runtime_adoption_track(
    value: &str,
) -> std::result::Result<RuntimeAdoptionTrack, ErrorData> {
    parse_runtime_adoption_track_opt(Some(value))?
        .ok_or_else(|| ErrorData::invalid_params("track is required", None))
}

fn parse_runtime_adoption_signal(
    value: &str,
) -> std::result::Result<RuntimeAdoptionSignal, ErrorData> {
    parse_enum(Some(value), "signal", |normalized| match normalized {
        "used" => Some(RuntimeAdoptionSignal::Used),
        "accepted" => Some(RuntimeAdoptionSignal::Accepted),
        "rejected" => Some(RuntimeAdoptionSignal::Rejected),
        "miss" => Some(RuntimeAdoptionSignal::Miss),
        "rollback" => Some(RuntimeAdoptionSignal::Rollback),
        "contradiction" => Some(RuntimeAdoptionSignal::Contradiction),
        "neutral" => Some(RuntimeAdoptionSignal::Neutral),
        _ => None,
    })?
    .ok_or_else(|| ErrorData::invalid_params("signal is required", None))
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

fn phase3_event_id(
    track: &RuntimeAdoptionTrack,
    signal: &RuntimeAdoptionSignal,
    feature: &str,
) -> String {
    let signal = match signal {
        RuntimeAdoptionSignal::Used => "used",
        RuntimeAdoptionSignal::Accepted => "accepted",
        RuntimeAdoptionSignal::Rejected => "rejected",
        RuntimeAdoptionSignal::Miss => "miss",
        RuntimeAdoptionSignal::Rollback => "rollback",
        RuntimeAdoptionSignal::Contradiction => "contradiction",
        RuntimeAdoptionSignal::Neutral => "neutral",
    };
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let sanitized_feature = feature
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!(
        "adoption_{}_{}_{}_{}",
        runtime_adoption_track_slug(track),
        signal,
        sanitized_feature,
        nanos
    )
}

fn runtime_adoption_stats(events: &[RuntimeAdoptionEvent]) -> RuntimeAdoptionStatsDto {
    let mut stats = RuntimeAdoptionStatsDto {
        total: events.len(),
        used: 0,
        accepted: 0,
        rejected: 0,
        misses: 0,
        rollbacks: 0,
        contradictions: 0,
        neutral: 0,
    };
    for event in events {
        match event.signal {
            RuntimeAdoptionSignal::Used => stats.used += 1,
            RuntimeAdoptionSignal::Accepted => stats.accepted += 1,
            RuntimeAdoptionSignal::Rejected => stats.rejected += 1,
            RuntimeAdoptionSignal::Miss => stats.misses += 1,
            RuntimeAdoptionSignal::Rollback => stats.rollbacks += 1,
            RuntimeAdoptionSignal::Contradiction => stats.contradictions += 1,
            RuntimeAdoptionSignal::Neutral => stats.neutral += 1,
        }
    }
    stats
}

fn phase3_gate_report(
    db: &Database,
    candidate: &str,
) -> std::result::Result<Phase3GateDto, ErrorData> {
    let (track, ready_fn): (RuntimeAdoptionTrack, fn(&RuntimeAdoptionStatsDto) -> bool) =
        match candidate {
            "card-context-default" => (RuntimeAdoptionTrack::CardContext, |stats| {
                stats.accepted >= 3 && stats.rollbacks == 0 && stats.rejected <= stats.accepted
            }),
            "card-embeddings" => (RuntimeAdoptionTrack::CardEmbedding, |stats| {
                stats.misses >= 3 && stats.rollbacks == 0
            }),
            "evaluator-api" => (RuntimeAdoptionTrack::Evaluator, |stats| {
                stats.accepted >= 3 && stats.rollbacks == 0 && stats.contradictions == 0
            }),
            "research-adapter" => (RuntimeAdoptionTrack::ResearchAdapter, |stats| {
                stats.accepted >= 1 && stats.contradictions == 0 && stats.rollbacks == 0
            }),
            other => {
                return Err(ErrorData::invalid_params(
                    format!("unsupported phase3 candidate: {other}"),
                    None,
                ));
            }
        };
    let events = db
        .list_runtime_adoption_events(
            &RuntimeAdoptionFilter {
                track: Some(track.clone()),
                feature: None,
            },
            10_000,
        )
        .map_err(|error| {
            ErrorData::internal_error(
                format!("failed to list runtime adoption events: {error}"),
                None,
            )
        })?;
    let stats = runtime_adoption_stats(&events);
    let ready = ready_fn(&stats);
    let mut reasons = Vec::new();
    if ready {
        reasons.push("minimum evidence threshold satisfied".to_string());
    } else {
        reasons.push("minimum evidence threshold not satisfied".to_string());
    }
    if stats.rollbacks > 0 {
        reasons.push("rollback signals block default or authority changes".to_string());
    }
    if stats.contradictions > 0 {
        reasons.push("contradiction signals require review before implementation".to_string());
    }
    Ok(Phase3GateDto {
        candidate: candidate.to_string(),
        ready,
        required_track: runtime_adoption_track_slug(&track).to_string(),
        stats,
        reasons,
    })
}

fn validate_research_adapter_plan_value(value: &serde_json::Value) -> ResearchAdapterPlanDto {
    let mut errors = Vec::new();
    let report_id = required_json_string(value, "report_id", &mut errors);
    let title = required_json_string(value, "title", &mut errors);
    let source_count = value
        .get("sources")
        .and_then(serde_json::Value::as_array)
        .map_or(0, Vec::len);
    if source_count == 0 {
        errors.push("sources must contain at least one item".to_string());
    }
    let finding_count = value
        .get("findings")
        .and_then(serde_json::Value::as_array)
        .map_or(0, Vec::len);
    if finding_count == 0 {
        errors.push("findings must contain at least one item".to_string());
    }
    let candidate_insight_count = value
        .get("candidate_insights")
        .and_then(serde_json::Value::as_array)
        .map_or(0, Vec::len);

    ResearchAdapterPlanDto {
        valid: errors.is_empty(),
        report_id,
        title,
        source_count,
        finding_count,
        candidate_insight_count,
        errors,
    }
}

fn required_json_string(
    value: &serde_json::Value,
    field: &'static str,
    errors: &mut Vec<String>,
) -> String {
    match value.get(field).and_then(serde_json::Value::as_str) {
        Some(raw) if !raw.trim().is_empty() => raw.trim().to_string(),
        _ => {
            errors.push(format!("{field} is required"));
            String::new()
        }
    }
}

fn anchor_error(error: anchor::AnchorError) -> ErrorData {
    ErrorData::invalid_params(error.to_string(), None)
}

#[tool_router(router = tool_router)]
impl MempalMcpServer {
    #[tool(
        name = "mempal_status",
        description = "Return schema version, drawer counts, taxonomy counts, database size, scope breakdown, the AAAK format spec, and the memory protocol. Call once at session start if you haven't seen the protocol yet."
    )]
    async fn mempal_status(&self) -> std::result::Result<Json<StatusResponse>, ErrorData> {
        let db = self.open_db()?;
        let schema_version = db.schema_version().map_err(db_error)?;
        let stale_drawer_count = db
            .stale_drawer_count(CURRENT_NORMALIZE_VERSION)
            .map_err(db_error)? as u64;
        let drawer_count = db.drawer_count().map_err(db_error)?;
        let taxonomy_count = db.taxonomy_count().map_err(db_error)?;
        let db_size_bytes = db.database_size_bytes().map_err(db_error)?;
        let diary_rollup_days = db.diary_rollup_days().map_err(db_error)?;
        let scopes = db
            .scope_counts()
            .map_err(db_error)?
            .into_iter()
            .map(|(wing, room, drawer_count)| ScopeCount {
                wing,
                room,
                drawer_count,
            })
            .collect();

        Ok(Json(StatusResponse {
            schema_version,
            normalize_version_current: CURRENT_NORMALIZE_VERSION,
            stale_drawer_count,
            drawer_count,
            taxonomy_count,
            db_size_bytes,
            diary_rollup_days,
            scopes,
            aaak_spec: crate::aaak::generate_spec(),
            memory_protocol: crate::core::protocol::MEMORY_PROTOCOL.to_string(),
        }))
    }

    #[tool(
        name = "mempal_search",
        description = "Search persistent project memory via vector embedding with optional wing/room filters. PREFER THIS over grepping files or guessing from general knowledge when answering ANY project-specific question — past decisions, design rationale, implementation details, bug history, how a component works, why something was built a certain way, or any other project knowledge. Every result includes drawer_id and source_file for citation, plus structured AAAK-derived signals (`entities`, `topics`, `flags`, `emotions`, `importance_stars`) for filtering and ranking."
    )]
    async fn mempal_search(
        &self,
        Parameters(request): Parameters<SearchRequest>,
    ) -> std::result::Result<Json<SearchResponse>, ErrorData> {
        let filters = SearchFilters {
            memory_kind: trim_to_owned(request.memory_kind.as_deref()),
            domain: trim_to_owned(request.domain.as_deref()),
            field: trim_to_owned(request.field.as_deref()),
            tier: trim_to_owned(request.tier.as_deref()),
            status: trim_to_owned(request.status.as_deref()),
            anchor_kind: trim_to_owned(request.anchor_kind.as_deref()),
        };
        let embedder = self.embedder_factory.build().await.map_err(|error| {
            ErrorData::internal_error(format!("failed to build embedder: {error}"), None)
        })?;
        let query_vector = embedder
            .embed(&[request.query.as_str()])
            .await
            .map_err(|error| ErrorData::internal_error(format!("embedding failed: {error}"), None))?
            .into_iter()
            .next()
            .ok_or_else(|| ErrorData::internal_error("embedder returned no query vector", None))?;
        let db = self.open_db()?;
        let route = resolve_route(
            &db,
            &request.query,
            request.wing.as_deref(),
            request.room.as_deref(),
        )
        .map_err(|error| ErrorData::internal_error(format!("routing failed: {error}"), None))?;
        let results = search_with_vector_options(
            &db,
            &request.query,
            &query_vector,
            route,
            SearchOptions {
                filters,
                with_neighbors: request.with_neighbors.unwrap_or(false),
            },
            request.top_k.unwrap_or(10),
        )
        .map_err(|error| ErrorData::internal_error(format!("search failed: {error}"), None))?;

        Ok(Json(SearchResponse {
            results: results
                .into_iter()
                .map(SearchResultDto::with_signals_from_result)
                .collect(),
        }))
    }

    #[tool(
        name = "mempal_context",
        description = "Assemble a mind-model runtime context pack from typed memory. Use this when you need ordered guidance rather than raw search results: dao_tian -> dao_ren -> shu -> qi, with evidence and Phase-2 knowledge cards opt-in. Returns source-backed items with citations and trigger_hints metadata, but never executes skills."
    )]
    async fn mempal_context(
        &self,
        Parameters(request): Parameters<ContextRequest>,
    ) -> std::result::Result<Json<ContextResponse>, ErrorData> {
        let max_items = request.max_items.unwrap_or(12);
        if max_items == 0 {
            return Err(ErrorData::invalid_params(
                "max_items must be greater than 0",
                None,
            ));
        }
        let dao_tian_limit = request.dao_tian_limit.unwrap_or(1);

        let domain = parse_domain(request.domain.as_deref())?.unwrap_or(MemoryDomain::Project);
        let cwd = match request.cwd.as_deref() {
            Some(value) if !value.trim().is_empty() => PathBuf::from(value),
            Some(_) => {
                return Err(ErrorData::invalid_params(
                    "cwd must not be empty when provided",
                    None,
                ));
            }
            None => std::env::current_dir().map_err(|error| {
                ErrorData::internal_error(
                    format!("failed to read current directory: {error}"),
                    None,
                )
            })?,
        };

        let embedder = self.embedder_factory.build().await.map_err(|error| {
            ErrorData::internal_error(format!("failed to build embedder: {error}"), None)
        })?;
        let query_vector = embedder
            .embed(&[request.query.as_str()])
            .await
            .map_err(|error| ErrorData::internal_error(format!("embedding failed: {error}"), None))?
            .into_iter()
            .next()
            .ok_or_else(|| ErrorData::internal_error("embedder returned no query vector", None))?;

        let db = self.open_db()?;
        let pack = assemble_context_with_vector(
            &db,
            crate::context::ContextRequest {
                query: request.query,
                domain,
                field: request
                    .field
                    .unwrap_or_else(|| anchor::DEFAULT_FIELD.to_string()),
                cwd,
                include_evidence: request.include_evidence.unwrap_or(false),
                include_cards: request
                    .include_cards
                    .unwrap_or(self.config.context.include_cards_default),
                max_items,
                dao_tian_limit,
            },
            &query_vector,
        )
        .map_err(context_error)?;

        Ok(Json(ContextResponse::from(pack)))
    }

    #[tool(
        name = "mempal_doctor",
        description = "MCP runtime diagnostics for mempal install/schema compatibility and server-advertised runtime tools. Read-only; does not migrate or create the database."
    )]
    async fn mempal_doctor(
        &self,
        Parameters(_request): Parameters<DoctorRequest>,
    ) -> std::result::Result<Json<DoctorResponse>, ErrorData> {
        let advertised_tools = self.tool_router.list_all();
        let mcp = DoctorMcpDto {
            required_tools: REQUIRED_MCP_TOOLS
                .iter()
                .map(|name| DoctorToolDto {
                    name: (*name).to_string(),
                    advertised: advertised_tools.iter().any(|tool| tool.name == *name),
                })
                .collect(),
            phase3_actions: PHASE3_ACTIONS
                .iter()
                .map(|action| (*action).to_string())
                .collect(),
            cowork_bus_actions: COWORK_BUS_ACTIONS
                .iter()
                .map(|action| (*action).to_string())
                .collect(),
        };
        Ok(Json(DoctorResponse::from_report(
            build_doctor_report(&self.db_path),
            mcp,
        )))
    }

    #[tool(
        name = "mempal_brief",
        description = "Assemble a deterministic citation-first cognitive brief from memory. Returns summary, key facts, evidence, cards, unresolved items, uncertainty, and next actions without LLM synthesis or writes."
    )]
    async fn mempal_brief(
        &self,
        Parameters(request): Parameters<BriefMcpRequest>,
    ) -> std::result::Result<Json<BriefMcpResponse>, ErrorData> {
        let max_items = request.max_items.unwrap_or(12);
        if max_items == 0 {
            return Err(ErrorData::invalid_params(
                "max_items must be greater than 0",
                None,
            ));
        }
        let domain = parse_domain(request.domain.as_deref())?.unwrap_or(MemoryDomain::Project);
        let cwd = match request.cwd.as_deref() {
            Some(value) if !value.trim().is_empty() => PathBuf::from(value),
            Some(_) => {
                return Err(ErrorData::invalid_params(
                    "cwd must not be empty when provided",
                    None,
                ));
            }
            None => std::env::current_dir().map_err(|error| {
                ErrorData::internal_error(
                    format!("failed to read current directory: {error}"),
                    None,
                )
            })?,
        };

        let embedder = self.embedder_factory.build().await.map_err(|error| {
            ErrorData::internal_error(format!("failed to build embedder: {error}"), None)
        })?;
        let query_vector = embedder
            .embed(&[request.query.as_str()])
            .await
            .map_err(|error| ErrorData::internal_error(format!("embedding failed: {error}"), None))?
            .into_iter()
            .next()
            .ok_or_else(|| ErrorData::internal_error("embedder returned no query vector", None))?;
        let db = self.open_db()?;
        let context = assemble_context_with_vector(
            &db,
            crate::context::ContextRequest {
                query: request.query,
                domain,
                field: request
                    .field
                    .unwrap_or_else(|| anchor::DEFAULT_FIELD.to_string()),
                cwd,
                include_evidence: true,
                include_cards: true,
                max_items,
                dao_tian_limit: request.dao_tian_limit.unwrap_or(1),
            },
            &query_vector,
        )
        .map_err(|error| ErrorData::internal_error(format!("brief failed: {error}"), None))?;
        let brief = brief_from_context(context);
        Ok(Json(BriefMcpResponse::from(brief)))
    }

    #[tool(
        name = "mempal_knowledge_distill",
        description = "Create candidate knowledge from existing evidence drawer refs. Deterministic Stage-1 distill: writes memory_kind=knowledge/status=candidate for tier dao_ren or qi, validates refs are evidence drawers, and never calls an LLM, promotes, or creates Phase-2 knowledge cards."
    )]
    async fn mempal_knowledge_distill(
        &self,
        Parameters(request): Parameters<KnowledgeDistillRequest>,
    ) -> std::result::Result<Json<KnowledgeDistillResponse>, ErrorData> {
        let dry_run = request.dry_run.unwrap_or(false);
        let core_request = CoreDistillRequest {
            statement: request.statement,
            content: request.content,
            tier: request.tier,
            supporting_refs: request.supporting_refs,
            wing: request.wing.unwrap_or_else(|| "mempal".to_string()),
            room: request.room.unwrap_or_else(|| "knowledge".to_string()),
            domain: request.domain.unwrap_or_else(|| "project".to_string()),
            field: request
                .field
                .unwrap_or_else(|| anchor::DEFAULT_FIELD.to_string()),
            cwd: request.cwd.map(PathBuf::from),
            scope_constraints: request.scope_constraints,
            counterexample_refs: request.counterexample_refs.unwrap_or_default(),
            teaching_refs: request.teaching_refs.unwrap_or_default(),
            trigger_hints: request.trigger_hints.as_ref().map(trigger_hints_from_dto),
            importance: request.importance.unwrap_or(3),
            dry_run,
        };
        let plan = {
            let db = self.open_db()?;
            prepare_distill(&db, core_request).map_err(knowledge_distill_error)?
        };
        let prepared = match plan {
            DistillPlan::Done(outcome) => return Ok(Json(KnowledgeDistillResponse::from(outcome))),
            DistillPlan::Create(prepared) => prepared,
        };

        let embedder = self.embedder_factory.build().await.map_err(|error| {
            ErrorData::internal_error(format!("failed to build embedder: {error}"), None)
        })?;
        let vector = embedder
            .embed(&[prepared.content.as_str()])
            .await
            .map_err(|error| ErrorData::internal_error(format!("embedding failed: {error}"), None))?
            .into_iter()
            .next()
            .ok_or_else(|| ErrorData::internal_error("embedder returned no vector", None))?;
        let db = self.open_db()?;
        let outcome = commit_distill(&db, *prepared, &vector).map_err(knowledge_distill_error)?;
        Ok(Json(KnowledgeDistillResponse::from(outcome)))
    }

    #[tool(
        name = "mempal_knowledge_gate",
        description = "Read-only promotion readiness check for a knowledge drawer. Evaluates whether dao_tian/dao_ren/shu/qi knowledge has enough supporting, verification, teaching, reviewer, and counterexample evidence for the target status. Does not mutate drawers, vectors, schema, audit logs, or lifecycle state."
    )]
    async fn mempal_knowledge_gate(
        &self,
        Parameters(request): Parameters<KnowledgeGateRequest>,
    ) -> std::result::Result<Json<KnowledgeGateResponse>, ErrorData> {
        let db = self.open_db()?;
        let report = evaluate_gate_by_id(
            &db,
            &request.drawer_id,
            request.target_status.as_deref(),
            request.reviewer.as_deref(),
            request.allow_counterexamples.unwrap_or(false),
        )
        .map_err(knowledge_gate_error)?;

        Ok(Json(KnowledgeGateResponse::from(report)))
    }

    #[tool(
        name = "mempal_knowledge_policy",
        description = "Read-only Stage-1 knowledge promotion policy table. Lists deterministic gate thresholds for dao_tian -> canonical, dao_ren -> promoted, shu -> promoted, and qi -> promoted without requiring a drawer and without mutating storage."
    )]
    async fn mempal_knowledge_policy(
        &self,
    ) -> std::result::Result<Json<KnowledgePolicyResponse>, ErrorData> {
        Ok(Json(KnowledgePolicyResponse::from(promotion_policy())))
    }

    #[tool(
        name = "mempal_knowledge_cards",
        description = "Phase-2 knowledge card inspection, linked-evidence retrieval, and governed lifecycle. Actions: list/get/retrieve/events/gate/promote/demote. Retrieve searches linked evidence and returns active cards with citations; promote/demote require evidence refs and append knowledge_events transactionally."
    )]
    async fn mempal_knowledge_cards(
        &self,
        Parameters(request): Parameters<KnowledgeCardsRequest>,
    ) -> std::result::Result<Json<KnowledgeCardsResponse>, ErrorData> {
        let action = trim_to_option(Some(request.action.as_str()))
            .ok_or_else(|| ErrorData::invalid_params("action must not be empty", None))?;

        match action {
            "list" => {
                let db = self.open_db()?;
                let filter = KnowledgeCardFilter {
                    tier: parse_tier(request.tier.as_deref())?,
                    status: parse_status(request.status.as_deref())?,
                    domain: parse_domain(request.domain.as_deref())?,
                    field: trim_to_owned(request.field.as_deref()),
                    anchor_kind: parse_anchor_kind(request.anchor_kind.as_deref())?,
                    anchor_id: trim_to_owned(request.anchor_id.as_deref()),
                };
                let cards = db.list_knowledge_cards(&filter).map_err(|error| {
                    ErrorData::internal_error(
                        format!("failed to list knowledge cards: {error}"),
                        None,
                    )
                })?;
                Ok(Json(KnowledgeCardsResponse {
                    cards: cards.into_iter().map(KnowledgeCardDto::from).collect(),
                    retrieved: Vec::new(),
                    events: Vec::new(),
                    gate: None,
                    promote: None,
                    demote: None,
                }))
            }
            "get" => {
                let db = self.open_db()?;
                let card_id = required_string(request.card_id.as_deref(), "card_id")?;
                let card = db
                    .get_knowledge_card(card_id)
                    .map_err(|error| {
                        ErrorData::internal_error(
                            format!("failed to get knowledge card: {error}"),
                            None,
                        )
                    })?
                    .ok_or_else(|| {
                        ErrorData::invalid_params(
                            format!("knowledge card not found: {card_id}"),
                            None,
                        )
                    })?;
                Ok(Json(KnowledgeCardsResponse {
                    cards: vec![KnowledgeCardDto::from(card)],
                    retrieved: Vec::new(),
                    events: Vec::new(),
                    gate: None,
                    promote: None,
                    demote: None,
                }))
            }
            "retrieve" => {
                let query = required_string(request.query.as_deref(), "query")?.to_string();
                let top_k = request.top_k.unwrap_or(5);
                if top_k == 0 {
                    return Err(ErrorData::invalid_params(
                        "top_k must be greater than 0",
                        None,
                    ));
                }
                let domain =
                    parse_domain(request.domain.as_deref())?.unwrap_or(MemoryDomain::Project);
                let field = trim_to_owned(request.field.as_deref())
                    .unwrap_or_else(|| "general".to_string());
                let cwd = request
                    .cwd
                    .as_deref()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| {
                        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
                    });
                let embedder = self.embedder_factory.build().await.map_err(|error| {
                    ErrorData::internal_error(format!("failed to build embedder: {error}"), None)
                })?;
                let query_vector = embedder
                    .embed(&[query.as_str()])
                    .await
                    .map_err(|error| {
                        ErrorData::internal_error(format!("embedding failed: {error}"), None)
                    })?
                    .into_iter()
                    .next()
                    .ok_or_else(|| {
                        ErrorData::internal_error("embedder returned no query vector", None)
                    })?;
                let db = self.open_db()?;
                let retrieved = retrieve_knowledge_cards_with_vector(
                    &db,
                    CoreCardRetrievalRequest {
                        query,
                        domain,
                        field,
                        cwd,
                        top_k,
                        evidence_top_k: request.evidence_top_k.unwrap_or(top_k * 4),
                    },
                    &query_vector,
                )
                .map_err(|error| {
                    ErrorData::internal_error(
                        format!("failed to retrieve knowledge cards: {error}"),
                        None,
                    )
                })?;
                Ok(Json(KnowledgeCardsResponse {
                    cards: Vec::new(),
                    retrieved: retrieved
                        .into_iter()
                        .map(RetrievedKnowledgeCardDto::from)
                        .collect(),
                    events: Vec::new(),
                    gate: None,
                    promote: None,
                    demote: None,
                }))
            }
            "events" => {
                let db = self.open_db()?;
                let card_id = required_string(request.card_id.as_deref(), "card_id")?;
                let events = db.knowledge_events(card_id).map_err(|error| {
                    ErrorData::internal_error(
                        format!("failed to list knowledge card events: {error}"),
                        None,
                    )
                })?;
                Ok(Json(KnowledgeCardsResponse {
                    cards: Vec::new(),
                    retrieved: Vec::new(),
                    events: events
                        .into_iter()
                        .map(KnowledgeCardEventDto::from)
                        .collect(),
                    gate: None,
                    promote: None,
                    demote: None,
                }))
            }
            "gate" => {
                let db = self.open_db()?;
                let card_id = required_string(request.card_id.as_deref(), "card_id")?;
                let report = evaluate_card_gate_by_id(
                    &db,
                    card_id,
                    request.target_status.as_deref(),
                    request.reviewer.as_deref(),
                    request.allow_counterexamples.unwrap_or(false),
                )
                .map_err(knowledge_card_lifecycle_error)?;
                Ok(Json(KnowledgeCardsResponse {
                    cards: Vec::new(),
                    retrieved: Vec::new(),
                    events: Vec::new(),
                    gate: Some(report.into()),
                    promote: None,
                    demote: None,
                }))
            }
            "promote" => {
                let db = self.open_db()?;
                let card_id = required_string(request.card_id.as_deref(), "card_id")?;
                let status = required_string(request.status.as_deref(), "status")?.to_string();
                let reason = required_string(request.reason.as_deref(), "reason")?.to_string();
                let verification_refs = request.verification_refs.unwrap_or_default();
                let outcome = promote_card(
                    &db,
                    CorePromoteCardRequest {
                        card_id: card_id.to_string(),
                        status,
                        verification_refs,
                        reason,
                        reviewer: request.reviewer,
                        allow_counterexamples: request.allow_counterexamples.unwrap_or(false),
                        enforce_gate: request.enforce_gate.unwrap_or(true),
                    },
                )
                .map_err(knowledge_card_lifecycle_error)?;
                Ok(Json(KnowledgeCardsResponse {
                    cards: Vec::new(),
                    retrieved: Vec::new(),
                    events: Vec::new(),
                    gate: None,
                    promote: Some(outcome.into()),
                    demote: None,
                }))
            }
            "demote" => {
                let db = self.open_db()?;
                let card_id = required_string(request.card_id.as_deref(), "card_id")?;
                let status = required_string(request.status.as_deref(), "status")?.to_string();
                let reason = required_string(request.reason.as_deref(), "reason")?.to_string();
                let reason_type =
                    required_string(request.reason_type.as_deref(), "reason_type")?.to_string();
                let evidence_refs = request.evidence_refs.unwrap_or_default();
                let outcome = demote_card(
                    &db,
                    CoreDemoteCardRequest {
                        card_id: card_id.to_string(),
                        status,
                        evidence_refs,
                        reason,
                        reason_type,
                    },
                )
                .map_err(knowledge_card_lifecycle_error)?;
                Ok(Json(KnowledgeCardsResponse {
                    cards: Vec::new(),
                    retrieved: Vec::new(),
                    events: Vec::new(),
                    gate: None,
                    promote: None,
                    demote: Some(outcome.into()),
                }))
            }
            other => Err(ErrorData::invalid_params(
                format!(
                    "unsupported knowledge cards action: {other}; actions are list, get, retrieve, events, gate, promote, demote"
                ),
                None,
            )),
        }
    }

    #[tool(
        name = "mempal_phase3",
        description = "Phase-3 runtime adoption evidence and readiness gates. Actions: guidance/instrumentation_policy/prepare_record/capture/evaluator_advise/default_proposal/rollback_control/check_record/record_checked/review/readiness/analytics/record/list/stats/gate/research_validate_plan/research_ingest_plan. Guidance explains when agents should record used/accepted/rejected/miss/rollback signals; instrumentation_policy defines opt-in live instrumentation boundaries without writing; prepare_record validates and returns record inputs without writing; capture maps surface/outcome observations into checked record inputs and writes only with execute=true; evaluator_advise returns deterministic advisory-only evaluator output and a surface=evaluator capture plan without lifecycle authority; default_proposal combines readiness with rollback criteria without changing defaults; rollback_control evaluates card-context rollback evidence without writing; check_record evaluates record quality without writing; record_checked runs the quality gate before writing; review summarizes adoption evidence without writing; readiness evaluates default eligibility without writing; analytics groups adoption evidence by track and feature without writing; record appends runtime_adoption_events; list/stats/gate are read-only; research_validate_plan validates external research report JSON; research_ingest_plan previews evidence drawer refs and distill suggestions without ingesting or promoting knowledge."
    )]
    async fn mempal_phase3(
        &self,
        Parameters(request): Parameters<Phase3Request>,
    ) -> std::result::Result<Json<Phase3Response>, ErrorData> {
        let action = trim_to_option(Some(request.action.as_str()))
            .ok_or_else(|| ErrorData::invalid_params("action must not be empty", None))?;

        match action {
            "guidance" => Ok(Json(Phase3Response {
                guidance: Some(runtime_adoption_guidance().into()),
                instrumentation_policy: None,
                record_plan: None,
                record_quality: None,
                record_checked: None,
                review_report: None,
                readiness_report: None,
                event: None,
                events: Vec::new(),
                stats: None,
                analytics: None,
                gate: None,
                research_plan: None,
                research_ingest_plan: None,
                evaluator_advice: None,
                default_proposal: None,
                rollback_control: None,
            })),
            "instrumentation_policy" => Ok(Json(Phase3Response {
                guidance: None,
                instrumentation_policy: Some(runtime_adoption_instrumentation_policy().into()),
                record_plan: None,
                record_quality: None,
                record_checked: None,
                review_report: None,
                readiness_report: None,
                event: None,
                events: Vec::new(),
                stats: None,
                analytics: None,
                gate: None,
                research_plan: None,
                research_ingest_plan: None,
                evaluator_advice: None,
                default_proposal: None,
                rollback_control: None,
            })),
            "prepare_record" => {
                let track = parse_runtime_adoption_track(required_string(
                    request.track.as_deref(),
                    "track",
                )?)?;
                let signal = parse_runtime_adoption_signal(required_string(
                    request.signal.as_deref(),
                    "signal",
                )?)?;
                let feature = required_string(request.feature.as_deref(), "feature")?.to_string();
                let plan = prepare_runtime_adoption_record(RuntimeAdoptionRecordPlanInput {
                    id: trim_to_owned(request.id.as_deref()),
                    track: runtime_adoption_track_slug(&track).to_string(),
                    signal: runtime_adoption_signal_slug(&signal).to_string(),
                    feature,
                    query: trim_to_owned(request.query.as_deref()),
                    context_hash: trim_to_owned(request.context_hash.as_deref()),
                    card_id: trim_to_owned(request.card_id.as_deref()),
                    evaluator_id: trim_to_owned(request.evaluator_id.as_deref()),
                    research_report_id: trim_to_owned(request.research_report_id.as_deref()),
                    note: trim_to_owned(request.note.as_deref()),
                    metadata: request.metadata,
                });
                Ok(Json(Phase3Response {
                    guidance: None,
                    instrumentation_policy: None,
                    record_plan: Some(plan.into()),
                    record_quality: None,
                    record_checked: None,
                    review_report: None,
                    readiness_report: None,
                    event: None,
                    events: Vec::new(),
                    stats: None,
                    analytics: None,
                    gate: None,
                    research_plan: None,
                    research_ingest_plan: None,
                    evaluator_advice: None,
                    default_proposal: None,
                    rollback_control: None,
                }))
            }
            "capture" => {
                let surface = required_string(request.surface.as_deref(), "surface")?.to_string();
                let outcome = required_string(request.outcome.as_deref(), "outcome")?.to_string();
                let record_input =
                    capture_runtime_adoption_record_input(RuntimeAdoptionCaptureInput {
                        id: trim_to_owned(request.id.as_deref()),
                        surface: surface.clone(),
                        outcome: outcome.clone(),
                        query: trim_to_owned(request.query.as_deref()),
                        context_hash: trim_to_owned(request.context_hash.as_deref()),
                        card_id: trim_to_owned(request.card_id.as_deref()),
                        evaluator_id: trim_to_owned(request.evaluator_id.as_deref()),
                        research_report_id: trim_to_owned(request.research_report_id.as_deref()),
                        note: trim_to_owned(request.note.as_deref()),
                        metadata: request.metadata,
                    })
                    .map_err(|error| ErrorData::invalid_params(error, None))?;
                let mut capture = prepare_runtime_adoption_capture(
                    surface,
                    outcome,
                    request.execute.unwrap_or(false),
                    record_input.clone(),
                );
                if request.execute.unwrap_or(false) {
                    let db = self.open_db()?;
                    let track = parse_runtime_adoption_track(&record_input.track)?;
                    let signal = parse_runtime_adoption_signal(&record_input.signal)?;
                    let should_write = should_write_checked_record(
                        &capture.record_quality,
                        request.allow_warnings.unwrap_or(false),
                    );
                    let event = if should_write {
                        let event = RuntimeAdoptionEvent {
                            id: record_input.id.unwrap_or_else(|| {
                                phase3_event_id(&track, &signal, &record_input.feature)
                            }),
                            track,
                            signal,
                            feature: record_input.feature,
                            query: record_input.query,
                            context_hash: record_input.context_hash,
                            card_id: record_input.card_id,
                            evaluator_id: record_input.evaluator_id,
                            research_report_id: record_input.research_report_id,
                            note: record_input.note,
                            metadata: record_input.metadata,
                            created_at: current_timestamp(),
                        };
                        db.insert_runtime_adoption_event(&event).map_err(|error| {
                            ErrorData::internal_error(
                                format!("failed to insert runtime adoption event: {error}"),
                                None,
                            )
                        })?;
                        Some(event)
                    } else {
                        None
                    };
                    capture.writes = event.is_some();
                    capture.record_checked = Some(RuntimeAdoptionCheckedRecordReport {
                        writes: event.is_some(),
                        blocked: event.is_none(),
                        record_quality: capture.record_quality.clone(),
                        event,
                    });
                }
                Ok(Json(Phase3Response {
                    guidance: None,
                    instrumentation_policy: None,
                    record_plan: Some(capture.record_plan.into()),
                    record_quality: Some(capture.record_quality.into()),
                    record_checked: capture.record_checked.map(Into::into),
                    review_report: None,
                    readiness_report: None,
                    event: None,
                    events: Vec::new(),
                    stats: None,
                    analytics: None,
                    gate: None,
                    research_plan: None,
                    research_ingest_plan: None,
                    evaluator_advice: None,
                    default_proposal: None,
                    rollback_control: None,
                }))
            }
            "evaluator_advise" => {
                let report = evaluator_advice(EvaluatorAdviceInput {
                    evaluator_id: required_string(request.evaluator_id.as_deref(), "evaluator_id")?
                        .to_string(),
                    subject_kind: required_string(request.subject_kind.as_deref(), "subject_kind")?
                        .to_string(),
                    subject_id: required_string(request.subject_id.as_deref(), "subject_id")?
                        .to_string(),
                    proposed_action: required_string(
                        request.proposed_action.as_deref(),
                        "proposed_action",
                    )?
                    .to_string(),
                    evidence_refs: request.evidence_refs.unwrap_or_default(),
                    counterexample_refs: request.counterexample_refs.unwrap_or_default(),
                    risk_notes: request.risk_notes.unwrap_or_default(),
                    note: trim_to_owned(request.note.as_deref()),
                })
                .map_err(|error| ErrorData::invalid_params(error, None))?;
                Ok(Json(Phase3Response {
                    guidance: None,
                    instrumentation_policy: None,
                    record_plan: None,
                    record_quality: None,
                    record_checked: None,
                    review_report: None,
                    readiness_report: None,
                    event: None,
                    events: Vec::new(),
                    stats: None,
                    analytics: None,
                    gate: None,
                    research_plan: None,
                    research_ingest_plan: None,
                    evaluator_advice: Some(report.into()),
                    default_proposal: None,
                    rollback_control: None,
                }))
            }
            "default_proposal" => {
                let candidate = required_string(request.candidate.as_deref(), "candidate")?;
                let report = match candidate {
                    "card-context" => {
                        let db = self.open_db()?;
                        let events = db
                            .list_runtime_adoption_events(
                                &RuntimeAdoptionFilter {
                                    track: Some(RuntimeAdoptionTrack::CardContext),
                                    feature: Some("include_cards".to_string()),
                                },
                                10_000,
                            )
                            .map_err(|error| {
                                ErrorData::internal_error(
                                    format!("failed to list runtime adoption events: {error}"),
                                    None,
                                )
                            })?;
                        card_context_default_proposal(
                            &events,
                            request.rollback_criteria.unwrap_or_default(),
                        )
                    }
                    other => {
                        return Err(ErrorData::invalid_params(
                            format!("unsupported phase3 default proposal candidate: {other}"),
                            None,
                        ));
                    }
                };
                Ok(Json(Phase3Response {
                    guidance: None,
                    instrumentation_policy: None,
                    record_plan: None,
                    record_quality: None,
                    record_checked: None,
                    review_report: None,
                    readiness_report: None,
                    event: None,
                    events: Vec::new(),
                    stats: None,
                    analytics: None,
                    gate: None,
                    research_plan: None,
                    research_ingest_plan: None,
                    evaluator_advice: None,
                    default_proposal: Some(report.into()),
                    rollback_control: None,
                }))
            }
            "rollback_control" => {
                let candidate = required_string(request.candidate.as_deref(), "candidate")?;
                let report = match candidate {
                    "card-context" => {
                        if request.execute.unwrap_or(false) {
                            return Err(ErrorData::invalid_params(
                                "rollback_control execute is only supported by CLI in P79",
                                None,
                            ));
                        }
                        let db = self.open_db()?;
                        let events = db
                            .list_runtime_adoption_events(
                                &RuntimeAdoptionFilter {
                                    track: Some(RuntimeAdoptionTrack::CardContext),
                                    feature: Some("include_cards".to_string()),
                                },
                                10_000,
                            )
                            .map_err(|error| {
                                ErrorData::internal_error(
                                    format!("failed to list runtime adoption events: {error}"),
                                    None,
                                )
                            })?;
                        card_context_rollback_control(
                            &events,
                            self.config.context.include_cards_default,
                            false,
                        )
                    }
                    other => {
                        return Err(ErrorData::invalid_params(
                            format!("unsupported phase3 rollback-control candidate: {other}"),
                            None,
                        ));
                    }
                };
                Ok(Json(Phase3Response {
                    guidance: None,
                    instrumentation_policy: None,
                    record_plan: None,
                    record_quality: None,
                    record_checked: None,
                    review_report: None,
                    readiness_report: None,
                    event: None,
                    events: Vec::new(),
                    stats: None,
                    analytics: None,
                    gate: None,
                    research_plan: None,
                    research_ingest_plan: None,
                    evaluator_advice: None,
                    default_proposal: None,
                    rollback_control: Some(report.into()),
                }))
            }
            "check_record" => {
                let track = parse_runtime_adoption_track(required_string(
                    request.track.as_deref(),
                    "track",
                )?)?;
                let signal = parse_runtime_adoption_signal(required_string(
                    request.signal.as_deref(),
                    "signal",
                )?)?;
                let feature = request.feature.unwrap_or_default();
                let input = RuntimeAdoptionRecordPlanInput {
                    id: trim_to_owned(request.id.as_deref()),
                    track: runtime_adoption_track_slug(&track).to_string(),
                    signal: runtime_adoption_signal_slug(&signal).to_string(),
                    feature,
                    query: trim_to_owned(request.query.as_deref()),
                    context_hash: trim_to_owned(request.context_hash.as_deref()),
                    card_id: trim_to_owned(request.card_id.as_deref()),
                    evaluator_id: trim_to_owned(request.evaluator_id.as_deref()),
                    research_report_id: trim_to_owned(request.research_report_id.as_deref()),
                    note: trim_to_owned(request.note.as_deref()),
                    metadata: request.metadata,
                };
                Ok(Json(Phase3Response {
                    guidance: None,
                    instrumentation_policy: None,
                    record_plan: None,
                    record_quality: Some(check_runtime_adoption_record(&input).into()),
                    record_checked: None,
                    review_report: None,
                    readiness_report: None,
                    event: None,
                    events: Vec::new(),
                    stats: None,
                    analytics: None,
                    gate: None,
                    research_plan: None,
                    research_ingest_plan: None,
                    evaluator_advice: None,
                    default_proposal: None,
                    rollback_control: None,
                }))
            }
            "record_checked" => {
                let db = self.open_db()?;
                let track = parse_runtime_adoption_track(required_string(
                    request.track.as_deref(),
                    "track",
                )?)?;
                let signal = parse_runtime_adoption_signal(required_string(
                    request.signal.as_deref(),
                    "signal",
                )?)?;
                let feature = request.feature.unwrap_or_default();
                let input = RuntimeAdoptionRecordPlanInput {
                    id: trim_to_owned(request.id.as_deref()),
                    track: runtime_adoption_track_slug(&track).to_string(),
                    signal: runtime_adoption_signal_slug(&signal).to_string(),
                    feature,
                    query: trim_to_owned(request.query.as_deref()),
                    context_hash: trim_to_owned(request.context_hash.as_deref()),
                    card_id: trim_to_owned(request.card_id.as_deref()),
                    evaluator_id: trim_to_owned(request.evaluator_id.as_deref()),
                    research_report_id: trim_to_owned(request.research_report_id.as_deref()),
                    note: trim_to_owned(request.note.as_deref()),
                    metadata: request.metadata,
                };
                let quality = check_runtime_adoption_record(&input);
                let should_write =
                    should_write_checked_record(&quality, request.allow_warnings.unwrap_or(false));
                let event = if should_write {
                    let event = RuntimeAdoptionEvent {
                        id: input
                            .id
                            .unwrap_or_else(|| phase3_event_id(&track, &signal, &input.feature)),
                        track,
                        signal,
                        feature: input.feature,
                        query: input.query,
                        context_hash: input.context_hash,
                        card_id: input.card_id,
                        evaluator_id: input.evaluator_id,
                        research_report_id: input.research_report_id,
                        note: input.note,
                        metadata: input.metadata,
                        created_at: current_timestamp(),
                    };
                    db.insert_runtime_adoption_event(&event).map_err(|error| {
                        ErrorData::internal_error(
                            format!("failed to insert runtime adoption event: {error}"),
                            None,
                        )
                    })?;
                    Some(event)
                } else {
                    None
                };
                Ok(Json(Phase3Response {
                    guidance: None,
                    instrumentation_policy: None,
                    record_plan: None,
                    record_quality: None,
                    record_checked: Some(
                        RuntimeAdoptionCheckedRecordReport {
                            writes: event.is_some(),
                            blocked: event.is_none(),
                            record_quality: quality,
                            event,
                        }
                        .into(),
                    ),
                    review_report: None,
                    readiness_report: None,
                    event: None,
                    events: Vec::new(),
                    stats: None,
                    analytics: None,
                    gate: None,
                    research_plan: None,
                    research_ingest_plan: None,
                    evaluator_advice: None,
                    default_proposal: None,
                    rollback_control: None,
                }))
            }
            "review" => {
                let db = self.open_db()?;
                let track = parse_runtime_adoption_track_opt(request.track.as_deref())?;
                let signal = request
                    .signal
                    .as_deref()
                    .map(parse_runtime_adoption_signal)
                    .transpose()?;
                let feature = trim_to_owned(request.feature.as_deref());
                let limit = request.limit.unwrap_or(10_000);
                let events = db
                    .list_runtime_adoption_events(
                        &RuntimeAdoptionFilter {
                            track: track.clone(),
                            feature: feature.clone(),
                        },
                        limit,
                    )
                    .map_err(|error| {
                        ErrorData::internal_error(
                            format!("failed to list runtime adoption events: {error}"),
                            None,
                        )
                    })?;
                let report = review_runtime_adoption_events(
                    &events,
                    RuntimeAdoptionReviewFilters {
                        track: track
                            .as_ref()
                            .map(runtime_adoption_track_slug)
                            .map(str::to_string),
                        feature,
                        signal: signal
                            .as_ref()
                            .map(runtime_adoption_signal_slug)
                            .map(str::to_string),
                        limit,
                    },
                );
                Ok(Json(Phase3Response {
                    guidance: None,
                    instrumentation_policy: None,
                    record_plan: None,
                    record_quality: None,
                    record_checked: None,
                    review_report: Some(report.into()),
                    readiness_report: None,
                    event: None,
                    events: Vec::new(),
                    stats: None,
                    analytics: None,
                    gate: None,
                    research_plan: None,
                    research_ingest_plan: None,
                    evaluator_advice: None,
                    default_proposal: None,
                    rollback_control: None,
                }))
            }
            "readiness" => {
                let db = self.open_db()?;
                let candidate = required_string(request.candidate.as_deref(), "candidate")?;
                let report = match candidate {
                    "card-context-default" => {
                        let events = db
                            .list_runtime_adoption_events(
                                &RuntimeAdoptionFilter {
                                    track: Some(RuntimeAdoptionTrack::CardContext),
                                    feature: Some("include_cards".to_string()),
                                },
                                10_000,
                            )
                            .map_err(|error| {
                                ErrorData::internal_error(
                                    format!("failed to list runtime adoption events: {error}"),
                                    None,
                                )
                            })?;
                        card_context_default_readiness(&events)
                    }
                    other => {
                        return Err(ErrorData::invalid_params(
                            format!("unsupported phase3 readiness candidate: {other}"),
                            None,
                        ));
                    }
                };
                Ok(Json(Phase3Response {
                    guidance: None,
                    instrumentation_policy: None,
                    record_plan: None,
                    record_quality: None,
                    record_checked: None,
                    review_report: None,
                    readiness_report: Some(report.into()),
                    event: None,
                    events: Vec::new(),
                    stats: None,
                    analytics: None,
                    gate: None,
                    research_plan: None,
                    research_ingest_plan: None,
                    evaluator_advice: None,
                    default_proposal: None,
                    rollback_control: None,
                }))
            }
            "record" => {
                let db = self.open_db()?;
                let track = parse_runtime_adoption_track(required_string(
                    request.track.as_deref(),
                    "track",
                )?)?;
                let signal = parse_runtime_adoption_signal(required_string(
                    request.signal.as_deref(),
                    "signal",
                )?)?;
                let feature = required_string(request.feature.as_deref(), "feature")?.to_string();
                let event = RuntimeAdoptionEvent {
                    id: request
                        .id
                        .unwrap_or_else(|| phase3_event_id(&track, &signal, &feature)),
                    track,
                    signal,
                    feature,
                    query: trim_to_owned(request.query.as_deref()),
                    context_hash: trim_to_owned(request.context_hash.as_deref()),
                    card_id: trim_to_owned(request.card_id.as_deref()),
                    evaluator_id: trim_to_owned(request.evaluator_id.as_deref()),
                    research_report_id: trim_to_owned(request.research_report_id.as_deref()),
                    note: trim_to_owned(request.note.as_deref()),
                    metadata: request.metadata,
                    created_at: current_timestamp(),
                };
                db.insert_runtime_adoption_event(&event).map_err(|error| {
                    ErrorData::internal_error(
                        format!("failed to insert runtime adoption event: {error}"),
                        None,
                    )
                })?;
                Ok(Json(Phase3Response {
                    guidance: None,
                    instrumentation_policy: None,
                    record_plan: None,
                    record_quality: None,
                    record_checked: None,
                    review_report: None,
                    readiness_report: None,
                    event: Some(RuntimeAdoptionEventDto::from(event)),
                    events: Vec::new(),
                    stats: None,
                    analytics: None,
                    gate: None,
                    research_plan: None,
                    research_ingest_plan: None,
                    evaluator_advice: None,
                    default_proposal: None,
                    rollback_control: None,
                }))
            }
            "list" => {
                let db = self.open_db()?;
                let events = db
                    .list_runtime_adoption_events(
                        &RuntimeAdoptionFilter {
                            track: parse_runtime_adoption_track_opt(request.track.as_deref())?,
                            feature: trim_to_owned(request.feature.as_deref()),
                        },
                        request.limit.unwrap_or(50),
                    )
                    .map_err(|error| {
                        ErrorData::internal_error(
                            format!("failed to list runtime adoption events: {error}"),
                            None,
                        )
                    })?;
                Ok(Json(Phase3Response {
                    guidance: None,
                    instrumentation_policy: None,
                    record_plan: None,
                    record_quality: None,
                    record_checked: None,
                    review_report: None,
                    readiness_report: None,
                    event: None,
                    events: events
                        .into_iter()
                        .map(RuntimeAdoptionEventDto::from)
                        .collect(),
                    stats: None,
                    analytics: None,
                    gate: None,
                    research_plan: None,
                    research_ingest_plan: None,
                    evaluator_advice: None,
                    default_proposal: None,
                    rollback_control: None,
                }))
            }
            "stats" => {
                let db = self.open_db()?;
                let events = db
                    .list_runtime_adoption_events(
                        &RuntimeAdoptionFilter {
                            track: parse_runtime_adoption_track_opt(request.track.as_deref())?,
                            feature: trim_to_owned(request.feature.as_deref()),
                        },
                        10_000,
                    )
                    .map_err(|error| {
                        ErrorData::internal_error(
                            format!("failed to list runtime adoption events: {error}"),
                            None,
                        )
                    })?;
                Ok(Json(Phase3Response {
                    guidance: None,
                    instrumentation_policy: None,
                    record_plan: None,
                    record_quality: None,
                    record_checked: None,
                    review_report: None,
                    readiness_report: None,
                    event: None,
                    events: Vec::new(),
                    stats: Some(runtime_adoption_stats(&events)),
                    analytics: None,
                    gate: None,
                    research_plan: None,
                    research_ingest_plan: None,
                    evaluator_advice: None,
                    default_proposal: None,
                    rollback_control: None,
                }))
            }
            "analytics" => {
                let db = self.open_db()?;
                let events = db
                    .list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10_000)
                    .map_err(|error| {
                        ErrorData::internal_error(
                            format!("failed to list runtime adoption events: {error}"),
                            None,
                        )
                    })?;
                Ok(Json(Phase3Response {
                    guidance: None,
                    instrumentation_policy: None,
                    record_plan: None,
                    record_quality: None,
                    record_checked: None,
                    review_report: None,
                    readiness_report: None,
                    event: None,
                    events: Vec::new(),
                    stats: None,
                    analytics: Some(build_runtime_adoption_analytics(&events).into()),
                    gate: None,
                    research_plan: None,
                    research_ingest_plan: None,
                    evaluator_advice: None,
                    default_proposal: None,
                    rollback_control: None,
                }))
            }
            "gate" => {
                let db = self.open_db()?;
                let candidate = required_string(request.candidate.as_deref(), "candidate")?;
                let gate = phase3_gate_report(&db, candidate)?;
                Ok(Json(Phase3Response {
                    guidance: None,
                    instrumentation_policy: None,
                    record_plan: None,
                    record_quality: None,
                    record_checked: None,
                    review_report: None,
                    readiness_report: None,
                    event: None,
                    events: Vec::new(),
                    stats: None,
                    analytics: None,
                    gate: Some(gate),
                    research_plan: None,
                    research_ingest_plan: None,
                    evaluator_advice: None,
                    default_proposal: None,
                    rollback_control: None,
                }))
            }
            "research_validate_plan" => {
                let report = request.report.ok_or_else(|| {
                    ErrorData::invalid_params("report is required for research_validate_plan", None)
                })?;
                Ok(Json(Phase3Response {
                    guidance: None,
                    instrumentation_policy: None,
                    record_plan: None,
                    record_quality: None,
                    record_checked: None,
                    review_report: None,
                    readiness_report: None,
                    event: None,
                    events: Vec::new(),
                    stats: None,
                    analytics: None,
                    gate: None,
                    research_plan: Some(validate_research_adapter_plan_value(&report)),
                    research_ingest_plan: None,
                    evaluator_advice: None,
                    default_proposal: None,
                    rollback_control: None,
                }))
            }
            "research_ingest_plan" => {
                let report = request.report.ok_or_else(|| {
                    ErrorData::invalid_params("report is required for research_ingest_plan", None)
                })?;
                Ok(Json(Phase3Response {
                    guidance: None,
                    instrumentation_policy: None,
                    record_plan: None,
                    record_quality: None,
                    record_checked: None,
                    review_report: None,
                    readiness_report: None,
                    event: None,
                    events: Vec::new(),
                    stats: None,
                    analytics: None,
                    gate: None,
                    research_plan: None,
                    research_ingest_plan: Some(ResearchIngestPlanDto::from(
                        build_research_ingest_plan_from_value(&report),
                    )),
                    evaluator_advice: None,
                    default_proposal: None,
                    rollback_control: None,
                }))
            }
            other => Err(ErrorData::invalid_params(
                format!(
                    "unsupported phase3 action: {other}; actions are guidance, instrumentation_policy, prepare_record, capture, evaluator_advise, default_proposal, rollback_control, check_record, record_checked, review, readiness, analytics, record, list, stats, gate, research_validate_plan, research_ingest_plan"
                ),
                None,
            )),
        }
    }

    #[tool(
        name = "mempal_knowledge_promote",
        description = "Promote a knowledge drawer after a deterministic gate pass. Appends verification evidence refs, evaluates promotion readiness, then updates lifecycle status and audit log only if the gate allows it."
    )]
    async fn mempal_knowledge_promote(
        &self,
        Parameters(request): Parameters<KnowledgePromoteRequest>,
    ) -> std::result::Result<Json<KnowledgePromoteResponse>, ErrorData> {
        let db = self.open_db()?;
        let outcome = promote_knowledge(
            &db,
            CorePromoteRequest {
                drawer_id: request.drawer_id,
                status: request.status,
                verification_refs: request.verification_refs,
                reason: request.reason,
                reviewer: request.reviewer,
                allow_counterexamples: request.allow_counterexamples.unwrap_or(false),
                enforce_gate: true,
            },
        )
        .map_err(knowledge_lifecycle_error)?;

        Ok(Json(KnowledgePromoteResponse::from(outcome)))
    }

    #[tool(
        name = "mempal_knowledge_demote",
        description = "Demote or retire a knowledge drawer with counterexample evidence. Appends evidence refs to counterexample_refs, updates lifecycle status, and writes an audit entry without touching vectors or schema."
    )]
    async fn mempal_knowledge_demote(
        &self,
        Parameters(request): Parameters<KnowledgeDemoteRequest>,
    ) -> std::result::Result<Json<KnowledgeDemoteResponse>, ErrorData> {
        let db = self.open_db()?;
        let outcome = demote_knowledge(
            &db,
            CoreDemoteRequest {
                drawer_id: request.drawer_id,
                status: request.status,
                evidence_refs: request.evidence_refs,
                reason: request.reason,
                reason_type: request.reason_type,
            },
        )
        .map_err(knowledge_lifecycle_error)?;

        Ok(Json(KnowledgeDemoteResponse::from(outcome)))
    }

    #[tool(
        name = "mempal_knowledge_publish_anchor",
        description = "Publish active knowledge outward across anchor scope. Metadata-only operation for worktree -> repo or repo -> global publication; updates anchor fields and audit log without touching content, vectors, schema, or tier/status lifecycle."
    )]
    async fn mempal_knowledge_publish_anchor(
        &self,
        Parameters(request): Parameters<KnowledgePublishAnchorRequest>,
    ) -> std::result::Result<Json<KnowledgePublishAnchorResponse>, ErrorData> {
        let db = self.open_db()?;
        let outcome = publish_anchor(
            &db,
            CorePublishAnchorRequest {
                drawer_id: request.drawer_id,
                to: request.to,
                target_anchor_id: request.target_anchor_id,
                cwd: request.cwd.map(PathBuf::from),
                reason: request.reason,
                reviewer: request.reviewer,
            },
        )
        .map_err(knowledge_anchor_error)?;

        Ok(Json(KnowledgePublishAnchorResponse::from(outcome)))
    }

    #[tool(
        name = "mempal_ingest",
        description = "Persist a decision, bug fix, or design insight to project memory. Call this when a decision is reached in conversation — include the rationale, not just the outcome. Wing is required; let mempal auto-route the room. Set dry_run=true to preview the drawer_id without writing."
    )]
    async fn mempal_ingest(
        &self,
        Parameters(request): Parameters<IngestRequest>,
    ) -> std::result::Result<Json<IngestResponse>, ErrorData> {
        let room = request.room.as_deref();
        if request.diary_rollup.unwrap_or(false) {
            validate_ingest_request(&request, &SourceType::Manual)?;
            if request.wing != DIARY_ROLLUP_WING {
                return Err(ingest_error(IngestError::DiaryRollupWrongWing {
                    wing: request.wing,
                }));
            }

            let room = room
                .filter(|room| !room.trim().is_empty())
                .ok_or_else(|| ingest_error(IngestError::DiaryRollupMissingRoom))?;
            let day = crate::ingest::diary::current_rollup_day_utc();
            let drawer_id = diary_rollup_drawer_id(room, &day);

            if request.dry_run.unwrap_or(false) {
                return Ok(Json(IngestResponse {
                    drawer_id,
                    duplicate_warning: None,
                    lock_wait_ms: None,
                }));
            }

            let prepared = {
                let db = self.open_db()?;
                prepare_diary_rollup(
                    &db,
                    &request.content,
                    DIARY_ROLLUP_WING,
                    DiaryRollupOptions {
                        room: Some(room),
                        day: Some(&day),
                        dry_run: false,
                        importance: request.importance.unwrap_or(0),
                    },
                )
                .map_err(ingest_error)?
            };
            let embedder = self.embedder_factory.build().await.map_err(|error| {
                ErrorData::internal_error(format!("failed to build embedder: {error}"), None)
            })?;
            let vector = embedder
                .embed(&[prepared.content.as_str()])
                .await
                .map_err(|error| {
                    ErrorData::internal_error(format!("embedding failed: {error}"), None)
                })?
                .into_iter()
                .next()
                .ok_or_else(|| ErrorData::internal_error("embedder returned no vector", None))?;
            let db = self.open_db()?;
            let outcome =
                commit_prepared_diary_rollup(&db, prepared, &vector).map_err(ingest_error)?;

            return Ok(Json(IngestResponse {
                drawer_id: outcome.drawer_id,
                duplicate_warning: None,
                lock_wait_ms: outcome.stats.lock_wait_ms,
            }));
        }

        let metadata = validate_ingest_request(&request, &SourceType::Manual)?;
        let drawer_id = build_bootstrap_drawer_id_from_parts(
            &request.wing,
            room,
            &request.content,
            metadata.identity_parts(),
        );

        if request.dry_run.unwrap_or(false) {
            return Ok(Json(IngestResponse {
                drawer_id,
                duplicate_warning: None,
                lock_wait_ms: None,
            }));
        }

        let embedder = self.embedder_factory.build().await.map_err(|error| {
            ErrorData::internal_error(format!("failed to build embedder: {error}"), None)
        })?;
        let vector = embedder
            .embed(&[request.content.as_str()])
            .await
            .map_err(|error| ErrorData::internal_error(format!("embedding failed: {error}"), None))?
            .into_iter()
            .next()
            .ok_or_else(|| ErrorData::internal_error("embedder returned no vector", None))?;
        let db = self.open_db()?;

        // P9-B: per-source ingest lock guards the dedup/insert critical
        // section. Lock key derives from the drawer_id (content-addressed,
        // filesystem-safe). Two concurrent mempal_ingest calls with the
        // same content serialize here.
        let mempal_home = db
            .path()
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        let lock_guard = crate::ingest::lock::acquire_source_lock(
            &mempal_home,
            &drawer_id,
            std::time::Duration::from_secs(5),
        )
        .map_err(|e| ErrorData::internal_error(format!("ingest lock: {e}"), None))?;
        let lock_wait_ms = Some(lock_guard.wait_duration().as_millis() as u64);

        // Semantic dedup check: find most similar existing drawer
        let duplicate_warning = check_semantic_duplicate(&db, &vector, &request.content);

        if !db.drawer_exists(&drawer_id).map_err(db_error)? {
            let source_file = match metadata.memory_kind {
                MemoryKind::Evidence => {
                    source_file_or_synthetic(&drawer_id, request.source.as_deref())
                }
                MemoryKind::Knowledge => knowledge_source_file(
                    &metadata.domain,
                    &metadata.field,
                    metadata.tier.as_ref().expect("knowledge tier validated"),
                    metadata
                        .statement
                        .as_deref()
                        .expect("knowledge statement validated"),
                ),
            };
            let drawer = Drawer {
                id: drawer_id.clone(),
                content: request.content,
                wing: request.wing,
                room: request.room,
                source_file: Some(source_file),
                source_type: SourceType::Manual,
                added_at: current_timestamp(),
                chunk_index: Some(0),
                normalize_version: CURRENT_NORMALIZE_VERSION,
                importance: request.importance.unwrap_or(0),
                memory_kind: metadata.memory_kind,
                domain: metadata.domain,
                field: metadata.field,
                anchor_kind: metadata.anchor_kind,
                anchor_id: metadata.anchor_id,
                parent_anchor_id: metadata.parent_anchor_id,
                provenance: metadata.provenance,
                statement: metadata.statement,
                tier: metadata.tier,
                status: metadata.status,
                supporting_refs: metadata.supporting_refs,
                counterexample_refs: metadata.counterexample_refs,
                teaching_refs: metadata.teaching_refs,
                verification_refs: metadata.verification_refs,
                scope_constraints: metadata.scope_constraints,
                trigger_hints: metadata.trigger_hints,
            };
            db.insert_drawer(&drawer).map_err(db_error)?;
            db.insert_vector(&drawer_id, &vector).map_err(db_error)?;
        }

        // lock_guard drops here, releasing the advisory lock.
        drop(lock_guard);

        Ok(Json(IngestResponse {
            drawer_id,
            duplicate_warning,
            lock_wait_ms,
        }))
    }

    #[tool(
        name = "mempal_delete",
        description = "Soft-delete a drawer by ID. The drawer is marked with a deleted_at timestamp and excluded from search results, but not physically removed. Use the CLI `mempal purge` to permanently remove soft-deleted drawers. Returns the drawer_id and whether it was found."
    )]
    async fn mempal_delete(
        &self,
        Parameters(request): Parameters<DeleteRequest>,
    ) -> std::result::Result<Json<DeleteResponse>, ErrorData> {
        let db = self.open_db()?;
        let deleted = db
            .soft_delete_drawer(&request.drawer_id)
            .map_err(db_error)?;
        let message = if deleted {
            format!("drawer {} soft-deleted", request.drawer_id)
        } else {
            format!("drawer {} not found or already deleted", request.drawer_id)
        };
        Ok(Json(DeleteResponse {
            drawer_id: request.drawer_id,
            deleted,
            message,
        }))
    }

    #[tool(
        name = "mempal_taxonomy",
        description = "List or edit wing/room taxonomy entries that drive query routing keywords."
    )]
    async fn mempal_taxonomy(
        &self,
        Parameters(request): Parameters<TaxonomyRequest>,
    ) -> std::result::Result<Json<TaxonomyResponse>, ErrorData> {
        let db = self.open_db()?;
        match request.action.as_str() {
            "list" => {
                let entries = db
                    .taxonomy_entries()
                    .map_err(db_error)?
                    .into_iter()
                    .map(TaxonomyEntryDto::from)
                    .collect();
                Ok(Json(TaxonomyResponse {
                    action: "list".to_string(),
                    entries,
                }))
            }
            "edit" => {
                let wing = request
                    .wing
                    .ok_or_else(|| ErrorData::invalid_params("missing wing", None))?;
                let room = request
                    .room
                    .ok_or_else(|| ErrorData::invalid_params("missing room", None))?;
                let keywords = request
                    .keywords
                    .ok_or_else(|| ErrorData::invalid_params("missing keywords", None))?;
                let entry = crate::core::types::TaxonomyEntry {
                    wing,
                    room,
                    display_name: None,
                    keywords,
                };
                db.upsert_taxonomy_entry(&entry).map_err(db_error)?;
                Ok(Json(TaxonomyResponse {
                    action: "edit".to_string(),
                    entries: vec![TaxonomyEntryDto::from(entry)],
                }))
            }
            action => Err(ErrorData::invalid_params(
                format!("unsupported taxonomy action: {action}"),
                None,
            )),
        }
    }

    #[tool(
        name = "mempal_field_taxonomy",
        description = "Read-only mind-model field taxonomy guidance. Lists recommended Stage-1 field values such as general, epistemics, software-engineering, debugging, tooling, research, writing, and diary. Guidance only; custom fields remain accepted."
    )]
    async fn mempal_field_taxonomy(
        &self,
    ) -> std::result::Result<Json<FieldTaxonomyResponse>, ErrorData> {
        Ok(Json(FieldTaxonomyResponse {
            entries: field_taxonomy()
                .into_iter()
                .map(FieldTaxonomyEntryDto::from)
                .collect(),
        }))
    }

    #[tool(
        name = "mempal_kg",
        description = "Knowledge graph: add, query, or invalidate triples (subject-predicate-object). Use 'add' to record structured relationships between entities. Use 'query' to find relationships by subject, predicate, or object. Use 'invalidate' to mark a triple as no longer valid."
    )]
    async fn mempal_kg(
        &self,
        Parameters(request): Parameters<KgRequest>,
    ) -> std::result::Result<Json<KgResponse>, ErrorData> {
        let db = self.open_db()?;
        match request.action.as_str() {
            "add" => {
                let subject = request
                    .subject
                    .ok_or_else(|| ErrorData::invalid_params("missing subject", None))?;
                let predicate = request
                    .predicate
                    .ok_or_else(|| ErrorData::invalid_params("missing predicate", None))?;
                let object = request
                    .object
                    .ok_or_else(|| ErrorData::invalid_params("missing object", None))?;
                let id = build_triple_id(&subject, &predicate, &object);
                let triple = Triple {
                    id: id.clone(),
                    subject,
                    predicate,
                    object,
                    valid_from: Some(current_timestamp()),
                    valid_to: None,
                    confidence: 1.0,
                    source_drawer: request.source_drawer,
                };
                db.insert_triple(&triple).map_err(db_error)?;
                Ok(Json(KgResponse {
                    action: "add".to_string(),
                    triples: vec![triple_to_dto(&triple)],
                    stats: None,
                }))
            }
            "query" => {
                let active_only = request.active_only.unwrap_or(true);
                let triples = db
                    .query_triples(
                        request.subject.as_deref(),
                        request.predicate.as_deref(),
                        request.object.as_deref(),
                        active_only,
                    )
                    .map_err(db_error)?;
                Ok(Json(KgResponse {
                    action: "query".to_string(),
                    triples: triples.iter().map(triple_to_dto).collect(),
                    stats: None,
                }))
            }
            "invalidate" => {
                let triple_id = request
                    .triple_id
                    .ok_or_else(|| ErrorData::invalid_params("missing triple_id", None))?;
                let invalidated = db.invalidate_triple(&triple_id).map_err(db_error)?;
                let message = if invalidated {
                    format!("triple {triple_id} invalidated")
                } else {
                    format!("triple {triple_id} not found or already invalidated")
                };
                Ok(Json(KgResponse {
                    action: message,
                    triples: vec![],
                    stats: None,
                }))
            }
            "timeline" => {
                let entity = request.subject.ok_or_else(|| {
                    ErrorData::invalid_params("missing subject for timeline", None)
                })?;
                let triples = db.timeline_for_entity(&entity).map_err(db_error)?;
                Ok(Json(KgResponse {
                    action: format!("timeline for {entity}"),
                    triples: triples.iter().map(triple_to_dto).collect(),
                    stats: None,
                }))
            }
            "stats" => {
                let stats = db.triple_stats().map_err(db_error)?;
                Ok(Json(KgResponse {
                    action: "stats".to_string(),
                    triples: vec![],
                    stats: Some(KgStatsDto {
                        total: stats.total,
                        active: stats.active,
                        expired: stats.expired,
                        entities: stats.entities,
                        top_predicates: stats.top_predicates,
                    }),
                }))
            }
            action => Err(ErrorData::invalid_params(
                format!("unsupported kg action: {action}"),
                None,
            )),
        }
    }

    #[tool(
        name = "mempal_tunnels",
        description = "Discover or manage cross-wing tunnels. Actions: discover/list passive same-room links, add/list/delete/follow explicit semantic links."
    )]
    async fn mempal_tunnels(
        &self,
        Parameters(request): Parameters<TunnelsRequest>,
    ) -> std::result::Result<Json<TunnelsResponse>, ErrorData> {
        let db = self.open_db()?;
        let action = request.action.as_deref().unwrap_or("discover");
        match action {
            "discover" => Ok(Json(TunnelsResponse {
                tunnels: passive_tunnel_dtos(&db, request.wing.as_deref())?,
            })),
            "list" => {
                let kind = request.kind.as_deref().unwrap_or("all");
                let mut tunnels = Vec::new();
                if matches!(kind, "all" | "passive") {
                    tunnels.extend(passive_tunnel_dtos(&db, request.wing.as_deref())?);
                }
                if matches!(kind, "all" | "explicit") {
                    tunnels.extend(
                        db.list_explicit_tunnels(request.wing.as_deref())
                            .map_err(db_error)?
                            .iter()
                            .map(explicit_tunnel_to_dto),
                    );
                }
                if !matches!(kind, "all" | "passive" | "explicit") {
                    return Err(ErrorData::invalid_params(
                        format!("unsupported tunnel kind: {kind}"),
                        None,
                    ));
                }
                Ok(Json(TunnelsResponse { tunnels }))
            }
            "add" => {
                let left = request
                    .left
                    .ok_or_else(|| ErrorData::invalid_params("missing left endpoint", None))?;
                let right = request
                    .right
                    .ok_or_else(|| ErrorData::invalid_params("missing right endpoint", None))?;
                let label = trim_to_option(request.label.as_deref())
                    .ok_or_else(|| ErrorData::invalid_params("missing label", None))?;
                let created_by = self
                    .client_name
                    .lock()
                    .map_err(|_| ErrorData::internal_error("client name lock poisoned", None))?
                    .clone();
                let tunnel = db
                    .create_tunnel(&left.into(), &right.into(), label, created_by.as_deref())
                    .map_err(db_error)?;
                Ok(Json(TunnelsResponse {
                    tunnels: vec![explicit_tunnel_to_dto(&tunnel)],
                }))
            }
            "delete" => {
                let tunnel_id = trim_to_option(request.tunnel_id.as_deref())
                    .ok_or_else(|| ErrorData::invalid_params("missing tunnel_id", None))?;
                if tunnel_id.starts_with("passive_") {
                    return Err(ErrorData::invalid_params(
                        "cannot delete passive tunnel",
                        None,
                    ));
                }
                if !db.delete_explicit_tunnel(tunnel_id).map_err(db_error)? {
                    return Err(ErrorData::invalid_params(
                        format!("tunnel not found: {tunnel_id}"),
                        None,
                    ));
                }
                Ok(Json(TunnelsResponse {
                    tunnels: Vec::new(),
                }))
            }
            "follow" => {
                let from = request
                    .from
                    .ok_or_else(|| ErrorData::invalid_params("missing from endpoint", None))?;
                let max_hops = request.max_hops.unwrap_or(1);
                if !(1..=2).contains(&max_hops) {
                    return Err(ErrorData::invalid_params("max_hops must be 1 or 2", None));
                }
                let tunnels = db
                    .follow_explicit_tunnels(&from.into(), max_hops)
                    .map_err(db_error)?
                    .into_iter()
                    .map(|result| TunnelDto {
                        tunnel_id: result.via_tunnel_id.clone(),
                        kind: "explicit".to_string(),
                        room: None,
                        wings: Vec::new(),
                        left: Some(TunnelEndpointDto::from(&result.endpoint)),
                        right: None,
                        label: None,
                        created_at: None,
                        created_by: None,
                        via_tunnel_id: Some(result.via_tunnel_id),
                        hop: Some(result.hop),
                    })
                    .collect();
                Ok(Json(TunnelsResponse { tunnels }))
            }
            other => Err(ErrorData::invalid_params(
                format!("unsupported tunnels action: {other}"),
                None,
            )),
        }
    }

    #[tool(
        name = "mempal_peek_partner",
        description = "Read the partner coding agent's LIVE session log (Claude Code ↔ Codex) without storing it in mempal. Returns the most recent user+assistant messages from their active session file. Use this for CURRENT partner state; use mempal_search for CRYSTALLIZED past decisions. Peek is a pure read — it never writes to mempal drawers. Pass tool=\"auto\" to infer the partner from MCP ClientInfo, or tool=\"claude\"/\"codex\" explicitly."
    )]
    async fn mempal_peek_partner(
        &self,
        Parameters(request): Parameters<PeekPartnerRequest>,
    ) -> std::result::Result<Json<PeekPartnerResponse>, ErrorData> {
        let tool = Tool::from_str_ci(&request.tool).ok_or_else(|| {
            ErrorData::invalid_params(
                format!(
                    "unknown tool `{}`: expected claude|codex|auto",
                    request.tool
                ),
                None,
            )
        })?;

        let caller_tool = self
            .client_name
            .lock()
            .ok()
            .and_then(|g| g.clone())
            .and_then(|n| Tool::from_str_ci(&n));

        let cwd = std::env::current_dir()
            .map_err(|e| ErrorData::internal_error(format!("cwd unavailable: {e}"), None))?;

        let cowork_req = CoworkPeekRequest {
            tool,
            limit: request.limit.unwrap_or(30),
            since: request.since,
            cwd,
            caller_tool,
            home_override: None,
        };

        let resp = peek_partner(cowork_req).map_err(|e| match e {
            PeekError::CannotInferPartner | PeekError::SelfPeek => {
                ErrorData::invalid_params(e.to_string(), None)
            }
            PeekError::Io(_) | PeekError::Parse(_) => {
                ErrorData::internal_error(e.to_string(), None)
            }
        })?;

        Ok(Json(PeekPartnerResponse {
            partner_tool: resp.partner_tool.as_str().to_string(),
            session_path: resp.session_path,
            session_mtime: resp.session_mtime,
            partner_active: resp.partner_active,
            messages: resp
                .messages
                .into_iter()
                .map(PeekMessageDto::from)
                .collect(),
            truncated: resp.truncated,
        }))
    }

    #[tool(
        name = "mempal_cowork_push",
        description = "Proactively deliver a short handoff message to the PARTNER agent's inbox. \
                       Partner reads it at their next UserPromptSubmit hook, NOT real-time. \
                       Use for transient handoffs too important for mempal_peek_partner \
                       and too ephemeral for mempal_ingest. Max 8 KB per message; total inbox \
                       capped at 32 KB / 16 messages (InboxFull error means partner must drain). \
                       Pass target_tool=\"claude\"/\"codex\" explicitly, or omit to infer partner \
                       from MCP client identity. Self-push is rejected."
    )]
    async fn mempal_cowork_push(
        &self,
        Parameters(request): Parameters<CoworkPushRequest>,
    ) -> std::result::Result<Json<CoworkPushResponse>, ErrorData> {
        let caller_name = self.client_name.lock().ok().and_then(|g| g.clone());
        let caller_tool = caller_name
            .as_deref()
            .and_then(Tool::from_str_ci)
            .ok_or_else(|| {
                ErrorData::invalid_params(
                    "cannot infer caller tool from MCP client info (client_name missing or unrecognized)",
                    None,
                )
            })?;

        let target = match request.target_tool.as_deref() {
            Some(name) => Tool::from_target_str(name).ok_or_else(|| {
                ErrorData::invalid_params(
                    format!("unknown target_tool `{name}`: expected claude|codex"),
                    None,
                )
            })?,
            None => caller_tool.partner().ok_or_else(|| {
                ErrorData::invalid_params("caller tool has no partner (tool=auto or unknown)", None)
            })?,
        };

        let mempal_home = crate::cowork::inbox::mempal_home();
        let cwd = PathBuf::from(&request.cwd);
        let pushed_at = current_rfc3339();

        let (path, size) = crate::cowork::inbox::push(
            &mempal_home,
            caller_tool,
            target,
            &cwd,
            request.content,
            pushed_at.clone(),
        )
        .map_err(|e| match e {
            crate::cowork::inbox::InboxError::SelfPush(_)
            | crate::cowork::inbox::InboxError::MessageTooLarge(_)
            | crate::cowork::inbox::InboxError::InvalidCwd(_)
            | crate::cowork::inbox::InboxError::InboxFull { .. } => {
                ErrorData::invalid_params(e.to_string(), None)
            }
            _ => ErrorData::internal_error(e.to_string(), None),
        })?;

        Ok(Json(CoworkPushResponse {
            target_tool: target.dir_name().to_string(),
            inbox_path: path.to_string_lossy().to_string(),
            pushed_at,
            inbox_size_after: size,
        }))
    }

    #[tool(
        name = "mempal_cowork_bus",
        description = "Multi-agent cowork bus for concrete agent instances in one project. \
                       Actions: register/list/send/broadcast/drain/events/deliveries/ack/heartbeat/channel_set/channel_list/channel_send/tmux_peek/doctor/session_create/session_list/session_status/session_close/handoff/capture. Uses explicit agent_id \
                       values such as claude-main, codex-a, codex-b, per-agent inbox files, \
                       and append-only events under ~/.mempal/cowork-bus/<project>. This is separate from legacy \
                       mempal_cowork_push partner routing and does not infer concrete instances \
                       from MCP client names. Most actions are file-backed runtime ops; action=capture writes \
                       evidence only when execute=true."
    )]
    async fn mempal_cowork_bus(
        &self,
        Parameters(request): Parameters<CoworkBusRequest>,
    ) -> std::result::Result<Json<CoworkBusResponse>, ErrorData> {
        use crate::cowork::bus::{self, RegisterAgentRequest, SendRequest};

        let mempal_home = crate::cowork::inbox::mempal_home();
        let cwd = PathBuf::from(&request.cwd);
        let action = request.action.as_str();

        match action {
            "register" => {
                let agent_id = required_bus_field(request.agent_id, "agent_id", action)?;
                let tool = required_bus_field(request.tool, "tool", action)?;
                let record = bus::register_agent(
                    &mempal_home,
                    &cwd,
                    RegisterAgentRequest {
                        agent_id,
                        tool,
                        transport: request.transport.unwrap_or_else(|| "inbox".to_string()),
                        tmux_target: request.tmux_target,
                    },
                )
                .map_err(bus_error_to_mcp)?;
                Ok(Json(CoworkBusResponse {
                    action: action.to_string(),
                    agents: vec![agent_record_to_dto(record)],
                    delivered: Vec::new(),
                    messages: Vec::new(),
                    events: Vec::new(),
                    deliveries: Vec::new(),
                    channels: Vec::new(),
                    tmux_peek: None,
                    doctor: None,
                    sessions: Vec::new(),
                    handoff: None,
                    capture: None,
                }))
            }
            "list" => {
                let statuses =
                    bus::list_agent_status_at(&mempal_home, &cwd, request.now.as_deref())
                        .map_err(bus_error_to_mcp)?
                        .into_iter()
                        .map(agent_status_to_dto)
                        .collect();
                Ok(Json(CoworkBusResponse {
                    action: action.to_string(),
                    agents: statuses,
                    delivered: Vec::new(),
                    messages: Vec::new(),
                    events: Vec::new(),
                    deliveries: Vec::new(),
                    channels: Vec::new(),
                    tmux_peek: None,
                    doctor: None,
                    sessions: Vec::new(),
                    handoff: None,
                    capture: None,
                }))
            }
            "send" | "broadcast" => {
                let from = required_bus_field(request.from, "from", action)?;
                if request.to.is_empty() {
                    return Err(ErrorData::invalid_params(
                        format!("action `{action}` requires at least one `to` agent_id"),
                        None,
                    ));
                }
                if action == "send" && request.to.len() != 1 {
                    return Err(ErrorData::invalid_params(
                        "action `send` requires exactly one `to`; use broadcast for fanout",
                        None,
                    ));
                }
                let message = required_bus_field(request.message, "message", action)?;
                let report = bus::send(
                    &mempal_home,
                    &cwd,
                    SendRequest {
                        from,
                        targets: request.to,
                        message,
                        operation: if action == "send" {
                            bus::SendOperation::Send
                        } else {
                            bus::SendOperation::Broadcast
                        },
                        thread_id: request.thread_id,
                        channel: request.channel,
                    },
                )
                .map_err(bus_error_to_mcp)?;
                Ok(Json(CoworkBusResponse {
                    action: action.to_string(),
                    agents: Vec::new(),
                    delivered: report.delivered.into_iter().map(delivery_to_dto).collect(),
                    messages: Vec::new(),
                    events: Vec::new(),
                    deliveries: Vec::new(),
                    channels: Vec::new(),
                    tmux_peek: None,
                    doctor: None,
                    sessions: Vec::new(),
                    handoff: None,
                    capture: None,
                }))
            }
            "drain" => {
                let agent_id = required_bus_field(request.agent_id, "agent_id", action)?;
                let messages = bus::drain_agent(&mempal_home, &cwd, &agent_id)
                    .map_err(bus_error_to_mcp)?
                    .into_iter()
                    .map(message_to_dto)
                    .collect();
                Ok(Json(CoworkBusResponse {
                    action: action.to_string(),
                    agents: Vec::new(),
                    delivered: Vec::new(),
                    messages,
                    events: Vec::new(),
                    deliveries: Vec::new(),
                    channels: Vec::new(),
                    tmux_peek: None,
                    doctor: None,
                    sessions: Vec::new(),
                    handoff: None,
                    capture: None,
                }))
            }
            "events" => {
                let events = bus::list_events(&mempal_home, &cwd, request.limit)
                    .map_err(bus_error_to_mcp)?
                    .into_iter()
                    .map(event_to_dto)
                    .collect();
                Ok(Json(CoworkBusResponse {
                    action: action.to_string(),
                    agents: Vec::new(),
                    delivered: Vec::new(),
                    messages: Vec::new(),
                    events,
                    deliveries: Vec::new(),
                    channels: Vec::new(),
                    tmux_peek: None,
                    doctor: None,
                    sessions: Vec::new(),
                    handoff: None,
                    capture: None,
                }))
            }
            "deliveries" => {
                let deliveries =
                    bus::list_delivery_statuses(&mempal_home, &cwd, request.agent_id.as_deref())
                        .map_err(bus_error_to_mcp)?
                        .into_iter()
                        .map(delivery_status_to_dto)
                        .collect();
                Ok(Json(CoworkBusResponse {
                    action: action.to_string(),
                    agents: Vec::new(),
                    delivered: Vec::new(),
                    messages: Vec::new(),
                    events: Vec::new(),
                    deliveries,
                    channels: Vec::new(),
                    tmux_peek: None,
                    doctor: None,
                    sessions: Vec::new(),
                    handoff: None,
                    capture: None,
                }))
            }
            "ack" => {
                let agent_id = required_bus_field(request.agent_id, "agent_id", action)?;
                let message_id = required_bus_field(request.message_id, "message_id", action)?;
                let status = bus::ack_delivery(&mempal_home, &cwd, &agent_id, &message_id)
                    .map_err(bus_error_to_mcp)?;
                Ok(Json(CoworkBusResponse {
                    action: action.to_string(),
                    agents: Vec::new(),
                    delivered: Vec::new(),
                    messages: Vec::new(),
                    events: Vec::new(),
                    deliveries: vec![delivery_status_to_dto(status)],
                    channels: Vec::new(),
                    tmux_peek: None,
                    doctor: None,
                    sessions: Vec::new(),
                    handoff: None,
                    capture: None,
                }))
            }
            "heartbeat" => {
                let agent_id = required_bus_field(request.agent_id, "agent_id", action)?;
                let record =
                    bus::heartbeat_agent(&mempal_home, &cwd, &agent_id, request.seen_at.as_deref())
                        .map_err(bus_error_to_mcp)?;
                Ok(Json(CoworkBusResponse {
                    action: action.to_string(),
                    agents: vec![agent_record_to_dto(record)],
                    delivered: Vec::new(),
                    messages: Vec::new(),
                    events: Vec::new(),
                    deliveries: Vec::new(),
                    channels: Vec::new(),
                    tmux_peek: None,
                    doctor: None,
                    sessions: Vec::new(),
                    handoff: None,
                    capture: None,
                }))
            }
            "channel_set" => {
                let channel = required_bus_field(request.channel, "channel", action)?;
                if request.agents.is_empty() {
                    return Err(ErrorData::invalid_params(
                        "action `channel_set` requires at least one `agents` entry",
                        None,
                    ));
                }
                let channel = bus::set_channel(&mempal_home, &cwd, &channel, request.agents)
                    .map_err(bus_error_to_mcp)?;
                Ok(Json(CoworkBusResponse {
                    action: action.to_string(),
                    agents: Vec::new(),
                    delivered: Vec::new(),
                    messages: Vec::new(),
                    events: Vec::new(),
                    deliveries: Vec::new(),
                    channels: vec![channel_to_dto(channel)],
                    tmux_peek: None,
                    doctor: None,
                    sessions: Vec::new(),
                    handoff: None,
                    capture: None,
                }))
            }
            "channel_list" => {
                let channels = bus::list_channels(&mempal_home, &cwd)
                    .map_err(bus_error_to_mcp)?
                    .into_iter()
                    .map(channel_to_dto)
                    .collect();
                Ok(Json(CoworkBusResponse {
                    action: action.to_string(),
                    agents: Vec::new(),
                    delivered: Vec::new(),
                    messages: Vec::new(),
                    events: Vec::new(),
                    deliveries: Vec::new(),
                    channels,
                    tmux_peek: None,
                    doctor: None,
                    sessions: Vec::new(),
                    handoff: None,
                    capture: None,
                }))
            }
            "channel_send" => {
                let from = required_bus_field(request.from, "from", action)?;
                let channel = required_bus_field(request.channel, "channel", action)?;
                let message = required_bus_field(request.message, "message", action)?;
                let report = bus::send_channel(
                    &mempal_home,
                    &cwd,
                    from,
                    channel,
                    message,
                    request.thread_id,
                )
                .map_err(bus_error_to_mcp)?;
                Ok(Json(CoworkBusResponse {
                    action: action.to_string(),
                    agents: Vec::new(),
                    delivered: report.delivered.into_iter().map(delivery_to_dto).collect(),
                    messages: Vec::new(),
                    events: Vec::new(),
                    deliveries: Vec::new(),
                    channels: Vec::new(),
                    tmux_peek: None,
                    doctor: None,
                    sessions: Vec::new(),
                    handoff: None,
                    capture: None,
                }))
            }
            "tmux_peek" => {
                let agent_id = required_bus_field(request.agent_id, "agent_id", action)?;
                let peek = bus::tmux_peek_agent(
                    &mempal_home,
                    &cwd,
                    &agent_id,
                    request.lines.unwrap_or(80),
                )
                .map_err(bus_error_to_mcp)?;
                Ok(Json(CoworkBusResponse {
                    action: action.to_string(),
                    agents: Vec::new(),
                    delivered: Vec::new(),
                    messages: Vec::new(),
                    events: Vec::new(),
                    deliveries: Vec::new(),
                    channels: Vec::new(),
                    tmux_peek: Some(tmux_peek_to_dto(peek)),
                    doctor: None,
                    sessions: Vec::new(),
                    handoff: None,
                    capture: None,
                }))
            }
            "doctor" => {
                let report = bus::doctor(
                    &mempal_home,
                    &cwd,
                    request.now.as_deref(),
                    request.probe_tmux.unwrap_or(false),
                )
                .map_err(bus_error_to_mcp)?;
                Ok(Json(CoworkBusResponse {
                    action: action.to_string(),
                    agents: Vec::new(),
                    delivered: Vec::new(),
                    messages: Vec::new(),
                    events: Vec::new(),
                    deliveries: Vec::new(),
                    channels: Vec::new(),
                    tmux_peek: None,
                    doctor: Some(doctor_to_dto(report)),
                    sessions: Vec::new(),
                    handoff: None,
                    capture: None,
                }))
            }
            "session_create" => {
                let session_id = required_bus_field(request.session_id, "session_id", action)?;
                let title = required_bus_field(request.title, "title", action)?;
                if request.agents.is_empty() {
                    return Err(ErrorData::invalid_params(
                        "action `session_create` requires at least one `agents` entry",
                        None,
                    ));
                }
                let session = bus::create_session(
                    &mempal_home,
                    &cwd,
                    bus::CreateSessionRequest {
                        session_id,
                        title,
                        goal: request.goal,
                        agents: request.agents,
                        channels: if let Some(channel) = request.channel {
                            vec![channel]
                        } else {
                            Vec::new()
                        },
                        thread_id: request.thread_id,
                    },
                )
                .map_err(bus_error_to_mcp)?;
                Ok(Json(CoworkBusResponse {
                    action: action.to_string(),
                    agents: Vec::new(),
                    delivered: Vec::new(),
                    messages: Vec::new(),
                    events: Vec::new(),
                    deliveries: Vec::new(),
                    channels: Vec::new(),
                    tmux_peek: None,
                    doctor: None,
                    sessions: vec![session_to_dto(session)],
                    handoff: None,
                    capture: None,
                }))
            }
            "session_list" => {
                let sessions = bus::list_sessions(&mempal_home, &cwd)
                    .map_err(bus_error_to_mcp)?
                    .into_iter()
                    .map(session_to_dto)
                    .collect();
                Ok(Json(CoworkBusResponse {
                    action: action.to_string(),
                    agents: Vec::new(),
                    delivered: Vec::new(),
                    messages: Vec::new(),
                    events: Vec::new(),
                    deliveries: Vec::new(),
                    channels: Vec::new(),
                    tmux_peek: None,
                    doctor: None,
                    sessions,
                    handoff: None,
                    capture: None,
                }))
            }
            "session_status" => {
                let session_id = required_bus_field(request.session_id, "session_id", action)?;
                let status = required_bus_field(request.status, "status", action)?;
                let session = bus::update_session_status(&mempal_home, &cwd, &session_id, &status)
                    .map_err(bus_error_to_mcp)?;
                Ok(Json(CoworkBusResponse {
                    action: action.to_string(),
                    agents: Vec::new(),
                    delivered: Vec::new(),
                    messages: Vec::new(),
                    events: Vec::new(),
                    deliveries: Vec::new(),
                    channels: Vec::new(),
                    tmux_peek: None,
                    doctor: None,
                    sessions: vec![session_to_dto(session)],
                    handoff: None,
                    capture: None,
                }))
            }
            "session_close" => {
                let session_id = required_bus_field(request.session_id, "session_id", action)?;
                let session = bus::update_session_status(&mempal_home, &cwd, &session_id, "closed")
                    .map_err(bus_error_to_mcp)?;
                let capture = if request.capture.unwrap_or(false) {
                    let execute = request.execute.unwrap_or(false);
                    let db = if execute { Some(self.open_db()?) } else { None };
                    Some(
                        bus::capture_handoff_to_memory(
                            db.as_ref(),
                            &mempal_home,
                            &cwd,
                            bus::CoworkCaptureRequest {
                                summary_source: request
                                    .summary_source
                                    .unwrap_or_else(|| "handoff".to_string()),
                                wing: request.wing.unwrap_or_else(|| "cowork-capture".to_string()),
                                room: request.room,
                                thread_id: request.thread_id,
                                channel: request.channel,
                                session_id: Some(session_id),
                                note: request.note,
                                execute,
                            },
                        )
                        .map_err(bus_error_to_mcp)?,
                    )
                } else {
                    None
                };
                Ok(Json(CoworkBusResponse {
                    action: action.to_string(),
                    agents: Vec::new(),
                    delivered: Vec::new(),
                    messages: Vec::new(),
                    events: Vec::new(),
                    deliveries: Vec::new(),
                    channels: Vec::new(),
                    tmux_peek: None,
                    doctor: None,
                    sessions: vec![session_to_dto(session)],
                    handoff: None,
                    capture: capture.map(capture_to_dto),
                }))
            }
            "handoff" => {
                let summary = bus::build_handoff_summary(
                    &mempal_home,
                    &cwd,
                    bus::HandoffFilters {
                        thread_id: request.thread_id,
                        channel: request.channel,
                        session_id: request.session_id,
                        limit: request.limit,
                    },
                )
                .map_err(bus_error_to_mcp)?;
                Ok(Json(CoworkBusResponse {
                    action: action.to_string(),
                    agents: Vec::new(),
                    delivered: Vec::new(),
                    messages: Vec::new(),
                    events: Vec::new(),
                    deliveries: Vec::new(),
                    channels: Vec::new(),
                    tmux_peek: None,
                    doctor: None,
                    sessions: Vec::new(),
                    handoff: Some(handoff_to_dto(summary)),
                    capture: None,
                }))
            }
            "capture" => {
                let execute = request.execute.unwrap_or(false);
                let db = if execute { Some(self.open_db()?) } else { None };
                let report = bus::capture_handoff_to_memory(
                    db.as_ref(),
                    &mempal_home,
                    &cwd,
                    bus::CoworkCaptureRequest {
                        summary_source: request
                            .summary_source
                            .unwrap_or_else(|| "handoff".to_string()),
                        wing: request.wing.unwrap_or_else(|| "cowork-capture".to_string()),
                        room: request.room,
                        thread_id: request.thread_id,
                        channel: request.channel,
                        session_id: request.session_id,
                        note: request.note,
                        execute,
                    },
                )
                .map_err(bus_error_to_mcp)?;
                Ok(Json(CoworkBusResponse {
                    action: action.to_string(),
                    agents: Vec::new(),
                    delivered: Vec::new(),
                    messages: Vec::new(),
                    events: Vec::new(),
                    deliveries: Vec::new(),
                    channels: Vec::new(),
                    tmux_peek: None,
                    doctor: None,
                    sessions: Vec::new(),
                    handoff: None,
                    capture: Some(capture_to_dto(report)),
                }))
            }
            other => Err(ErrorData::invalid_params(
                format!(
                    "unknown action `{other}`: expected register|list|send|broadcast|drain|events|deliveries|ack|heartbeat|channel_set|channel_list|channel_send|tmux_peek|doctor|session_create|session_list|session_status|session_close|handoff|capture"
                ),
                None,
            )),
        }
    }

    #[tool(
        name = "mempal_fact_check",
        description = "Detect contradictions in text against KG triples + known entities. \
                       Returns SimilarNameConflict (similar-name typos), RelationContradiction \
                       (incompatible predicate for same endpoints), and StaleFact (KG valid_to \
                       expired) issues. Pure read, zero LLM, zero network, deterministic. \
                       Call before ingesting decisions that assert relationships between named \
                       entities to catch typos or outdated assumptions early."
    )]
    async fn mempal_fact_check(
        &self,
        Parameters(request): Parameters<FactCheckRequest>,
    ) -> std::result::Result<Json<FactCheckResponse>, ErrorData> {
        let db = self.open_db()?;
        let now_secs =
            crate::factcheck::resolve_now(request.now.as_deref()).map_err(fact_check_error)?;
        let scope =
            crate::factcheck::validate_scope(request.wing.as_deref(), request.room.as_deref())
                .map_err(fact_check_error)?;

        let report = tokio::task::block_in_place(|| {
            crate::factcheck::check(&request.text, &db, now_secs, scope)
        })
        .map_err(fact_check_error)?;

        Ok(Json(FactCheckResponse {
            issues: report.issues,
            checked_entities: report.checked_entities,
            kg_triples_scanned: report.kg_triples_scanned,
        }))
    }
}

/// Return the current UTC timestamp in RFC 3339 format (seconds precision).
/// Matches the format used by P6 peek_partner messages.
fn current_rfc3339() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    // Use the same days_to_ymd+format_rfc3339 helpers as cowork::peek,
    // but we don't need to pull them in — format as a simple UTC timestamp.
    // Use the existing format_rfc3339 via SystemTime conversion.
    let secs = now;
    // Reuse cowork::peek::format_rfc3339 is pub; call it to stay consistent.
    crate::cowork::peek::format_rfc3339(UNIX_EPOCH + std::time::Duration::from_secs(secs as u64))
}

fn required_bus_field(
    value: Option<String>,
    field: &str,
    action: &str,
) -> std::result::Result<String, ErrorData> {
    value.ok_or_else(|| {
        ErrorData::invalid_params(format!("action `{action}` requires `{field}`"), None)
    })
}

fn bus_error_to_mcp(error: BusError) -> ErrorData {
    match error {
        BusError::InvalidAgentId(_)
        | BusError::InvalidTool(_)
        | BusError::UnsupportedTransport(_)
        | BusError::InvalidChannel(_)
        | BusError::InvalidThreadId(_)
        | BusError::UnknownChannel(_)
        | BusError::EmptyChannel(_)
        | BusError::TmuxTargetRequired
        | BusError::TmuxFailed(_)
        | BusError::TmuxCaptureFailed(_)
        | BusError::TmuxProbeFailed(_)
        | BusError::NotTmuxAgent(_)
        | BusError::InvalidLineCount(_)
        | BusError::InvalidSessionId(_)
        | BusError::EmptySession(_)
        | BusError::UnknownSession(_)
        | BusError::InvalidSessionStatus(_)
        | BusError::UnsupportedCaptureSource(_)
        | BusError::MissingCaptureDatabase
        | BusError::InvalidTimestamp(_)
        | BusError::UnknownSource(_)
        | BusError::UnknownTarget(_)
        | BusError::UnknownAgent(_)
        | BusError::UnknownDelivery(_)
        | BusError::DeliveryTargetMismatch { .. }
        | BusError::CannotAckFailed(_)
        | BusError::SelfSend(_)
        | BusError::MessageTooLarge(_)
        | BusError::InboxFull { .. } => ErrorData::invalid_params(error.to_string(), None),
        BusError::LegacyInbox(_) | BusError::Io(_) | BusError::Json(_) | BusError::Db(_) => {
            ErrorData::internal_error(error.to_string(), None)
        }
    }
}

fn agent_record_to_dto(record: AgentRecord) -> CoworkBusAgentDto {
    CoworkBusAgentDto {
        agent_id: record.agent_id,
        tool: record.tool,
        transport: record.transport,
        tmux_target: record.tmux_target,
        registered_at: record.registered_at,
        updated_at: record.updated_at,
        presence: if record.last_seen_at.is_some() {
            "online".to_string()
        } else {
            "never_seen".to_string()
        },
        last_seen_at: record.last_seen_at,
        pending_count: 0,
        pending_bytes: 0,
    }
}

fn agent_status_to_dto(status: AgentStatus) -> CoworkBusAgentDto {
    CoworkBusAgentDto {
        agent_id: status.record.agent_id,
        tool: status.record.tool,
        transport: status.record.transport,
        tmux_target: status.record.tmux_target,
        registered_at: status.record.registered_at,
        updated_at: status.record.updated_at,
        last_seen_at: status.record.last_seen_at,
        presence: status.presence,
        pending_count: status.pending_count,
        pending_bytes: status.pending_bytes,
    }
}

fn delivery_to_dto(delivery: DeliveryReport) -> CoworkBusDeliveryDto {
    CoworkBusDeliveryDto {
        message_id: delivery.message_id,
        target_agent_id: delivery.target_agent_id,
        transport: delivery.transport,
        inbox_path: delivery
            .inbox_path
            .map(|path| path.to_string_lossy().to_string()),
        inbox_size_after: delivery.inbox_size_after,
        tmux_target: delivery.tmux_target,
        thread_id: delivery.thread_id,
        channel: delivery.channel,
    }
}

fn delivery_status_to_dto(
    status: crate::cowork::bus::DeliveryStatus,
) -> CoworkBusDeliveryStatusDto {
    CoworkBusDeliveryStatusDto {
        message_id: status.message_id,
        event_type: status.event_type,
        status: status.status,
        from: status.from,
        target_agent_id: status.target_agent_id,
        transport: status.transport,
        message_preview: status.message_preview,
        thread_id: status.thread_id,
        channel: status.channel,
        delivered_at: status.delivered_at,
        updated_at: status.updated_at,
        acked_by: status.acked_by,
    }
}

fn message_to_dto(message: InboxMessage) -> CoworkBusMessageDto {
    CoworkBusMessageDto {
        pushed_at: message.pushed_at,
        from: message.from,
        content: message.content,
        thread_id: message.thread_id,
        channel: message.channel,
    }
}

fn event_to_dto(event: crate::cowork::bus::BusEvent) -> CoworkBusEventDto {
    CoworkBusEventDto {
        event_id: event.event_id,
        occurred_at: event.occurred_at,
        event_type: event.event_type,
        status: event.status,
        actor_agent_id: event.actor_agent_id,
        target_agent_ids: event.target_agent_ids,
        transport: event.transport,
        message_preview: event.message_preview,
        thread_id: event.details.get("thread_id").cloned(),
        channel: event.details.get("channel").cloned(),
        details: event.details,
    }
}

fn channel_to_dto(channel: crate::cowork::bus::ChannelRecord) -> CoworkBusChannelDto {
    CoworkBusChannelDto {
        channel: channel.channel,
        agents: channel.agents,
        updated_at: channel.updated_at,
    }
}

fn tmux_peek_to_dto(peek: crate::cowork::bus::TmuxPeek) -> CoworkBusTmuxPeekDto {
    CoworkBusTmuxPeekDto {
        agent_id: peek.agent_id,
        tmux_target: peek.tmux_target,
        lines: peek.lines,
        content: peek.content,
    }
}

fn doctor_to_dto(report: crate::cowork::bus::DoctorReport) -> CoworkBusDoctorDto {
    CoworkBusDoctorDto {
        status: report.status,
        agent_count: report.agent_count,
        channel_count: report.channel_count,
        session_count: report.session_count,
        stale_agents: report.stale_agents,
        never_seen_agents: report.never_seen_agents,
        pending_deliveries: report.pending_deliveries,
        warnings: report.warnings,
        tmux: report
            .tmux
            .into_iter()
            .map(|probe| CoworkBusTmuxProbeDto {
                agent_id: probe.agent_id,
                tmux_target: probe.tmux_target,
                status: probe.status,
                detail: probe.detail,
            })
            .collect(),
    }
}

fn session_to_dto(session: crate::cowork::bus::TeamSession) -> CoworkBusSessionDto {
    CoworkBusSessionDto {
        session_id: session.session_id,
        title: session.title,
        goal: session.goal,
        agents: session.agents,
        channels: session.channels,
        thread_id: session.thread_id,
        status: session.status,
        created_at: session.created_at,
        updated_at: session.updated_at,
    }
}

fn handoff_to_dto(summary: crate::cowork::bus::HandoffSummary) -> CoworkBusHandoffDto {
    CoworkBusHandoffDto {
        filters: CoworkBusHandoffFiltersDto {
            thread_id: summary.filters.thread_id,
            channel: summary.filters.channel,
            session_id: summary.filters.session_id,
            limit: summary.filters.limit,
        },
        sessions: summary.sessions.into_iter().map(session_to_dto).collect(),
        agents: summary
            .agents
            .into_iter()
            .map(|agent| CoworkBusHandoffAgentDto {
                agent_id: agent.agent_id,
                tool: agent.tool,
                presence: agent.presence,
                pending_count: agent.pending_count,
            })
            .collect(),
        pending_deliveries: summary
            .pending_deliveries
            .into_iter()
            .map(delivery_status_to_dto)
            .collect(),
        recent_events: summary
            .recent_events
            .into_iter()
            .map(event_to_dto)
            .collect(),
    }
}

fn capture_to_dto(report: crate::cowork::bus::CoworkCaptureReport) -> CoworkBusCaptureDto {
    CoworkBusCaptureDto {
        writes: report.writes,
        drawer_id: report.drawer_id,
        wing: report.wing,
        room: report.room,
        source: report.source,
        content: report.content,
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for MempalMcpServer {
    fn get_info(&self) -> ServerInfo {
        // MCP spec: `instructions` is auto-injected into the LLM system prompt
        // by most clients at connection time. Putting the memory protocol here
        // means every client (Claude Code, Codex, Cursor, Continue, ...) sees
        // it without needing to call any tool first. This is the primary
        // mechanism; `mempal_status` keeps the same text as a fallback/reference.
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(crate::core::protocol::MEMORY_PROTOCOL)
    }

    fn initialize(
        &self,
        request: rmcp::model::InitializeRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<
        Output = std::result::Result<rmcp::model::InitializeResult, ErrorData>,
    > + Send
    + '_ {
        // Capture the calling client's tool name so `mempal_peek_partner`
        // with `tool: "auto"` can infer which partner to read (e.g.,
        // caller=claude-code ⇒ peek codex; caller=codex-cli ⇒ peek claude).
        if let Ok(mut guard) = self.client_name.lock() {
            *guard = Some(request.client_info.name.clone());
        }
        // Preserve rmcp's default behavior: store peer_info so downstream
        // rmcp internals can read client capabilities.
        if context.peer.peer_info().is_none() {
            context.peer.set_peer_info(request);
        }
        std::future::ready(Ok(self.get_info()))
    }
}

fn db_error(error: impl std::fmt::Display) -> ErrorData {
    ErrorData::internal_error(format!("{error}"), None)
}

fn ingest_error(error: IngestError) -> ErrorData {
    match error {
        IngestError::DiaryRollupWrongWing { .. }
        | IngestError::DiaryRollupMissingRoom
        | IngestError::DailyRollupFull { .. } => ErrorData::invalid_params(error.to_string(), None),
        _ => ErrorData::internal_error(error.to_string(), None),
    }
}

fn fact_check_error(error: crate::factcheck::FactCheckError) -> ErrorData {
    match error {
        crate::factcheck::FactCheckError::InvalidScope(_)
        | crate::factcheck::FactCheckError::InvalidNow(_) => {
            ErrorData::invalid_params(error.to_string(), None)
        }
        crate::factcheck::FactCheckError::Db(_) => {
            ErrorData::internal_error(format!("fact_check: {error}"), None)
        }
    }
}

fn knowledge_gate_error(error: anyhow::Error) -> ErrorData {
    ErrorData::invalid_params(error.to_string(), None)
}

fn knowledge_distill_error(error: anyhow::Error) -> ErrorData {
    let message = error.to_string();
    if message.contains("failed to embed")
        || message.contains("failed to insert")
        || message.contains("failed to append audit")
        || message.contains("embedder required")
    {
        return ErrorData::internal_error(message, None);
    }
    ErrorData::invalid_params(message, None)
}

fn knowledge_lifecycle_error(error: anyhow::Error) -> ErrorData {
    let message = error.to_string();
    if message.contains("failed to update")
        || message.contains("failed to append audit")
        || message.contains("failed to open audit")
        || message.contains("failed to write audit")
    {
        return ErrorData::internal_error(message, None);
    }
    ErrorData::invalid_params(message, None)
}

fn knowledge_card_lifecycle_error(error: anyhow::Error) -> ErrorData {
    let message = error.to_string();
    if message.contains("failed to update")
        || message.contains("failed to insert")
        || message.contains("failed to append")
        || message.contains("failed to list")
    {
        return ErrorData::internal_error(message, None);
    }
    ErrorData::invalid_params(message, None)
}

fn knowledge_anchor_error(error: anyhow::Error) -> ErrorData {
    let message = error.to_string();
    if message.contains("failed to update")
        || message.contains("failed to append audit")
        || message.contains("failed to open audit")
        || message.contains("failed to write audit")
    {
        return ErrorData::internal_error(message, None);
    }
    ErrorData::invalid_params(message, None)
}

fn context_error(error: crate::context::ContextError) -> ErrorData {
    match error {
        crate::context::ContextError::DeriveAnchor(_) => {
            ErrorData::invalid_params(error.to_string(), None)
        }
        crate::context::ContextError::EmbedQuery(_)
        | crate::context::ContextError::MissingQueryVector
        | crate::context::ContextError::Search(_)
        | crate::context::ContextError::LoadDrawer(_)
        | crate::context::ContextError::LoadCard(_) => {
            ErrorData::internal_error(format!("context assembly failed: {error}"), None)
        }
    }
}

const DEDUP_THRESHOLD: f32 = 0.85;

fn check_semantic_duplicate(
    db: &Database,
    vector: &[f32],
    _content: &str,
) -> Option<DuplicateWarning> {
    use crate::core::types::RouteDecision;

    let route = RouteDecision {
        wing: None,
        room: None,
        confidence: 0.0,
        reason: "dedup check".to_string(),
    };
    let results = crate::search::search_by_vector(db, vector, route, 1).ok()?;
    let top = results.first()?;
    if top.similarity >= DEDUP_THRESHOLD {
        Some(DuplicateWarning {
            similar_drawer_id: top.drawer_id.clone(),
            similarity: top.similarity,
            preview: top.content.chars().take(100).collect(),
        })
    } else {
        None
    }
}

fn triple_to_dto(triple: &Triple) -> TripleDto {
    TripleDto {
        id: triple.id.clone(),
        subject: triple.subject.clone(),
        predicate: triple.predicate.clone(),
        object: triple.object.clone(),
        valid_from: triple.valid_from.clone(),
        valid_to: triple.valid_to.clone(),
        confidence: triple.confidence,
        source_drawer: triple.source_drawer.clone(),
    }
}

fn passive_tunnel_dtos(
    db: &Database,
    wing: Option<&str>,
) -> std::result::Result<Vec<TunnelDto>, ErrorData> {
    let wing = wing.map(str::trim).filter(|value| !value.is_empty());
    let tunnels = db
        .find_tunnels()
        .map_err(db_error)?
        .into_iter()
        .filter(|(_, wings)| wing.is_none_or(|filter| wings.iter().any(|item| item == filter)))
        .map(|(room, wings)| TunnelDto {
            tunnel_id: passive_tunnel_id(&room),
            kind: "passive".to_string(),
            room: Some(room),
            wings,
            left: None,
            right: None,
            label: None,
            created_at: None,
            created_by: None,
            via_tunnel_id: None,
            hop: None,
        })
        .collect();
    Ok(tunnels)
}

fn explicit_tunnel_to_dto(tunnel: &ExplicitTunnel) -> TunnelDto {
    TunnelDto {
        tunnel_id: tunnel.id.clone(),
        kind: "explicit".to_string(),
        room: None,
        wings: vec![tunnel.left.wing.clone(), tunnel.right.wing.clone()],
        left: Some(TunnelEndpointDto::from(&tunnel.left)),
        right: Some(TunnelEndpointDto::from(&tunnel.right)),
        label: Some(tunnel.label.clone()),
        created_at: Some(tunnel.created_at.clone()),
        created_by: tunnel.created_by.clone(),
        via_tunnel_id: None,
        hop: None,
    }
}

fn passive_tunnel_id(room: &str) -> String {
    let sanitized = room
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("passive_{sanitized}")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use async_trait::async_trait;
    use rusqlite::params;
    use tempfile::TempDir;

    use super::*;
    use crate::core::types::{
        BootstrapEvidenceArgs, KnowledgeCard, KnowledgeCardEvent, KnowledgeEventType,
        KnowledgeEvidenceLink, KnowledgeEvidenceRole, RuntimeAdoptionFilter, RuntimeAdoptionTrack,
    };
    use crate::embed::Embedder;

    #[derive(Clone)]
    struct StubEmbedderFactory {
        vector: Vec<f32>,
    }

    struct StubEmbedder {
        vector: Vec<f32>,
    }

    #[derive(Default)]
    struct KnowledgeRefs {
        supporting: Vec<String>,
        counterexample: Vec<String>,
        teaching: Vec<String>,
        verification: Vec<String>,
    }

    struct KnowledgeAnchorArgs<'a> {
        domain: MemoryDomain,
        anchor_kind: AnchorKind,
        anchor_id: &'a str,
        parent_anchor_id: Option<&'a str>,
    }

    #[async_trait]
    impl crate::embed::EmbedderFactory for StubEmbedderFactory {
        async fn build(&self) -> crate::embed::Result<Box<dyn Embedder>> {
            Ok(Box::new(StubEmbedder {
                vector: self.vector.clone(),
            }))
        }
    }

    #[async_trait]
    impl Embedder for StubEmbedder {
        async fn embed(&self, texts: &[&str]) -> crate::embed::Result<Vec<Vec<f32>>> {
            Ok(texts.iter().map(|_| self.vector.clone()).collect())
        }

        fn dimensions(&self) -> usize {
            self.vector.len()
        }

        fn name(&self) -> &str {
            "stub"
        }
    }

    fn setup_server() -> (TempDir, PathBuf, MempalMcpServer) {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let db_path = tempdir.path().join("palace.db");
        let server = MempalMcpServer::new_with_factory(
            db_path.clone(),
            Arc::new(StubEmbedderFactory {
                vector: vec![0.1, 0.2, 0.3],
            }),
        );
        (tempdir, db_path, server)
    }

    fn setup_server_with_config(
        config: crate::core::config::Config,
    ) -> (TempDir, PathBuf, MempalMcpServer) {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let db_path = tempdir.path().join("palace.db");
        let server = MempalMcpServer::new_with_config_and_factory(
            db_path.clone(),
            config,
            Arc::new(StubEmbedderFactory {
                vector: vec![0.1, 0.2, 0.3],
            }),
        );
        (tempdir, db_path, server)
    }

    fn knowledge_card(
        id: &str,
        tier: KnowledgeTier,
        status: KnowledgeStatus,
        field: &str,
    ) -> KnowledgeCard {
        KnowledgeCard {
            id: id.to_string(),
            statement: format!("Statement for {id}."),
            content: format!("Content for {id}."),
            tier,
            status,
            domain: MemoryDomain::Project,
            field: field.to_string(),
            anchor_kind: AnchorKind::Repo,
            anchor_id: "repo://mempal".to_string(),
            parent_anchor_id: None,
            scope_constraints: Some("Only for MCP read tests.".to_string()),
            trigger_hints: Some(TriggerHints {
                intent_tags: vec!["memory".to_string()],
                workflow_bias: vec!["inspect-first".to_string()],
                tool_needs: vec!["mcp".to_string()],
            }),
            created_at: "1713000000".to_string(),
            updated_at: "1713000000".to_string(),
        }
    }

    fn insert_knowledge_card(db_path: &Path, card: KnowledgeCard) {
        let db = Database::open(db_path).expect("open db");
        db.insert_knowledge_card(&card)
            .expect("insert knowledge card");
    }

    fn insert_knowledge_card_event(
        db_path: &Path,
        id: &str,
        card_id: &str,
        event_type: KnowledgeEventType,
        created_at: &str,
    ) {
        let db = Database::open(db_path).expect("open db");
        db.append_knowledge_event(&KnowledgeCardEvent {
            id: id.to_string(),
            card_id: card_id.to_string(),
            event_type,
            from_status: Some(KnowledgeStatus::Candidate),
            to_status: Some(KnowledgeStatus::Promoted),
            reason: format!("reason for {id}"),
            actor: Some("codex".to_string()),
            metadata: Some(serde_json::json!({ "source": "test" })),
            created_at: created_at.to_string(),
        })
        .expect("append knowledge card event");
    }

    fn insert_knowledge_card_link(
        db_path: &Path,
        id: &str,
        card_id: &str,
        evidence_drawer_id: &str,
        role: KnowledgeEvidenceRole,
    ) {
        let db = Database::open(db_path).expect("open db");
        db.insert_knowledge_evidence_link(&KnowledgeEvidenceLink {
            id: id.to_string(),
            card_id: card_id.to_string(),
            evidence_drawer_id: evidence_drawer_id.to_string(),
            role,
            note: None,
            created_at: "1713000000".to_string(),
        })
        .expect("insert knowledge card link");
    }

    fn insert_drawer(
        db_path: &Path,
        id: &str,
        content: &str,
        wing: &str,
        room: Option<&str>,
        source_file: &str,
        importance: i32,
    ) {
        let db = Database::open(db_path).expect("open db");
        db.insert_drawer(&Drawer::new_bootstrap_evidence(BootstrapEvidenceArgs {
            id: id.to_string(),
            content: content.to_string(),
            wing: wing.to_string(),
            room: room.map(str::to_string),
            source_file: Some(source_file.to_string()),
            source_type: SourceType::Manual,
            added_at: "1713000000".to_string(),
            chunk_index: Some(0),
            importance,
        }))
        .expect("insert drawer");
        db.insert_vector(id, &[0.1, 0.2, 0.3])
            .expect("insert vector");
    }

    fn insert_knowledge_drawer(
        db_path: &Path,
        id: &str,
        tier: KnowledgeTier,
        status: KnowledgeStatus,
        statement: &str,
        content: &str,
    ) {
        insert_knowledge_drawer_with_refs(
            db_path,
            id,
            tier,
            status,
            statement,
            content,
            KnowledgeRefs {
                supporting: vec!["drawer_supporting_ev".to_string()],
                ..KnowledgeRefs::default()
            },
        );
    }

    fn insert_knowledge_drawer_with_refs(
        db_path: &Path,
        id: &str,
        tier: KnowledgeTier,
        status: KnowledgeStatus,
        statement: &str,
        content: &str,
        refs: KnowledgeRefs,
    ) {
        let db = Database::open(db_path).expect("open db");
        let drawer = Drawer {
            id: id.to_string(),
            content: content.to_string(),
            wing: "mempal".to_string(),
            room: Some("context".to_string()),
            source_file: Some(format!("knowledge://project/context/{id}")),
            source_type: SourceType::Manual,
            added_at: "1713000000".to_string(),
            chunk_index: Some(0),
            normalize_version: 1,
            importance: 3,
            memory_kind: MemoryKind::Knowledge,
            domain: MemoryDomain::Project,
            field: anchor::DEFAULT_FIELD.to_string(),
            anchor_kind: AnchorKind::Repo,
            anchor_id: anchor::LEGACY_REPO_ANCHOR_ID.to_string(),
            parent_anchor_id: None,
            provenance: None,
            statement: Some(statement.to_string()),
            tier: Some(tier),
            status: Some(status),
            supporting_refs: refs.supporting,
            counterexample_refs: refs.counterexample,
            teaching_refs: refs.teaching,
            verification_refs: refs.verification,
            scope_constraints: None,
            trigger_hints: None,
        };
        db.insert_drawer(&drawer).expect("insert knowledge drawer");
        db.insert_vector(id, &[0.1, 0.2, 0.3])
            .expect("insert vector");
    }

    fn insert_knowledge_drawer_with_anchor(
        db_path: &Path,
        id: &str,
        status: KnowledgeStatus,
        anchor_args: KnowledgeAnchorArgs<'_>,
    ) {
        let db = Database::open(db_path).expect("open db");
        let drawer = Drawer {
            id: id.to_string(),
            content: format!("{id} content"),
            wing: "mempal".to_string(),
            room: Some("context".to_string()),
            source_file: Some(format!("knowledge://project/context/{id}")),
            source_type: SourceType::Manual,
            added_at: "1713000000".to_string(),
            chunk_index: Some(0),
            normalize_version: 1,
            importance: 3,
            memory_kind: MemoryKind::Knowledge,
            domain: anchor_args.domain,
            field: anchor::DEFAULT_FIELD.to_string(),
            anchor_kind: anchor_args.anchor_kind,
            anchor_id: anchor_args.anchor_id.to_string(),
            parent_anchor_id: anchor_args.parent_anchor_id.map(str::to_string),
            provenance: None,
            statement: Some(format!("{id} statement")),
            tier: Some(KnowledgeTier::Shu),
            status: Some(status),
            supporting_refs: vec!["drawer_supporting_ev".to_string()],
            counterexample_refs: Vec::new(),
            teaching_refs: Vec::new(),
            verification_refs: Vec::new(),
            scope_constraints: None,
            trigger_hints: None,
        };
        db.insert_drawer(&drawer)
            .expect("insert anchored knowledge drawer");
        db.insert_vector(id, &[0.1, 0.2, 0.3])
            .expect("insert anchored knowledge vector");
    }

    fn audit_line_count(db_path: &Path) -> usize {
        let audit_path = db_path
            .parent()
            .expect("db path has parent")
            .join("audit.jsonl");
        fs::read_to_string(audit_path)
            .map(|content| content.lines().count())
            .unwrap_or(0)
    }

    fn last_audit_entry(db_path: &Path) -> serde_json::Value {
        let audit_path = db_path
            .parent()
            .expect("db path has parent")
            .join("audit.jsonl");
        let content = fs::read_to_string(audit_path).expect("read audit log");
        serde_json::from_str(content.lines().last().expect("last audit line")).expect("audit json")
    }

    fn vector_row_count(db: &Database, id: &str) -> i64 {
        db.conn()
            .query_row(
                "SELECT COUNT(*) FROM drawer_vectors WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .expect("count vector rows")
    }

    fn total_vector_count(db: &Database) -> i64 {
        db.conn()
            .query_row("SELECT COUNT(*) FROM drawer_vectors", [], |row| row.get(0))
            .expect("count vector rows")
    }

    fn insert_triple(
        db_path: &Path,
        subject: &str,
        predicate: &str,
        object: &str,
        valid_from: Option<&str>,
        valid_to: Option<&str>,
    ) {
        let db = Database::open(db_path).expect("open db");
        db.insert_triple(&Triple {
            id: crate::core::utils::build_triple_id(subject, predicate, object),
            subject: subject.to_string(),
            predicate: predicate.to_string(),
            object: object.to_string(),
            valid_from: valid_from.map(str::to_string),
            valid_to: valid_to.map(str::to_string),
            confidence: 1.0,
            source_drawer: None,
        })
        .expect("insert triple");
    }

    async fn run_search(
        server: &MempalMcpServer,
        query: &str,
        wing: Option<&str>,
        room: Option<&str>,
        top_k: usize,
    ) -> SearchResponse {
        server
            .mempal_search(Parameters(SearchRequest {
                query: query.to_string(),
                wing: wing.map(str::to_string),
                room: room.map(str::to_string),
                top_k: Some(top_k),
                memory_kind: None,
                domain: None,
                field: None,
                tier: None,
                status: None,
                anchor_kind: None,
                with_neighbors: None,
            }))
            .await
            .expect("search should succeed")
            .0
    }

    #[tokio::test]
    async fn test_mempal_search_includes_structured_signals_and_preserves_raw_fields() {
        let (_tempdir, db_path, server) = setup_server();
        insert_drawer(
            &db_path,
            "drawer-1",
            "We decided to use Arc<Mutex<>> for state because shared ownership mattered",
            "mempal",
            Some("signals"),
            "/tmp/decision.md",
            4,
        );
        insert_drawer(
            &db_path,
            "drawer-2",
            "上海决定采用共享内存同步机制解决状态漂移问题",
            "mempal",
            Some("signals"),
            "/tmp/cjk.md",
            3,
        );

        let response = run_search(&server, "state", None, None, 2).await;

        assert_eq!(response.results.len(), 2);

        let decision = response
            .results
            .iter()
            .find(|result| result.drawer_id == "drawer-1")
            .expect("decision result");
        assert_eq!(
            decision.content,
            "We decided to use Arc<Mutex<>> for state because shared ownership mattered"
        );
        assert_eq!(decision.source_file, "/tmp/decision.md");
        assert!(decision.flags.contains(&"DECISION".to_string()));
        assert!(!decision.entities.is_empty());
        assert!(!decision.emotions.is_empty());
        assert!(decision.importance_stars >= 2);

        let cjk = response
            .results
            .iter()
            .find(|result| result.drawer_id == "drawer-2")
            .expect("cjk result");
        assert_ne!(cjk.entities, vec!["UNK".to_string()]);
    }

    #[tokio::test]
    async fn test_mempal_search_returns_empty_results_when_filters_exclude_all_drawers() {
        let (_tempdir, db_path, server) = setup_server();
        insert_drawer(
            &db_path,
            "drawer-1",
            "We decided to use Arc<Mutex<>> for state because shared ownership mattered",
            "mempal",
            Some("signals"),
            "/tmp/decision.md",
            4,
        );

        let response = run_search(&server, "state", Some("other-wing"), None, 5).await;

        assert!(response.results.is_empty());
    }

    #[tokio::test]
    async fn test_mempal_search_has_no_db_side_effects() {
        let (_tempdir, db_path, server) = setup_server();
        insert_drawer(
            &db_path,
            "drawer-1",
            "We decided to use Arc<Mutex<>> for state because shared ownership mattered",
            "mempal",
            Some("signals"),
            "/tmp/decision.md",
            4,
        );

        let db = Database::open(&db_path).expect("open db");
        let baseline_drawers = db.drawer_count().expect("drawer count");
        let baseline_triples = db.triple_count().expect("triple count");
        let baseline_schema = db.schema_version().expect("schema version");

        for _ in 0..3 {
            let response = run_search(&server, "state", None, None, 5).await;
            assert!(!response.results.is_empty());
        }

        let db = Database::open(&db_path).expect("reopen db");
        assert_eq!(db.drawer_count().expect("drawer count"), baseline_drawers);
        assert_eq!(db.triple_count().expect("triple count"), baseline_triples);
        assert_eq!(
            db.schema_version().expect("schema version"),
            baseline_schema
        );
    }

    #[tokio::test]
    async fn test_mcp_context_returns_tier_ordered_sections() {
        let (_tempdir, db_path, server) = setup_server();
        insert_knowledge_drawer(
            &db_path,
            "drawer_qi",
            KnowledgeTier::Qi,
            KnowledgeStatus::Promoted,
            "Use cargo test.",
            "debug failing build qi",
        );
        insert_knowledge_drawer(
            &db_path,
            "drawer_shu",
            KnowledgeTier::Shu,
            KnowledgeStatus::Promoted,
            "Reproduce before patching.",
            "debug failing build shu",
        );
        insert_knowledge_drawer(
            &db_path,
            "drawer_dao_ren",
            KnowledgeTier::DaoRen,
            KnowledgeStatus::Promoted,
            "Software changes need executable feedback.",
            "debug failing build dao ren",
        );
        insert_knowledge_drawer(
            &db_path,
            "drawer_dao_tian",
            KnowledgeTier::DaoTian,
            KnowledgeStatus::Canonical,
            "Evidence precedes assertion.",
            "debug failing build dao tian",
        );

        let response = server
            .context_json_for_test(serde_json::json!({
                "query": "debug failing build"
            }))
            .await
            .expect("context should succeed");
        let names = response
            .sections
            .iter()
            .map(|section| section.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["dao_tian", "dao_ren", "shu", "qi"]);
        for section in response.sections {
            assert_eq!(section.items.len(), 1);
            assert!(!section.items[0].drawer_id.is_empty());
            assert!(!section.items[0].source_file.is_empty());
        }
    }

    #[tokio::test]
    async fn test_mcp_context_defaults_match_cli_context_defaults() {
        let (_tempdir, db_path, server) = setup_server();
        insert_knowledge_drawer(
            &db_path,
            "drawer_shu",
            KnowledgeTier::Shu,
            KnowledgeStatus::Promoted,
            "Debug by reproducing.",
            "debug default body",
        );

        let response = server
            .context_json_for_test(serde_json::json!({ "query": "debug" }))
            .await
            .expect("context should succeed");
        assert_eq!(response.domain, "project");
        assert_eq!(response.field, "general");
        assert!(!response.anchors.is_empty());
        assert!(
            response
                .sections
                .iter()
                .all(|section| section.name != "evidence")
        );
        assert_eq!(response.sections[0].name, "shu");
        assert_eq!(response.sections[0].items[0].drawer_id, "drawer_shu");
    }

    #[tokio::test]
    async fn test_mcp_context_include_evidence_appends_evidence_section() {
        let (_tempdir, db_path, server) = setup_server();
        insert_knowledge_drawer(
            &db_path,
            "drawer_qi",
            KnowledgeTier::Qi,
            KnowledgeStatus::Promoted,
            "Use cargo test.",
            "observed failure qi",
        );
        insert_drawer(
            &db_path,
            "drawer_evidence",
            "observed failure",
            "mempal",
            Some("context"),
            "/tmp/evidence.md",
            2,
        );

        let response = server
            .context_json_for_test(serde_json::json!({
                "query": "observed failure",
                "include_evidence": true
            }))
            .await
            .expect("context should succeed");
        let names = response
            .sections
            .iter()
            .map(|section| section.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["qi", "evidence"]);
        assert_eq!(response.sections[1].items[0].drawer_id, "drawer_evidence");
    }

    #[tokio::test]
    async fn test_mcp_context_include_cards_appends_card_items() {
        let (_tempdir, db_path, server) = setup_server();
        insert_drawer(
            &db_path,
            "drawer_card_evidence",
            "card evidence",
            "mempal",
            Some("context"),
            "/tmp/card-evidence.md",
            2,
        );
        let mut card = knowledge_card(
            "card_context",
            KnowledgeTier::Shu,
            KnowledgeStatus::Promoted,
            "general",
        );
        card.anchor_id = anchor::LEGACY_REPO_ANCHOR_ID.to_string();
        insert_knowledge_card(&db_path, card);
        insert_knowledge_card_link(
            &db_path,
            "link_card_context_supporting",
            "card_context",
            "drawer_card_evidence",
            KnowledgeEvidenceRole::Supporting,
        );

        let response = server
            .context_json_for_test(serde_json::json!({
                "query": "card context",
                "include_cards": true
            }))
            .await
            .expect("context should succeed");
        let card = response
            .sections
            .iter()
            .flat_map(|section| section.items.iter())
            .find(|item| item.card_id.as_deref() == Some("card_context"))
            .expect("card context item");

        assert_eq!(card.drawer_id, "card_context");
        assert_eq!(card.source_file, "knowledge-card://card_context");
        assert_eq!(card.evidence_citations.len(), 1);
        assert_eq!(
            card.evidence_citations[0].evidence_drawer_id,
            "drawer_card_evidence"
        );
        assert_eq!(card.evidence_citations[0].role, "supporting");
        assert_eq!(
            card.evidence_citations[0].source_file,
            "/tmp/card-evidence.md"
        );
    }

    #[tokio::test]
    async fn test_mcp_context_include_cards_omitted_uses_config_default() {
        let mut config = crate::core::config::Config::default();
        config.context.include_cards_default = true;
        let (_tempdir, db_path, server) = setup_server_with_config(config);
        insert_drawer(
            &db_path,
            "drawer_card_default_evidence",
            "card evidence",
            "mempal",
            Some("context"),
            "/tmp/card-default-evidence.md",
            2,
        );
        let mut card = knowledge_card(
            "card_context_default",
            KnowledgeTier::Shu,
            KnowledgeStatus::Promoted,
            "general",
        );
        card.anchor_id = anchor::LEGACY_REPO_ANCHOR_ID.to_string();
        insert_knowledge_card(&db_path, card);
        insert_knowledge_card_link(
            &db_path,
            "link_card_context_default_supporting",
            "card_context_default",
            "drawer_card_default_evidence",
            KnowledgeEvidenceRole::Supporting,
        );

        let response = server
            .context_json_for_test(serde_json::json!({
                "query": "card context"
            }))
            .await
            .expect("context should succeed");
        assert!(
            response
                .sections
                .iter()
                .flat_map(|section| section.items.iter())
                .any(|item| item.card_id.as_deref() == Some("card_context_default"))
        );
    }

    #[tokio::test]
    async fn test_mcp_context_dao_tian_limit_zero_omits_section() {
        let (_tempdir, db_path, server) = setup_server();
        insert_knowledge_drawer(
            &db_path,
            "drawer_dao_tian",
            KnowledgeTier::DaoTian,
            KnowledgeStatus::Canonical,
            "Evidence precedes assertion.",
            "debug universal principle",
        );
        insert_knowledge_drawer(
            &db_path,
            "drawer_shu",
            KnowledgeTier::Shu,
            KnowledgeStatus::Promoted,
            "Reproduce before patching.",
            "debug workflow rule",
        );

        let response = server
            .context_json_for_test(serde_json::json!({
                "query": "debug",
                "dao_tian_limit": 0
            }))
            .await
            .expect("context should succeed");
        let names = response
            .sections
            .iter()
            .map(|section| section.name.as_str())
            .collect::<Vec<_>>();
        assert!(!names.contains(&"dao_tian"));
        assert!(names.contains(&"shu"));
    }

    #[tokio::test]
    async fn test_mcp_context_rejects_max_items_zero() {
        let (_tempdir, _db_path, server) = setup_server();
        let error = server
            .context_json_for_test(serde_json::json!({
                "query": "debug",
                "max_items": 0
            }))
            .await
            .expect_err("max_items=0 should reject");
        assert!(error.to_string().contains("max_items"));
    }

    #[tokio::test]
    async fn test_mcp_context_rejects_unsupported_domain() {
        let (_tempdir, _db_path, server) = setup_server();
        let error = server
            .context_json_for_test(serde_json::json!({
                "query": "debug",
                "domain": "invalid"
            }))
            .await
            .expect_err("invalid domain should reject");
        assert!(error.to_string().contains("domain"));
    }

    #[tokio::test]
    async fn test_mcp_context_has_no_db_side_effects() {
        let (_tempdir, db_path, server) = setup_server();
        insert_knowledge_drawer(
            &db_path,
            "drawer_shu",
            KnowledgeTier::Shu,
            KnowledgeStatus::Promoted,
            "Debug by reproducing.",
            "debug side-effect body",
        );

        let db = Database::open(&db_path).expect("open db");
        let baseline_schema = db.schema_version().expect("schema");
        let baseline_drawers = db.drawer_count().expect("drawers");
        let baseline_triples = db.triple_count().expect("triples");
        let baseline_taxonomy = db.taxonomy_count().expect("taxonomy");
        let baseline_scopes = db.scope_counts().expect("scopes");

        for _ in 0..3 {
            let response = server
                .context_json_for_test(serde_json::json!({ "query": "debug" }))
                .await
                .expect("context should succeed");
            assert!(!response.sections.is_empty());
        }

        let db = Database::open(&db_path).expect("reopen db");
        assert_eq!(db.schema_version().expect("schema"), baseline_schema);
        assert_eq!(db.drawer_count().expect("drawers"), baseline_drawers);
        assert_eq!(db.triple_count().expect("triples"), baseline_triples);
        assert_eq!(db.taxonomy_count().expect("taxonomy"), baseline_taxonomy);
        assert_eq!(db.scope_counts().expect("scopes"), baseline_scopes);

        let search = run_search(&server, "debug", None, None, 1).await;
        assert_eq!(search.results[0].drawer_id, "drawer_shu");
        assert!(!search.results[0].content.is_empty());
    }

    #[test]
    fn test_mcp_tool_registry_includes_mempal_context() {
        let (_tempdir, _db_path, server) = setup_server();
        let tools = server.tool_router.list_all();
        let context_tool = tools
            .iter()
            .find(|tool| tool.name == "mempal_context")
            .expect("mempal_context tool exists");
        assert!(
            context_tool
                .description
                .as_deref()
                .unwrap_or_default()
                .contains("dao_tian -> dao_ren -> shu -> qi")
        );
    }

    #[test]
    fn test_mcp_tool_registry_includes_mempal_doctor() {
        let (_tempdir, _db_path, server) = setup_server();
        let tools = server.tool_router.list_all();
        let doctor_tool = tools
            .iter()
            .find(|tool| tool.name == "mempal_doctor")
            .expect("mempal_doctor tool exists");
        assert!(
            doctor_tool
                .description
                .as_deref()
                .unwrap_or_default()
                .contains("MCP runtime diagnostics")
        );
    }

    #[tokio::test]
    async fn test_mcp_doctor_reports_runtime_tools() {
        let (_tempdir, _db_path, server) = setup_server();
        let response = server
            .doctor_json_for_test(serde_json::json!({}))
            .await
            .expect("doctor should succeed");
        assert!(
            response
                .mcp
                .required_tools
                .iter()
                .any(|tool| tool.name == "mempal_context" && tool.advertised)
        );
        assert!(
            response
                .mcp
                .required_tools
                .iter()
                .any(|tool| tool.name == "mempal_phase3" && tool.advertised)
        );
        assert!(
            response
                .mcp
                .required_tools
                .iter()
                .any(|tool| tool.name == "mempal_cowork_bus" && tool.advertised)
        );
    }

    #[test]
    fn test_mcp_tool_registry_and_protocol_include_mempal_doctor() {
        let (_tempdir, _db_path, server) = setup_server();
        let tools = server.tool_router.list_all();
        assert!(tools.iter().any(|tool| tool.name == "mempal_doctor"));
        assert!(crate::core::protocol::MEMORY_PROTOCOL.contains("mempal_doctor"));
    }

    #[tokio::test]
    async fn test_mcp_brief_returns_cognitive_brief() {
        let (_tempdir, db_path, server) = setup_server();
        insert_drawer(
            &db_path,
            "drawer_brief_evidence",
            "Debug evidence with unresolved action item.",
            "mempal",
            Some("brief"),
            "/tmp/brief-evidence.md",
            3,
        );
        insert_knowledge_drawer(
            &db_path,
            "drawer_brief_knowledge",
            KnowledgeTier::Shu,
            KnowledgeStatus::Promoted,
            "Debug by reading cited evidence first.",
            "debug workflow",
        );

        let response = server
            .brief_json_for_test(serde_json::json!({
                "query": "debug",
                "include_cards": false
            }))
            .await
            .expect("brief should succeed");
        assert!(response.summary.key_fact_count + response.summary.evidence_count > 0);
        assert!(
            response
                .key_facts
                .iter()
                .any(|fact| !fact.citation.drawer_id.is_empty())
                || response
                    .evidence
                    .iter()
                    .any(|evidence| !evidence.citation.drawer_id.is_empty())
        );
    }

    #[tokio::test]
    async fn test_mcp_brief_rejects_max_items_zero() {
        let (_tempdir, _db_path, server) = setup_server();
        let error = server
            .brief_json_for_test(serde_json::json!({
                "query": "debug",
                "max_items": 0
            }))
            .await
            .expect_err("max_items=0 should reject");
        assert!(error.to_string().contains("max_items"));
    }

    #[test]
    fn test_mcp_tool_registry_and_protocol_include_mempal_brief() {
        let (_tempdir, _db_path, server) = setup_server();
        let tools = server.tool_router.list_all();
        assert!(tools.iter().any(|tool| tool.name == "mempal_brief"));
        assert!(crate::core::protocol::MEMORY_PROTOCOL.contains("mempal_brief"));
    }

    #[tokio::test]
    async fn test_mcp_field_taxonomy_lists_stage1_fields() {
        let (_tempdir, _db_path, server) = setup_server();
        let response = server
            .field_taxonomy_json_for_test()
            .await
            .expect("field taxonomy should succeed");
        for field in [
            "general",
            "epistemics",
            "software-engineering",
            "tooling",
            "diary",
        ] {
            assert!(
                response.entries.iter().any(|entry| entry.field == field),
                "missing field {field}"
            );
        }
        let epistemics = response
            .entries
            .iter()
            .find(|entry| entry.field == "epistemics")
            .expect("epistemics field");
        assert!(epistemics.domains.iter().any(|domain| domain == "global"));
    }

    #[test]
    fn test_mcp_tool_registry_includes_mempal_field_taxonomy() {
        let (_tempdir, _db_path, server) = setup_server();
        let tools = server.tool_router.list_all();
        let field_tool = tools
            .iter()
            .find(|tool| tool.name == "mempal_field_taxonomy")
            .expect("mempal_field_taxonomy tool exists");
        assert!(
            field_tool
                .description
                .as_deref()
                .unwrap_or_default()
                .contains("custom fields remain accepted")
        );
    }

    #[tokio::test]
    async fn test_mcp_knowledge_policy_lists_stage1_thresholds() {
        let (_tempdir, _db_path, server) = setup_server();
        let response = server
            .knowledge_policy_json_for_test()
            .await
            .expect("policy should succeed");
        let dao_tian = response
            .entries
            .iter()
            .find(|entry| entry.tier == "dao_tian" && entry.target_status == "canonical")
            .expect("dao_tian policy");
        assert_eq!(dao_tian.requirements.min_supporting_refs, 3);
        assert_eq!(dao_tian.requirements.min_verification_refs, 2);
        assert_eq!(dao_tian.requirements.min_teaching_refs, 1);
        assert!(dao_tian.requirements.reviewer_required);

        let dao_ren = response
            .entries
            .iter()
            .find(|entry| entry.tier == "dao_ren" && entry.target_status == "promoted")
            .expect("dao_ren policy");
        assert_eq!(dao_ren.requirements.min_supporting_refs, 2);
        assert_eq!(dao_ren.requirements.min_verification_refs, 1);
    }

    #[tokio::test]
    async fn test_mcp_knowledge_policy_has_no_db_side_effects() {
        let (_tempdir, db_path, server) = setup_server();
        insert_drawer(
            &db_path,
            "drawer_evidence",
            "policy side-effect evidence",
            "mempal",
            Some("policy"),
            "/tmp/policy.md",
            2,
        );
        let db = Database::open(&db_path).expect("open db");
        let baseline_schema = db.schema_version().expect("schema");
        let baseline_drawers = db.drawer_count().expect("drawers");
        let baseline_triples = db.triple_count().expect("triples");
        let baseline_taxonomy = db.taxonomy_count().expect("taxonomy");

        for _ in 0..3 {
            let response = server
                .knowledge_policy_json_for_test()
                .await
                .expect("policy should succeed");
            assert!(!response.entries.is_empty());
        }

        let db = Database::open(&db_path).expect("reopen db");
        assert_eq!(db.schema_version().expect("schema"), baseline_schema);
        assert_eq!(db.drawer_count().expect("drawers"), baseline_drawers);
        assert_eq!(db.triple_count().expect("triples"), baseline_triples);
        assert_eq!(db.taxonomy_count().expect("taxonomy"), baseline_taxonomy);
    }

    #[test]
    fn test_mcp_tool_registry_includes_mempal_knowledge_policy() {
        let (_tempdir, _db_path, server) = setup_server();
        let tools = server.tool_router.list_all();
        let policy_tool = tools
            .iter()
            .find(|tool| tool.name == "mempal_knowledge_policy")
            .expect("mempal_knowledge_policy tool exists");
        assert!(
            policy_tool
                .description
                .as_deref()
                .unwrap_or_default()
                .contains("Stage-1 knowledge promotion policy")
        );
    }

    #[tokio::test]
    async fn test_mcp_knowledge_cards_list_filters() {
        let (_tempdir, db_path, server) = setup_server();
        insert_knowledge_card(
            &db_path,
            knowledge_card(
                "card_match",
                KnowledgeTier::DaoRen,
                KnowledgeStatus::Promoted,
                "rust",
            ),
        );
        insert_knowledge_card(
            &db_path,
            knowledge_card(
                "card_wrong_tier",
                KnowledgeTier::Shu,
                KnowledgeStatus::Promoted,
                "rust",
            ),
        );
        insert_knowledge_card(
            &db_path,
            knowledge_card(
                "card_wrong_field",
                KnowledgeTier::DaoRen,
                KnowledgeStatus::Promoted,
                "docs",
            ),
        );

        let response = server
            .knowledge_cards_json_for_test(serde_json::json!({
                "action": "list",
                "tier": "dao_ren",
                "status": "promoted",
                "field": "rust"
            }))
            .await
            .expect("list cards");

        assert_eq!(response.cards.len(), 1);
        assert_eq!(response.cards[0].id, "card_match");
        assert!(response.events.is_empty());
    }

    #[tokio::test]
    async fn test_mcp_knowledge_cards_get_and_missing() {
        let (_tempdir, db_path, server) = setup_server();
        insert_knowledge_card(
            &db_path,
            knowledge_card(
                "card_get",
                KnowledgeTier::Shu,
                KnowledgeStatus::Promoted,
                "rust",
            ),
        );

        let response = server
            .knowledge_cards_json_for_test(serde_json::json!({
                "action": "get",
                "card_id": "card_get"
            }))
            .await
            .expect("get card");
        let card = response.cards.first().expect("card");
        assert_eq!(card.id, "card_get");
        assert_eq!(card.statement, "Statement for card_get.");
        assert_eq!(card.content, "Content for card_get.");
        assert_eq!(card.tier, "shu");
        assert_eq!(card.status, "promoted");
        assert_eq!(card.domain, "project");
        assert_eq!(card.field, "rust");
        assert_eq!(card.anchor_kind, "repo");
        assert_eq!(card.anchor_id, "repo://mempal");
        assert_eq!(
            card.trigger_hints
                .as_ref()
                .expect("trigger hints")
                .tool_needs,
            vec!["mcp"]
        );

        let missing = server
            .knowledge_cards_json_for_test(serde_json::json!({
                "action": "get",
                "card_id": "card_missing"
            }))
            .await
            .expect_err("missing card should fail");
        assert!(missing.to_string().contains("knowledge card not found"));
    }

    #[tokio::test]
    async fn test_mcp_knowledge_cards_events() {
        let (_tempdir, db_path, server) = setup_server();
        insert_knowledge_card(
            &db_path,
            knowledge_card(
                "card_events",
                KnowledgeTier::Shu,
                KnowledgeStatus::Promoted,
                "rust",
            ),
        );
        insert_knowledge_card_event(
            &db_path,
            "event_b",
            "card_events",
            KnowledgeEventType::Promoted,
            "1713000002",
        );
        insert_knowledge_card_event(
            &db_path,
            "event_a",
            "card_events",
            KnowledgeEventType::Created,
            "1713000001",
        );

        let response = server
            .knowledge_cards_json_for_test(serde_json::json!({
                "action": "events",
                "card_id": "card_events"
            }))
            .await
            .expect("list events");

        assert!(response.cards.is_empty());
        assert_eq!(response.events.len(), 2);
        assert_eq!(response.events[0].id, "event_a");
        assert_eq!(response.events[0].event_type, "created");
        assert_eq!(response.events[1].id, "event_b");
        assert_eq!(response.events[1].event_type, "promoted");
    }

    #[tokio::test]
    async fn test_mcp_knowledge_cards_rejects_unknown_actions_without_mutation() {
        let (_tempdir, db_path, server) = setup_server();
        let before = {
            let db = Database::open(&db_path).expect("open db");
            db.list_knowledge_cards(&KnowledgeCardFilter::default())
                .expect("list cards")
                .len()
        };

        let error = server
            .knowledge_cards_json_for_test(serde_json::json!({
                "action": "create"
            }))
            .await
            .expect_err("unknown action should be rejected");
        assert!(
            error
                .to_string()
                .contains("actions are list, get, retrieve, events, gate, promote, demote")
        );

        let after = {
            let db = Database::open(&db_path).expect("open db");
            db.list_knowledge_cards(&KnowledgeCardFilter::default())
                .expect("list cards")
                .len()
        };
        assert_eq!(before, after);
    }

    #[tokio::test]
    async fn test_mcp_knowledge_cards_gate_and_lifecycle_actions() {
        let (_tempdir, db_path, server) = setup_server();
        insert_knowledge_card(
            &db_path,
            knowledge_card(
                "card_lifecycle",
                KnowledgeTier::Qi,
                KnowledgeStatus::Candidate,
                "rust",
            ),
        );
        insert_drawer(
            &db_path,
            "drawer_supporting",
            "supporting evidence",
            "mempal",
            Some("phase2"),
            "/tmp/supporting.md",
            2,
        );
        insert_drawer(
            &db_path,
            "drawer_verification",
            "verification evidence",
            "mempal",
            Some("phase2"),
            "/tmp/verification.md",
            2,
        );
        insert_drawer(
            &db_path,
            "drawer_counterexample",
            "counterexample evidence",
            "mempal",
            Some("phase2"),
            "/tmp/counterexample.md",
            2,
        );
        insert_knowledge_card_link(
            &db_path,
            "link_supporting",
            "card_lifecycle",
            "drawer_supporting",
            KnowledgeEvidenceRole::Supporting,
        );

        let gate = server
            .knowledge_cards_json_for_test(serde_json::json!({
                "action": "gate",
                "card_id": "card_lifecycle",
                "target_status": "promoted"
            }))
            .await
            .expect("gate card");
        assert!(!gate.gate.expect("gate").allowed);

        let promoted = server
            .knowledge_cards_json_for_test(serde_json::json!({
                "action": "promote",
                "card_id": "card_lifecycle",
                "status": "promoted",
                "verification_refs": ["drawer_verification"],
                "reason": "verified through MCP"
            }))
            .await
            .expect("promote card");
        assert_eq!(promoted.promote.expect("promote").new_status, "promoted");

        let demoted = server
            .knowledge_cards_json_for_test(serde_json::json!({
                "action": "demote",
                "card_id": "card_lifecycle",
                "status": "demoted",
                "evidence_refs": ["drawer_counterexample"],
                "reason": "contradicted through MCP",
                "reason_type": "contradicted"
            }))
            .await
            .expect("demote card");
        assert_eq!(demoted.demote.expect("demote").new_status, "demoted");
        let db = Database::open(&db_path).expect("open db");
        assert_eq!(
            db.get_knowledge_card("card_lifecycle")
                .expect("get card")
                .expect("card exists")
                .status,
            KnowledgeStatus::Demoted
        );
        assert_eq!(
            db.knowledge_events("card_lifecycle").expect("events").len(),
            2
        );
    }

    #[tokio::test]
    async fn test_mcp_phase3_record_stats_and_gate_actions() {
        let (_tempdir, db_path, server) = setup_server();

        for i in 0..3 {
            let response = server
                .phase3_json_for_test(serde_json::json!({
                    "action": "record",
                    "id": format!("mcp_card_context_accept_{i}"),
                    "track": "card_context",
                    "signal": "accepted",
                    "feature": "include_cards",
                    "query": "skill trigger context",
                    "metadata": { "source": "mcp-test" }
                }))
                .await
                .expect("record phase3 event");
            assert_eq!(
                response.event.expect("event").id,
                format!("mcp_card_context_accept_{i}")
            );
        }

        let stats = server
            .phase3_json_for_test(serde_json::json!({
                "action": "stats",
                "track": "card_context",
                "feature": "include_cards"
            }))
            .await
            .expect("stats");
        let stats = stats.stats.expect("stats");
        assert_eq!(stats.total, 3);
        assert_eq!(stats.accepted, 3);
        assert_eq!(stats.rollbacks, 0);

        let gate = server
            .phase3_json_for_test(serde_json::json!({
                "action": "gate",
                "candidate": "card-context-default"
            }))
            .await
            .expect("gate");
        let gate = gate.gate.expect("gate");
        assert!(gate.ready);
        assert_eq!(gate.required_track, "card_context");

        let db = Database::open(&db_path).expect("open db");
        let events = db
            .list_runtime_adoption_events(
                &RuntimeAdoptionFilter {
                    track: Some(RuntimeAdoptionTrack::CardContext),
                    feature: Some("include_cards".to_string()),
                },
                10,
            )
            .expect("events");
        assert_eq!(events.len(), 3);
    }

    #[tokio::test]
    async fn test_mcp_phase3_research_validate_plan_is_read_only() {
        let (_tempdir, db_path, server) = setup_server();
        let baseline = {
            let db = Database::open(&db_path).expect("open db");
            (
                db.drawer_count().expect("drawers"),
                db.list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
                    .expect("events")
                    .len(),
            )
        };

        let response = server
            .phase3_json_for_test(serde_json::json!({
                "action": "research_validate_plan",
                "report": {
                    "report_id": "research_001",
                    "title": "Agent memory retrieval notes",
                    "sources": [{ "url": "https://example.invalid/report" }],
                    "findings": [{ "summary": "Measure card context before defaults." }],
                    "candidate_insights": [{ "statement": "Measure before defaulting cards." }]
                }
            }))
            .await
            .expect("validate research plan");
        let plan = response.research_plan.expect("research plan");
        assert!(plan.valid);
        assert_eq!(plan.report_id, "research_001");
        assert_eq!(plan.source_count, 1);
        assert_eq!(plan.candidate_insight_count, 1);

        let db = Database::open(&db_path).expect("reopen db");
        assert_eq!(db.drawer_count().expect("drawers"), baseline.0);
        assert_eq!(
            db.list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
                .expect("events")
                .len(),
            baseline.1
        );
    }

    #[tokio::test]
    async fn test_mcp_phase3_research_ingest_plan_is_read_only() {
        let (_tempdir, db_path, server) = setup_server();
        let baseline = {
            let db = Database::open(&db_path).expect("open db");
            (
                db.drawer_count().expect("drawers"),
                db.list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
                    .expect("events")
                    .len(),
            )
        };

        let response = server
            .phase3_json_for_test(serde_json::json!({
                "action": "research_ingest_plan",
                "report": {
                    "report_id": "research_001",
                    "title": "Agent memory retrieval notes",
                    "sources": [{ "url": "https://example.invalid/report" }],
                    "findings": [{ "summary": "Measure card context before defaults." }],
                    "candidate_insights": [{ "statement": "Measure before defaulting cards." }]
                }
            }))
            .await
            .expect("plan research ingest");
        let plan = response.research_ingest_plan.expect("research ingest plan");
        assert!(plan.valid);
        assert!(!plan.writes);
        assert_eq!(plan.report_id, "research_001");
        assert_eq!(plan.planned_evidence_count, 1);
        assert_eq!(plan.evidence_drawers.len(), 1);
        assert!(
            plan.evidence_drawers[0]
                .drawer_id
                .starts_with("drawer_mempal_research_")
        );
        assert_eq!(plan.candidate_insights.len(), 1);
        assert_eq!(
            &plan.candidate_insights[0].suggested_command[0..3],
            ["mempal", "knowledge", "distill"]
        );

        let db = Database::open(&db_path).expect("reopen db");
        assert_eq!(db.drawer_count().expect("drawers"), baseline.0);
        assert_eq!(
            db.list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
                .expect("events")
                .len(),
            baseline.1
        );
    }

    #[tokio::test]
    async fn test_mcp_phase3_research_ingest_plan_invalid_report_no_write() {
        let (_tempdir, db_path, server) = setup_server();
        let baseline = {
            let db = Database::open(&db_path).expect("open db");
            (
                db.drawer_count().expect("drawers"),
                db.list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
                    .expect("events")
                    .len(),
            )
        };

        let response = server
            .phase3_json_for_test(serde_json::json!({
                "action": "research_ingest_plan",
                "report": {}
            }))
            .await
            .expect("plan invalid research ingest");
        let plan = response.research_ingest_plan.expect("research ingest plan");
        assert!(!plan.valid);
        assert!(!plan.writes);
        assert!(
            plan.errors
                .iter()
                .any(|error| error.contains("report_id is required"))
        );
        assert!(
            plan.errors
                .iter()
                .any(|error| error.contains("findings must contain at least one item"))
        );

        let db = Database::open(&db_path).expect("reopen db");
        assert_eq!(db.drawer_count().expect("drawers"), baseline.0);
        assert_eq!(
            db.list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
                .expect("events")
                .len(),
            baseline.1
        );
    }

    #[tokio::test]
    async fn test_mcp_phase3_rejects_invalid_action_without_mutation() {
        let (_tempdir, db_path, server) = setup_server();
        let before = {
            let db = Database::open(&db_path).expect("open db");
            db.list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
                .expect("events")
                .len()
        };

        let error = server
            .phase3_json_for_test(serde_json::json!({
                "action": "promote"
            }))
            .await
            .expect_err("invalid action should fail");
        assert!(
            error.to_string().contains(
                "actions are guidance, instrumentation_policy, prepare_record, capture, evaluator_advise, default_proposal, rollback_control, check_record, record_checked, review, readiness, analytics, record, list, stats, gate, research_validate_plan, research_ingest_plan"
            )
        );

        let db = Database::open(&db_path).expect("open db");
        assert_eq!(
            db.list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
                .expect("events")
                .len(),
            before
        );
    }

    #[tokio::test]
    async fn test_mcp_phase3_guidance_action_is_read_only() {
        let (_tempdir, db_path, server) = setup_server();
        let baseline = {
            let db = Database::open(&db_path).expect("open db");
            (
                db.drawer_count().expect("drawers"),
                db.list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
                    .expect("events")
                    .len(),
            )
        };

        let response = server
            .phase3_json_for_test(serde_json::json!({
                "action": "guidance"
            }))
            .await
            .expect("guidance");
        let guidance = response.guidance.expect("guidance");
        assert_eq!(guidance.version, 1);
        assert_eq!(
            guidance.recording_rule,
            "record only concrete runtime outcomes, not speculation"
        );
        assert!(guidance.required_fields.contains(&"track".to_string()));
        assert!(guidance.required_fields.contains(&"signal".to_string()));
        assert!(guidance.required_fields.contains(&"feature".to_string()));
        assert!(
            guidance
                .signals
                .iter()
                .any(|signal| signal.signal == "used" && signal.when.contains("actually consumed"))
        );
        assert!(
            guidance
                .signals
                .iter()
                .any(|signal| signal.signal == "rollback" && signal.when.contains("reverted"))
        );
        assert!(guidance.tracks.iter().any(|track| {
            track.track == "card_context"
                && track
                    .feature_examples
                    .contains(&"include_cards".to_string())
        }));

        let db = Database::open(&db_path).expect("reopen db");
        assert_eq!(db.drawer_count().expect("drawers"), baseline.0);
        assert_eq!(
            db.list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
                .expect("events")
                .len(),
            baseline.1
        );
    }

    #[tokio::test]
    async fn test_mcp_phase3_instrumentation_policy_action_is_read_only() {
        let (_tempdir, db_path, server) = setup_server();
        let baseline = {
            let db = Database::open(&db_path).expect("open db");
            (
                db.drawer_count().expect("drawers"),
                db.list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
                    .expect("events")
                    .len(),
            )
        };

        let response = server
            .phase3_json_for_test(serde_json::json!({
                "action": "instrumentation_policy"
            }))
            .await
            .expect("instrumentation policy");
        let policy = response
            .instrumentation_policy
            .expect("instrumentation policy");
        assert!(!policy.writes);
        assert_eq!(policy.default_mode, "manual_only");
        assert!(policy.allowed_modes.iter().any(|mode| {
            mode.mode == "opt_in_wrapper" && mode.requires_execute && mode.requires_checked_capture
        }));
        assert!(
            policy
                .forbidden_modes
                .contains(&"implicit_background_capture".to_string())
        );

        let db = Database::open(&db_path).expect("reopen db");
        assert_eq!(db.drawer_count().expect("drawers"), baseline.0);
        assert_eq!(
            db.list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
                .expect("events")
                .len(),
            baseline.1
        );
    }

    #[tokio::test]
    async fn test_mcp_phase3_prepare_record_action_is_read_only() {
        let (_tempdir, db_path, server) = setup_server();
        let before = {
            let db = Database::open(&db_path).expect("open db");
            db.list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
                .expect("events")
                .len()
        };

        let response = server
            .phase3_json_for_test(serde_json::json!({
                "action": "prepare_record",
                "track": "card_context",
                "signal": "accepted",
                "feature": "include_cards",
                "query": "skill trigger"
            }))
            .await
            .expect("prepare record");
        let plan = response.record_plan.expect("record plan");
        assert!(!plan.writes);
        assert_eq!(plan.record_payload["action"], "record");
        assert_eq!(plan.record_payload["track"], "card_context");
        assert_eq!(plan.record_payload["signal"], "accepted");
        assert_eq!(plan.record_payload["feature"], "include_cards");
        assert_eq!(plan.record_payload["query"], "skill trigger");
        assert_eq!(plan.record_command[0], "mempal");
        assert_eq!(plan.record_command[1], "phase3");
        assert_eq!(plan.record_command[2], "adoption");
        assert_eq!(plan.record_command[3], "record");

        let db = Database::open(&db_path).expect("reopen db");
        assert_eq!(
            db.list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
                .expect("events")
                .len(),
            before
        );
    }

    #[tokio::test]
    async fn test_mcp_phase3_check_record_action_is_read_only() {
        let (_tempdir, db_path, server) = setup_server();
        let before = {
            let db = Database::open(&db_path).expect("open db");
            db.list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
                .expect("events")
                .len()
        };

        let response = server
            .phase3_json_for_test(serde_json::json!({
                "action": "check_record",
                "track": "card_context",
                "signal": "accepted",
                "feature": "include_cards"
            }))
            .await
            .expect("check record");
        let report = response.record_quality.expect("record quality");
        assert!(!report.writes);
        assert!(report.valid);
        assert_eq!(report.quality, "warning");
        assert!(report.errors.is_empty());
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("concrete outcome context"))
        );

        let db = Database::open(&db_path).expect("reopen db");
        assert_eq!(
            db.list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
                .expect("events")
                .len(),
            before
        );
    }

    #[tokio::test]
    async fn test_mcp_phase3_capture_action_dry_run_and_execute() {
        let (_tempdir, db_path, server) = setup_server();

        let dry_run = server
            .phase3_json_for_test(serde_json::json!({
                "action": "capture",
                "surface": "card-context",
                "outcome": "accepted",
                "card_id": "card_1",
                "query": "skill trigger",
                "note": "card helped"
            }))
            .await
            .expect("capture dry run");
        let plan = dry_run.record_plan.expect("record plan");
        assert!(!plan.writes);
        assert_eq!(plan.record_payload["track"], "card_context");
        assert_eq!(plan.record_payload["signal"], "accepted");
        assert_eq!(plan.record_payload["feature"], "include_cards");
        assert_eq!(
            dry_run.record_quality.expect("record quality").quality,
            "ready"
        );
        assert!(dry_run.record_checked.is_none());

        let db = Database::open(&db_path).expect("open db");
        assert!(
            db.list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
                .expect("events")
                .is_empty()
        );
        insert_knowledge_card(
            &db_path,
            knowledge_card(
                "card_1",
                KnowledgeTier::Shu,
                KnowledgeStatus::Promoted,
                "general",
            ),
        );

        let executed = server
            .phase3_json_for_test(serde_json::json!({
                "action": "capture",
                "surface": "card-context",
                "outcome": "accepted",
                "card_id": "card_1",
                "query": "skill trigger",
                "note": "card helped",
                "execute": true
            }))
            .await
            .expect("capture execute");
        let checked = executed.record_checked.expect("checked record");
        assert!(checked.writes);
        assert!(!checked.blocked);
        assert_eq!(checked.record_quality.quality, "ready");

        let db = Database::open(&db_path).expect("reopen db");
        let events = db
            .list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
            .expect("events");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].feature, "include_cards");
    }

    #[tokio::test]
    async fn test_mcp_phase3_evaluator_advise_action_is_read_only() {
        let (_tempdir, db_path, server) = setup_server();

        let response = server
            .phase3_json_for_test(serde_json::json!({
                "action": "evaluator_advise",
                "evaluator_id": "eval_policy",
                "subject_kind": "dao_ren",
                "subject_id": "k1",
                "proposed_action": "promote",
                "evidence_refs": ["e1", "e2"]
            }))
            .await
            .expect("evaluator advice");
        let advice = response.evaluator_advice.expect("evaluator advice");
        assert!(!advice.writes);
        assert!(!advice.lifecycle_authority);
        assert!(advice.deterministic_gate_required);
        assert_eq!(advice.recommendation, "advisory_support");
        assert_eq!(advice.adoption_capture.record_payload["track"], "evaluator");

        let db = Database::open(&db_path).expect("open db");
        assert!(
            db.list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
                .expect("events")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn test_mcp_phase3_record_checked_quality_gated() {
        let (_tempdir, db_path, server) = setup_server();

        let ready = server
            .phase3_json_for_test(serde_json::json!({
                "action": "record_checked",
                "track": "runtime_adoption",
                "signal": "accepted",
                "feature": "context_pack",
                "query": "skill trigger",
                "note": "context guidance helped"
            }))
            .await
            .expect("record checked ready");
        let checked = ready.record_checked.expect("checked record");
        assert!(checked.writes);
        assert!(!checked.blocked);
        assert_eq!(checked.record_quality.quality, "ready");
        assert!(checked.event.is_some());

        let db = Database::open(&db_path).expect("open db");
        assert_eq!(
            db.list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
                .expect("events")
                .len(),
            1
        );

        let warning = server
            .phase3_json_for_test(serde_json::json!({
                "action": "record_checked",
                "track": "card_context",
                "signal": "accepted",
                "feature": "include_cards"
            }))
            .await
            .expect("record checked warning");
        let checked = warning.record_checked.expect("checked record");
        assert!(!checked.writes);
        assert!(checked.blocked);
        assert_eq!(checked.record_quality.quality, "warning");
        assert!(checked.event.is_none());

        let db = Database::open(&db_path).expect("reopen db");
        assert_eq!(
            db.list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
                .expect("events")
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn test_mcp_phase3_review_action_is_read_only() {
        let (_tempdir, db_path, server) = setup_server();
        let before = {
            let db = Database::open(&db_path).expect("open db");
            db.insert_runtime_adoption_event(&RuntimeAdoptionEvent {
                id: "review_event".to_string(),
                track: RuntimeAdoptionTrack::CardContext,
                signal: RuntimeAdoptionSignal::Accepted,
                feature: "include_cards".to_string(),
                query: Some("skill trigger".to_string()),
                context_hash: None,
                card_id: None,
                evaluator_id: None,
                research_report_id: None,
                note: Some("helped".to_string()),
                metadata: None,
                created_at: "1777710000".to_string(),
            })
            .expect("insert event");
            db.list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
                .expect("events")
                .len()
        };

        let response = server
            .phase3_json_for_test(serde_json::json!({
                "action": "review",
                "track": "card_context"
            }))
            .await
            .expect("review");
        let report = response.review_report.expect("review report");
        assert!(!report.writes);
        assert_eq!(report.total, 1);
        assert_eq!(report.stats.accepted, 1);
        assert_eq!(report.features[0].feature, "include_cards");

        let db = Database::open(&db_path).expect("reopen db");
        assert_eq!(
            db.list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
                .expect("events")
                .len(),
            before
        );
    }

    #[tokio::test]
    async fn test_mcp_phase3_adoption_analytics_action() {
        let (_tempdir, db_path, server) = setup_server();
        let before = {
            let db = Database::open(&db_path).expect("open db");
            for (id, signal) in [
                ("analytics_accept", RuntimeAdoptionSignal::Accepted),
                ("analytics_reject", RuntimeAdoptionSignal::Rejected),
            ] {
                db.insert_runtime_adoption_event(&RuntimeAdoptionEvent {
                    id: id.to_string(),
                    track: RuntimeAdoptionTrack::CardContext,
                    signal,
                    feature: "include_cards".to_string(),
                    query: Some("skill trigger".to_string()),
                    context_hash: None,
                    card_id: None,
                    evaluator_id: None,
                    research_report_id: None,
                    note: Some("analytics fixture".to_string()),
                    metadata: None,
                    created_at: "1777710000".to_string(),
                })
                .expect("insert event");
            }
            db.list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
                .expect("events")
                .len()
        };

        let response = server
            .phase3_json_for_test(serde_json::json!({
                "action": "analytics"
            }))
            .await
            .expect("analytics");
        let analytics = response.analytics.expect("analytics report");
        assert!(!analytics.writes);
        assert_eq!(analytics.total_events, 2);
        assert!(
            analytics
                .groups
                .iter()
                .any(|group| group.feature == "include_cards" && group.accepted == 1)
        );

        let db = Database::open(&db_path).expect("reopen db");
        assert_eq!(
            db.list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
                .expect("events")
                .len(),
            before
        );
    }

    #[tokio::test]
    async fn test_mcp_phase3_readiness_card_context_default_is_read_only() {
        let (_tempdir, db_path, server) = setup_server();
        let before = {
            let db = Database::open(&db_path).expect("open db");
            for i in 0..3 {
                db.insert_runtime_adoption_event(&RuntimeAdoptionEvent {
                    id: format!("readiness_event_{i}"),
                    track: RuntimeAdoptionTrack::CardContext,
                    signal: RuntimeAdoptionSignal::Accepted,
                    feature: "include_cards".to_string(),
                    query: Some("skill trigger".to_string()),
                    context_hash: None,
                    card_id: None,
                    evaluator_id: None,
                    research_report_id: None,
                    note: Some("helped".to_string()),
                    metadata: None,
                    created_at: "1777710000".to_string(),
                })
                .expect("insert event");
            }
            db.list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
                .expect("events")
                .len()
        };

        let response = server
            .phase3_json_for_test(serde_json::json!({
                "action": "readiness",
                "candidate": "card-context-default"
            }))
            .await
            .expect("readiness");
        let report = response.readiness_report.expect("readiness report");
        assert!(!report.writes);
        assert!(report.ready);
        assert_eq!(report.decision, "eligible_for_future_default_spec");
        assert_eq!(report.review.stats.accepted, 3);

        let db = Database::open(&db_path).expect("reopen db");
        assert_eq!(
            db.list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
                .expect("events")
                .len(),
            before
        );
    }

    #[tokio::test]
    async fn test_mcp_phase3_default_proposal_card_context_is_read_only() {
        let (_tempdir, db_path, server) = setup_server();
        let before = {
            let db = Database::open(&db_path).expect("open db");
            for i in 0..3 {
                db.insert_runtime_adoption_event(&RuntimeAdoptionEvent {
                    id: format!("default_proposal_event_{i}"),
                    track: RuntimeAdoptionTrack::CardContext,
                    signal: RuntimeAdoptionSignal::Accepted,
                    feature: "include_cards".to_string(),
                    query: Some("skill trigger".to_string()),
                    context_hash: None,
                    card_id: None,
                    evaluator_id: None,
                    research_report_id: None,
                    note: Some("helped".to_string()),
                    metadata: None,
                    created_at: "1777710000".to_string(),
                })
                .expect("insert event");
            }
            db.list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
                .expect("events")
                .len()
        };

        let response = server
            .phase3_json_for_test(serde_json::json!({
                "action": "default_proposal",
                "candidate": "card-context",
                "rollback_criteria": ["rollback on contradiction or user-visible degradation"]
            }))
            .await
            .expect("default proposal");
        let report = response.default_proposal.expect("default proposal");
        assert!(!report.writes);
        assert!(report.proposal_ready);
        assert_eq!(report.decision, "eligible_for_default_on_spec");
        assert!(report.readiness.ready);

        let db = Database::open(&db_path).expect("reopen db");
        assert_eq!(
            db.list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
                .expect("events")
                .len(),
            before
        );
    }

    #[tokio::test]
    async fn test_mcp_phase3_rollback_control_check_is_read_only() {
        let mut config = crate::core::config::Config::default();
        config.context.include_cards_default = true;
        let (_tempdir, db_path, server) = setup_server_with_config(config);
        let before = {
            let db = Database::open(&db_path).expect("open db");
            db.insert_runtime_adoption_event(&RuntimeAdoptionEvent {
                id: "rollback_control_event".to_string(),
                track: RuntimeAdoptionTrack::CardContext,
                signal: RuntimeAdoptionSignal::Rollback,
                feature: "include_cards".to_string(),
                query: None,
                context_hash: None,
                card_id: None,
                evaluator_id: None,
                research_report_id: None,
                note: Some("reverted default".to_string()),
                metadata: None,
                created_at: "1777710000".to_string(),
            })
            .expect("insert event");
            db.list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
                .expect("events")
                .len()
        };

        let response = server
            .phase3_json_for_test(serde_json::json!({
                "action": "rollback_control",
                "candidate": "card-context"
            }))
            .await
            .expect("rollback control");
        let report = response.rollback_control.expect("rollback control");
        assert!(!report.writes);
        assert!(report.rollback_required);
        assert!(!report.applied);
        assert!(report.include_cards_default_before);
        assert!(report.include_cards_default_after);
        assert_eq!(report.review.stats.rollbacks, 1);

        let db = Database::open(&db_path).expect("reopen db");
        assert_eq!(
            db.list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10)
                .expect("events")
                .len(),
            before
        );
    }

    #[test]
    fn test_mcp_tool_registry_and_protocol_include_phase3_runtime_surface() {
        let (_tempdir, _db_path, server) = setup_server();
        let tools = server.tool_router.list_all();
        let tool = tools
            .iter()
            .find(|tool| tool.name == "mempal_phase3")
            .expect("mempal_phase3 tool exists");
        let description = tool.description.as_deref().unwrap_or_default();
        assert!(description.contains("Phase-3 runtime adoption evidence"));
        assert!(description.contains(
            "Actions: guidance/instrumentation_policy/prepare_record/capture/evaluator_advise/default_proposal/rollback_control/check_record/record_checked/review/readiness/analytics/record/list/stats/gate/research_validate_plan/research_ingest_plan"
        ));
        assert!(crate::core::protocol::MEMORY_PROTOCOL.contains("mempal_phase3"));
        assert!(crate::core::protocol::MEMORY_PROTOCOL.contains("action=guidance"));
        assert!(crate::core::protocol::MEMORY_PROTOCOL.contains("action=instrumentation_policy"));
        assert!(crate::core::protocol::MEMORY_PROTOCOL.contains("action=readiness"));
        assert!(crate::core::protocol::MEMORY_PROTOCOL.contains("action=analytics"));
        assert!(crate::core::protocol::MEMORY_PROTOCOL.contains("action=capture"));
        assert!(crate::core::protocol::MEMORY_PROTOCOL.contains("action=evaluator_advise"));
        assert!(crate::core::protocol::MEMORY_PROTOCOL.contains("action=default_proposal"));
        assert!(crate::core::protocol::MEMORY_PROTOCOL.contains("action=rollback_control"));
        assert!(crate::core::protocol::MEMORY_PROTOCOL.contains("record_checked"));
        assert!(crate::core::protocol::MEMORY_PROTOCOL.contains("research_ingest_plan"));
        assert!(crate::core::protocol::MEMORY_PROTOCOL.contains("live instrumentation is opt-in"));
        assert!(crate::core::protocol::MEMORY_PROTOCOL.contains("checked capture"));
        assert!(
            crate::core::protocol::MEMORY_PROTOCOL
                .contains("used when guidance was actually consumed")
        );
        assert!(
            crate::core::protocol::MEMORY_PROTOCOL.contains("rollback when behavior was reverted")
        );
        assert!(
            crate::core::protocol::MEMORY_PROTOCOL.contains("runtime adoption"),
            "protocol should explain the Phase-3 runtime evidence surface"
        );
    }

    #[test]
    fn test_mcp_tool_registry_and_protocol_include_knowledge_cards_lifecycle() {
        let (_tempdir, _db_path, server) = setup_server();
        let tools = server.tool_router.list_all();
        let tool = tools
            .iter()
            .find(|tool| tool.name == "mempal_knowledge_cards")
            .expect("mempal_knowledge_cards tool exists");
        let description = tool.description.as_deref().unwrap_or_default();
        assert!(description.contains("Phase-2 knowledge card inspection"));
        assert!(description.contains("linked-evidence retrieval"));
        assert!(description.contains("Actions: list/get/retrieve/events/gate/promote/demote"));
        assert!(crate::core::protocol::MEMORY_PROTOCOL.contains("mempal_knowledge_cards"));
        assert!(crate::core::protocol::MEMORY_PROTOCOL.contains("Phase-2 knowledge card"));
    }

    #[tokio::test]
    async fn test_mcp_knowledge_distill_creates_candidate_knowledge() {
        let (_tempdir, db_path, server) = setup_server();
        insert_drawer(
            &db_path,
            "drawer_evidence",
            "evidence first observation",
            "mempal",
            Some("distill"),
            "/tmp/evidence.md",
            2,
        );

        let response = server
            .knowledge_distill_json_for_test(serde_json::json!({
                "statement": "Prefer evidence first",
                "content": "Use cited evidence before asserting project facts.",
                "tier": "dao_ren",
                "supporting_refs": ["drawer_evidence"]
            }))
            .await
            .expect("distill should succeed");
        assert!(response.created);
        assert!(!response.dry_run);
        assert!(response.drawer_id.starts_with("drawer_"));

        let db = Database::open(&db_path).expect("open db");
        let drawer = db
            .get_drawer(&response.drawer_id)
            .expect("load drawer")
            .expect("drawer exists");
        assert_eq!(drawer.memory_kind, MemoryKind::Knowledge);
        assert_eq!(drawer.tier, Some(KnowledgeTier::DaoRen));
        assert_eq!(drawer.status, Some(KnowledgeStatus::Candidate));
        assert_eq!(drawer.supporting_refs, vec!["drawer_evidence"]);

        let context = server
            .context_json_for_test(serde_json::json!({
                "query": "evidence first",
                "cwd": db_path.parent().expect("db parent").to_string_lossy()
            }))
            .await
            .expect("context should succeed");
        let context_ids: Vec<_> = context
            .sections
            .into_iter()
            .flat_map(|section| section.items)
            .map(|item| item.drawer_id)
            .collect();
        assert!(!context_ids.contains(&response.drawer_id));
    }

    #[tokio::test]
    async fn test_mcp_knowledge_distill_dry_run_no_write() {
        let (_tempdir, db_path, server) = setup_server();
        insert_drawer(
            &db_path,
            "drawer_evidence",
            "dry run evidence",
            "mempal",
            Some("distill"),
            "/tmp/evidence.md",
            2,
        );
        let db = Database::open(&db_path).expect("open db");
        let drawer_count_before = db.drawer_count().expect("drawer count");
        let vector_count_before = total_vector_count(&db);
        let schema_before = db.schema_version().expect("schema");
        let audit_before = audit_line_count(&db_path);

        let request = serde_json::json!({
            "statement": "Dry run candidate",
            "content": "This should not be written.",
            "tier": "qi",
            "supporting_refs": ["drawer_evidence"],
            "dry_run": true
        });
        let first = server
            .knowledge_distill_json_for_test(request.clone())
            .await
            .expect("first dry-run should succeed");
        let second = server
            .knowledge_distill_json_for_test(request)
            .await
            .expect("second dry-run should succeed");

        assert_eq!(first.drawer_id, second.drawer_id);
        assert!(!first.created);
        assert!(first.dry_run);
        assert!(!second.created);
        assert!(second.dry_run);
        assert_eq!(
            db.drawer_count().expect("drawer count"),
            drawer_count_before
        );
        assert_eq!(total_vector_count(&db), vector_count_before);
        assert_eq!(db.schema_version().expect("schema"), schema_before);
        assert_eq!(audit_line_count(&db_path), audit_before);
    }

    #[tokio::test]
    async fn test_mcp_knowledge_distill_rejects_dao_tian_candidate() {
        let (_tempdir, db_path, server) = setup_server();
        insert_drawer(
            &db_path,
            "drawer_evidence",
            "dao tian evidence",
            "mempal",
            Some("distill"),
            "/tmp/evidence.md",
            2,
        );

        let error = server
            .knowledge_distill_json_for_test(serde_json::json!({
                "statement": "Universal law",
                "content": "This should not be candidate dao_tian.",
                "tier": "dao_tian",
                "supporting_refs": ["drawer_evidence"]
            }))
            .await
            .expect_err("dao_tian candidate should be rejected");
        assert!(
            error
                .to_string()
                .contains("distill only allows candidate dao_ren or qi"),
            "error={error}"
        );
    }

    #[tokio::test]
    async fn test_mcp_knowledge_distill_rejects_missing_supporting_refs() {
        let (_tempdir, db_path, server) = setup_server();
        let missing = server
            .knowledge_distill_json_for_test(serde_json::json!({
                "statement": "Missing refs",
                "content": "This should fail before writing.",
                "tier": "qi",
                "supporting_refs": []
            }))
            .await
            .expect_err("missing refs should be rejected");
        assert!(
            missing.to_string().contains("supporting_refs"),
            "error={missing}"
        );

        insert_drawer(
            &db_path,
            "drawer_evidence",
            "support evidence",
            "mempal",
            Some("distill"),
            "/tmp/evidence.md",
            2,
        );
        insert_knowledge_drawer_with_refs(
            &db_path,
            "drawer_ref_knowledge",
            KnowledgeTier::Qi,
            KnowledgeStatus::Candidate,
            "Tool candidate.",
            "Knowledge ref content",
            KnowledgeRefs {
                supporting: vec!["drawer_evidence".to_string()],
                ..KnowledgeRefs::default()
            },
        );

        let wrong_kind = server
            .knowledge_distill_json_for_test(serde_json::json!({
                "statement": "Wrong ref kind",
                "content": "This should fail before writing.",
                "tier": "qi",
                "supporting_refs": ["drawer_ref_knowledge"]
            }))
            .await
            .expect_err("knowledge refs should be rejected");
        assert!(
            wrong_kind
                .to_string()
                .contains("supporting_refs must point to evidence drawers"),
            "error={wrong_kind}"
        );
    }

    #[tokio::test]
    async fn test_mcp_knowledge_distill_stores_trigger_hints() {
        let (_tempdir, db_path, server) = setup_server();
        insert_drawer(
            &db_path,
            "drawer_evidence",
            "trigger hint evidence",
            "mempal",
            Some("distill"),
            "/tmp/evidence.md",
            2,
        );

        let response = server
            .knowledge_distill_json_for_test(serde_json::json!({
                "statement": "Reproduce before patching",
                "content": "Reproduce failures before changing code.",
                "tier": "qi",
                "supporting_refs": ["drawer_evidence"],
                "trigger_hints": {
                    "intent_tags": ["debugging"],
                    "workflow_bias": ["reproduce-first"],
                    "tool_needs": ["cargo-test"]
                }
            }))
            .await
            .expect("distill should succeed");
        let db = Database::open(&db_path).expect("open db");
        let drawer = db
            .get_drawer(&response.drawer_id)
            .expect("load drawer")
            .expect("drawer exists");
        let hints = drawer.trigger_hints.expect("trigger hints");
        assert_eq!(hints.intent_tags, vec!["debugging"]);
        assert_eq!(hints.workflow_bias, vec!["reproduce-first"]);
        assert_eq!(hints.tool_needs, vec!["cargo-test"]);
        assert!(
            crate::core::protocol::MEMORY_PROTOCOL.contains("trigger_hints as bias metadata only")
        );
    }

    #[tokio::test]
    async fn test_mcp_knowledge_distill_existing_drawer_no_duplicate_or_audit() {
        let (_tempdir, db_path, server) = setup_server();
        insert_drawer(
            &db_path,
            "drawer_evidence",
            "idempotent evidence",
            "mempal",
            Some("distill"),
            "/tmp/evidence.md",
            2,
        );
        let request = serde_json::json!({
            "statement": "Idempotent distill",
            "content": "Equivalent requests should not duplicate drawers.",
            "tier": "dao_ren",
            "supporting_refs": ["drawer_evidence"]
        });
        let first = server
            .knowledge_distill_json_for_test(request.clone())
            .await
            .expect("first distill should create");
        assert!(first.created);
        let db = Database::open(&db_path).expect("open db");
        let drawer_count_before_second = db.drawer_count().expect("drawer count");
        let vector_count_before_second = total_vector_count(&db);
        let audit_before_second = audit_line_count(&db_path);

        let second = server
            .knowledge_distill_json_for_test(request)
            .await
            .expect("second distill should be idempotent");
        assert_eq!(second.drawer_id, first.drawer_id);
        assert!(!second.created);
        assert_eq!(
            db.drawer_count().expect("drawer count"),
            drawer_count_before_second
        );
        assert_eq!(total_vector_count(&db), vector_count_before_second);
        assert_eq!(audit_line_count(&db_path), audit_before_second);
        assert_eq!(vector_row_count(&db, &first.drawer_id), 1);
    }

    #[test]
    fn test_mcp_tool_registry_and_protocol_include_mempal_knowledge_distill() {
        let (_tempdir, _db_path, server) = setup_server();
        let tools = server.tool_router.list_all();
        let distill_tool = tools
            .iter()
            .find(|tool| tool.name == "mempal_knowledge_distill")
            .expect("mempal_knowledge_distill tool exists");
        assert!(
            distill_tool
                .description
                .as_deref()
                .unwrap_or_default()
                .contains("candidate knowledge from existing evidence")
        );
        assert!(crate::core::protocol::MEMORY_PROTOCOL.contains("mempal_knowledge_distill"));
    }

    #[tokio::test]
    async fn test_mcp_knowledge_gate_allows_dao_ren_promotion() {
        let (_tempdir, db_path, server) = setup_server();
        insert_drawer(
            &db_path,
            "drawer_support_1",
            "support 1",
            "mempal",
            Some("gate"),
            "/tmp/support-1.md",
            2,
        );
        insert_drawer(
            &db_path,
            "drawer_support_2",
            "support 2",
            "mempal",
            Some("gate"),
            "/tmp/support-2.md",
            2,
        );
        insert_drawer(
            &db_path,
            "drawer_verify_1",
            "verify 1",
            "mempal",
            Some("gate"),
            "/tmp/verify-1.md",
            2,
        );
        insert_knowledge_drawer_with_refs(
            &db_path,
            "drawer_knowledge_gate",
            KnowledgeTier::DaoRen,
            KnowledgeStatus::Candidate,
            "Domain rules need evidence.",
            "Knowledge content",
            KnowledgeRefs {
                supporting: vec![
                    "drawer_support_1".to_string(),
                    "drawer_support_2".to_string(),
                ],
                verification: vec!["drawer_verify_1".to_string()],
                ..KnowledgeRefs::default()
            },
        );

        let response = server
            .knowledge_gate_json_for_test(serde_json::json!({
                "drawer_id": "drawer_knowledge_gate"
            }))
            .await
            .expect("gate should succeed");

        assert!(response.allowed, "reasons={:?}", response.reasons);
        assert_eq!(response.target_status, "promoted");
        assert_eq!(response.evidence_counts.supporting, 2);
        assert_eq!(response.evidence_counts.verification, 1);
    }

    #[tokio::test]
    async fn test_mcp_knowledge_gate_rejects_missing_verification() {
        let (_tempdir, db_path, server) = setup_server();
        insert_drawer(
            &db_path,
            "drawer_support_1",
            "support 1",
            "mempal",
            Some("gate"),
            "/tmp/support-1.md",
            2,
        );
        insert_drawer(
            &db_path,
            "drawer_support_2",
            "support 2",
            "mempal",
            Some("gate"),
            "/tmp/support-2.md",
            2,
        );
        insert_knowledge_drawer_with_refs(
            &db_path,
            "drawer_knowledge_gate",
            KnowledgeTier::DaoRen,
            KnowledgeStatus::Candidate,
            "Domain rules need verification.",
            "Knowledge content",
            KnowledgeRefs {
                supporting: vec![
                    "drawer_support_1".to_string(),
                    "drawer_support_2".to_string(),
                ],
                ..KnowledgeRefs::default()
            },
        );

        let db = Database::open(&db_path).expect("open db");
        let schema_before = db.schema_version().expect("schema");
        let drawer_count_before = db.drawer_count().expect("drawer count");
        let triple_count_before = db.triple_count().expect("triple count");
        let audit_before = audit_line_count(&db_path);

        let response = server
            .knowledge_gate_json_for_test(serde_json::json!({
                "drawer_id": "drawer_knowledge_gate"
            }))
            .await
            .expect("gate should return advisory denial");

        assert!(!response.allowed);
        assert!(
            response
                .reasons
                .iter()
                .any(|reason| reason.contains("verification evidence refs below requirement")),
            "reasons={:?}",
            response.reasons
        );
        assert_eq!(db.schema_version().expect("schema"), schema_before);
        assert_eq!(
            db.drawer_count().expect("drawer count"),
            drawer_count_before
        );
        assert_eq!(
            db.triple_count().expect("triple count"),
            triple_count_before
        );
        assert_eq!(audit_line_count(&db_path), audit_before);
    }

    #[tokio::test]
    async fn test_mcp_knowledge_gate_requires_reviewer_for_dao_tian() {
        let (_tempdir, db_path, server) = setup_server();
        for id in [
            "drawer_support_1",
            "drawer_support_2",
            "drawer_support_3",
            "drawer_verify_1",
            "drawer_verify_2",
            "drawer_teach_1",
        ] {
            insert_drawer(
                &db_path,
                id,
                id,
                "mempal",
                Some("gate"),
                &format!("/tmp/{id}.md"),
                2,
            );
        }
        insert_knowledge_drawer_with_refs(
            &db_path,
            "drawer_knowledge_gate",
            KnowledgeTier::DaoTian,
            KnowledgeStatus::Canonical,
            "Stable cross-domain principle.",
            "Knowledge content",
            KnowledgeRefs {
                supporting: vec![
                    "drawer_support_1".to_string(),
                    "drawer_support_2".to_string(),
                    "drawer_support_3".to_string(),
                ],
                verification: vec!["drawer_verify_1".to_string(), "drawer_verify_2".to_string()],
                teaching: vec!["drawer_teach_1".to_string()],
                ..KnowledgeRefs::default()
            },
        );

        let without_reviewer = server
            .knowledge_gate_json_for_test(serde_json::json!({
                "drawer_id": "drawer_knowledge_gate"
            }))
            .await
            .expect("gate should return advisory denial");
        assert!(!without_reviewer.allowed);
        assert!(
            without_reviewer
                .reasons
                .iter()
                .any(|reason| reason.contains("reviewer is required")),
            "reasons={:?}",
            without_reviewer.reasons
        );

        let with_reviewer = server
            .knowledge_gate_json_for_test(serde_json::json!({
                "drawer_id": "drawer_knowledge_gate",
                "reviewer": "alex"
            }))
            .await
            .expect("gate should allow with reviewer");
        assert!(with_reviewer.allowed, "reasons={:?}", with_reviewer.reasons);
        assert_eq!(with_reviewer.target_status, "canonical");
    }

    #[tokio::test]
    async fn test_mcp_knowledge_gate_blocks_counterexamples_by_default() {
        let (_tempdir, db_path, server) = setup_server();
        for id in ["drawer_support_1", "drawer_verify_1", "drawer_counter_1"] {
            insert_drawer(
                &db_path,
                id,
                id,
                "mempal",
                Some("gate"),
                &format!("/tmp/{id}.md"),
                2,
            );
        }
        insert_knowledge_drawer_with_refs(
            &db_path,
            "drawer_knowledge_gate",
            KnowledgeTier::Shu,
            KnowledgeStatus::Promoted,
            "Reusable method.",
            "Knowledge content",
            KnowledgeRefs {
                supporting: vec!["drawer_support_1".to_string()],
                verification: vec!["drawer_verify_1".to_string()],
                counterexample: vec!["drawer_counter_1".to_string()],
                ..KnowledgeRefs::default()
            },
        );

        let blocked = server
            .knowledge_gate_json_for_test(serde_json::json!({
                "drawer_id": "drawer_knowledge_gate"
            }))
            .await
            .expect("gate should return advisory denial");
        assert!(!blocked.allowed);
        assert!(
            blocked
                .reasons
                .iter()
                .any(|reason| reason.contains("counterexample refs present")),
            "reasons={:?}",
            blocked.reasons
        );

        let allowed = server
            .knowledge_gate_json_for_test(serde_json::json!({
                "drawer_id": "drawer_knowledge_gate",
                "allow_counterexamples": true
            }))
            .await
            .expect("gate should allow explicit counterexample override");
        assert!(allowed.allowed, "reasons={:?}", allowed.reasons);
    }

    #[tokio::test]
    async fn test_mcp_knowledge_gate_rejects_evidence_drawer() {
        let (_tempdir, db_path, server) = setup_server();
        insert_drawer(
            &db_path,
            "drawer_evidence",
            "evidence",
            "mempal",
            Some("gate"),
            "/tmp/evidence.md",
            2,
        );

        let error = server
            .knowledge_gate_json_for_test(serde_json::json!({
                "drawer_id": "drawer_evidence"
            }))
            .await
            .expect_err("evidence drawer should be rejected");
        assert!(
            error.to_string().contains("knowledge drawer"),
            "error={error}"
        );
    }

    #[tokio::test]
    async fn test_mcp_knowledge_gate_validates_role_refs() {
        let (_tempdir, db_path, server) = setup_server();
        insert_drawer(
            &db_path,
            "drawer_support_1",
            "support",
            "mempal",
            Some("gate"),
            "/tmp/support.md",
            2,
        );
        insert_knowledge_drawer_with_refs(
            &db_path,
            "drawer_ref_knowledge",
            KnowledgeTier::Qi,
            KnowledgeStatus::Candidate,
            "Tool capability.",
            "Knowledge ref content",
            KnowledgeRefs {
                supporting: vec!["drawer_support_1".to_string()],
                ..KnowledgeRefs::default()
            },
        );
        insert_knowledge_drawer_with_refs(
            &db_path,
            "drawer_knowledge_gate",
            KnowledgeTier::DaoRen,
            KnowledgeStatus::Candidate,
            "Domain rule.",
            "Knowledge content",
            KnowledgeRefs {
                supporting: vec![
                    "drawer_support_1".to_string(),
                    "drawer_support_1".to_string(),
                ],
                verification: vec!["drawer_ref_knowledge".to_string()],
                ..KnowledgeRefs::default()
            },
        );

        let error = server
            .knowledge_gate_json_for_test(serde_json::json!({
                "drawer_id": "drawer_knowledge_gate"
            }))
            .await
            .expect_err("knowledge ref should be rejected");
        assert!(
            error
                .to_string()
                .contains("gate refs must point to evidence drawers"),
            "error={error}"
        );
    }

    #[test]
    fn test_mcp_tool_registry_and_protocol_include_mempal_knowledge_gate() {
        let (_tempdir, _db_path, server) = setup_server();
        let tools = server.tool_router.list_all();
        let gate_tool = tools
            .iter()
            .find(|tool| tool.name == "mempal_knowledge_gate")
            .expect("mempal_knowledge_gate tool exists");
        assert!(
            gate_tool
                .description
                .as_deref()
                .unwrap_or_default()
                .contains("Read-only promotion readiness")
        );
        assert!(crate::core::protocol::MEMORY_PROTOCOL.contains("mempal_knowledge_gate"));
    }

    #[tokio::test]
    async fn test_mcp_knowledge_promote_updates_status_after_gate_pass() {
        let (_tempdir, db_path, server) = setup_server();
        for id in ["drawer_support_1", "drawer_support_2", "drawer_verify_1"] {
            insert_drawer(
                &db_path,
                id,
                id,
                "mempal",
                Some("lifecycle"),
                &format!("/tmp/{id}.md"),
                2,
            );
        }
        insert_knowledge_drawer_with_refs(
            &db_path,
            "drawer_lifecycle_promote",
            KnowledgeTier::DaoRen,
            KnowledgeStatus::Candidate,
            "Gate-passed knowledge can be promoted.",
            "Knowledge content",
            KnowledgeRefs {
                supporting: vec![
                    "drawer_support_1".to_string(),
                    "drawer_support_2".to_string(),
                ],
                ..KnowledgeRefs::default()
            },
        );
        let audit_before = audit_line_count(&db_path);

        let response = server
            .knowledge_promote_json_for_test(serde_json::json!({
                "drawer_id": "drawer_lifecycle_promote",
                "status": "promoted",
                "verification_refs": ["drawer_verify_1"],
                "reason": "validated by MCP lifecycle test",
                "reviewer": "test"
            }))
            .await
            .expect("promote should pass");

        assert_eq!(response.old_status, "candidate");
        assert_eq!(response.new_status, "promoted");
        let gate = response.gate.expect("MCP promote returns gate report");
        assert!(gate.allowed, "reasons={:?}", gate.reasons);
        assert_eq!(response.verification_refs, vec!["drawer_verify_1"]);
        let db = Database::open(&db_path).expect("open db");
        let drawer = db
            .get_drawer("drawer_lifecycle_promote")
            .expect("load drawer")
            .expect("drawer exists");
        assert_eq!(drawer.status, Some(KnowledgeStatus::Promoted));
        assert_eq!(drawer.verification_refs, vec!["drawer_verify_1"]);
        assert_eq!(audit_line_count(&db_path), audit_before + 1);
    }

    #[tokio::test]
    async fn test_mcp_knowledge_promote_rejects_gate_failure_without_mutation() {
        let (_tempdir, db_path, server) = setup_server();
        insert_drawer(
            &db_path,
            "drawer_support_1",
            "support 1",
            "mempal",
            Some("lifecycle"),
            "/tmp/support-1.md",
            2,
        );
        insert_drawer(
            &db_path,
            "drawer_verify_1",
            "verify 1",
            "mempal",
            Some("lifecycle"),
            "/tmp/verify-1.md",
            2,
        );
        insert_knowledge_drawer_with_refs(
            &db_path,
            "drawer_lifecycle_gate_fail",
            KnowledgeTier::DaoRen,
            KnowledgeStatus::Candidate,
            "Insufficiently supported knowledge cannot be promoted.",
            "Knowledge content",
            KnowledgeRefs {
                supporting: vec!["drawer_support_1".to_string()],
                ..KnowledgeRefs::default()
            },
        );
        let db = Database::open(&db_path).expect("open db");
        let schema_before = db.schema_version().expect("schema");
        let vector_count_before = vector_row_count(&db, "drawer_lifecycle_gate_fail");
        let audit_before = audit_line_count(&db_path);

        let error = server
            .knowledge_promote_json_for_test(serde_json::json!({
                "drawer_id": "drawer_lifecycle_gate_fail",
                "status": "promoted",
                "verification_refs": ["drawer_verify_1"],
                "reason": "should fail gate"
            }))
            .await
            .expect_err("promote should fail gate");

        assert!(
            error.to_string().contains("promotion gate failed"),
            "error={error}"
        );
        let drawer = db
            .get_drawer("drawer_lifecycle_gate_fail")
            .expect("load drawer")
            .expect("drawer exists");
        assert_eq!(drawer.status, Some(KnowledgeStatus::Candidate));
        assert!(drawer.verification_refs.is_empty());
        assert_eq!(db.schema_version().expect("schema"), schema_before);
        assert_eq!(
            vector_row_count(&db, "drawer_lifecycle_gate_fail"),
            vector_count_before
        );
        assert_eq!(audit_line_count(&db_path), audit_before);
    }

    #[tokio::test]
    async fn test_mcp_knowledge_demote_updates_status_and_counterexample_refs() {
        let (_tempdir, db_path, server) = setup_server();
        insert_drawer(
            &db_path,
            "drawer_counterexample_1",
            "counterexample 1",
            "mempal",
            Some("lifecycle"),
            "/tmp/counterexample-1.md",
            2,
        );
        insert_knowledge_drawer_with_refs(
            &db_path,
            "drawer_lifecycle_demote",
            KnowledgeTier::Shu,
            KnowledgeStatus::Promoted,
            "A workflow can be demoted.",
            "Knowledge content",
            KnowledgeRefs::default(),
        );
        let audit_before = audit_line_count(&db_path);

        let response = server
            .knowledge_demote_json_for_test(serde_json::json!({
                "drawer_id": "drawer_lifecycle_demote",
                "status": "demoted",
                "evidence_refs": ["drawer_counterexample_1"],
                "reason": "contradicted by MCP lifecycle test",
                "reason_type": "contradicted"
            }))
            .await
            .expect("demote should pass");

        assert_eq!(response.old_status, "promoted");
        assert_eq!(response.new_status, "demoted");
        assert_eq!(
            response.counterexample_refs,
            vec!["drawer_counterexample_1"]
        );
        let db = Database::open(&db_path).expect("open db");
        let drawer = db
            .get_drawer("drawer_lifecycle_demote")
            .expect("load drawer")
            .expect("drawer exists");
        assert_eq!(drawer.status, Some(KnowledgeStatus::Demoted));
        assert_eq!(drawer.counterexample_refs, vec!["drawer_counterexample_1"]);
        assert_eq!(audit_line_count(&db_path), audit_before + 1);
    }

    #[tokio::test]
    async fn test_mcp_knowledge_lifecycle_rejects_evidence_drawer_targets() {
        let (_tempdir, db_path, server) = setup_server();
        insert_drawer(
            &db_path,
            "drawer_evidence_target",
            "evidence target",
            "mempal",
            Some("lifecycle"),
            "/tmp/evidence-target.md",
            2,
        );
        let promote_error = server
            .knowledge_promote_json_for_test(serde_json::json!({
                "drawer_id": "drawer_evidence_target",
                "status": "promoted",
                "verification_refs": ["drawer_evidence_target"],
                "reason": "bad target"
            }))
            .await
            .expect_err("evidence target should be rejected");
        assert!(
            promote_error
                .to_string()
                .contains("knowledge lifecycle requires a knowledge drawer"),
            "error={promote_error}"
        );

        let demote_error = server
            .knowledge_demote_json_for_test(serde_json::json!({
                "drawer_id": "drawer_evidence_target",
                "status": "demoted",
                "evidence_refs": ["drawer_evidence_target"],
                "reason": "bad target",
                "reason_type": "contradicted"
            }))
            .await
            .expect_err("evidence target should be rejected");
        assert!(
            demote_error
                .to_string()
                .contains("knowledge lifecycle requires a knowledge drawer"),
            "error={demote_error}"
        );
    }

    #[tokio::test]
    async fn test_mcp_knowledge_lifecycle_validates_refs_are_evidence_drawers() {
        let (_tempdir, db_path, server) = setup_server();
        insert_knowledge_drawer_with_refs(
            &db_path,
            "drawer_lifecycle_target",
            KnowledgeTier::Qi,
            KnowledgeStatus::Candidate,
            "Knowledge target.",
            "Knowledge content",
            KnowledgeRefs::default(),
        );
        insert_knowledge_drawer_with_refs(
            &db_path,
            "drawer_wrong_ref_kind",
            KnowledgeTier::Qi,
            KnowledgeStatus::Candidate,
            "Wrong ref kind.",
            "Knowledge content",
            KnowledgeRefs::default(),
        );

        let promote_error = server
            .knowledge_promote_json_for_test(serde_json::json!({
                "drawer_id": "drawer_lifecycle_target",
                "status": "promoted",
                "verification_refs": ["drawer_wrong_ref_kind"],
                "reason": "bad ref"
            }))
            .await
            .expect_err("knowledge ref should be rejected");
        assert!(
            promote_error
                .to_string()
                .contains("lifecycle refs must point to evidence drawers"),
            "error={promote_error}"
        );

        let demote_error = server
            .knowledge_demote_json_for_test(serde_json::json!({
                "drawer_id": "drawer_lifecycle_target",
                "status": "demoted",
                "evidence_refs": ["drawer_wrong_ref_kind"],
                "reason": "bad ref",
                "reason_type": "contradicted"
            }))
            .await
            .expect_err("knowledge ref should be rejected");
        assert!(
            demote_error
                .to_string()
                .contains("lifecycle refs must point to evidence drawers"),
            "error={demote_error}"
        );
    }

    #[test]
    fn test_mcp_tool_registry_and_protocol_include_knowledge_lifecycle_tools() {
        let (_tempdir, _db_path, server) = setup_server();
        let tools = server.tool_router.list_all();
        let promote_tool = tools
            .iter()
            .find(|tool| tool.name == "mempal_knowledge_promote")
            .expect("mempal_knowledge_promote tool exists");
        assert!(
            promote_tool
                .description
                .as_deref()
                .unwrap_or_default()
                .contains("gate pass")
        );
        let demote_tool = tools
            .iter()
            .find(|tool| tool.name == "mempal_knowledge_demote")
            .expect("mempal_knowledge_demote tool exists");
        assert!(
            demote_tool
                .description
                .as_deref()
                .unwrap_or_default()
                .contains("counterexample evidence")
        );
        assert!(crate::core::protocol::MEMORY_PROTOCOL.contains("mempal_knowledge_promote"));
        assert!(crate::core::protocol::MEMORY_PROTOCOL.contains("MCP promotion is gate-enforced"));
    }

    #[tokio::test]
    async fn test_mcp_knowledge_publish_anchor_worktree_to_repo() {
        let (_tempdir, db_path, server) = setup_server();
        insert_knowledge_drawer_with_anchor(
            &db_path,
            "drawer_publish_worktree",
            KnowledgeStatus::Promoted,
            KnowledgeAnchorArgs {
                domain: MemoryDomain::Project,
                anchor_kind: AnchorKind::Worktree,
                anchor_id: "worktree:///tmp/mcp-publish-worktree",
                parent_anchor_id: Some("repo://parent"),
            },
        );
        let db = Database::open(&db_path).expect("open db");
        let before = db
            .get_drawer("drawer_publish_worktree")
            .expect("load drawer")
            .expect("drawer exists");
        let schema_before = db.schema_version().expect("schema");
        let vector_count_before = vector_row_count(&db, "drawer_publish_worktree");
        let audit_before = audit_line_count(&db_path);

        let response = server
            .knowledge_publish_anchor_json_for_test(serde_json::json!({
                "drawer_id": "drawer_publish_worktree",
                "to": "repo",
                "reason": "share stable MCP rule"
            }))
            .await
            .expect("publish should pass");

        assert_eq!(response.old_anchor_kind, "worktree");
        assert_eq!(
            response.old_anchor_id,
            "worktree:///tmp/mcp-publish-worktree"
        );
        assert_eq!(response.new_anchor_kind, "repo");
        assert_eq!(response.new_anchor_id, "repo://parent");
        assert_eq!(response.new_parent_anchor_id, None);
        let after = db
            .get_drawer("drawer_publish_worktree")
            .expect("load drawer")
            .expect("drawer exists");
        assert_eq!(after.anchor_kind, AnchorKind::Repo);
        assert_eq!(after.anchor_id, "repo://parent");
        assert_eq!(after.parent_anchor_id, None);
        assert_eq!(after.content, before.content);
        assert_eq!(after.statement, before.statement);
        assert_eq!(after.status, before.status);
        assert_eq!(after.supporting_refs, before.supporting_refs);
        assert_eq!(db.schema_version().expect("schema"), schema_before);
        assert_eq!(
            vector_row_count(&db, "drawer_publish_worktree"),
            vector_count_before
        );
        assert_eq!(audit_line_count(&db_path), audit_before + 1);
        assert_eq!(
            last_audit_entry(&db_path)["command"],
            "knowledge_publish_anchor"
        );
    }

    #[tokio::test]
    async fn test_mcp_knowledge_publish_anchor_repo_to_global() {
        let (_tempdir, db_path, server) = setup_server();
        insert_knowledge_drawer_with_anchor(
            &db_path,
            "drawer_publish_global",
            KnowledgeStatus::Canonical,
            KnowledgeAnchorArgs {
                domain: MemoryDomain::Global,
                anchor_kind: AnchorKind::Repo,
                anchor_id: "repo://global-ready",
                parent_anchor_id: None,
            },
        );

        let response = server
            .knowledge_publish_anchor_json_for_test(serde_json::json!({
                "drawer_id": "drawer_publish_global",
                "to": "global",
                "target_anchor_id": "global://epistemics",
                "reason": "global law",
                "reviewer": "human"
            }))
            .await
            .expect("publish should pass");

        assert_eq!(response.new_anchor_kind, "global");
        assert_eq!(response.new_anchor_id, "global://epistemics");
        let db = Database::open(&db_path).expect("open db");
        let drawer = db
            .get_drawer("drawer_publish_global")
            .expect("load drawer")
            .expect("drawer exists");
        assert_eq!(drawer.anchor_kind, AnchorKind::Global);
        assert_eq!(drawer.anchor_id, "global://epistemics");
        assert_eq!(last_audit_entry(&db_path)["details"]["reviewer"], "human");
    }

    #[tokio::test]
    async fn test_mcp_knowledge_publish_anchor_rejects_invalid_chain_without_mutation() {
        let (_tempdir, db_path, server) = setup_server();
        insert_knowledge_drawer_with_anchor(
            &db_path,
            "drawer_publish_invalid_chain",
            KnowledgeStatus::Promoted,
            KnowledgeAnchorArgs {
                domain: MemoryDomain::Global,
                anchor_kind: AnchorKind::Worktree,
                anchor_id: "worktree:///tmp/mcp-publish-invalid",
                parent_anchor_id: Some("repo://parent"),
            },
        );
        let db = Database::open(&db_path).expect("open db");
        let before = db
            .get_drawer("drawer_publish_invalid_chain")
            .expect("load drawer")
            .expect("drawer exists");
        let schema_before = db.schema_version().expect("schema");
        let vector_count_before = vector_row_count(&db, "drawer_publish_invalid_chain");
        let audit_before = audit_line_count(&db_path);

        let error = server
            .knowledge_publish_anchor_json_for_test(serde_json::json!({
                "drawer_id": "drawer_publish_invalid_chain",
                "to": "global",
                "target_anchor_id": "global://x",
                "reason": "skip chain"
            }))
            .await
            .expect_err("invalid chain should fail");

        assert!(
            error
                .to_string()
                .contains("worktree -> global publication is not allowed"),
            "error={error}"
        );
        let after = db
            .get_drawer("drawer_publish_invalid_chain")
            .expect("load drawer")
            .expect("drawer exists");
        assert_eq!(after.anchor_kind, before.anchor_kind);
        assert_eq!(after.anchor_id, before.anchor_id);
        assert_eq!(after.parent_anchor_id, before.parent_anchor_id);
        assert_eq!(db.schema_version().expect("schema"), schema_before);
        assert_eq!(
            vector_row_count(&db, "drawer_publish_invalid_chain"),
            vector_count_before
        );
        assert_eq!(audit_line_count(&db_path), audit_before);
    }

    #[tokio::test]
    async fn test_mcp_knowledge_publish_anchor_rejects_inactive_or_evidence() {
        let (_tempdir, db_path, server) = setup_server();
        insert_drawer(
            &db_path,
            "drawer_publish_evidence",
            "evidence",
            "mempal",
            Some("publish"),
            "/tmp/publish-evidence.md",
            2,
        );
        insert_knowledge_drawer_with_anchor(
            &db_path,
            "drawer_publish_candidate",
            KnowledgeStatus::Candidate,
            KnowledgeAnchorArgs {
                domain: MemoryDomain::Project,
                anchor_kind: AnchorKind::Worktree,
                anchor_id: "worktree:///tmp/mcp-publish-candidate",
                parent_anchor_id: Some("repo://parent"),
            },
        );

        let evidence_error = server
            .knowledge_publish_anchor_json_for_test(serde_json::json!({
                "drawer_id": "drawer_publish_evidence",
                "to": "repo",
                "reason": "bad"
            }))
            .await
            .expect_err("evidence should be rejected");
        assert!(
            evidence_error.to_string().contains("knowledge drawer"),
            "error={evidence_error}"
        );

        let candidate_error = server
            .knowledge_publish_anchor_json_for_test(serde_json::json!({
                "drawer_id": "drawer_publish_candidate",
                "to": "repo",
                "reason": "bad"
            }))
            .await
            .expect_err("candidate should be rejected");
        assert!(
            candidate_error
                .to_string()
                .contains("promoted or canonical"),
            "error={candidate_error}"
        );
    }

    #[test]
    fn test_mcp_tool_registry_and_protocol_include_knowledge_publish_anchor() {
        let (_tempdir, _db_path, server) = setup_server();
        let tools = server.tool_router.list_all();
        let publish_tool = tools
            .iter()
            .find(|tool| tool.name == "mempal_knowledge_publish_anchor")
            .expect("mempal_knowledge_publish_anchor tool exists");
        assert!(
            publish_tool
                .description
                .as_deref()
                .unwrap_or_default()
                .contains("outward across anchor scope")
        );
        assert!(crate::core::protocol::MEMORY_PROTOCOL.contains("mempal_knowledge_publish_anchor"));
        assert!(
            crate::core::protocol::MEMORY_PROTOCOL
                .contains("Anchor publication is separate from tier/status promotion")
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_mcp_fact_check_round_trip() {
        let (_tempdir, db_path, server) = setup_server();
        insert_triple(
            &db_path,
            "Bob",
            "husband_of",
            "Alice",
            Some("1799900000"),
            None,
        );
        insert_triple(
            &db_path,
            "Alice",
            "works_at",
            "Acme",
            Some("1700000000"),
            Some("1799999999"),
        );

        let response = server
            .mempal_fact_check(Parameters(FactCheckRequest {
                text: "Bob is Alice's brother. Alice works at Acme.".to_string(),
                wing: None,
                room: None,
                now: Some("2027-01-15T08:00:00Z".to_string()),
            }))
            .await
            .expect("fact check should succeed")
            .0;

        assert_eq!(response.issues.len(), 2, "issues={:?}", response.issues);

        let json = serde_json::to_vec(&response).expect("serialize");
        let back: FactCheckResponse = serde_json::from_slice(&json).expect("deserialize");
        assert_eq!(back.issues, response.issues);
        assert_eq!(back.checked_entities, response.checked_entities);
        assert_eq!(back.kg_triples_scanned, response.kg_triples_scanned);
    }

    #[tokio::test]
    async fn test_mcp_fact_check_invalid_scope_maps_to_invalid_params() {
        let (_tempdir, _db_path, server) = setup_server();

        let err = match server
            .mempal_fact_check(Parameters(FactCheckRequest {
                text: "Bob is Alice's brother".to_string(),
                wing: None,
                room: Some("design".to_string()),
                now: None,
            }))
            .await
        {
            Ok(_) => panic!("room without wing must be rejected"),
            Err(err) => err,
        };

        assert!(
            err.to_string().contains("room requires wing"),
            "expected invalid scope error, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_mcp_fact_check_invalid_now_maps_to_invalid_params() {
        let (_tempdir, _db_path, server) = setup_server();

        let err = match server
            .mempal_fact_check(Parameters(FactCheckRequest {
                text: "Bob is Alice's brother".to_string(),
                wing: None,
                room: None,
                now: Some("not-a-timestamp".to_string()),
            }))
            .await
        {
            Ok(_) => panic!("invalid now must be rejected"),
            Err(err) => err,
        };

        assert!(
            err.to_string().contains("expected RFC3339"),
            "expected invalid now error, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_mcp_ingest_response_exposes_lock_wait() {
        let (_tempdir, _db_path, server) = setup_server();

        let response = server
            .mempal_ingest(Parameters(IngestRequest {
                content: "same content for lock contention".to_string(),
                wing: "mempal".to_string(),
                room: Some("review".to_string()),
                source: None,
                importance: None,
                dry_run: None,
                diary_rollup: None,
                memory_kind: None,
                domain: None,
                field: None,
                provenance: None,
                statement: None,
                tier: None,
                status: None,
                supporting_refs: None,
                counterexample_refs: None,
                teaching_refs: None,
                verification_refs: None,
                scope_constraints: None,
                trigger_hints: None,
                anchor_kind: None,
                anchor_id: None,
                parent_anchor_id: None,
                cwd: None,
            }))
            .await
            .expect("ingest should succeed")
            .0;

        assert!(
            response.lock_wait_ms.is_some(),
            "non-dry-run MCP ingest must expose lock_wait_ms"
        );

        let json = serde_json::to_value(&response).expect("serialize");
        assert!(
            json.get("lock_wait_ms").is_some(),
            "JSON must expose lock_wait_ms"
        );
    }

    // =========================================================================
    // mempal_cowork_push MCP handler tests (P8 task 7, Codex review round-2 #2)
    // =========================================================================
    //
    // These tests exercise the HANDLER itself — caller identity inference,
    // target auto-inference, self-push rejection, and InboxError → ErrorData
    // mapping. They complement the integration tests in tests/cowork_inbox.rs,
    // which only cover the CLI and inbox layers.

    use super::super::tools::{CoworkBusRequest, CoworkPushRequest};
    use tokio::sync::Mutex as TokioMutex;

    // Tests below mutate $HOME env var to point mempal_home() at a tempdir.
    // Rust's default test runner runs tests in parallel threads, so they
    // would race on shared process state. Serialize them behind a process-
    // wide async Mutex whose guard CAN be held across .await points
    // (unlike std::sync::Mutex, which clippy rejects with await_holding_lock).
    // Every cowork push handler test must acquire this guard before
    // mutating $HOME and hold it for its entire lifetime.
    static COWORK_HOME_LOCK: TokioMutex<()> = TokioMutex::const_new(());

    async fn setup_cowork_home(
        tempdir: &TempDir,
    ) -> (PathBuf, PathBuf, tokio::sync::MutexGuard<'static, ()>) {
        // Lock FIRST before touching $HOME so no other parallel cowork
        // test can observe a half-written env var.
        let guard = COWORK_HOME_LOCK.lock().await;
        let home = tempdir.path().to_path_buf();
        let mempal_home = home.join(".mempal");
        let repo = home.join("proj");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        unsafe {
            std::env::set_var("HOME", &home);
        }
        (mempal_home, repo, guard)
    }

    async fn mcp_bus_register(server: &MempalMcpServer, repo: &Path, agent_id: &str, tool: &str) {
        let response = server
            .mempal_cowork_bus(Parameters(CoworkBusRequest {
                action: "register".to_string(),
                cwd: repo.to_string_lossy().into_owned(),
                agent_id: Some(agent_id.to_string()),
                tool: Some(tool.to_string()),
                transport: None,
                tmux_target: None,
                from: None,
                to: Vec::new(),
                agents: Vec::new(),
                message: None,
                thread_id: None,
                channel: None,
                message_id: None,
                limit: None,
                now: None,
                seen_at: None,
                lines: None,
                probe_tmux: None,
                session_id: None,
                title: None,
                goal: None,
                status: None,
                summary_source: None,
                wing: None,
                room: None,
                note: None,
                capture: None,
                execute: None,
            }))
            .await
            .unwrap_or_else(|e| panic!("register {agent_id} failed: {e}"));
        assert_eq!(response.0.action, "register");
    }

    fn bus_request(action: &str, repo: &Path) -> CoworkBusRequest {
        CoworkBusRequest {
            action: action.to_string(),
            cwd: repo.to_string_lossy().into_owned(),
            agent_id: None,
            tool: None,
            transport: None,
            tmux_target: None,
            from: None,
            to: Vec::new(),
            agents: Vec::new(),
            message: None,
            thread_id: None,
            channel: None,
            message_id: None,
            limit: None,
            now: None,
            seen_at: None,
            lines: None,
            probe_tmux: None,
            session_id: None,
            title: None,
            goal: None,
            status: None,
            summary_source: None,
            wing: None,
            room: None,
            note: None,
            capture: None,
            execute: None,
        }
    }

    fn install_fake_tmux_for_mcp(tempdir: &TempDir, exit_code: i32) -> PathBuf {
        let bin_dir = tempdir.path().join("fake-bin");
        std::fs::create_dir_all(&bin_dir).expect("fake bin");
        let log_path = tempdir.path().join("tmux.log");
        let script = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" >> \"$TMUX_LOG\"\nif [ \"$1\" = \"capture-pane\" ]; then printf 'mcp pane line\\n'; fi\nexit {exit_code}\n"
        );
        let tmux_path = bin_dir.join("tmux");
        std::fs::write(&tmux_path, script).expect("write fake tmux");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&tmux_path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&tmux_path, perms).unwrap();
        }
        let old_path = std::env::var("PATH").unwrap_or_default();
        unsafe {
            std::env::set_var("PATH", format!("{}:{old_path}", bin_dir.display()));
            std::env::set_var("TMUX_LOG", &log_path);
        }
        log_path
    }

    #[tokio::test]
    async fn test_mcp_push_without_client_info_rejects_auto_target() {
        let (tempdir, _db_path, server) = setup_server();
        let (_mempal_home, repo, _guard) = setup_cowork_home(&tempdir).await;

        // client_name is None because we never called initialize().
        // Pushing without an explicit target must fail with "cannot infer".
        let result = server
            .mempal_cowork_push(Parameters(CoworkPushRequest {
                content: "hello".into(),
                target_tool: None,
                cwd: repo.to_string_lossy().into_owned(),
            }))
            .await;

        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("expected push to fail when client_name is None"),
        };
        // MCP error message must mention inference failure.
        assert!(
            err.to_string().contains("cannot infer"),
            "expected inference error, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_mcp_cowork_bus_register_and_list() {
        let (tempdir, _db_path, server) = setup_server();
        let (_mempal_home, repo, _guard) = setup_cowork_home(&tempdir).await;

        mcp_bus_register(&server, &repo, "claude-main", "claude").await;
        mcp_bus_register(&server, &repo, "codex-a", "codex").await;
        mcp_bus_register(&server, &repo, "codex-b", "codex").await;

        let response = server
            .mempal_cowork_bus(Parameters(bus_request("list", &repo)))
            .await
            .expect("list bus");
        let ids: Vec<String> = response
            .0
            .agents
            .iter()
            .map(|agent| agent.agent_id.clone())
            .collect();
        assert_eq!(ids, vec!["claude-main", "codex-a", "codex-b"]);
    }

    #[tokio::test]
    async fn test_mcp_cowork_bus_send_drains_only_target() {
        let (tempdir, _db_path, server) = setup_server();
        let (_mempal_home, repo, _guard) = setup_cowork_home(&tempdir).await;

        mcp_bus_register(&server, &repo, "claude-main", "claude").await;
        mcp_bus_register(&server, &repo, "codex-a", "codex").await;
        mcp_bus_register(&server, &repo, "codex-b", "codex").await;

        let mut send = bus_request("send", &repo);
        send.from = Some("claude-main".to_string());
        send.to = vec!["codex-a".to_string()];
        send.message = Some("review mcp bus routing".to_string());
        let response = server
            .mempal_cowork_bus(Parameters(send))
            .await
            .expect("send bus message");
        assert_eq!(response.0.delivered.len(), 1);
        assert_eq!(response.0.delivered[0].target_agent_id, "codex-a");

        let mut drain_a = bus_request("drain", &repo);
        drain_a.agent_id = Some("codex-a".to_string());
        let response_a = server
            .mempal_cowork_bus(Parameters(drain_a))
            .await
            .expect("drain codex-a");
        assert_eq!(response_a.0.messages.len(), 1);
        assert_eq!(response_a.0.messages[0].content, "review mcp bus routing");

        let mut drain_b = bus_request("drain", &repo);
        drain_b.agent_id = Some("codex-b".to_string());
        let response_b = server
            .mempal_cowork_bus(Parameters(drain_b))
            .await
            .expect("drain codex-b");
        assert!(response_b.0.messages.is_empty());
    }

    #[tokio::test]
    async fn test_mcp_cowork_bus_broadcast_fans_out() {
        let (tempdir, _db_path, server) = setup_server();
        let (_mempal_home, repo, _guard) = setup_cowork_home(&tempdir).await;

        mcp_bus_register(&server, &repo, "claude-main", "claude").await;
        mcp_bus_register(&server, &repo, "codex-a", "codex").await;
        mcp_bus_register(&server, &repo, "codex-b", "codex").await;

        let mut broadcast = bus_request("broadcast", &repo);
        broadcast.from = Some("claude-main".to_string());
        broadcast.to = vec!["codex-a".to_string(), "codex-b".to_string()];
        broadcast.message = Some("pull latest before continuing".to_string());
        let response = server
            .mempal_cowork_bus(Parameters(broadcast))
            .await
            .expect("broadcast bus message");
        assert_eq!(response.0.delivered.len(), 2);

        for agent_id in ["codex-a", "codex-b"] {
            let mut drain = bus_request("drain", &repo);
            drain.agent_id = Some(agent_id.to_string());
            let response = server
                .mempal_cowork_bus(Parameters(drain))
                .await
                .expect("drain target");
            assert_eq!(response.0.messages.len(), 1);
            assert_eq!(
                response.0.messages[0].content,
                "pull latest before continuing"
            );
        }
    }

    #[tokio::test]
    async fn test_mcp_cowork_bus_send_to_tmux_transport_invokes_tmux() {
        let (tempdir, _db_path, server) = setup_server();
        let (_mempal_home, repo, _guard) = setup_cowork_home(&tempdir).await;
        let log_path = install_fake_tmux_for_mcp(&tempdir, 0);

        mcp_bus_register(&server, &repo, "claude-main", "claude").await;
        let mut register_tmux = bus_request("register", &repo);
        register_tmux.agent_id = Some("codex-a".to_string());
        register_tmux.tool = Some("codex".to_string());
        register_tmux.transport = Some("tmux".to_string());
        register_tmux.tmux_target = Some("mempal:0.1".to_string());
        server
            .mempal_cowork_bus(Parameters(register_tmux))
            .await
            .expect("register tmux target");

        let mut send = bus_request("send", &repo);
        send.from = Some("claude-main".to_string());
        send.to = vec!["codex-a".to_string()];
        send.message = Some("mcp tmux hello".to_string());
        let response = server
            .mempal_cowork_bus(Parameters(send))
            .await
            .expect("send to tmux target");
        assert_eq!(response.0.delivered.len(), 1);
        assert_eq!(response.0.delivered[0].target_agent_id, "codex-a");
        assert_eq!(response.0.delivered[0].transport, "tmux");

        let log = std::fs::read_to_string(log_path).expect("tmux log");
        assert!(log.contains("send-keys"), "{log}");
        assert!(log.contains("mempal:0.1"), "{log}");
        assert!(
            log.contains("[mempal bus from claude-main to codex-a] mcp tmux hello"),
            "{log}"
        );
    }

    #[tokio::test]
    async fn test_mcp_cowork_bus_events_lists_log() {
        let (tempdir, _db_path, server) = setup_server();
        let (mempal_home, repo, _guard) = setup_cowork_home(&tempdir).await;
        mcp_bus_register(&server, &repo, "claude-main", "claude").await;
        mcp_bus_register(&server, &repo, "codex-a", "codex").await;

        let mut send = bus_request("send", &repo);
        send.from = Some("claude-main".to_string());
        send.to = vec!["codex-a".to_string()];
        send.message = Some("event replay through mcp".to_string());
        server
            .mempal_cowork_bus(Parameters(send))
            .await
            .expect("send bus message");

        let mut events = bus_request("events", &repo);
        events.limit = Some(2);
        let response = server
            .mempal_cowork_bus(Parameters(events))
            .await
            .expect("list events");
        assert_eq!(response.0.action, "events");
        assert_eq!(response.0.events.len(), 2);
        assert!(
            response
                .0
                .events
                .iter()
                .any(|event| event.event_type == "send" && event.status == "delivered")
        );
        assert!(
            !mempal_home.join("palace.db").exists(),
            "cowork bus events must not write the db-backed memory store"
        );

        let mut drain = bus_request("drain", &repo);
        drain.agent_id = Some("codex-a".to_string());
        let drained = server
            .mempal_cowork_bus(Parameters(drain))
            .await
            .expect("drain after event replay");
        assert_eq!(drained.0.messages.len(), 1);
        assert_eq!(drained.0.messages[0].content, "event replay through mcp");
    }

    #[tokio::test]
    async fn test_mcp_cowork_bus_deliveries_and_ack() {
        let (tempdir, _db_path, server) = setup_server();
        let (mempal_home, repo, _guard) = setup_cowork_home(&tempdir).await;
        mcp_bus_register(&server, &repo, "claude-main", "claude").await;
        mcp_bus_register(&server, &repo, "codex-a", "codex").await;

        let mut send = bus_request("send", &repo);
        send.from = Some("claude-main".to_string());
        send.to = vec!["codex-a".to_string()];
        send.message = Some("ack through mcp".to_string());
        let sent = server
            .mempal_cowork_bus(Parameters(send))
            .await
            .expect("send bus message");
        let message_id = sent.0.delivered[0].message_id.clone();

        let mut ack = bus_request("ack", &repo);
        ack.agent_id = Some("codex-a".to_string());
        ack.message_id = Some(message_id.clone());
        let acked = server
            .mempal_cowork_bus(Parameters(ack))
            .await
            .expect("ack delivery");
        assert_eq!(acked.0.deliveries.len(), 1);
        assert_eq!(acked.0.deliveries[0].status, "acked");

        let mut deliveries = bus_request("deliveries", &repo);
        deliveries.agent_id = Some("codex-a".to_string());
        let response = server
            .mempal_cowork_bus(Parameters(deliveries))
            .await
            .expect("delivery statuses");
        assert_eq!(response.0.deliveries.len(), 1);
        assert_eq!(response.0.deliveries[0].message_id, message_id);
        assert_eq!(response.0.deliveries[0].status, "acked");
        assert_eq!(
            response.0.deliveries[0].acked_by.as_deref(),
            Some("codex-a")
        );
        assert!(
            !mempal_home.join("palace.db").exists(),
            "cowork delivery status must not write the db-backed memory store"
        );
    }

    #[tokio::test]
    async fn test_mcp_cowork_bus_heartbeat_and_presence() {
        let (tempdir, _db_path, server) = setup_server();
        let (mempal_home, repo, _guard) = setup_cowork_home(&tempdir).await;
        mcp_bus_register(&server, &repo, "codex-a", "codex").await;

        let mut heartbeat = bus_request("heartbeat", &repo);
        heartbeat.agent_id = Some("codex-a".to_string());
        heartbeat.seen_at = Some("2026-05-25T00:00:00Z".to_string());
        let response = server
            .mempal_cowork_bus(Parameters(heartbeat))
            .await
            .expect("heartbeat");
        assert_eq!(response.0.action, "heartbeat");
        assert_eq!(
            response.0.agents[0].last_seen_at.as_deref(),
            Some("2026-05-25T00:00:00Z")
        );

        let mut list = bus_request("list", &repo);
        list.now = Some("2026-05-25T00:05:00Z".to_string());
        let listed = server
            .mempal_cowork_bus(Parameters(list))
            .await
            .expect("list presence");
        assert_eq!(listed.0.agents[0].agent_id, "codex-a");
        assert_eq!(listed.0.agents[0].presence, "online");
        assert_eq!(
            listed.0.agents[0].last_seen_at.as_deref(),
            Some("2026-05-25T00:00:00Z")
        );
        assert!(
            !mempal_home.join("palace.db").exists(),
            "cowork presence must not write the db-backed memory store"
        );
    }

    #[tokio::test]
    async fn test_mcp_cowork_bus_channel_send() {
        let (tempdir, _db_path, server) = setup_server();
        let (mempal_home, repo, _guard) = setup_cowork_home(&tempdir).await;
        mcp_bus_register(&server, &repo, "claude-main", "claude").await;
        mcp_bus_register(&server, &repo, "codex-a", "codex").await;
        mcp_bus_register(&server, &repo, "codex-b", "codex").await;

        let mut set = bus_request("channel_set", &repo);
        set.channel = Some("review".to_string());
        set.agents = vec!["codex-a".to_string(), "codex-b".to_string()];
        let set_response = server
            .mempal_cowork_bus(Parameters(set))
            .await
            .expect("set channel");
        assert_eq!(set_response.0.channels.len(), 1);
        assert_eq!(set_response.0.channels[0].channel, "review");

        let mut send = bus_request("channel_send", &repo);
        send.from = Some("claude-main".to_string());
        send.channel = Some("review".to_string());
        send.thread_id = Some("p90-review".to_string());
        send.message = Some("mcp channel send".to_string());
        let response = server
            .mempal_cowork_bus(Parameters(send))
            .await
            .expect("send channel");
        assert_eq!(response.0.delivered.len(), 2);
        assert!(
            response
                .0
                .delivered
                .iter()
                .all(|delivery| delivery.channel.as_deref() == Some("review"))
        );
        assert!(
            response
                .0
                .delivered
                .iter()
                .all(|delivery| delivery.thread_id.as_deref() == Some("p90-review"))
        );
        assert!(
            !mempal_home.join("palace.db").exists(),
            "cowork channels must not write the db-backed memory store"
        );
    }

    #[tokio::test]
    async fn test_mcp_cowork_bus_tmux_peek() {
        let (tempdir, _db_path, server) = setup_server();
        let (mempal_home, repo, _guard) = setup_cowork_home(&tempdir).await;
        let log_path = install_fake_tmux_for_mcp(&tempdir, 0);

        let mut register_tmux = bus_request("register", &repo);
        register_tmux.agent_id = Some("codex-a".to_string());
        register_tmux.tool = Some("codex".to_string());
        register_tmux.transport = Some("tmux".to_string());
        register_tmux.tmux_target = Some("mempal:0.1".to_string());
        server
            .mempal_cowork_bus(Parameters(register_tmux))
            .await
            .expect("register tmux target");

        let mut peek = bus_request("tmux_peek", &repo);
        peek.agent_id = Some("codex-a".to_string());
        peek.lines = Some(20);
        let response = server
            .mempal_cowork_bus(Parameters(peek))
            .await
            .expect("tmux peek");
        let peek = response.0.tmux_peek.expect("peek payload");
        assert_eq!(peek.agent_id, "codex-a");
        assert_eq!(peek.tmux_target, "mempal:0.1");
        assert_eq!(peek.lines, 20);
        assert!(peek.content.contains("mcp pane line"));
        let log = std::fs::read_to_string(log_path).expect("tmux log");
        assert!(log.contains("capture-pane"), "{log}");
        assert!(
            !mempal_home.join("palace.db").exists(),
            "tmux peek must not write the db-backed memory store"
        );
    }

    #[tokio::test]
    async fn test_mcp_cowork_bus_doctor() {
        let (tempdir, _db_path, server) = setup_server();
        let (mempal_home, repo, _guard) = setup_cowork_home(&tempdir).await;
        mcp_bus_register(&server, &repo, "claude-main", "claude").await;
        mcp_bus_register(&server, &repo, "codex-a", "codex").await;

        let mut send = bus_request("send", &repo);
        send.from = Some("claude-main".to_string());
        send.to = vec!["codex-a".to_string()];
        send.message = Some("doctor pending".to_string());
        server
            .mempal_cowork_bus(Parameters(send))
            .await
            .expect("send");

        let response = server
            .mempal_cowork_bus(Parameters(bus_request("doctor", &repo)))
            .await
            .expect("doctor");
        let doctor = response.0.doctor.expect("doctor payload");
        assert_eq!(doctor.pending_deliveries, 1);
        assert_eq!(doctor.status, "warning");
        assert!(
            !mempal_home.join("palace.db").exists(),
            "doctor must not write the db-backed memory store"
        );
    }

    #[tokio::test]
    async fn test_mcp_cowork_bus_sessions() {
        let (tempdir, _db_path, server) = setup_server();
        let (mempal_home, repo, _guard) = setup_cowork_home(&tempdir).await;
        mcp_bus_register(&server, &repo, "claude-main", "claude").await;
        mcp_bus_register(&server, &repo, "codex-a", "codex").await;

        let mut create = bus_request("session_create", &repo);
        create.session_id = Some("review-1".to_string());
        create.title = Some("Review 1".to_string());
        create.agents = vec!["claude-main".to_string(), "codex-a".to_string()];
        create.thread_id = Some("review-1".to_string());
        let response = server
            .mempal_cowork_bus(Parameters(create))
            .await
            .expect("create session");
        assert_eq!(response.0.sessions[0].session_id, "review-1");

        let response = server
            .mempal_cowork_bus(Parameters(bus_request("session_list", &repo)))
            .await
            .expect("list sessions");
        assert_eq!(response.0.sessions.len(), 1);
        assert_eq!(
            response.0.sessions[0].thread_id.as_deref(),
            Some("review-1")
        );
        assert!(
            !mempal_home.join("palace.db").exists(),
            "sessions must not write the db-backed memory store"
        );
    }

    #[tokio::test]
    async fn test_mcp_cowork_bus_session_close() {
        let (tempdir, db_path, server) = setup_server();
        let (mempal_home, repo, _guard) = setup_cowork_home(&tempdir).await;
        mcp_bus_register(&server, &repo, "claude-main", "claude").await;
        mcp_bus_register(&server, &repo, "codex-a", "codex").await;

        let mut create = bus_request("session_create", &repo);
        create.session_id = Some("review-1".to_string());
        create.title = Some("Review 1".to_string());
        create.agents = vec!["claude-main".to_string(), "codex-a".to_string()];
        server
            .mempal_cowork_bus(Parameters(create))
            .await
            .expect("create session");

        let mut close = bus_request("session_close", &repo);
        close.session_id = Some("review-1".to_string());
        close.capture = Some(true);
        close.execute = Some(false);
        let response = server
            .mempal_cowork_bus(Parameters(close))
            .await
            .expect("close session");
        assert_eq!(response.0.sessions[0].status, "closed");
        assert!(!response.0.capture.as_ref().expect("capture payload").writes);
        assert!(
            !db_path.exists(),
            "dry-run session_close capture must not create palace.db"
        );
        assert!(
            mempal_home.join("cowork-bus").exists(),
            "session close should remain a cowork bus runtime operation"
        );
    }

    #[tokio::test]
    async fn test_mcp_cowork_bus_handoff() {
        let (tempdir, _db_path, server) = setup_server();
        let (mempal_home, repo, _guard) = setup_cowork_home(&tempdir).await;
        mcp_bus_register(&server, &repo, "claude-main", "claude").await;
        mcp_bus_register(&server, &repo, "codex-a", "codex").await;

        let mut create = bus_request("session_create", &repo);
        create.session_id = Some("review-1".to_string());
        create.title = Some("Review 1".to_string());
        create.agents = vec!["claude-main".to_string(), "codex-a".to_string()];
        server
            .mempal_cowork_bus(Parameters(create))
            .await
            .expect("create session");

        let mut send = bus_request("send", &repo);
        send.from = Some("claude-main".to_string());
        send.to = vec!["codex-a".to_string()];
        send.message = Some("handoff pending".to_string());
        server
            .mempal_cowork_bus(Parameters(send))
            .await
            .expect("send");

        let response = server
            .mempal_cowork_bus(Parameters(bus_request("handoff", &repo)))
            .await
            .expect("handoff");
        let handoff = response.0.handoff.expect("handoff payload");
        assert_eq!(handoff.sessions.len(), 1);
        assert_eq!(handoff.pending_deliveries.len(), 1);
        assert!(
            !mempal_home.join("palace.db").exists(),
            "handoff must not write the db-backed memory store"
        );
    }

    #[tokio::test]
    async fn test_mcp_cowork_bus_capture() {
        let (tempdir, db_path, server) = setup_server();
        let (_mempal_home, repo, _guard) = setup_cowork_home(&tempdir).await;
        mcp_bus_register(&server, &repo, "claude-main", "claude").await;

        let mut dry_run = bus_request("capture", &repo);
        dry_run.summary_source = Some("handoff".to_string());
        let response = server
            .mempal_cowork_bus(Parameters(dry_run))
            .await
            .expect("capture dry run");
        let capture = response.0.capture.expect("capture payload");
        assert!(!capture.writes);
        assert!(!db_path.exists(), "dry-run capture must not open palace.db");

        let mut execute = bus_request("capture", &repo);
        execute.summary_source = Some("handoff".to_string());
        execute.execute = Some(true);
        let response = server
            .mempal_cowork_bus(Parameters(execute))
            .await
            .expect("capture execute");
        let capture = response.0.capture.expect("capture payload");
        assert!(capture.writes);
        let drawer_id = capture.drawer_id.expect("drawer id");
        let db = Database::open(&db_path).expect("open db");
        assert!(
            db.get_drawer(&drawer_id)
                .expect("get drawer")
                .expect("drawer exists")
                .content
                .contains("Cowork Handoff Capture")
        );
    }

    #[tokio::test]
    async fn test_mcp_cowork_bus_rejects_invalid_action_and_addressing() {
        let (tempdir, _db_path, server) = setup_server();
        let (_mempal_home, repo, _guard) = setup_cowork_home(&tempdir).await;
        mcp_bus_register(&server, &repo, "codex-a", "codex").await;

        let mut bad_id = bus_request("register", &repo);
        bad_id.agent_id = Some("bad/id".to_string());
        bad_id.tool = Some("codex".to_string());
        let err = match server.mempal_cowork_bus(Parameters(bad_id)).await {
            Err(error) => error,
            Ok(_) => panic!("bad agent id must fail"),
        };
        assert!(err.to_string().contains("invalid agent id"));

        let mut self_send = bus_request("send", &repo);
        self_send.from = Some("codex-a".to_string());
        self_send.to = vec!["codex-a".to_string()];
        self_send.message = Some("self".to_string());
        let err = match server.mempal_cowork_bus(Parameters(self_send)).await {
            Err(error) => error,
            Ok(_) => panic!("self-send must fail"),
        };
        assert!(err.to_string().contains("cannot send to self"));

        let err = match server
            .mempal_cowork_bus(Parameters(bus_request("unknown", &repo)))
            .await
        {
            Err(error) => error,
            Ok(_) => panic!("unknown action must fail"),
        };
        assert!(err.to_string().contains("unknown action"));
    }

    #[test]
    fn test_mcp_tool_registry_and_protocol_include_cowork_bus() {
        let (_tempdir, _db_path, server) = setup_server();
        let tools = server.tool_router.list_all();
        let tool = tools
            .iter()
            .find(|tool| tool.name == "mempal_cowork_bus")
            .expect("mempal_cowork_bus tool exists");
        let description = tool.description.as_deref().unwrap_or_default();
        assert!(description.contains("Multi-agent cowork bus"));
        assert!(description.contains("agent_id"));
        assert!(description.contains("register/list/send/broadcast/drain"));
        assert!(crate::core::protocol::MEMORY_PROTOCOL.contains("mempal_cowork_bus"));
        assert!(crate::core::protocol::MEMORY_PROTOCOL.contains("concrete agent_id"));
        assert!(
            crate::core::protocol::MEMORY_PROTOCOL
                .contains("separate from legacy mempal_cowork_push")
        );
    }

    #[tokio::test]
    async fn test_mcp_push_succeeds_with_captured_client_name_and_auto_target() {
        let (tempdir, _db_path, server) = setup_server();
        let (mempal_home, repo, _guard) = setup_cowork_home(&tempdir).await;

        // Simulate a completed `initialize` handshake: caller identified
        // as "claude-code" (Claude Code's standard MCP client name).
        *server.client_name.lock().unwrap() = Some("claude-code".to_string());

        let response = match server
            .mempal_cowork_push(Parameters(CoworkPushRequest {
                content: "from claude to partner".into(),
                target_tool: None,
                cwd: repo.to_string_lossy().into_owned(),
            }))
            .await
        {
            Ok(r) => r,
            Err(e) => panic!("push should succeed with valid client_name: {e}"),
        };

        // Target auto-inferred as partner of Claude → Codex.
        assert_eq!(response.0.target_tool, "codex");
        assert!(response.0.inbox_size_after > 0);

        // Verify the message actually landed in the codex inbox by draining.
        let messages = crate::cowork::inbox::drain(&mempal_home, Tool::Codex, &repo).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "from claude to partner");
        assert_eq!(messages[0].from, "claude");
    }

    #[tokio::test]
    async fn test_mcp_push_self_push_rejected_via_inbox_error_mapping() {
        let (tempdir, _db_path, server) = setup_server();
        let (_mempal_home, repo, _guard) = setup_cowork_home(&tempdir).await;

        // Caller is Codex, target explicitly Codex → SelfPush error from
        // inbox::push. Handler must map it to InvalidParams MCP error.
        *server.client_name.lock().unwrap() = Some("codex".to_string());

        let err = match server
            .mempal_cowork_push(Parameters(CoworkPushRequest {
                content: "would be self push".into(),
                target_tool: Some("codex".to_string()),
                cwd: repo.to_string_lossy().into_owned(),
            }))
            .await
        {
            Err(e) => e,
            Ok(_) => panic!("expected self-push to be rejected"),
        };

        assert!(
            err.to_string().contains("self"),
            "expected self-push error, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_mcp_push_explicit_target_overrides_auto_inference() {
        let (tempdir, _db_path, server) = setup_server();
        let (mempal_home, repo, _guard) = setup_cowork_home(&tempdir).await;

        *server.client_name.lock().unwrap() = Some("claude-code".to_string());

        // Caller=Claude; auto would infer Codex. Override explicitly to Codex
        // (same effective target, but proves the explicit branch runs).
        let response = match server
            .mempal_cowork_push(Parameters(CoworkPushRequest {
                content: "explicit target".into(),
                target_tool: Some("codex".to_string()),
                cwd: repo.to_string_lossy().into_owned(),
            }))
            .await
        {
            Ok(r) => r,
            Err(e) => panic!("explicit target push should succeed: {e}"),
        };
        assert_eq!(response.0.target_tool, "codex");

        let messages = crate::cowork::inbox::drain(&mempal_home, Tool::Codex, &repo).unwrap();
        assert_eq!(messages.len(), 1);
    }

    #[tokio::test]
    async fn test_mcp_push_rejects_explicit_auto_target() {
        // Guard for Codex review finding 1: `target_tool="auto"` must NOT
        // be accepted as an explicit target. Per spec lines 37/39 target is
        // limited to claude|codex. Previously `Tool::from_str_ci` let "auto"
        // through, which would silently write to an orphan
        // ~/.mempal/cowork-inbox/auto/*.jsonl that no partner drains.
        let (tempdir, _db_path, server) = setup_server();
        let (mempal_home, repo, _guard) = setup_cowork_home(&tempdir).await;

        *server.client_name.lock().unwrap() = Some("claude-code".to_string());

        for bad in ["auto", "AUTO", "Auto"] {
            let err = match server
                .mempal_cowork_push(Parameters(CoworkPushRequest {
                    content: "should not land".into(),
                    target_tool: Some(bad.to_string()),
                    cwd: repo.to_string_lossy().into_owned(),
                }))
                .await
            {
                Err(e) => e,
                Ok(_) => panic!("target_tool={bad:?} must be rejected"),
            };
            assert!(
                err.to_string().contains("expected claude|codex"),
                "error for target_tool={bad:?} should mention expected targets, got: {err}"
            );
        }

        // And ensure nothing was written to the orphan `auto/` inbox dir.
        let auto_inbox_dir = mempal_home.join("cowork-inbox").join("auto");
        assert!(
            !auto_inbox_dir.exists(),
            "rejected push must not create orphan auto/ inbox dir at {}",
            auto_inbox_dir.display()
        );
    }
}
