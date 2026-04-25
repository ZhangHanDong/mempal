use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::core::{
    db::Database,
    types::{Drawer, KnowledgeStatus, KnowledgeTier, MemoryKind},
};

#[derive(Debug, Clone, Serialize)]
pub struct GateReport {
    pub drawer_id: String,
    pub tier: String,
    pub status: String,
    pub target_status: String,
    pub allowed: bool,
    pub reasons: Vec<String>,
    pub requirements: GateRequirements,
    pub evidence_counts: GateEvidenceCounts,
}

#[derive(Debug, Clone, Serialize)]
pub struct GateRequirements {
    pub min_supporting_refs: usize,
    pub min_verification_refs: usize,
    pub min_teaching_refs: usize,
    pub reviewer_required: bool,
    pub counterexamples_block: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct GateEvidenceCounts {
    pub supporting: usize,
    pub counterexample: usize,
    pub teaching: usize,
    pub verification: usize,
}

pub fn evaluate_gate_by_id(
    db: &Database,
    drawer_id: &str,
    target_status: Option<&str>,
    reviewer: Option<&str>,
    allow_counterexamples: bool,
) -> Result<GateReport> {
    let drawer = load_gate_knowledge_drawer(db, drawer_id)?;
    let tier = drawer.tier.as_ref().expect("knowledge drawer has tier");
    let target_status = match target_status {
        Some(value) => parse_status(value)?,
        None => default_target_status(tier),
    };
    validate_tier_status(tier, &target_status)?;
    evaluate_gate(db, &drawer, &target_status, reviewer, allow_counterexamples)
}

fn load_gate_knowledge_drawer(db: &Database, drawer_id: &str) -> Result<Drawer> {
    let drawer = db
        .get_drawer(drawer_id)
        .context("failed to look up drawer")?
        .with_context(|| format!("drawer not found: {drawer_id}"))?;
    if drawer.memory_kind != MemoryKind::Knowledge {
        bail!("knowledge gate requires a knowledge drawer");
    }
    if drawer.tier.is_none() || drawer.status.is_none() {
        bail!("knowledge gate requires tier and status metadata");
    }
    Ok(drawer)
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

fn evaluate_gate(
    db: &Database,
    drawer: &Drawer,
    target_status: &KnowledgeStatus,
    reviewer: Option<&str>,
    allow_counterexamples: bool,
) -> Result<GateReport> {
    validate_gate_refs(db, &drawer.supporting_refs)?;
    validate_gate_refs(db, &drawer.counterexample_refs)?;
    validate_gate_refs(db, &drawer.teaching_refs)?;
    validate_gate_refs(db, &drawer.verification_refs)?;

    let tier = drawer.tier.as_ref().expect("knowledge drawer has tier");
    let status = drawer.status.as_ref().expect("knowledge drawer has status");
    let requirements = gate_requirements(tier, target_status);
    let evidence_counts = GateEvidenceCounts {
        supporting: drawer.supporting_refs.len(),
        counterexample: drawer.counterexample_refs.len(),
        teaching: drawer.teaching_refs.len(),
        verification: drawer.verification_refs.len(),
    };
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

    Ok(GateReport {
        drawer_id: drawer.id.clone(),
        tier: tier_slug(tier).to_string(),
        status: status_slug(status).to_string(),
        target_status: status_slug(target_status).to_string(),
        allowed: reasons.is_empty(),
        reasons,
        requirements,
        evidence_counts,
    })
}

fn validate_gate_refs(db: &Database, refs: &[String]) -> Result<()> {
    for drawer_id in refs {
        if !drawer_id.starts_with("drawer_") {
            bail!("gate refs must contain drawer ids");
        }
        let drawer = db
            .get_drawer(drawer_id)
            .with_context(|| format!("failed to load ref drawer {drawer_id}"))?
            .with_context(|| format!("ref drawer not found: {drawer_id}"))?;
        if drawer.memory_kind != MemoryKind::Evidence {
            bail!("gate refs must point to evidence drawers");
        }
    }
    Ok(())
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
