use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::core::{
    anchor::{self, DerivedAnchor},
    db::Database,
    types::{AnchorKind, Drawer, KnowledgeStatus, MemoryDomain, MemoryKind},
    utils::current_timestamp,
};

#[derive(Debug, Clone)]
pub struct PublishAnchorRequest {
    pub drawer_id: String,
    pub to: String,
    pub target_anchor_id: Option<String>,
    pub cwd: Option<PathBuf>,
    pub reason: String,
    pub reviewer: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublishAnchorOutcome {
    pub drawer_id: String,
    pub old_anchor_kind: String,
    pub old_anchor_id: String,
    pub old_parent_anchor_id: Option<String>,
    pub new_anchor_kind: String,
    pub new_anchor_id: String,
    pub new_parent_anchor_id: Option<String>,
}

pub fn publish_anchor(
    db: &Database,
    request: PublishAnchorRequest,
) -> Result<PublishAnchorOutcome> {
    let drawer = load_publishable_knowledge(db, &request.drawer_id)?;
    let target = resolve_target_anchor(&drawer, &request)?;
    if drawer.anchor_kind == target.anchor_kind && drawer.anchor_id == target.anchor_id {
        bail!("same-anchor publication is not allowed");
    }
    validate_publication_chain(&drawer, &target)?;
    anchor::validate_anchor_domain(&drawer.domain, &target.anchor_kind)
        .map_err(|message| anyhow::anyhow!(message))?;
    anchor::validate_explicit_anchor(&target.anchor_kind, &target.anchor_id)?;

    db.update_knowledge_anchor(
        &request.drawer_id,
        &target.anchor_kind,
        &target.anchor_id,
        target.parent_anchor_id.as_deref(),
    )
    .context("failed to update knowledge anchor")?;
    append_audit_entry(
        db,
        "knowledge_publish_anchor",
        &serde_json::json!({
            "drawer_id": request.drawer_id,
            "old_anchor_kind": anchor_kind_slug(&drawer.anchor_kind),
            "old_anchor_id": drawer.anchor_id,
            "old_parent_anchor_id": drawer.parent_anchor_id,
            "new_anchor_kind": anchor_kind_slug(&target.anchor_kind),
            "new_anchor_id": target.anchor_id,
            "new_parent_anchor_id": target.parent_anchor_id,
            "reason": request.reason,
            "reviewer": request.reviewer,
        }),
    )
    .context("failed to append audit log")?;

    Ok(PublishAnchorOutcome {
        drawer_id: drawer.id,
        old_anchor_kind: anchor_kind_slug(&drawer.anchor_kind).to_string(),
        old_anchor_id: drawer.anchor_id,
        old_parent_anchor_id: drawer.parent_anchor_id,
        new_anchor_kind: anchor_kind_slug(&target.anchor_kind).to_string(),
        new_anchor_id: target.anchor_id,
        new_parent_anchor_id: target.parent_anchor_id,
    })
}

fn load_publishable_knowledge(db: &Database, drawer_id: &str) -> Result<Drawer> {
    let drawer = db
        .get_drawer(drawer_id)
        .context("failed to look up drawer")?
        .with_context(|| format!("drawer not found: {drawer_id}"))?;
    if drawer.memory_kind != MemoryKind::Knowledge {
        bail!("knowledge anchor publication requires a knowledge drawer");
    }
    match drawer.status {
        Some(KnowledgeStatus::Promoted | KnowledgeStatus::Canonical) => Ok(drawer),
        _ => bail!("publish-anchor requires promoted or canonical knowledge"),
    }
}

fn resolve_target_anchor(drawer: &Drawer, request: &PublishAnchorRequest) -> Result<DerivedAnchor> {
    match request.to.trim() {
        "repo" => resolve_repo_target(drawer, request),
        "global" => resolve_global_target(drawer, request),
        other => bail!("unsupported publish target: {other}"),
    }
}

fn resolve_repo_target(drawer: &Drawer, request: &PublishAnchorRequest) -> Result<DerivedAnchor> {
    if let Some(anchor_id) = request.target_anchor_id.as_deref() {
        anchor::validate_explicit_anchor(&AnchorKind::Repo, anchor_id)?;
        return Ok(DerivedAnchor {
            anchor_kind: AnchorKind::Repo,
            anchor_id: anchor_id.to_string(),
            parent_anchor_id: None,
        });
    }
    if let Some(parent_anchor_id) = drawer.parent_anchor_id.as_deref() {
        anchor::validate_explicit_anchor(&AnchorKind::Repo, parent_anchor_id)?;
        return Ok(DerivedAnchor {
            anchor_kind: AnchorKind::Repo,
            anchor_id: parent_anchor_id.to_string(),
            parent_anchor_id: None,
        });
    }
    if let Some(cwd) = request.cwd.as_deref() {
        let derived = anchor::derive_anchor_from_cwd(Some(Path::new(cwd)))?;
        if let Some(parent_anchor_id) = derived.parent_anchor_id {
            return Ok(DerivedAnchor {
                anchor_kind: AnchorKind::Repo,
                anchor_id: parent_anchor_id,
                parent_anchor_id: None,
            });
        }
    }
    bail!("repo publication requires --target-anchor-id, parent_anchor_id, or --cwd with a repo")
}

fn resolve_global_target(drawer: &Drawer, request: &PublishAnchorRequest) -> Result<DerivedAnchor> {
    if drawer.domain != MemoryDomain::Global {
        bail!("global anchor requires domain=global");
    }
    let anchor_id = request
        .target_anchor_id
        .as_deref()
        .context("--target-anchor-id is required for global publication")?;
    anchor::validate_explicit_anchor(&AnchorKind::Global, anchor_id)?;
    Ok(DerivedAnchor {
        anchor_kind: AnchorKind::Global,
        anchor_id: anchor_id.to_string(),
        parent_anchor_id: None,
    })
}

fn validate_publication_chain(drawer: &Drawer, target: &DerivedAnchor) -> Result<()> {
    match (&drawer.anchor_kind, &target.anchor_kind) {
        (AnchorKind::Worktree, AnchorKind::Repo) => Ok(()),
        (AnchorKind::Repo, AnchorKind::Global) => Ok(()),
        (AnchorKind::Worktree, AnchorKind::Global) => {
            bail!("worktree -> global publication is not allowed")
        }
        (AnchorKind::Global, _) => bail!("inward publication from global is not allowed"),
        (AnchorKind::Repo, AnchorKind::Worktree) | (AnchorKind::Worktree, AnchorKind::Worktree) => {
            bail!("inward publication is not allowed")
        }
        (AnchorKind::Repo, AnchorKind::Repo) => bail!("same-anchor publication is not allowed"),
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

fn anchor_kind_slug(value: &AnchorKind) -> &'static str {
    match value {
        AnchorKind::Global => "global",
        AnchorKind::Repo => "repo",
        AnchorKind::Worktree => "worktree",
    }
}
