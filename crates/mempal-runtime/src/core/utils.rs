use std::collections::BTreeSet;
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use super::{
    anchor,
    types::{
        AnchorKind, BootstrapIdentityParts, KnowledgeStatus, KnowledgeTier, MemoryDomain,
        MemoryKind, Provenance, SourceType, TaxonomyEntry, TunnelEndpoint,
    },
};

pub const DEFAULT_ROOM: &str = "default";
const BOOTSTRAP_DRAWER_ID_HASH_LEN: usize = 12;

pub fn build_drawer_id(wing: &str, room: Option<&str>, content: &str) -> String {
    let room = room.unwrap_or(DEFAULT_ROOM);
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let digest = format!("{:x}", hasher.finalize());

    format!(
        "drawer_{}_{}_{}",
        sanitize_component(wing),
        sanitize_component(room),
        &digest[..8]
    )
}

pub fn build_bootstrap_drawer_id(
    wing: &str,
    room: Option<&str>,
    content: &str,
    identity_components: &[String],
) -> String {
    let room = room.unwrap_or(DEFAULT_ROOM);
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    for component in identity_components {
        hasher.update([0]);
        hasher.update(component.as_bytes());
    }
    let digest = format!("{:x}", hasher.finalize());

    format!(
        "drawer_{}_{}_{}",
        sanitize_component(wing),
        sanitize_component(room),
        &digest[..BOOTSTRAP_DRAWER_ID_HASH_LEN]
    )
}

pub fn bootstrap_identity_components(parts: BootstrapIdentityParts<'_>) -> Vec<String> {
    let supporting_refs = normalized_sorted_strings(parts.supporting_refs);
    let counterexample_refs = normalized_sorted_strings(parts.counterexample_refs);
    let teaching_refs = normalized_sorted_strings(parts.teaching_refs);
    let verification_refs = normalized_sorted_strings(parts.verification_refs);
    let mut components = vec![
        format!("memory_kind={}", memory_kind_as_str(parts.memory_kind)),
        format!("domain={}", memory_domain_as_str(parts.domain)),
        format!("field={}", parts.field),
        format!("anchor_kind={}", anchor_kind_as_str(parts.anchor_kind)),
        format!("anchor_id={}", parts.anchor_id),
        format!("parent_anchor_id={}", parts.parent_anchor_id.unwrap_or("")),
        format!(
            "provenance={}",
            parts.provenance.map(provenance_as_str).unwrap_or("")
        ),
        format!("statement={}", parts.statement.unwrap_or("")),
        format!(
            "tier={}",
            parts.tier.map(knowledge_tier_as_str).unwrap_or("")
        ),
        format!(
            "status={}",
            parts.status.map(knowledge_status_as_str).unwrap_or("")
        ),
        format!(
            "scope_constraints={}",
            parts.scope_constraints.unwrap_or("")
        ),
        format!("supporting_refs={}", supporting_refs.join(",")),
        format!("counterexample_refs={}", counterexample_refs.join(",")),
        format!("teaching_refs={}", teaching_refs.join(",")),
        format!("verification_refs={}", verification_refs.join(",")),
    ];

    if let Some(trigger_hints) = parts.trigger_hints {
        components.push(format!(
            "intent_tags={}",
            normalized_sorted_strings(&trigger_hints.intent_tags).join(",")
        ));
        components.push(format!(
            "workflow_bias={}",
            normalized_sorted_strings(&trigger_hints.workflow_bias).join(",")
        ));
        components.push(format!(
            "tool_needs={}",
            normalized_sorted_strings(&trigger_hints.tool_needs).join(",")
        ));
    }

    components
}

pub fn build_bootstrap_drawer_id_from_parts(
    wing: &str,
    room: Option<&str>,
    content: &str,
    parts: BootstrapIdentityParts<'_>,
) -> String {
    build_bootstrap_drawer_id(wing, room, content, &bootstrap_identity_components(parts))
}

pub fn source_file_identity_component(source_file: Option<&str>) -> String {
    format!(
        "source_file={}",
        source_file
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("")
    )
}

pub fn build_bootstrap_drawer_id_from_parts_with_source_file(
    wing: &str,
    room: Option<&str>,
    content: &str,
    parts: BootstrapIdentityParts<'_>,
    source_file: Option<&str>,
) -> String {
    let mut components = bootstrap_identity_components(parts);
    components.push(source_file_identity_component(source_file));
    build_bootstrap_drawer_id(wing, room, content, &components)
}

pub fn build_bootstrap_evidence_drawer_id(
    wing: &str,
    room: Option<&str>,
    content: &str,
    source_type: &SourceType,
    source_file: Option<&str>,
) -> String {
    let defaults = anchor::bootstrap_defaults(source_type);
    let memory_kind = MemoryKind::Evidence;
    let domain = MemoryDomain::Project;
    let empty_refs: &[String] = &[];
    build_bootstrap_drawer_id_from_parts_with_source_file(
        wing,
        room,
        content,
        BootstrapIdentityParts {
            memory_kind: &memory_kind,
            domain: &domain,
            field: &defaults.field,
            anchor_kind: &defaults.anchor_kind,
            anchor_id: &defaults.anchor_id,
            parent_anchor_id: defaults.parent_anchor_id.as_deref(),
            provenance: Some(&defaults.provenance),
            statement: None,
            tier: None,
            status: None,
            supporting_refs: empty_refs,
            counterexample_refs: empty_refs,
            teaching_refs: empty_refs,
            verification_refs: empty_refs,
            scope_constraints: None,
            trigger_hints: None,
        },
        source_file,
    )
}

pub fn build_triple_id(subject: &str, predicate: &str, object: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(subject.as_bytes());
    hasher.update([0]);
    hasher.update(predicate.as_bytes());
    hasher.update([0]);
    hasher.update(object.as_bytes());
    let digest = format!("{:x}", hasher.finalize());

    format!(
        "triple_{}_{}_{}",
        sanitize_component_prefix(subject, 8),
        sanitize_component_prefix(predicate, 8),
        &digest[..8]
    )
}

pub fn build_tunnel_id(left: &TunnelEndpoint, right: &TunnelEndpoint) -> String {
    let mut endpoints = [tunnel_endpoint_key(left), tunnel_endpoint_key(right)];
    endpoints.sort();

    let mut hasher = Sha256::new();
    for component in [
        endpoints[0].0.as_str(),
        endpoints[0].1.as_str(),
        endpoints[1].0.as_str(),
        endpoints[1].1.as_str(),
    ] {
        hasher.update([0]);
        hasher.update(component.as_bytes());
    }
    let digest = format!("{:x}", hasher.finalize());
    format!("tunnel_{}", &digest[..16])
}

pub fn format_tunnel_endpoint(endpoint: &TunnelEndpoint) -> String {
    match endpoint.room.as_deref() {
        Some(room) if !room.is_empty() => format!("{}:{room}", endpoint.wing),
        _ => endpoint.wing.clone(),
    }
}

pub fn current_timestamp() -> String {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs().to_string(),
        Err(_) => "0".to_string(),
    }
}

pub fn synthetic_source_file(drawer_id: &str) -> String {
    format!("mempal://drawer/{drawer_id}")
}

pub fn knowledge_source_file(
    domain: &MemoryDomain,
    field: &str,
    tier: &KnowledgeTier,
    statement: &str,
) -> String {
    format!(
        "knowledge://{}/{}/{}/{}",
        enum_slug(memory_domain_as_str(domain)),
        slugify_uri_component(field),
        knowledge_tier_as_str(tier),
        slugify_uri_component(statement)
    )
}

pub fn source_file_or_synthetic(drawer_id: &str, source_file: Option<&str>) -> String {
    source_file
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| synthetic_source_file(drawer_id))
}

pub fn route_room_from_taxonomy(content: &str, wing: &str, taxonomy: &[TaxonomyEntry]) -> String {
    let normalized_content = content.to_lowercase();
    let content_terms = content_terms(&normalized_content);

    taxonomy
        .iter()
        .filter(|entry| entry.wing == wing)
        .filter_map(|entry| {
            let matched_keywords = matched_keywords(&normalized_content, &content_terms, entry);
            (!matched_keywords.is_empty()).then_some((entry, matched_keywords))
        })
        .max_by(|(left_entry, left_matches), (right_entry, right_matches)| {
            left_matches
                .len()
                .cmp(&right_matches.len())
                .then_with(|| {
                    left_matches
                        .iter()
                        .map(String::len)
                        .sum::<usize>()
                        .cmp(&right_matches.iter().map(String::len).sum::<usize>())
                })
                .then_with(|| left_entry.keywords.len().cmp(&right_entry.keywords.len()))
        })
        .map(|(entry, _)| {
            if entry.room.trim().is_empty() {
                DEFAULT_ROOM.to_string()
            } else {
                entry.room.clone()
            }
        })
        .unwrap_or_else(|| DEFAULT_ROOM.to_string())
}

fn tunnel_endpoint_key(endpoint: &TunnelEndpoint) -> (String, String) {
    (
        endpoint.wing.trim().to_string(),
        endpoint
            .room
            .as_deref()
            .map(str::trim)
            .filter(|room| !room.is_empty())
            .unwrap_or("")
            .to_string(),
    )
}

fn sanitize_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn sanitize_component_prefix(value: &str, max_len: usize) -> String {
    let sanitized = sanitize_component(value);
    let prefix: String = sanitized.chars().take(max_len).collect();
    if prefix.is_empty() {
        "x".to_string()
    } else {
        prefix
    }
}

pub fn slugify_uri_component(value: &str) -> String {
    let mut slug = String::new();
    let mut prev_dash = false;

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            slug.push('-');
            prev_dash = true;
        }
    }

    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        "x".to_string()
    } else {
        trimmed.to_string()
    }
}

fn enum_slug(value: &str) -> String {
    value.replace('_', "-")
}

fn memory_kind_as_str(kind: &MemoryKind) -> &'static str {
    match kind {
        MemoryKind::Evidence => "evidence",
        MemoryKind::Knowledge => "knowledge",
    }
}

fn memory_domain_as_str(domain: &MemoryDomain) -> &'static str {
    match domain {
        MemoryDomain::Project => "project",
        MemoryDomain::Agent => "agent",
        MemoryDomain::Skill => "skill",
        MemoryDomain::Global => "global",
    }
}

fn anchor_kind_as_str(kind: &AnchorKind) -> &'static str {
    match kind {
        AnchorKind::Global => "global",
        AnchorKind::Repo => "repo",
        AnchorKind::Worktree => "worktree",
    }
}

fn provenance_as_str(provenance: &Provenance) -> &'static str {
    match provenance {
        Provenance::Runtime => "runtime",
        Provenance::Research => "research",
        Provenance::Human => "human",
    }
}

fn knowledge_tier_as_str(tier: &KnowledgeTier) -> &'static str {
    match tier {
        KnowledgeTier::Qi => "qi",
        KnowledgeTier::Shu => "shu",
        KnowledgeTier::DaoRen => "dao_ren",
        KnowledgeTier::DaoTian => "dao_tian",
    }
}

fn knowledge_status_as_str(status: &KnowledgeStatus) -> &'static str {
    match status {
        KnowledgeStatus::Candidate => "candidate",
        KnowledgeStatus::Promoted => "promoted",
        KnowledgeStatus::Canonical => "canonical",
        KnowledgeStatus::Demoted => "demoted",
        KnowledgeStatus::Retired => "retired",
    }
}

fn normalized_sorted_strings(values: &[String]) -> Vec<String> {
    let mut normalized = values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    normalized.sort();
    normalized
}

fn matched_keywords(
    normalized_content: &str,
    content_terms: &BTreeSet<String>,
    entry: &TaxonomyEntry,
) -> Vec<String> {
    entry
        .keywords
        .iter()
        .map(|keyword| keyword.trim().to_lowercase())
        .filter(|keyword| {
            !keyword.is_empty()
                && (content_terms.contains(keyword)
                    || normalized_content.contains(keyword.as_str()))
        })
        .collect()
}

fn content_terms(content: &str) -> BTreeSet<String> {
    content
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|term| !term.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}
