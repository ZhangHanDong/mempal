use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::core::db::{Database, DbError};
use crate::core::types::{
    Drawer, KnowledgeCard, KnowledgeCardEvent, KnowledgeCardFilter, KnowledgeEventType,
    KnowledgeEvidenceLink, KnowledgeEvidenceRole,
};
use crate::core::utils::current_timestamp;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeCardBackfillReport {
    pub ready_count: usize,
    pub skipped_count: usize,
    pub already_exists_count: usize,
    pub candidates: Vec<KnowledgeCardBackfillCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeCardBackfillCandidate {
    pub source_drawer_id: String,
    pub prospective_card_id: String,
    pub status: KnowledgeCardBackfillStatus,
    pub reasons: Vec<String>,
    pub statement: Option<String>,
    pub tier: Option<String>,
    pub knowledge_status: Option<String>,
    pub domain: String,
    pub field: String,
    pub anchor_kind: String,
    pub anchor_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeCardBackfillStatus {
    Ready,
    Skipped,
    AlreadyExists,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeCardBackfillApplyOptions {
    pub execute: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeCardBackfillApplyResult {
    pub dry_run: bool,
    pub ready_count: usize,
    pub skipped_count: usize,
    pub already_exists_count: usize,
    pub created_count: usize,
    pub linked_count: usize,
    pub event_count: usize,
    pub link_errors: Vec<KnowledgeCardBackfillLinkError>,
    pub candidates: Vec<KnowledgeCardBackfillCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeCardBackfillLinkError {
    pub card_id: String,
    pub evidence_drawer_id: String,
    pub role: String,
    pub error: String,
}

pub fn build_backfill_report(
    db: &Database,
    filter: &KnowledgeCardFilter,
) -> Result<KnowledgeCardBackfillReport, DbError> {
    let drawers = db.list_knowledge_drawers_for_card_backfill(filter)?;
    let mut candidates = Vec::with_capacity(drawers.len());
    let mut ready_count = 0;
    let mut skipped_count = 0;
    let mut already_exists_count = 0;

    for drawer in drawers {
        let candidate = classify_drawer(db, drawer)?;
        match candidate.status {
            KnowledgeCardBackfillStatus::Ready => ready_count += 1,
            KnowledgeCardBackfillStatus::Skipped => skipped_count += 1,
            KnowledgeCardBackfillStatus::AlreadyExists => already_exists_count += 1,
        }
        candidates.push(candidate);
    }

    Ok(KnowledgeCardBackfillReport {
        ready_count,
        skipped_count,
        already_exists_count,
        candidates,
    })
}

pub fn apply_backfill(
    db: &Database,
    filter: &KnowledgeCardFilter,
    options: KnowledgeCardBackfillApplyOptions,
) -> Result<KnowledgeCardBackfillApplyResult, DbError> {
    let drawers = db.list_knowledge_drawers_for_card_backfill(filter)?;
    let mut candidates = Vec::with_capacity(drawers.len());
    let mut ready_count = 0;
    let mut skipped_count = 0;
    let mut already_exists_count = 0;
    let mut created_count = 0;
    let mut linked_count = 0;
    let mut event_count = 0;
    let mut link_errors = Vec::new();

    for drawer in drawers {
        let candidate = classify_drawer(db, drawer.clone())?;
        match candidate.status {
            KnowledgeCardBackfillStatus::Ready => {
                ready_count += 1;
                if options.execute {
                    let card_id = candidate.prospective_card_id.clone();
                    create_card_and_event(db, &drawer, &card_id)?;
                    created_count += 1;
                    event_count += 1;
                    for (role, evidence_drawer_id) in evidence_refs_by_role(&drawer) {
                        let link = KnowledgeEvidenceLink {
                            id: prospective_link_id(&card_id, &evidence_drawer_id, &role),
                            card_id: card_id.clone(),
                            evidence_drawer_id: evidence_drawer_id.clone(),
                            role: role.clone(),
                            note: Some(format!("backfilled from {}", drawer.id)),
                            created_at: current_timestamp(),
                        };
                        match db.insert_knowledge_evidence_link(&link) {
                            Ok(()) => linked_count += 1,
                            Err(error) => link_errors.push(KnowledgeCardBackfillLinkError {
                                card_id: card_id.clone(),
                                evidence_drawer_id: evidence_drawer_id.clone(),
                                role: evidence_role_slug(&role).to_string(),
                                error: error.to_string(),
                            }),
                        }
                    }
                }
            }
            KnowledgeCardBackfillStatus::Skipped => skipped_count += 1,
            KnowledgeCardBackfillStatus::AlreadyExists => already_exists_count += 1,
        }
        candidates.push(candidate);
    }

    Ok(KnowledgeCardBackfillApplyResult {
        dry_run: !options.execute,
        ready_count,
        skipped_count,
        already_exists_count,
        created_count,
        linked_count,
        event_count,
        link_errors,
        candidates,
    })
}

pub fn prospective_card_id(source_drawer_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"knowledge-card-backfill-v1");
    hasher.update([0]);
    hasher.update(source_drawer_id.as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    format!("card_{}", &digest[..16])
}

fn prospective_link_id(
    card_id: &str,
    evidence_drawer_id: &str,
    role: &KnowledgeEvidenceRole,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"knowledge-card-backfill-link-v1");
    hasher.update([0]);
    hasher.update(card_id.as_bytes());
    hasher.update([0]);
    hasher.update(evidence_drawer_id.as_bytes());
    hasher.update([0]);
    hasher.update(evidence_role_slug(role).as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    format!("link_{}", &digest[..16])
}

fn classify_drawer(
    db: &Database,
    drawer: Drawer,
) -> Result<KnowledgeCardBackfillCandidate, DbError> {
    let prospective_card_id = prospective_card_id(&drawer.id);
    let reasons = skip_reasons(&drawer);
    let status = if !reasons.is_empty() {
        KnowledgeCardBackfillStatus::Skipped
    } else if db.get_knowledge_card(&prospective_card_id)?.is_some() {
        KnowledgeCardBackfillStatus::AlreadyExists
    } else {
        KnowledgeCardBackfillStatus::Ready
    };

    Ok(KnowledgeCardBackfillCandidate {
        source_drawer_id: drawer.id,
        prospective_card_id,
        status,
        reasons,
        statement: drawer.statement,
        tier: drawer
            .tier
            .as_ref()
            .map(knowledge_tier_slug)
            .map(str::to_string),
        knowledge_status: drawer
            .status
            .as_ref()
            .map(knowledge_status_slug)
            .map(str::to_string),
        domain: memory_domain_slug(&drawer.domain).to_string(),
        field: drawer.field,
        anchor_kind: anchor_kind_slug(&drawer.anchor_kind).to_string(),
        anchor_id: drawer.anchor_id,
    })
}

fn create_card_and_event(db: &Database, drawer: &Drawer, card_id: &str) -> Result<(), DbError> {
    let now = current_timestamp();
    let card = KnowledgeCard {
        id: card_id.to_string(),
        statement: drawer.statement.clone().unwrap_or_default(),
        content: drawer.content.clone(),
        tier: drawer.tier.clone().expect("ready candidate must have tier"),
        status: drawer
            .status
            .clone()
            .expect("ready candidate must have status"),
        domain: drawer.domain.clone(),
        field: drawer.field.clone(),
        anchor_kind: drawer.anchor_kind.clone(),
        anchor_id: drawer.anchor_id.clone(),
        parent_anchor_id: drawer.parent_anchor_id.clone(),
        scope_constraints: drawer.scope_constraints.clone(),
        trigger_hints: drawer.trigger_hints.clone(),
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    let event = KnowledgeCardEvent {
        id: prospective_created_event_id(card_id),
        card_id: card_id.to_string(),
        event_type: KnowledgeEventType::Created,
        from_status: None,
        to_status: drawer.status.clone(),
        reason: format!("backfilled from Stage-1 knowledge drawer {}", drawer.id),
        actor: Some("mempal".to_string()),
        metadata: Some(serde_json::json!({
            "source_drawer_id": drawer.id,
            "source_file": drawer.source_file,
        })),
        created_at: now,
    };

    db.conn().execute_batch("BEGIN IMMEDIATE TRANSACTION")?;
    let result = db
        .insert_knowledge_card(&card)
        .and_then(|()| db.append_knowledge_event(&event));
    match result {
        Ok(()) => {
            db.conn().execute_batch("COMMIT")?;
            Ok(())
        }
        Err(error) => {
            let _ = db.conn().execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

fn prospective_created_event_id(card_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"knowledge-card-backfill-created-event-v1");
    hasher.update([0]);
    hasher.update(card_id.as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    format!("event_{}", &digest[..16])
}

fn evidence_refs_by_role(drawer: &Drawer) -> Vec<(KnowledgeEvidenceRole, String)> {
    let mut refs = Vec::new();
    refs.extend(
        drawer
            .supporting_refs
            .iter()
            .cloned()
            .map(|id| (KnowledgeEvidenceRole::Supporting, id)),
    );
    refs.extend(
        drawer
            .verification_refs
            .iter()
            .cloned()
            .map(|id| (KnowledgeEvidenceRole::Verification, id)),
    );
    refs.extend(
        drawer
            .counterexample_refs
            .iter()
            .cloned()
            .map(|id| (KnowledgeEvidenceRole::Counterexample, id)),
    );
    refs.extend(
        drawer
            .teaching_refs
            .iter()
            .cloned()
            .map(|id| (KnowledgeEvidenceRole::Teaching, id)),
    );
    refs
}

fn skip_reasons(drawer: &Drawer) -> Vec<String> {
    let mut reasons = Vec::new();
    if drawer
        .statement
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
    {
        reasons.push("missing statement".to_string());
    }
    if drawer.tier.is_none() {
        reasons.push("missing tier".to_string());
    }
    if drawer.status.is_none() {
        reasons.push("missing status".to_string());
    }
    if drawer.field.trim().is_empty() {
        reasons.push("missing field".to_string());
    }
    if drawer.anchor_id.trim().is_empty() {
        reasons.push("missing anchor_id".to_string());
    }
    reasons
}

fn evidence_role_slug(value: &KnowledgeEvidenceRole) -> &'static str {
    match value {
        KnowledgeEvidenceRole::Supporting => "supporting",
        KnowledgeEvidenceRole::Verification => "verification",
        KnowledgeEvidenceRole::Counterexample => "counterexample",
        KnowledgeEvidenceRole::Teaching => "teaching",
    }
}

fn memory_domain_slug(value: &crate::core::types::MemoryDomain) -> &'static str {
    match value {
        crate::core::types::MemoryDomain::Project => "project",
        crate::core::types::MemoryDomain::Agent => "agent",
        crate::core::types::MemoryDomain::Skill => "skill",
        crate::core::types::MemoryDomain::Global => "global",
    }
}

fn knowledge_tier_slug(value: &crate::core::types::KnowledgeTier) -> &'static str {
    match value {
        crate::core::types::KnowledgeTier::Qi => "qi",
        crate::core::types::KnowledgeTier::Shu => "shu",
        crate::core::types::KnowledgeTier::DaoRen => "dao_ren",
        crate::core::types::KnowledgeTier::DaoTian => "dao_tian",
    }
}

fn knowledge_status_slug(value: &crate::core::types::KnowledgeStatus) -> &'static str {
    match value {
        crate::core::types::KnowledgeStatus::Candidate => "candidate",
        crate::core::types::KnowledgeStatus::Promoted => "promoted",
        crate::core::types::KnowledgeStatus::Canonical => "canonical",
        crate::core::types::KnowledgeStatus::Demoted => "demoted",
        crate::core::types::KnowledgeStatus::Retired => "retired",
    }
}

fn anchor_kind_slug(value: &crate::core::types::AnchorKind) -> &'static str {
    match value {
        crate::core::types::AnchorKind::Global => "global",
        crate::core::types::AnchorKind::Repo => "repo",
        crate::core::types::AnchorKind::Worktree => "worktree",
    }
}
