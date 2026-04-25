use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::core::{
    db::Database,
    types::{Drawer, KnowledgeStatus, KnowledgeTier, MemoryKind},
    utils::current_timestamp,
};
use crate::knowledge_gate::{GateReport, evaluate_gate_for_drawer};

#[derive(Debug, Clone)]
pub struct PromoteRequest {
    pub drawer_id: String,
    pub status: String,
    pub verification_refs: Vec<String>,
    pub reason: String,
    pub reviewer: Option<String>,
    pub allow_counterexamples: bool,
    pub enforce_gate: bool,
}

#[derive(Debug, Clone)]
pub struct DemoteRequest {
    pub drawer_id: String,
    pub status: String,
    pub evidence_refs: Vec<String>,
    pub reason: String,
    pub reason_type: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PromoteOutcome {
    pub drawer_id: String,
    pub old_status: String,
    pub new_status: String,
    pub verification_refs: Vec<String>,
    pub gate: Option<GateReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DemoteOutcome {
    pub drawer_id: String,
    pub old_status: String,
    pub new_status: String,
    pub counterexample_refs: Vec<String>,
}

pub fn promote_knowledge(db: &Database, request: PromoteRequest) -> Result<PromoteOutcome> {
    let target_status = parse_lifecycle_status(&request.status)?;
    if !matches!(
        target_status,
        KnowledgeStatus::Promoted | KnowledgeStatus::Canonical
    ) {
        bail!("promote status must be promoted or canonical");
    }
    let mut drawer = load_lifecycle_knowledge_drawer(db, &request.drawer_id)?;
    validate_tier_status(
        drawer.tier.as_ref().expect("knowledge drawer has tier"),
        &target_status,
    )?;
    validate_lifecycle_refs(db, &request.verification_refs)?;

    let old_status = drawer.status.clone().expect("knowledge drawer has status");
    append_unique_refs(&mut drawer.verification_refs, &request.verification_refs);
    let gate = if request.enforce_gate {
        let gate = evaluate_gate_for_drawer(
            db,
            &drawer,
            &target_status,
            request.reviewer.as_deref(),
            request.allow_counterexamples,
        )?;
        if !gate.allowed {
            bail!("promotion gate failed: {}", gate.reasons.join("; "));
        }
        Some(gate)
    } else {
        None
    };

    db.update_knowledge_lifecycle(
        &request.drawer_id,
        &target_status,
        &drawer.verification_refs,
        &drawer.counterexample_refs,
    )
    .context("failed to update knowledge lifecycle")?;
    append_audit_entry(
        db,
        "knowledge_promote",
        &serde_json::json!({
            "drawer_id": request.drawer_id,
            "old_status": knowledge_status_slug(&old_status),
            "new_status": knowledge_status_slug(&target_status),
            "verification_refs": request.verification_refs,
            "reason": request.reason,
            "reviewer": request.reviewer,
        }),
    )
    .context("failed to append audit log")?;

    Ok(PromoteOutcome {
        drawer_id: drawer.id,
        old_status: knowledge_status_slug(&old_status).to_string(),
        new_status: knowledge_status_slug(&target_status).to_string(),
        verification_refs: drawer.verification_refs,
        gate,
    })
}

pub fn demote_knowledge(db: &Database, request: DemoteRequest) -> Result<DemoteOutcome> {
    let target_status = parse_lifecycle_status(&request.status)?;
    if !matches!(
        target_status,
        KnowledgeStatus::Demoted | KnowledgeStatus::Retired
    ) {
        bail!("demote status must be demoted or retired");
    }
    validate_demote_reason_type(&request.reason_type)?;
    let mut drawer = load_lifecycle_knowledge_drawer(db, &request.drawer_id)?;
    validate_tier_status(
        drawer.tier.as_ref().expect("knowledge drawer has tier"),
        &target_status,
    )?;
    validate_lifecycle_refs(db, &request.evidence_refs)?;

    let old_status = drawer.status.clone().expect("knowledge drawer has status");
    append_unique_refs(&mut drawer.counterexample_refs, &request.evidence_refs);
    db.update_knowledge_lifecycle(
        &request.drawer_id,
        &target_status,
        &drawer.verification_refs,
        &drawer.counterexample_refs,
    )
    .context("failed to update knowledge lifecycle")?;
    append_audit_entry(
        db,
        "knowledge_demote",
        &serde_json::json!({
            "drawer_id": request.drawer_id,
            "old_status": knowledge_status_slug(&old_status),
            "new_status": knowledge_status_slug(&target_status),
            "evidence_refs": request.evidence_refs,
            "reason": request.reason,
            "reason_type": request.reason_type,
        }),
    )
    .context("failed to append audit log")?;

    Ok(DemoteOutcome {
        drawer_id: drawer.id,
        old_status: knowledge_status_slug(&old_status).to_string(),
        new_status: knowledge_status_slug(&target_status).to_string(),
        counterexample_refs: drawer.counterexample_refs,
    })
}

fn load_lifecycle_knowledge_drawer(db: &Database, drawer_id: &str) -> Result<Drawer> {
    let drawer = db
        .get_drawer(drawer_id)
        .context("failed to look up drawer")?
        .with_context(|| format!("drawer not found: {drawer_id}"))?;
    if drawer.memory_kind != MemoryKind::Knowledge {
        bail!("knowledge lifecycle requires a knowledge drawer");
    }
    if drawer.tier.is_none() || drawer.status.is_none() {
        bail!("knowledge lifecycle requires tier and status metadata");
    }
    Ok(drawer)
}

fn parse_lifecycle_status(value: &str) -> Result<KnowledgeStatus> {
    match value.trim() {
        "candidate" => Ok(KnowledgeStatus::Candidate),
        "promoted" => Ok(KnowledgeStatus::Promoted),
        "canonical" => Ok(KnowledgeStatus::Canonical),
        "demoted" => Ok(KnowledgeStatus::Demoted),
        "retired" => Ok(KnowledgeStatus::Retired),
        other => bail!("unsupported knowledge status: {other}"),
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

fn validate_lifecycle_refs(db: &Database, refs: &[String]) -> Result<()> {
    if refs.is_empty() {
        bail!("at least one lifecycle evidence ref is required");
    }
    for drawer_id in refs {
        if !drawer_id.starts_with("drawer_") {
            bail!("lifecycle refs must contain drawer ids");
        }
        let drawer = db
            .get_drawer(drawer_id)
            .with_context(|| format!("failed to load ref drawer {drawer_id}"))?
            .with_context(|| format!("ref drawer not found: {drawer_id}"))?;
        if drawer.memory_kind != MemoryKind::Evidence {
            bail!("lifecycle refs must point to evidence drawers");
        }
    }
    Ok(())
}

fn append_unique_refs(target: &mut Vec<String>, refs: &[String]) {
    for item in refs {
        if !target.iter().any(|existing| existing == item) {
            target.push(item.clone());
        }
    }
}

fn append_audit_entry(db: &Database, command: &str, details: &serde_json::Value) -> Result<()> {
    let audit_path = db
        .path()
        .parent()
        .map(|parent| parent.join("audit.jsonl"))
        .unwrap_or_else(|| PathBuf::from("audit.jsonl"));
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&audit_path)
        .with_context(|| format!("failed to open audit log {}", audit_path.display()))?;
    let entry = serde_json::json!({
        "timestamp": current_timestamp(),
        "command": command,
        "details": details,
    });
    writeln!(file, "{entry}")
        .with_context(|| format!("failed to write audit log {}", audit_path.display()))?;
    Ok(())
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
