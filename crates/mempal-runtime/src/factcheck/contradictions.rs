//! KG-backed contradiction + staleness detection for extracted triples.

use crate::core::db::{Database, DbError};

use super::FactIssue;

/// Dictionary of symmetric predicate incompatibilities.
///
/// `are_incompatible(a, b)` returns true for any `(a, b)` or `(b, a)`
/// listed here. Not an ontology — just a small sanity net. Callers can
/// extend via follow-up spec or config in future.
const INCOMPATIBLE_PREDICATES: &[(&str, &str)] = &[
    ("husband_of", "brother_of"),
    ("husband_of", "father_of"),
    ("husband_of", "son_of"),
    ("wife_of", "sister_of"),
    ("wife_of", "mother_of"),
    ("wife_of", "daughter_of"),
    ("mother_of", "wife_of"),
    ("father_of", "husband_of"),
    ("employee_of", "founder_of"),
    ("employee_of", "owner_of"),
    ("reports_to", "manages"),
    ("subordinate_of", "manager_of"),
];

pub fn are_incompatible(p1: &str, p2: &str) -> bool {
    INCOMPATIBLE_PREDICATES
        .iter()
        .any(|(a, b)| (a == &p1 && b == &p2) || (a == &p2 && b == &p1))
}

pub fn detect_relation_contradictions(
    db: &Database,
    text_triples: &[(String, String, String)],
) -> Result<Vec<FactIssue>, DbError> {
    let mut issues = Vec::new();

    for (subject, text_pred, object) in text_triples {
        // Fetch active KG triples with the same endpoints. Ignore
        // predicate filter on the DB side so we can cross-check.
        let active = db.query_triples(Some(subject), None, Some(object), true)?;
        for kg in active {
            if are_incompatible(text_pred, &kg.predicate) {
                issues.push(FactIssue::RelationContradiction {
                    subject: subject.clone(),
                    text_claim: text_pred.clone(),
                    kg_fact: kg.predicate.clone(),
                    triple_id: kg.id.clone(),
                    source_drawer: kg.source_drawer.clone(),
                });
            }
        }
    }
    Ok(issues)
}

pub fn detect_stale_facts(
    db: &Database,
    text_triples: &[(String, String, String)],
    now_unix_secs: u64,
) -> Result<Vec<FactIssue>, DbError> {
    let mut issues = Vec::new();

    for (subject, predicate, object) in text_triples {
        let rows = db.query_triples(Some(subject), Some(predicate), Some(object), false)?;
        for kg in rows {
            let Some(valid_to) = kg.valid_to.as_deref() else {
                continue;
            };
            let Ok(expiry_secs) = valid_to.parse::<u64>() else {
                continue;
            };
            if expiry_secs < now_unix_secs {
                issues.push(FactIssue::StaleFact {
                    subject: kg.subject.clone(),
                    predicate: kg.predicate.clone(),
                    object: kg.object.clone(),
                    valid_to: valid_to.to_string(),
                    triple_id: kg.id.clone(),
                });
            }
        }
    }
    Ok(issues)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_incompatible_predicates_symmetric() {
        assert!(are_incompatible("husband_of", "brother_of"));
        assert!(are_incompatible("brother_of", "husband_of"));
        assert!(!are_incompatible("husband_of", "wife_of"));
    }

    #[test]
    fn test_incompatible_unknown_predicates_returns_false() {
        assert!(!are_incompatible("foo_of", "bar_of"));
    }
}
