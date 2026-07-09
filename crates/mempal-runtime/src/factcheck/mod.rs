//! Offline fact-checking against KG triples + entity registry (P9-A).
//!
//! Given a text blob, detect three contradiction classes:
//! 1. SimilarNameConflict — mentioned name ≤2 edit distance from known entity
//! 2. RelationContradiction — KG has incompatible predicate for same (subject, object)
//! 3. StaleFact — text asserts a triple whose KG row has valid_to < now
//!
//! Zero LLM, zero network, deterministic. Time is Unix seconds (String) to
//! match the existing KG storage convention (no chrono dep).

use rmcp::schemars::{self, JsonSchema};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::core::db::{Database, DbError};

pub mod contradictions;
pub mod names;
pub mod relations;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FactIssue {
    SimilarNameConflict {
        mentioned: String,
        known_entity: String,
        edit_distance: usize,
    },
    RelationContradiction {
        subject: String,
        text_claim: String,
        kg_fact: String,
        triple_id: String,
        source_drawer: Option<String>,
    },
    StaleFact {
        subject: String,
        predicate: String,
        object: String,
        /// Unix seconds as stored in DB (triple.valid_to).
        valid_to: String,
        triple_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct FactCheckReport {
    pub issues: Vec<FactIssue>,
    pub checked_entities: Vec<String>,
    pub kg_triples_scanned: usize,
}

#[derive(Debug, Error)]
pub enum FactCheckError {
    #[error("db error: {0}")]
    Db(#[from] DbError),
    #[error("invalid scope: {0}")]
    InvalidScope(String),
    #[error("invalid `now`: {0}")]
    InvalidNow(String),
}

pub fn validate_scope<'a>(
    wing: Option<&'a str>,
    room: Option<&'a str>,
) -> Result<Option<(&'a str, Option<&'a str>)>, FactCheckError> {
    match (wing.map(str::trim), room.map(str::trim)) {
        (None, Some(_)) => Err(FactCheckError::InvalidScope(
            "room requires wing".to_string(),
        )),
        (Some(""), _) => Err(FactCheckError::InvalidScope(
            "wing must not be empty".to_string(),
        )),
        (_, Some("")) => Err(FactCheckError::InvalidScope(
            "room must not be empty".to_string(),
        )),
        (Some(wing), room) => Ok(Some((wing, room))),
        (None, None) => Ok(None),
    }
}

pub fn resolve_now(now: Option<&str>) -> Result<u64, FactCheckError> {
    match now {
        Some(raw) => {
            let ts = crate::cowork::peek::parse_rfc3339(raw).ok_or_else(|| {
                FactCheckError::InvalidNow(format!("expected RFC3339 timestamp, got `{raw}`"))
            })?;
            u64::try_from(ts).map_err(|_| {
                FactCheckError::InvalidNow(format!(
                    "timestamp before Unix epoch is unsupported: {raw}"
                ))
            })
        }
        None => {
            use std::time::{SystemTime, UNIX_EPOCH};
            Ok(SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0))
        }
    }
}

/// Run fact check against the KG.
///
/// `now_unix_secs`: Unix seconds for the "now" cutoff used by StaleFact
/// detection. Matches the KG storage convention (`valid_to` is text Unix
/// seconds). Callers should use `crate::core::utils::current_timestamp()`
/// to obtain the current value.
///
/// `scope`: optional `(wing, room)` filter for which drawers contribute
/// to the known-entity set. `None` = all wings.
pub fn check(
    text: &str,
    db: &Database,
    now_unix_secs: u64,
    scope: Option<(&str, Option<&str>)>,
) -> Result<FactCheckReport, FactCheckError> {
    let scope = match scope {
        Some((wing, room)) => validate_scope(Some(wing), room)?,
        None => None,
    };
    let text_names = names::candidates_from_text(text);
    let known = names::query_known_entities(db, scope)?;
    let mut issues = names::detect_similar_name_conflicts(&text_names, &known);

    let text_triples = relations::extract_triples(text);
    let kg_triples_scanned = db.triple_count().unwrap_or(0) as usize;

    issues.extend(contradictions::detect_relation_contradictions(
        db,
        &text_triples,
    )?);
    issues.extend(contradictions::detect_stale_facts(
        db,
        &text_triples,
        now_unix_secs,
    )?);

    Ok(FactCheckReport {
        issues,
        checked_entities: text_names,
        kg_triples_scanned,
    })
}
