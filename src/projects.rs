//! P109: cross-project resume.
//!
//! mempal stores every project's memory in one global `palace.db` keyed by
//! `wing`, and each project's absolute worktree path lives in its
//! `worktree://` anchor_id. This module surfaces that so an agent can, from any
//! directory, list known projects and resume one by fuzzy name. Everything here
//! is deterministic, embedder-free, and read-only — no LLM, no writes.

use std::collections::HashMap;
use std::path::Path;

use rmcp::schemars::{self, JsonSchema};
use serde::Serialize;

use crate::core::db::{Database, DbError};

const WORKTREE_PREFIX: &str = "worktree://";

/// One project's at-a-glance summary, derived from its drawers.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ProjectSummary {
    pub wing: String,
    /// Absolute worktree path from the latest `worktree://` anchor, or null when
    /// the project only has legacy `repo://legacy` anchors.
    pub path: Option<String>,
    /// Epoch-seconds (string) of the most recent drawer in this wing.
    pub last_activity: String,
    pub total: i64,
    pub evidence: i64,
    pub knowledge: i64,
}

/// A recent evidence drawer, for the resume pack.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ResumeEvidence {
    pub drawer_id: String,
    pub source_file: Option<String>,
    pub snippet: String,
    pub added_at: String,
    pub importance: i64,
}

/// An in-flight candidate knowledge drawer (distilled but not yet promoted).
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ResumeCandidate {
    pub drawer_id: String,
    pub statement: Option<String>,
    pub tier: Option<String>,
}

/// Everything an agent needs to pick a project back up.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ResumePack {
    pub wing: String,
    pub path: Option<String>,
    pub last_activity: String,
    pub total: i64,
    pub evidence: i64,
    pub knowledge: i64,
    pub recent_evidence: Vec<ResumeEvidence>,
    pub in_flight: Vec<ResumeCandidate>,
    /// Concrete next step, e.g. `cd <path> && mempal context "resume <wing>"`.
    pub next_step: String,
}

/// Outcome of resolving a fuzzy resume query.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(tag = "resolution", rename_all = "snake_case")]
pub enum ResumeResolution {
    Resolved(Box<ResumePack>),
    Ambiguous {
        query: String,
        candidates: Vec<ProjectSummary>,
    },
    NotFound {
        query: String,
        available: Vec<String>,
    },
}

/// List every project (wing) mempal knows, newest activity first.
pub fn list_projects(db: &Database) -> Result<Vec<ProjectSummary>, DbError> {
    let mut agg = db.conn().prepare(
        r#"
        SELECT wing,
               COUNT(*) AS total,
               SUM(CASE WHEN memory_kind = 'evidence' THEN 1 ELSE 0 END) AS evidence,
               SUM(CASE WHEN memory_kind = 'knowledge' THEN 1 ELSE 0 END) AS knowledge,
               MAX(added_at) AS last_activity
        FROM drawers
        WHERE deleted_at IS NULL
        GROUP BY wing
        "#,
    )?;
    let mut summaries: Vec<ProjectSummary> = agg
        .query_map([], |row| {
            Ok(ProjectSummary {
                wing: row.get(0)?,
                path: None,
                total: row.get(1)?,
                evidence: row.get(2)?,
                knowledge: row.get(3)?,
                last_activity: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    // Latest worktree path per wing. With MAX(added_at), SQLite returns the
    // anchor_id from the row holding that max — i.e. the most recent worktree.
    let mut paths = db.conn().prepare(
        r#"
        SELECT wing, anchor_id, MAX(added_at)
        FROM drawers
        WHERE deleted_at IS NULL
          AND anchor_kind = 'worktree'
          AND anchor_id LIKE 'worktree://%'
        GROUP BY wing
        "#,
    )?;
    let path_map: HashMap<String, String> = paths
        .query_map([], |row| {
            let wing: String = row.get(0)?;
            let anchor: String = row.get(1)?;
            Ok((wing, anchor))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?
        .into_iter()
        .map(|(wing, anchor)| {
            let path = anchor
                .strip_prefix(WORKTREE_PREFIX)
                .unwrap_or(&anchor)
                .to_string();
            (wing, path)
        })
        .collect();

    for summary in &mut summaries {
        summary.path = path_map.get(&summary.wing).cloned();
    }

    summaries.sort_by(|a, b| {
        b.last_activity
            .parse::<i64>()
            .unwrap_or(0)
            .cmp(&a.last_activity.parse::<i64>().unwrap_or(0))
            .then_with(|| a.wing.cmp(&b.wing))
    });
    Ok(summaries)
}

/// Resolve a fuzzy project name and, on a unique match, build a resume pack.
pub fn resume_project(
    db: &Database,
    query: &str,
    evidence_limit: usize,
    candidate_limit: usize,
) -> Result<ResumeResolution, DbError> {
    let needle = query.trim().to_lowercase();
    let projects = list_projects(db)?;
    let available = || projects.iter().map(|p| p.wing.clone()).collect::<Vec<_>>();

    if needle.is_empty() {
        return Ok(ResumeResolution::NotFound {
            query: query.to_string(),
            available: available(),
        });
    }

    // An exact wing match wins over substring matches.
    let exact: Vec<&ProjectSummary> = projects
        .iter()
        .filter(|p| p.wing.to_lowercase() == needle)
        .collect();
    let matches: Vec<&ProjectSummary> = if !exact.is_empty() {
        exact
    } else {
        projects
            .iter()
            .filter(|p| {
                p.wing.to_lowercase().contains(&needle) || path_basename_matches(p, &needle)
            })
            .collect()
    };

    match matches.len() {
        0 => Ok(ResumeResolution::NotFound {
            query: query.to_string(),
            available: available(),
        }),
        1 => {
            let p = matches[0].clone();
            let recent_evidence = recent_evidence(db, &p.wing, evidence_limit)?;
            let in_flight = candidate_knowledge(db, &p.wing, candidate_limit)?;
            let next_step = match &p.path {
                Some(path) => {
                    format!("cd {path} && mempal context \"resume {}\"", p.wing)
                }
                None => format!(
                    "project '{}' has no recorded worktree path (legacy drawers); \
                     supply the path, then run mempal context",
                    p.wing
                ),
            };
            Ok(ResumeResolution::Resolved(Box::new(ResumePack {
                wing: p.wing,
                path: p.path,
                last_activity: p.last_activity,
                total: p.total,
                evidence: p.evidence,
                knowledge: p.knowledge,
                recent_evidence,
                in_flight,
                next_step,
            })))
        }
        _ => Ok(ResumeResolution::Ambiguous {
            query: query.to_string(),
            candidates: matches.into_iter().cloned().collect(),
        }),
    }
}

fn path_basename_matches(project: &ProjectSummary, needle: &str) -> bool {
    project
        .path
        .as_deref()
        .and_then(|path| Path::new(path).file_name())
        .and_then(|name| name.to_str())
        .map(|name| name.to_lowercase().contains(needle))
        .unwrap_or(false)
}

fn recent_evidence(
    db: &Database,
    wing: &str,
    limit: usize,
) -> Result<Vec<ResumeEvidence>, DbError> {
    let mut statement = db.conn().prepare(
        r#"
        SELECT id, source_file, substr(content, 1, 200), added_at, importance
        FROM drawers
        WHERE deleted_at IS NULL AND wing = ?1 AND memory_kind = 'evidence'
        ORDER BY added_at DESC, importance DESC
        LIMIT ?2
        "#,
    )?;
    let rows = statement
        .query_map(rusqlite::params![wing, limit as i64], |row| {
            Ok(ResumeEvidence {
                drawer_id: row.get(0)?,
                source_file: row.get::<_, Option<String>>(1)?,
                snippet: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                added_at: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                importance: row.get::<_, Option<i64>>(4)?.unwrap_or(0),
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn candidate_knowledge(
    db: &Database,
    wing: &str,
    limit: usize,
) -> Result<Vec<ResumeCandidate>, DbError> {
    let mut statement = db.conn().prepare(
        r#"
        SELECT id, statement, tier
        FROM drawers
        WHERE deleted_at IS NULL AND wing = ?1
          AND memory_kind = 'knowledge' AND status = 'candidate'
        ORDER BY added_at DESC
        LIMIT ?2
        "#,
    )?;
    let rows = statement
        .query_map(rusqlite::params![wing, limit as i64], |row| {
            Ok(ResumeCandidate {
                drawer_id: row.get(0)?,
                statement: row.get::<_, Option<String>>(1)?,
                tier: row.get::<_, Option<String>>(2)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}
