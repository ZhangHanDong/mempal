#![warn(clippy::all)]

use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::Serialize;
use thiserror::Error;

use crate::core::{
    anchor,
    db::Database,
    db::DbError,
    types::{
        AnchorKind, KnowledgeStatus, KnowledgeTier, MemoryDomain, MemoryKind, RouteDecision,
        SearchResult, TriggerHints,
    },
};
use crate::embed::{EmbedError, Embedder};
use crate::search::{SearchError, SearchFilters, SearchOptions, search_with_vector_options};

pub type Result<T> = std::result::Result<T, ContextError>;

#[derive(Debug, Error)]
pub enum ContextError {
    #[error("failed to derive context anchors")]
    DeriveAnchor(#[from] anchor::AnchorError),
    #[error("failed to embed context query")]
    EmbedQuery(#[source] EmbedError),
    #[error("embedder returned no context query vector")]
    MissingQueryVector,
    #[error("failed to search context candidates")]
    Search(#[source] SearchError),
    #[error("failed to load context drawer metadata")]
    LoadDrawer(#[source] DbError),
}

#[derive(Debug, Clone)]
pub struct ContextRequest {
    pub query: String,
    pub domain: MemoryDomain,
    pub field: String,
    pub cwd: PathBuf,
    pub include_evidence: bool,
    pub max_items: usize,
    pub dao_tian_limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContextAnchor {
    pub anchor_kind: AnchorKind,
    pub anchor_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextPack {
    pub query: String,
    pub domain: MemoryDomain,
    pub field: String,
    pub anchors: Vec<ContextAnchor>,
    pub sections: Vec<ContextSection>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextSection {
    pub name: String,
    pub items: Vec<ContextItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextItem {
    pub drawer_id: String,
    pub source_file: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<KnowledgeTier>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<KnowledgeStatus>,
    pub anchor_kind: AnchorKind,
    pub anchor_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_anchor_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_hints: Option<TriggerHints>,
}

#[derive(Debug, Clone)]
struct AnchorCandidate {
    anchor_kind: AnchorKind,
    anchor_id: String,
    domain: MemoryDomain,
}

#[derive(Debug, Clone)]
struct CandidateQuery<'a> {
    request: &'a ContextRequest,
    query_vector: &'a [f32],
    route: &'a RouteDecision,
    anchor: &'a AnchorCandidate,
    memory_kind: MemoryKind,
    tier: Option<KnowledgeTier>,
    status: Option<KnowledgeStatus>,
    top_k: usize,
}

pub async fn assemble_context<E: Embedder + ?Sized>(
    db: &Database,
    embedder: &E,
    request: ContextRequest,
) -> Result<ContextPack> {
    let query_vector = embedder
        .embed(&[request.query.as_str()])
        .await
        .map_err(ContextError::EmbedQuery)?
        .into_iter()
        .next()
        .ok_or(ContextError::MissingQueryVector)?;

    assemble_context_with_vector(db, request, &query_vector)
}

pub fn assemble_context_with_vector(
    db: &Database,
    request: ContextRequest,
    query_vector: &[f32],
) -> Result<ContextPack> {
    let anchors = context_anchors(&request)?;
    let route = RouteDecision {
        wing: None,
        room: None,
        confidence: 0.0,
        reason: "mind-model context assembly".to_string(),
    };

    let mut sections = Vec::new();
    let mut remaining = request.max_items;
    let mut seen = BTreeSet::new();

    for tier in tier_order() {
        if remaining == 0 {
            break;
        }
        let mut tier_remaining = if matches!(tier, KnowledgeTier::DaoTian) {
            request.dao_tian_limit.min(remaining)
        } else {
            remaining
        };
        if tier_remaining == 0 {
            continue;
        }
        let mut items = Vec::new();
        for anchor in &anchors {
            if remaining == 0 || tier_remaining == 0 {
                break;
            }
            for status in active_statuses() {
                if remaining == 0 || tier_remaining == 0 {
                    break;
                }
                let mut results = search_context_candidates(
                    db,
                    CandidateQuery {
                        request: &request,
                        query_vector,
                        route: &route,
                        anchor,
                        memory_kind: MemoryKind::Knowledge,
                        tier: Some(tier.clone()),
                        status: Some(status.clone()),
                        top_k: tier_remaining,
                    },
                )?;
                results.retain(|result| result.anchor_id == anchor.anchor_id);
                for result in results {
                    if remaining == 0 || tier_remaining == 0 {
                        break;
                    }
                    if !seen.insert(result.drawer_id.clone()) {
                        continue;
                    }
                    items.push(context_item_from_result(db, result)?);
                    remaining -= 1;
                    tier_remaining -= 1;
                }
            }
        }
        if !items.is_empty() {
            sections.push(ContextSection {
                name: tier_slug(tier).to_string(),
                items,
            });
        }
    }

    if request.include_evidence && remaining > 0 {
        let mut items = Vec::new();
        for anchor in &anchors {
            if remaining == 0 {
                break;
            }
            let mut results = search_context_candidates(
                db,
                CandidateQuery {
                    request: &request,
                    query_vector,
                    route: &route,
                    anchor,
                    memory_kind: MemoryKind::Evidence,
                    tier: None,
                    status: None,
                    top_k: remaining,
                },
            )?;
            results.retain(|result| result.anchor_id == anchor.anchor_id);
            for result in results {
                if remaining == 0 {
                    break;
                }
                if !seen.insert(result.drawer_id.clone()) {
                    continue;
                }
                items.push(context_item_from_result(db, result)?);
                remaining -= 1;
            }
        }
        if !items.is_empty() {
            sections.push(ContextSection {
                name: "evidence".to_string(),
                items,
            });
        }
    }

    Ok(ContextPack {
        query: request.query,
        domain: request.domain,
        field: request.field,
        anchors: anchors
            .into_iter()
            .map(|anchor| ContextAnchor {
                anchor_kind: anchor.anchor_kind,
                anchor_id: anchor.anchor_id,
            })
            .collect(),
        sections,
    })
}

fn context_anchors(request: &ContextRequest) -> Result<Vec<AnchorCandidate>> {
    let derived = anchor::derive_anchor_from_cwd(Some(&request.cwd))?;
    let mut anchors = Vec::new();
    anchors.push(AnchorCandidate {
        anchor_kind: AnchorKind::Worktree,
        anchor_id: derived.anchor_id,
        domain: request.domain.clone(),
    });

    let repo_anchor_id = derived
        .parent_anchor_id
        .unwrap_or_else(|| anchor::LEGACY_REPO_ANCHOR_ID.to_string());
    anchors.push(AnchorCandidate {
        anchor_kind: AnchorKind::Repo,
        anchor_id: repo_anchor_id,
        domain: request.domain.clone(),
    });

    // P12 backfilled existing drawers to repo://legacy. Keep it as a fallback
    // so the first runtime assembler remains useful on pre-anchor databases.
    anchors.push(AnchorCandidate {
        anchor_kind: AnchorKind::Repo,
        anchor_id: anchor::LEGACY_REPO_ANCHOR_ID.to_string(),
        domain: request.domain.clone(),
    });

    anchors.push(AnchorCandidate {
        anchor_kind: AnchorKind::Global,
        anchor_id: "global://default".to_string(),
        domain: MemoryDomain::Global,
    });

    Ok(dedup_anchors(anchors))
}

fn dedup_anchors(anchors: Vec<AnchorCandidate>) -> Vec<AnchorCandidate> {
    let mut seen = BTreeSet::new();
    anchors
        .into_iter()
        .filter(|anchor| {
            seen.insert((
                anchor_kind_slug(&anchor.anchor_kind).to_string(),
                anchor.anchor_id.clone(),
            ))
        })
        .collect()
}

fn search_context_candidates(
    db: &Database,
    query: CandidateQuery<'_>,
) -> Result<Vec<SearchResult>> {
    let filters = SearchFilters {
        memory_kind: Some(memory_kind_slug(&query.memory_kind).to_string()),
        domain: Some(domain_slug(&query.anchor.domain).to_string()),
        field: Some(query.request.field.clone()),
        tier: query.tier.as_ref().map(tier_slug).map(str::to_string),
        status: query.status.as_ref().map(status_slug).map(str::to_string),
        anchor_kind: Some(anchor_kind_slug(&query.anchor.anchor_kind).to_string()),
    };

    search_with_vector_options(
        db,
        &query.request.query,
        query.query_vector,
        query.route.clone(),
        SearchOptions {
            filters,
            with_neighbors: false,
        },
        query.top_k,
    )
    .map_err(ContextError::Search)
}

fn context_item_from_result(db: &Database, result: SearchResult) -> Result<ContextItem> {
    let trigger_hints = db
        .get_drawer(&result.drawer_id)
        .map_err(ContextError::LoadDrawer)?
        .and_then(|drawer| drawer.trigger_hints);
    let text = match result.memory_kind {
        MemoryKind::Knowledge => result
            .statement
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(result.content.as_str())
            .to_string(),
        MemoryKind::Evidence => result.content,
    };
    Ok(ContextItem {
        drawer_id: result.drawer_id,
        source_file: result.source_file,
        text,
        tier: result.tier,
        status: result.status,
        anchor_kind: result.anchor_kind,
        anchor_id: result.anchor_id,
        parent_anchor_id: result.parent_anchor_id,
        trigger_hints,
    })
}

fn tier_order() -> &'static [KnowledgeTier] {
    &[
        KnowledgeTier::DaoTian,
        KnowledgeTier::DaoRen,
        KnowledgeTier::Shu,
        KnowledgeTier::Qi,
    ]
}

fn active_statuses() -> &'static [KnowledgeStatus] {
    &[KnowledgeStatus::Canonical, KnowledgeStatus::Promoted]
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

fn tier_slug(value: &KnowledgeTier) -> &'static str {
    match value {
        KnowledgeTier::Qi => "qi",
        KnowledgeTier::Shu => "shu",
        KnowledgeTier::DaoRen => "dao_ren",
        KnowledgeTier::DaoTian => "dao_tian",
    }
}

fn status_slug(value: &KnowledgeStatus) -> &'static str {
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
