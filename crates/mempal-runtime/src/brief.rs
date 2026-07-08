#![warn(clippy::all)]

use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::Serialize;
use thiserror::Error;

use crate::context::{ContextError, ContextItem, ContextPack, ContextRequest, assemble_context};
use crate::core::types::{AnchorKind, KnowledgeEvidenceRole, MemoryDomain};
use crate::embed::Embedder;

pub type Result<T> = std::result::Result<T, BriefError>;

#[derive(Debug, Error)]
pub enum BriefError {
    #[error("failed to assemble brief context")]
    Context(#[from] ContextError),
}

#[derive(Debug, Clone)]
pub struct BriefRequest {
    pub query: String,
    pub domain: MemoryDomain,
    pub field: String,
    pub cwd: PathBuf,
    pub max_items: usize,
    pub dao_tian_limit: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CognitiveBrief {
    pub query: String,
    pub domain: MemoryDomain,
    pub field: String,
    pub summary: BriefSummary,
    pub key_facts: Vec<BriefFact>,
    pub evidence: Vec<BriefEvidence>,
    pub cards: Vec<BriefCard>,
    pub entities: Vec<String>,
    pub unresolved_items: Vec<BriefUnresolvedItem>,
    pub uncertainty: Vec<BriefUncertainty>,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BriefSummary {
    pub narrative: String,
    pub key_fact_count: usize,
    pub evidence_count: usize,
    pub card_count: usize,
    pub unresolved_count: usize,
    pub uncertainty_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct BriefFact {
    pub text: String,
    pub section: String,
    pub citation: BriefCitation,
}

#[derive(Debug, Clone, Serialize)]
pub struct BriefEvidence {
    pub text: String,
    pub citation: BriefCitation,
}

#[derive(Debug, Clone, Serialize)]
pub struct BriefCard {
    pub card_id: String,
    pub text: String,
    pub citation: BriefCitation,
    pub evidence_citations: Vec<BriefEvidenceCitation>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BriefUnresolvedItem {
    pub text: String,
    pub citation: BriefCitation,
}

#[derive(Debug, Clone, Serialize)]
pub struct BriefUncertainty {
    pub kind: String,
    pub message: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub citations: Vec<BriefCitation>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BriefCitation {
    pub drawer_id: String,
    pub source_file: String,
    pub anchor_kind: AnchorKind,
    pub anchor_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BriefEvidenceCitation {
    pub evidence_drawer_id: String,
    pub role: KnowledgeEvidenceRole,
    pub source_file: String,
}

pub async fn assemble_brief<E: Embedder + ?Sized>(
    db: &crate::core::db::Database,
    embedder: &E,
    request: BriefRequest,
) -> Result<CognitiveBrief> {
    let context = assemble_context(
        db,
        embedder,
        ContextRequest {
            query: request.query,
            domain: request.domain,
            field: request.field,
            cwd: request.cwd,
            include_evidence: true,
            include_cards: true,
            max_items: request.max_items,
            dao_tian_limit: request.dao_tian_limit,
            // brief is a separate surface; the P106 distill signal is scoped to
            // mempal context / mempal_context only.
            include_distill_suggestions: false,
        },
    )
    .await?;
    Ok(brief_from_context(context))
}

pub fn brief_from_context(context: ContextPack) -> CognitiveBrief {
    let mut key_facts = Vec::new();
    let mut evidence = Vec::new();
    let mut cards = Vec::new();
    let mut unresolved_items = Vec::new();
    let mut all_text = Vec::new();

    for section in &context.sections {
        for item in &section.items {
            all_text.push(item.text.clone());
            if let Some(card_id) = item.card_id.as_deref() {
                cards.push(BriefCard {
                    card_id: card_id.to_string(),
                    text: item.text.clone(),
                    citation: citation_from_item(item),
                    evidence_citations: item
                        .evidence_citations
                        .iter()
                        .map(|citation| BriefEvidenceCitation {
                            evidence_drawer_id: citation.evidence_drawer_id.clone(),
                            role: citation.role.clone(),
                            source_file: citation.source_file.clone(),
                        })
                        .collect(),
                });
            } else if section.name == "evidence" {
                evidence.push(BriefEvidence {
                    text: item.text.clone(),
                    citation: citation_from_item(item),
                });
            } else {
                key_facts.push(BriefFact {
                    text: item.text.clone(),
                    section: section.name.clone(),
                    citation: citation_from_item(item),
                });
            }

            if looks_unresolved(&item.text) {
                unresolved_items.push(BriefUnresolvedItem {
                    text: item.text.clone(),
                    citation: citation_from_item(item),
                });
            }
        }
    }

    let mut uncertainty = build_uncertainty(&key_facts, &evidence, &cards, &unresolved_items);
    uncertainty.extend(conflict_uncertainty(&all_text, &context));
    let next_actions = build_next_actions(&key_facts, &evidence, &cards, &unresolved_items);
    let entities = extract_entities(&all_text);
    let summary = BriefSummary {
        narrative: build_narrative(
            key_facts.len(),
            evidence.len(),
            cards.len(),
            unresolved_items.len(),
            uncertainty.len(),
        ),
        key_fact_count: key_facts.len(),
        evidence_count: evidence.len(),
        card_count: cards.len(),
        unresolved_count: unresolved_items.len(),
        uncertainty_count: uncertainty.len(),
    };

    CognitiveBrief {
        query: context.query,
        domain: context.domain,
        field: context.field,
        summary,
        key_facts,
        evidence,
        cards,
        entities,
        unresolved_items,
        uncertainty,
        next_actions,
    }
}

fn citation_from_item(item: &ContextItem) -> BriefCitation {
    BriefCitation {
        drawer_id: item.drawer_id.clone(),
        source_file: item.source_file.clone(),
        anchor_kind: item.anchor_kind.clone(),
        anchor_id: item.anchor_id.clone(),
        card_id: item.card_id.clone(),
    }
}

fn build_narrative(
    key_fact_count: usize,
    evidence_count: usize,
    card_count: usize,
    unresolved_count: usize,
    uncertainty_count: usize,
) -> String {
    if key_fact_count == 0 && evidence_count == 0 && card_count == 0 {
        return "No cited memory was found for this query; treat any answer as unsupported until evidence is ingested.".to_string();
    }

    format!(
        "Brief assembled from {key_fact_count} cited key facts, {evidence_count} evidence items, and {card_count} cards; {unresolved_count} unresolved cues and {uncertainty_count} uncertainty signals require review."
    )
}

fn build_uncertainty(
    key_facts: &[BriefFact],
    evidence: &[BriefEvidence],
    cards: &[BriefCard],
    unresolved_items: &[BriefUnresolvedItem],
) -> Vec<BriefUncertainty> {
    let mut uncertainty = Vec::new();
    if evidence.is_empty() {
        uncertainty.push(BriefUncertainty {
            kind: "no_evidence".to_string(),
            message: "No cited evidence was found for this query.".to_string(),
            citations: Vec::new(),
        });
    }
    if key_facts.is_empty() {
        uncertainty.push(BriefUncertainty {
            kind: "no_key_facts".to_string(),
            message: "No governed knowledge statement was found for this query.".to_string(),
            citations: Vec::new(),
        });
    }
    if cards.is_empty() {
        uncertainty.push(BriefUncertainty {
            kind: "no_cards".to_string(),
            message: "No active knowledge card was found for this query.".to_string(),
            citations: Vec::new(),
        });
    }
    if !unresolved_items.is_empty() {
        uncertainty.push(BriefUncertainty {
            kind: "unresolved_items".to_string(),
            message: format!(
                "{} cited item(s) mention unresolved work or follow-up.",
                unresolved_items.len()
            ),
            citations: unresolved_items
                .iter()
                .map(|item| item.citation.clone())
                .collect(),
        });
    }
    uncertainty
}

fn conflict_uncertainty(texts: &[String], context: &ContextPack) -> Vec<BriefUncertainty> {
    let mut citations = Vec::new();
    for section in &context.sections {
        for item in &section.items {
            if mentions_conflict(&item.text) {
                citations.push(citation_from_item(item));
            }
        }
    }
    if citations.is_empty() && texts.iter().any(|text| mentions_conflict(text)) {
        return vec![BriefUncertainty {
            kind: "conflict_cue".to_string(),
            message:
                "Relevant text contains contradiction, rollback, stale, or counterexample language."
                    .to_string(),
            citations: Vec::new(),
        }];
    }
    if citations.is_empty() {
        Vec::new()
    } else {
        vec![BriefUncertainty {
            kind: "conflict_cue".to_string(),
            message: "Relevant cited text contains contradiction, rollback, stale, or counterexample language.".to_string(),
            citations,
        }]
    }
}

fn build_next_actions(
    key_facts: &[BriefFact],
    evidence: &[BriefEvidence],
    cards: &[BriefCard],
    unresolved_items: &[BriefUnresolvedItem],
) -> Vec<String> {
    let mut actions = Vec::new();
    if evidence.is_empty() {
        actions.push("Ingest or add evidence before relying on this brief.".to_string());
    }
    if key_facts.is_empty() {
        actions
            .push("Distill candidate knowledge only after supporting evidence exists.".to_string());
    }
    if cards.is_empty() {
        actions.push("Create or retrieve a knowledge card if this topic recurs.".to_string());
    }
    if !unresolved_items.is_empty() {
        actions.push("Review and resolve the cited unresolved items.".to_string());
    }
    if actions.is_empty() {
        actions.push("Review the cited evidence before acting on the brief.".to_string());
    }
    actions
}

fn looks_unresolved(text: &str) -> bool {
    let lower = text.to_lowercase();
    [
        "todo",
        "action item",
        "follow up",
        "unresolved",
        "remain",
        "blocked",
        "next step",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn mentions_conflict(text: &str) -> bool {
    let lower = text.to_lowercase();
    ["contradiction", "rollback", "stale", "counterexample"]
        .iter()
        .any(|needle| lower.contains(needle))
}

fn extract_entities(texts: &[String]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    for text in texts {
        for raw in text.split(|ch: char| !ch.is_alphanumeric() && ch != '-') {
            let token = raw.trim_matches('-');
            if token.chars().count() < 2 || is_entity_stopword(token) {
                continue;
            }
            if token
                .chars()
                .next()
                .is_some_and(|first| first.is_uppercase())
            {
                seen.insert(token.to_string());
            }
        }
    }
    seen.into_iter().collect()
}

fn is_entity_stopword(token: &str) -> bool {
    matches!(
        token,
        "The" | "This" | "That" | "No" | "Brief" | "Use" | "Review"
    )
}
