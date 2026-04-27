use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::core::db::{Database, DbError};
use crate::core::types::{Drawer, KnowledgeCardFilter};

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

pub fn prospective_card_id(source_drawer_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"knowledge-card-backfill-v1");
    hasher.update([0]);
    hasher.update(source_drawer_id.as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    format!("card_{}", &digest[..16])
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
