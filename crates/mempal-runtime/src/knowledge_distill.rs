use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::core::{
    anchor,
    db::Database,
    types::{
        BootstrapIdentityParts, Drawer, KnowledgeStatus, KnowledgeTier, MemoryDomain, MemoryKind,
        SourceType, TriggerHints,
    },
    utils::{build_bootstrap_drawer_id_from_parts, current_timestamp, knowledge_source_file},
};
use crate::ingest::normalize::CURRENT_NORMALIZE_VERSION;

#[derive(Debug, Clone)]
pub struct DistillRequest {
    pub statement: String,
    pub content: String,
    pub tier: String,
    pub supporting_refs: Vec<String>,
    pub wing: String,
    pub room: String,
    pub domain: String,
    pub field: String,
    pub cwd: Option<PathBuf>,
    pub scope_constraints: Option<String>,
    pub counterexample_refs: Vec<String>,
    pub teaching_refs: Vec<String>,
    pub trigger_hints: Option<TriggerHints>,
    pub importance: i32,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DistillOutcome {
    pub drawer_id: String,
    pub created: bool,
    pub dry_run: bool,
}

#[derive(Debug, Clone)]
pub enum DistillPlan {
    Done(DistillOutcome),
    Create(Box<PreparedDistill>),
}

#[derive(Debug, Clone)]
pub struct PreparedDistill {
    pub drawer_id: String,
    pub content: String,
    drawer: Drawer,
    supporting_refs: Vec<String>,
    counterexample_refs: Vec<String>,
    teaching_refs: Vec<String>,
}

pub fn prepare_distill(db: &Database, request: DistillRequest) -> Result<DistillPlan> {
    if !(0..=5).contains(&request.importance) {
        bail!("importance must be between 0 and 5");
    }
    let statement = trim_required(&request.statement, "statement")?;
    let content = trim_required(&request.content, "content")?;
    let wing = trim_required(&request.wing, "wing")?;
    let room = trim_required(&request.room, "room")?;
    let field = trim_required(&request.field, "field")?;
    let domain = parse_domain(&request.domain)?;
    let tier = parse_distill_tier(&request.tier)?;
    let memory_kind = MemoryKind::Knowledge;
    let status = KnowledgeStatus::Candidate;
    let verification_refs: &[String] = &[];

    let supporting_refs = normalized_nonempty_strings(&request.supporting_refs);
    let counterexample_refs = normalized_nonempty_strings(&request.counterexample_refs);
    let teaching_refs = normalized_nonempty_strings(&request.teaching_refs);
    validate_distill_refs(db, "supporting_refs", &supporting_refs)?;
    validate_distill_refs(db, "counterexample_refs", &counterexample_refs)?;
    validate_distill_refs(db, "teaching_refs", &teaching_refs)?;

    let scope_constraints = request
        .scope_constraints
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let trigger_hints = request.trigger_hints.and_then(normalize_trigger_hints);
    let anchor = distill_anchor(&domain, request.cwd.as_deref())?;
    anchor::validate_anchor_domain(&domain, &anchor.anchor_kind)
        .map_err(|message| anyhow::anyhow!(message.to_string()))?;

    let drawer_id = build_bootstrap_drawer_id_from_parts(
        &wing,
        Some(&room),
        &content,
        BootstrapIdentityParts {
            memory_kind: &memory_kind,
            domain: &domain,
            field: &field,
            anchor_kind: &anchor.anchor_kind,
            anchor_id: &anchor.anchor_id,
            parent_anchor_id: anchor.parent_anchor_id.as_deref(),
            provenance: None,
            statement: Some(&statement),
            tier: Some(&tier),
            status: Some(&status),
            supporting_refs: &supporting_refs,
            counterexample_refs: &counterexample_refs,
            teaching_refs: &teaching_refs,
            verification_refs,
            scope_constraints: scope_constraints.as_deref(),
            trigger_hints: trigger_hints.as_ref(),
        },
    );

    if request.dry_run {
        return Ok(DistillPlan::Done(DistillOutcome {
            drawer_id,
            created: false,
            dry_run: true,
        }));
    }

    if db
        .drawer_exists(&drawer_id)
        .context("failed to check existing distilled drawer")?
    {
        return Ok(DistillPlan::Done(DistillOutcome {
            drawer_id,
            created: false,
            dry_run: false,
        }));
    }

    let drawer = Drawer {
        id: drawer_id.clone(),
        content: content.clone(),
        wing,
        room: Some(room),
        source_file: Some(knowledge_source_file(&domain, &field, &tier, &statement)),
        source_type: SourceType::Manual,
        added_at: current_timestamp(),
        chunk_index: Some(0),
        normalize_version: CURRENT_NORMALIZE_VERSION,
        importance: request.importance,
        memory_kind: MemoryKind::Knowledge,
        domain,
        field,
        anchor_kind: anchor.anchor_kind,
        anchor_id: anchor.anchor_id,
        parent_anchor_id: anchor.parent_anchor_id,
        provenance: None,
        statement: Some(statement),
        tier: Some(tier),
        status: Some(status),
        supporting_refs: supporting_refs.clone(),
        counterexample_refs: counterexample_refs.clone(),
        teaching_refs: teaching_refs.clone(),
        verification_refs: Vec::new(),
        scope_constraints,
        trigger_hints,
    };

    Ok(DistillPlan::Create(Box::new(PreparedDistill {
        drawer_id,
        content,
        drawer,
        supporting_refs,
        counterexample_refs,
        teaching_refs,
    })))
}

pub fn commit_distill(
    db: &Database,
    prepared: PreparedDistill,
    vector: &[f32],
) -> Result<DistillOutcome> {
    db.insert_drawer(&prepared.drawer)
        .context("failed to insert distilled knowledge drawer")?;
    db.insert_vector(&prepared.drawer_id, vector)
        .context("failed to insert distilled knowledge vector")?;
    append_audit_entry(
        db,
        "knowledge_distill",
        &serde_json::json!({
            "drawer_id": prepared.drawer_id,
            "statement": prepared.drawer.statement,
            "tier": prepared.drawer.tier.as_ref().map(tier_slug),
            "status": prepared.drawer.status.as_ref().map(status_slug),
            "supporting_refs": prepared.supporting_refs,
            "counterexample_refs": prepared.counterexample_refs,
            "teaching_refs": prepared.teaching_refs,
        }),
    )
    .context("failed to append audit log")?;

    Ok(DistillOutcome {
        drawer_id: prepared.drawer_id,
        created: true,
        dry_run: false,
    })
}

fn trim_required(value: &str, field: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("{field} must not be empty");
    }
    Ok(trimmed.to_string())
}

fn normalized_nonempty_strings(values: &[String]) -> Vec<String> {
    values
        .iter()
        .filter_map(|value| {
            let trimmed = value.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        })
        .collect()
}

fn normalize_trigger_hints(hints: TriggerHints) -> Option<TriggerHints> {
    let intent_tags = normalized_nonempty_strings(&hints.intent_tags);
    let workflow_bias = normalized_nonempty_strings(&hints.workflow_bias);
    let tool_needs = normalized_nonempty_strings(&hints.tool_needs);
    if intent_tags.is_empty() && workflow_bias.is_empty() && tool_needs.is_empty() {
        return None;
    }
    Some(TriggerHints {
        intent_tags,
        workflow_bias,
        tool_needs,
    })
}

fn parse_domain(value: &str) -> Result<MemoryDomain> {
    match value.trim() {
        "project" => Ok(MemoryDomain::Project),
        "agent" => Ok(MemoryDomain::Agent),
        "skill" => Ok(MemoryDomain::Skill),
        "global" => Ok(MemoryDomain::Global),
        other => bail!("unsupported domain: {other}"),
    }
}

fn parse_distill_tier(value: &str) -> Result<KnowledgeTier> {
    match value.trim() {
        "dao_ren" => Ok(KnowledgeTier::DaoRen),
        "qi" => Ok(KnowledgeTier::Qi),
        "dao_tian" | "shu" => bail!("distill only allows candidate dao_ren or qi"),
        other => bail!("unsupported knowledge tier: {other}"),
    }
}

fn distill_anchor(domain: &MemoryDomain, cwd: Option<&Path>) -> Result<anchor::DerivedAnchor> {
    if matches!(domain, MemoryDomain::Global) {
        return Ok(anchor::DerivedAnchor {
            anchor_kind: crate::core::types::AnchorKind::Global,
            anchor_id: "global://default".to_string(),
            parent_anchor_id: None,
        });
    }
    let cwd = match cwd {
        Some(path) => path.to_path_buf(),
        None => std::env::current_dir().context("failed to read current directory")?,
    };
    anchor::derive_anchor_from_cwd(Some(&cwd)).context("failed to derive distill anchor")
}

fn validate_distill_refs(db: &Database, field: &str, refs: &[String]) -> Result<()> {
    if field == "supporting_refs" && refs.is_empty() {
        bail!("supporting_refs must not be empty");
    }
    for drawer_id in refs {
        if !drawer_id.starts_with("drawer_") {
            bail!("{field} must contain drawer ids");
        }
        let drawer = db
            .get_drawer(drawer_id)
            .with_context(|| format!("failed to load ref drawer {drawer_id}"))?
            .with_context(|| format!("ref drawer not found: {drawer_id}"))?;
        if drawer.memory_kind != MemoryKind::Evidence {
            bail!("{field} must point to evidence drawers");
        }
    }
    Ok(())
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
        "ts": current_timestamp(),
        "command": command,
        "details": details,
    });
    writeln!(
        file,
        "{}",
        serde_json::to_string(&entry).context("failed to serialize audit entry")?
    )
    .with_context(|| format!("failed to write audit log {}", audit_path.display()))?;
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
