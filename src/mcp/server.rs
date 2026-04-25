use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::context::assemble_context_with_vector;
use crate::core::{
    anchor::{self, DerivedAnchor},
    db::Database,
    types::{
        AnchorKind, BootstrapIdentityParts, Drawer, ExplicitTunnel, KnowledgeStatus, KnowledgeTier,
        MemoryDomain, MemoryKind, Provenance, SourceType, TriggerHints, Triple,
    },
    utils::{
        build_bootstrap_drawer_id_from_parts, build_triple_id, current_timestamp,
        knowledge_source_file, source_file_or_synthetic,
    },
};
use crate::cowork::{PeekError, PeekRequest as CoworkPeekRequest, Tool, peek_partner};
use crate::embed::EmbedderFactory;
use crate::ingest::{
    IngestError,
    diary::{
        DIARY_ROLLUP_WING, DiaryRollupOptions, commit_prepared_diary_rollup,
        diary_rollup_drawer_id, prepare_diary_rollup,
    },
    normalize::CURRENT_NORMALIZE_VERSION,
};
use crate::knowledge_distill::{
    DistillPlan, DistillRequest as CoreDistillRequest, commit_distill, prepare_distill,
};
use crate::knowledge_gate::evaluate_gate_by_id;
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
    ContextRequest, ContextResponse, CoworkPushRequest, CoworkPushResponse, DeleteRequest,
    DeleteResponse, DuplicateWarning, FactCheckRequest, FactCheckResponse, IngestRequest,
    IngestResponse, KgRequest, KgResponse, KgStatsDto, KnowledgeDistillRequest,
    KnowledgeDistillResponse, KnowledgeGateRequest, KnowledgeGateResponse, PeekMessageDto,
    PeekPartnerRequest, PeekPartnerResponse, ScopeCount, SearchRequest, SearchResponse,
    SearchResultDto, StatusResponse, TaxonomyEntryDto, TaxonomyRequest, TaxonomyResponse,
    TriggerHintsDto, TripleDto, TunnelDto, TunnelEndpointDto, TunnelsRequest, TunnelsResponse,
};

#[derive(Clone)]
pub struct MempalMcpServer {
    db_path: PathBuf,
    embedder_factory: Arc<dyn EmbedderFactory>,
    tool_router: ToolRouter<Self>,
    /// Captured via `initialize` override so `auto` peek mode can infer the
    /// partner from the calling MCP client's self-reported name.
    client_name: Arc<Mutex<Option<String>>>,
}

impl MempalMcpServer {
    pub fn new(db_path: PathBuf, config: crate::core::config::Config) -> Self {
        Self::new_with_factory(
            db_path,
            Arc::new(crate::embed::ConfiguredEmbedderFactory::new(config)),
        )
    }

    pub fn new_with_factory(db_path: PathBuf, embedder_factory: Arc<dyn EmbedderFactory>) -> Self {
        Self {
            db_path,
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
        description = "Assemble a mind-model runtime context pack from typed memory. Use this when you need ordered guidance rather than raw search results: dao_tian -> dao_ren -> shu -> qi, with evidence opt-in. Returns source-backed items with drawer_id/source_file citations and trigger_hints metadata, but never executes skills."
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
                max_items,
            },
            &query_vector,
        )
        .map_err(context_error)?;

        Ok(Json(ContextResponse::from(pack)))
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

fn context_error(error: crate::context::ContextError) -> ErrorData {
    match error {
        crate::context::ContextError::DeriveAnchor(_) => {
            ErrorData::invalid_params(error.to_string(), None)
        }
        crate::context::ContextError::EmbedQuery(_)
        | crate::context::ContextError::MissingQueryVector
        | crate::context::ContextError::Search(_)
        | crate::context::ContextError::LoadDrawer(_) => {
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
    use crate::core::types::BootstrapEvidenceArgs;
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

    fn audit_line_count(db_path: &Path) -> usize {
        let audit_path = db_path
            .parent()
            .expect("db path has parent")
            .join("audit.jsonl");
        fs::read_to_string(audit_path)
            .map(|content| content.lines().count())
            .unwrap_or(0)
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

    use super::super::tools::CoworkPushRequest;
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
