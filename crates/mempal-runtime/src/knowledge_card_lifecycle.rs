use anyhow::{Context, Result, bail};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::core::{
    db::Database,
    types::{
        KnowledgeCard, KnowledgeCardEvent, KnowledgeEventType, KnowledgeEvidenceLink,
        KnowledgeEvidenceRole, KnowledgeStatus, KnowledgeTier, MemoryKind,
    },
    utils::current_timestamp,
};
use crate::knowledge_gate::{GateEvidenceCounts, GateRequirements};

struct CardLifecycleMutation {
    old_status: KnowledgeStatus,
    target_status: KnowledgeStatus,
    event_type: KnowledgeEventType,
    reason: String,
    actor: Option<String>,
    new_refs: Vec<String>,
    link_role: KnowledgeEvidenceRole,
    metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct KnowledgeCardGateReport {
    pub card_id: String,
    pub tier: String,
    pub status: String,
    pub target_status: String,
    pub allowed: bool,
    pub reasons: Vec<String>,
    pub requirements: GateRequirements,
    pub evidence_counts: GateEvidenceCounts,
}

#[derive(Debug, Clone)]
pub struct PromoteCardRequest {
    pub card_id: String,
    pub status: String,
    pub verification_refs: Vec<String>,
    pub reason: String,
    pub reviewer: Option<String>,
    pub allow_counterexamples: bool,
    pub enforce_gate: bool,
}

#[derive(Debug, Clone)]
pub struct DemoteCardRequest {
    pub card_id: String,
    pub status: String,
    pub evidence_refs: Vec<String>,
    pub reason: String,
    pub reason_type: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PromoteCardOutcome {
    pub card_id: String,
    pub old_status: String,
    pub new_status: String,
    pub verification_refs: Vec<String>,
    pub gate: Option<KnowledgeCardGateReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DemoteCardOutcome {
    pub card_id: String,
    pub old_status: String,
    pub new_status: String,
    pub counterexample_refs: Vec<String>,
}

pub fn evaluate_card_gate_by_id(
    db: &Database,
    card_id: &str,
    target_status: Option<&str>,
    reviewer: Option<&str>,
    allow_counterexamples: bool,
) -> Result<KnowledgeCardGateReport> {
    let card = load_card(db, card_id)?;
    let target_status = match target_status {
        Some(value) => parse_status(value)?,
        None => default_target_status(&card.tier),
    };
    validate_tier_status(&card.tier, &target_status)?;
    let counts = evidence_counts(db, card_id)?;
    evaluate_card_gate(
        &card,
        &target_status,
        reviewer,
        allow_counterexamples,
        counts,
    )
}

pub fn promote_card(db: &Database, request: PromoteCardRequest) -> Result<PromoteCardOutcome> {
    let target_status = parse_status(&request.status)?;
    if !matches!(
        target_status,
        KnowledgeStatus::Promoted | KnowledgeStatus::Canonical
    ) {
        bail!("promote status must be promoted or canonical");
    }
    validate_evidence_refs(db, &request.verification_refs)?;
    let mut card = load_card(db, &request.card_id)?;
    validate_tier_status(&card.tier, &target_status)?;
    let old_status = card.status.clone();

    let existing_links = db
        .knowledge_evidence_links(&request.card_id)
        .context("failed to list knowledge card evidence links")?;
    let new_refs = missing_refs_for_role(
        &existing_links,
        &request.verification_refs,
        &KnowledgeEvidenceRole::Verification,
    );
    let projected_counts = projected_counts(
        counts_from_links(&existing_links),
        &new_refs,
        &KnowledgeEvidenceRole::Verification,
    );
    let gate = if request.enforce_gate {
        let gate = evaluate_card_gate(
            &card,
            &target_status,
            request.reviewer.as_deref(),
            request.allow_counterexamples,
            projected_counts,
        )?;
        if !gate.allowed {
            bail!("promotion gate failed: {}", gate.reasons.join("; "));
        }
        Some(gate)
    } else {
        None
    };

    card.status = target_status.clone();
    card.updated_at = current_timestamp();
    apply_card_lifecycle(
        db,
        &card,
        CardLifecycleMutation {
            old_status: old_status.clone(),
            target_status: target_status.clone(),
            event_type: KnowledgeEventType::Promoted,
            reason: request.reason.clone(),
            actor: request.reviewer.clone(),
            new_refs,
            link_role: KnowledgeEvidenceRole::Verification,
            metadata: serde_json::json!({
                "verification_refs": request.verification_refs,
                "gate_enforced": request.enforce_gate,
            }),
        },
    )?;

    Ok(PromoteCardOutcome {
        card_id: request.card_id,
        old_status: status_slug(&old_status).to_string(),
        new_status: status_slug(&target_status).to_string(),
        verification_refs: refs_for_role(
            &db.knowledge_evidence_links(&card.id)
                .context("failed to list knowledge card evidence links")?,
            &KnowledgeEvidenceRole::Verification,
        ),
        gate,
    })
}

pub fn demote_card(db: &Database, request: DemoteCardRequest) -> Result<DemoteCardOutcome> {
    let target_status = parse_status(&request.status)?;
    if !matches!(
        target_status,
        KnowledgeStatus::Demoted | KnowledgeStatus::Retired
    ) {
        bail!("demote status must be demoted or retired");
    }
    validate_demote_reason_type(&request.reason_type)?;
    validate_evidence_refs(db, &request.evidence_refs)?;
    let mut card = load_card(db, &request.card_id)?;
    validate_tier_status(&card.tier, &target_status)?;
    let old_status = card.status.clone();
    let existing_links = db
        .knowledge_evidence_links(&request.card_id)
        .context("failed to list knowledge card evidence links")?;
    let new_refs = missing_refs_for_role(
        &existing_links,
        &request.evidence_refs,
        &KnowledgeEvidenceRole::Counterexample,
    );

    card.status = target_status.clone();
    card.updated_at = current_timestamp();
    let event_type = if matches!(target_status, KnowledgeStatus::Retired) {
        KnowledgeEventType::Retired
    } else {
        KnowledgeEventType::Demoted
    };
    apply_card_lifecycle(
        db,
        &card,
        CardLifecycleMutation {
            old_status: old_status.clone(),
            target_status: target_status.clone(),
            event_type,
            reason: request.reason.clone(),
            actor: None,
            new_refs,
            link_role: KnowledgeEvidenceRole::Counterexample,
            metadata: serde_json::json!({
                "evidence_refs": request.evidence_refs,
                "reason_type": request.reason_type,
            }),
        },
    )?;

    Ok(DemoteCardOutcome {
        card_id: request.card_id,
        old_status: status_slug(&old_status).to_string(),
        new_status: status_slug(&target_status).to_string(),
        counterexample_refs: refs_for_role(
            &db.knowledge_evidence_links(&card.id)
                .context("failed to list knowledge card evidence links")?,
            &KnowledgeEvidenceRole::Counterexample,
        ),
    })
}

fn load_card(db: &Database, card_id: &str) -> Result<KnowledgeCard> {
    db.get_knowledge_card(card_id)
        .context("failed to look up knowledge card")?
        .with_context(|| format!("knowledge card not found: {card_id}"))
}

fn evaluate_card_gate(
    card: &KnowledgeCard,
    target_status: &KnowledgeStatus,
    reviewer: Option<&str>,
    allow_counterexamples: bool,
    evidence_counts: GateEvidenceCounts,
) -> Result<KnowledgeCardGateReport> {
    validate_tier_status(&card.tier, target_status)?;
    let requirements = gate_requirements(&card.tier, target_status);
    let mut reasons = Vec::new();
    if evidence_counts.supporting < requirements.min_supporting_refs {
        reasons.push(format!(
            "supporting evidence refs below requirement: have {}, need {}",
            evidence_counts.supporting, requirements.min_supporting_refs
        ));
    }
    if evidence_counts.verification < requirements.min_verification_refs {
        reasons.push(format!(
            "verification evidence refs below requirement: have {}, need {}",
            evidence_counts.verification, requirements.min_verification_refs
        ));
    }
    if evidence_counts.teaching < requirements.min_teaching_refs {
        reasons.push(format!(
            "teaching evidence refs below requirement: have {}, need {}",
            evidence_counts.teaching, requirements.min_teaching_refs
        ));
    }
    if requirements.reviewer_required
        && reviewer
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
    {
        reasons.push("reviewer is required for this gate".to_string());
    }
    if requirements.counterexamples_block
        && evidence_counts.counterexample > 0
        && !allow_counterexamples
    {
        reasons.push(format!(
            "counterexample refs present: {}",
            evidence_counts.counterexample
        ));
    }

    Ok(KnowledgeCardGateReport {
        card_id: card.id.clone(),
        tier: tier_slug(&card.tier).to_string(),
        status: status_slug(&card.status).to_string(),
        target_status: status_slug(target_status).to_string(),
        allowed: reasons.is_empty(),
        reasons,
        requirements,
        evidence_counts,
    })
}

fn apply_card_lifecycle(
    db: &Database,
    card: &KnowledgeCard,
    mutation: CardLifecycleMutation,
) -> Result<()> {
    db.conn().execute_batch("BEGIN IMMEDIATE TRANSACTION")?;
    let result = (|| {
        for evidence_drawer_id in &mutation.new_refs {
            let link = KnowledgeEvidenceLink {
                id: deterministic_link_id(&card.id, evidence_drawer_id, &mutation.link_role),
                card_id: card.id.clone(),
                evidence_drawer_id: evidence_drawer_id.clone(),
                role: mutation.link_role.clone(),
                note: Some(format!("linked during card lifecycle: {}", mutation.reason)),
                created_at: current_timestamp(),
            };
            db.insert_knowledge_evidence_link(&link)
                .context("failed to insert knowledge card evidence link")?;
        }
        db.update_knowledge_card(card)
            .context("failed to update knowledge card")?;
        let event = KnowledgeCardEvent {
            id: deterministic_event_id(&card.id, &mutation.event_type, &mutation.reason),
            card_id: card.id.clone(),
            event_type: mutation.event_type,
            from_status: Some(mutation.old_status),
            to_status: Some(mutation.target_status),
            reason: mutation.reason,
            actor: mutation.actor,
            metadata: Some(mutation.metadata),
            created_at: current_timestamp(),
        };
        db.append_knowledge_event(&event)
            .context("failed to append knowledge card event")?;
        Ok(())
    })();

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

fn evidence_counts(db: &Database, card_id: &str) -> Result<GateEvidenceCounts> {
    let links = db
        .knowledge_evidence_links(card_id)
        .context("failed to list knowledge card evidence links")?;
    Ok(counts_from_links(&links))
}

fn counts_from_links(links: &[KnowledgeEvidenceLink]) -> GateEvidenceCounts {
    GateEvidenceCounts {
        supporting: links
            .iter()
            .filter(|link| matches!(link.role, KnowledgeEvidenceRole::Supporting))
            .count(),
        counterexample: links
            .iter()
            .filter(|link| matches!(link.role, KnowledgeEvidenceRole::Counterexample))
            .count(),
        teaching: links
            .iter()
            .filter(|link| matches!(link.role, KnowledgeEvidenceRole::Teaching))
            .count(),
        verification: links
            .iter()
            .filter(|link| matches!(link.role, KnowledgeEvidenceRole::Verification))
            .count(),
    }
}

fn projected_counts(
    mut counts: GateEvidenceCounts,
    new_refs: &[String],
    role: &KnowledgeEvidenceRole,
) -> GateEvidenceCounts {
    match role {
        KnowledgeEvidenceRole::Supporting => counts.supporting += new_refs.len(),
        KnowledgeEvidenceRole::Verification => counts.verification += new_refs.len(),
        KnowledgeEvidenceRole::Counterexample => counts.counterexample += new_refs.len(),
        KnowledgeEvidenceRole::Teaching => counts.teaching += new_refs.len(),
    }
    counts
}

fn missing_refs_for_role(
    existing_links: &[KnowledgeEvidenceLink],
    refs: &[String],
    role: &KnowledgeEvidenceRole,
) -> Vec<String> {
    refs.iter()
        .filter(|drawer_id| {
            !existing_links
                .iter()
                .any(|link| link.role == *role && link.evidence_drawer_id == **drawer_id)
        })
        .cloned()
        .collect()
}

fn refs_for_role(links: &[KnowledgeEvidenceLink], role: &KnowledgeEvidenceRole) -> Vec<String> {
    links
        .iter()
        .filter(|link| link.role == *role)
        .map(|link| link.evidence_drawer_id.clone())
        .collect()
}

fn validate_evidence_refs(db: &Database, refs: &[String]) -> Result<()> {
    if refs.is_empty() {
        bail!("at least one evidence ref is required");
    }
    for drawer_id in refs {
        if !drawer_id.starts_with("drawer_") {
            bail!("evidence refs must contain drawer ids");
        }
        let drawer = db
            .get_drawer(drawer_id)
            .with_context(|| format!("failed to load evidence drawer {drawer_id}"))?
            .with_context(|| format!("evidence drawer not found: {drawer_id}"))?;
        if drawer.memory_kind != MemoryKind::Evidence {
            bail!("evidence refs must point to evidence drawers");
        }
    }
    Ok(())
}

fn parse_status(value: &str) -> Result<KnowledgeStatus> {
    match value.trim() {
        "candidate" => Ok(KnowledgeStatus::Candidate),
        "promoted" => Ok(KnowledgeStatus::Promoted),
        "canonical" => Ok(KnowledgeStatus::Canonical),
        "demoted" => Ok(KnowledgeStatus::Demoted),
        "retired" => Ok(KnowledgeStatus::Retired),
        other => bail!("unsupported knowledge status: {other}"),
    }
}

fn default_target_status(tier: &KnowledgeTier) -> KnowledgeStatus {
    match tier {
        KnowledgeTier::DaoTian => KnowledgeStatus::Canonical,
        KnowledgeTier::DaoRen | KnowledgeTier::Shu | KnowledgeTier::Qi => KnowledgeStatus::Promoted,
    }
}

fn validate_tier_status(tier: &KnowledgeTier, status: &KnowledgeStatus) -> Result<()> {
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

    match tier {
        KnowledgeTier::DaoTian => bail!("dao_tian only allows canonical or demoted"),
        KnowledgeTier::DaoRen => {
            bail!("dao_ren only allows candidate, promoted, demoted, or retired")
        }
        KnowledgeTier::Shu => bail!("shu only allows promoted, demoted, or retired"),
        KnowledgeTier::Qi => bail!("qi only allows candidate, promoted, demoted, or retired"),
    }
}

fn validate_demote_reason_type(value: &str) -> Result<()> {
    match value.trim() {
        "contradicted" | "obsolete" | "superseded" | "out_of_scope" | "unsafe" => Ok(()),
        other => bail!("unsupported demote reason_type: {other}"),
    }
}

fn gate_requirements(tier: &KnowledgeTier, target_status: &KnowledgeStatus) -> GateRequirements {
    match (tier, target_status) {
        (KnowledgeTier::DaoTian, KnowledgeStatus::Canonical) => GateRequirements {
            min_supporting_refs: 3,
            min_verification_refs: 2,
            min_teaching_refs: 1,
            reviewer_required: true,
            counterexamples_block: true,
        },
        (KnowledgeTier::DaoRen, KnowledgeStatus::Promoted) => GateRequirements {
            min_supporting_refs: 2,
            min_verification_refs: 1,
            min_teaching_refs: 0,
            reviewer_required: false,
            counterexamples_block: true,
        },
        (KnowledgeTier::Shu | KnowledgeTier::Qi, KnowledgeStatus::Promoted) => GateRequirements {
            min_supporting_refs: 1,
            min_verification_refs: 1,
            min_teaching_refs: 0,
            reviewer_required: false,
            counterexamples_block: true,
        },
        _ => GateRequirements {
            min_supporting_refs: 0,
            min_verification_refs: 0,
            min_teaching_refs: 0,
            reviewer_required: false,
            counterexamples_block: true,
        },
    }
}

fn deterministic_link_id(
    card_id: &str,
    evidence_drawer_id: &str,
    role: &KnowledgeEvidenceRole,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"knowledge-card-lifecycle-link-v1");
    hasher.update([0]);
    hasher.update(card_id.as_bytes());
    hasher.update([0]);
    hasher.update(evidence_drawer_id.as_bytes());
    hasher.update([0]);
    hasher.update(evidence_role_slug(role).as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    format!("link_{}", &digest[..16])
}

fn deterministic_event_id(card_id: &str, event_type: &KnowledgeEventType, reason: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"knowledge-card-lifecycle-event-v1");
    hasher.update([0]);
    hasher.update(card_id.as_bytes());
    hasher.update([0]);
    hasher.update(event_type_slug(event_type).as_bytes());
    hasher.update([0]);
    hasher.update(reason.as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    format!("event_{}", &digest[..16])
}

fn evidence_role_slug(value: &KnowledgeEvidenceRole) -> &'static str {
    match value {
        KnowledgeEvidenceRole::Supporting => "supporting",
        KnowledgeEvidenceRole::Verification => "verification",
        KnowledgeEvidenceRole::Counterexample => "counterexample",
        KnowledgeEvidenceRole::Teaching => "teaching",
    }
}

fn event_type_slug(value: &KnowledgeEventType) -> &'static str {
    match value {
        KnowledgeEventType::Created => "created",
        KnowledgeEventType::Promoted => "promoted",
        KnowledgeEventType::Demoted => "demoted",
        KnowledgeEventType::Retired => "retired",
        KnowledgeEventType::Linked => "linked",
        KnowledgeEventType::Unlinked => "unlinked",
        KnowledgeEventType::Updated => "updated",
        KnowledgeEventType::PublishedAnchor => "published_anchor",
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
