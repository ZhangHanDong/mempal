#![warn(clippy::all)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Serialize;
use thiserror::Error;

use crate::core::{
    anchor,
    db::{Database, DbError},
    types::{
        AnchorKind, KnowledgeCard, KnowledgeEvidenceLink, KnowledgeEvidenceRole, KnowledgeStatus,
        MemoryDomain, MemoryKind, RouteDecision,
    },
};
use crate::embed::{EmbedError, Embedder};
use crate::search::{SearchError, SearchFilters, SearchOptions, search_with_vector_options};

pub type Result<T> = std::result::Result<T, KnowledgeCardRetrievalError>;

#[derive(Debug, Error)]
pub enum KnowledgeCardRetrievalError {
    #[error("failed to derive retrieval anchors")]
    DeriveAnchor(#[from] anchor::AnchorError),
    #[error("failed to embed card retrieval query")]
    EmbedQuery(#[source] EmbedError),
    #[error("embedder returned no card retrieval query vector")]
    MissingQueryVector,
    #[error("failed to search linked evidence")]
    SearchEvidence(#[source] SearchError),
    #[error("failed to load card retrieval metadata")]
    LoadMetadata(#[source] DbError),
}

#[derive(Debug, Clone)]
pub struct KnowledgeCardRetrievalRequest {
    pub query: String,
    pub domain: MemoryDomain,
    pub field: String,
    pub cwd: PathBuf,
    pub top_k: usize,
    pub evidence_top_k: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct RetrievedKnowledgeCard {
    pub card: KnowledgeCard,
    pub evidence_citations: Vec<RetrievedEvidenceCitation>,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct RetrievedEvidenceCitation {
    pub evidence_drawer_id: String,
    pub role: KnowledgeEvidenceRole,
    pub source_file: String,
    pub score: f32,
}

#[derive(Debug, Clone)]
struct AnchorCandidate {
    anchor_kind: AnchorKind,
    anchor_id: String,
    domain: MemoryDomain,
}

pub async fn retrieve_knowledge_cards<E: Embedder + ?Sized>(
    db: &Database,
    embedder: &E,
    request: KnowledgeCardRetrievalRequest,
) -> Result<Vec<RetrievedKnowledgeCard>> {
    if request.top_k == 0 {
        return Ok(Vec::new());
    }
    let query_vector = embedder
        .embed(&[request.query.as_str()])
        .await
        .map_err(KnowledgeCardRetrievalError::EmbedQuery)?
        .into_iter()
        .next()
        .ok_or(KnowledgeCardRetrievalError::MissingQueryVector)?;
    retrieve_knowledge_cards_with_vector(db, request, &query_vector)
}

pub fn retrieve_knowledge_cards_with_vector(
    db: &Database,
    request: KnowledgeCardRetrievalRequest,
    query_vector: &[f32],
) -> Result<Vec<RetrievedKnowledgeCard>> {
    if request.top_k == 0 {
        return Ok(Vec::new());
    }

    let mut by_card = BTreeMap::<String, RetrievedKnowledgeCard>::new();
    let route = RouteDecision {
        wing: None,
        room: None,
        confidence: 0.0,
        reason: "knowledge card linked-evidence retrieval".to_string(),
    };

    for anchor in retrieval_anchors(&request)? {
        let filters = SearchFilters {
            memory_kind: Some(memory_kind_slug(&MemoryKind::Evidence).to_string()),
            domain: Some(domain_slug(&anchor.domain).to_string()),
            field: Some(request.field.clone()),
            tier: None,
            status: None,
            anchor_kind: Some(anchor_kind_slug(&anchor.anchor_kind).to_string()),
        };
        let evidence_results = search_with_vector_options(
            db,
            &request.query,
            query_vector,
            route.clone(),
            SearchOptions {
                filters,
                with_neighbors: false,
            },
            request.evidence_top_k.max(request.top_k),
        )
        .map_err(KnowledgeCardRetrievalError::SearchEvidence)?;

        for evidence in evidence_results {
            if evidence.anchor_id != anchor.anchor_id {
                continue;
            }
            let links = db
                .knowledge_evidence_links_for_drawer(&evidence.drawer_id)
                .map_err(KnowledgeCardRetrievalError::LoadMetadata)?;
            for link in links {
                let Some(card) = db
                    .get_knowledge_card(&link.card_id)
                    .map_err(KnowledgeCardRetrievalError::LoadMetadata)?
                else {
                    continue;
                };
                if !card_is_retrievable(&card, &request, &anchor) {
                    continue;
                }
                let citation =
                    citation_from_link(&link, &evidence.source_file, evidence.similarity);
                match by_card.get_mut(&card.id) {
                    Some(existing) => {
                        if citation.score > existing.score {
                            existing.score = citation.score;
                        }
                        existing.evidence_citations.push(citation);
                    }
                    None => {
                        by_card.insert(
                            card.id.clone(),
                            RetrievedKnowledgeCard {
                                card,
                                score: citation.score,
                                evidence_citations: vec![citation],
                            },
                        );
                    }
                }
            }
        }
    }

    let mut results = by_card.into_values().collect::<Vec<_>>();
    results.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.card.id.cmp(&right.card.id))
    });
    results.truncate(request.top_k);
    Ok(results)
}

fn retrieval_anchors(request: &KnowledgeCardRetrievalRequest) -> Result<Vec<AnchorCandidate>> {
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

    let mut seen = BTreeMap::new();
    Ok(anchors
        .into_iter()
        .filter(|anchor| {
            seen.insert(
                (
                    anchor_kind_slug(&anchor.anchor_kind).to_string(),
                    anchor.anchor_id.clone(),
                ),
                (),
            )
            .is_none()
        })
        .collect())
}

fn card_is_retrievable(
    card: &KnowledgeCard,
    request: &KnowledgeCardRetrievalRequest,
    anchor: &AnchorCandidate,
) -> bool {
    matches!(
        card.status,
        KnowledgeStatus::Canonical | KnowledgeStatus::Promoted
    ) && card.domain == anchor.domain
        && card.field == request.field
        && card.anchor_kind == anchor.anchor_kind
        && card.anchor_id == anchor.anchor_id
}

fn citation_from_link(
    link: &KnowledgeEvidenceLink,
    source_file: &str,
    score: f32,
) -> RetrievedEvidenceCitation {
    RetrievedEvidenceCitation {
        evidence_drawer_id: link.evidence_drawer_id.clone(),
        role: link.role.clone(),
        source_file: source_file.to_string(),
        score,
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

fn anchor_kind_slug(value: &AnchorKind) -> &'static str {
    match value {
        AnchorKind::Global => "global",
        AnchorKind::Repo => "repo",
        AnchorKind::Worktree => "worktree",
    }
}
