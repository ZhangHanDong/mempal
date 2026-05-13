use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{Map, Value};

use super::{
    anchor,
    types::{
        AnchorKind, BootstrapIdentityParts, MemoryDomain, MemoryKind, Provenance,
        RuntimeAdoptionEvent, RuntimeAdoptionSignal,
    },
    utils::build_bootstrap_drawer_id_from_parts,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeAdoptionGuidance {
    pub version: u32,
    pub recording_rule: String,
    pub required_fields: Vec<String>,
    pub optional_fields: Vec<String>,
    pub signals: Vec<RuntimeAdoptionSignalGuidance>,
    pub tracks: Vec<RuntimeAdoptionTrackGuidance>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeAdoptionSignalGuidance {
    pub signal: String,
    pub when: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeAdoptionTrackGuidance {
    pub track: String,
    pub when: String,
    pub feature_examples: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RuntimeAdoptionRecordPlan {
    pub writes: bool,
    pub record_command: Vec<String>,
    pub record_payload: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeAdoptionRecordQualityReport {
    pub writes: bool,
    pub valid: bool,
    pub quality: String,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RuntimeAdoptionCheckedRecordReport {
    pub writes: bool,
    pub blocked: bool,
    pub record_quality: RuntimeAdoptionRecordQualityReport,
    pub event: Option<RuntimeAdoptionEvent>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RuntimeAdoptionCaptureReport {
    pub writes: bool,
    pub execute: bool,
    pub surface: String,
    pub outcome: String,
    pub record_plan: RuntimeAdoptionRecordPlan,
    pub record_quality: RuntimeAdoptionRecordQualityReport,
    pub record_checked: Option<RuntimeAdoptionCheckedRecordReport>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvaluatorAdviceInput {
    pub evaluator_id: String,
    pub subject_kind: String,
    pub subject_id: String,
    pub proposed_action: String,
    pub evidence_refs: Vec<String>,
    pub counterexample_refs: Vec<String>,
    pub risk_notes: Vec<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EvaluatorAdviceReport {
    pub writes: bool,
    pub evaluator_id: String,
    pub subject_kind: String,
    pub subject_id: String,
    pub proposed_action: String,
    pub recommendation: String,
    pub lifecycle_authority: bool,
    pub deterministic_gate_required: bool,
    pub requires_human_review: bool,
    pub evidence_refs: Vec<String>,
    pub counterexample_refs: Vec<String>,
    pub risk_notes: Vec<String>,
    pub reasons: Vec<String>,
    pub adoption_capture: RuntimeAdoptionRecordPlan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeAdoptionReviewFilters {
    pub track: Option<String>,
    pub feature: Option<String>,
    pub signal: Option<String>,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct RuntimeAdoptionSignalCounts {
    pub total: usize,
    pub used: usize,
    pub accepted: usize,
    pub rejected: usize,
    pub misses: usize,
    pub rollbacks: usize,
    pub contradictions: usize,
    pub neutral: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeAdoptionFeatureReview {
    pub feature: String,
    pub stats: RuntimeAdoptionSignalCounts,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeAdoptionReviewReport {
    pub writes: bool,
    pub filters: RuntimeAdoptionReviewFilters,
    pub total: usize,
    pub stats: RuntimeAdoptionSignalCounts,
    pub features: Vec<RuntimeAdoptionFeatureReview>,
    pub conclusion: String,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Phase3ReadinessReport {
    pub writes: bool,
    pub candidate: String,
    pub ready: bool,
    pub decision: String,
    pub required_track: String,
    pub required_feature: String,
    pub review: RuntimeAdoptionReviewReport,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResearchEvidenceDrawerPlan {
    pub drawer_id: String,
    pub finding_index: usize,
    pub source_file: String,
    pub created: bool,
    pub skipped: bool,
    #[serde(skip)]
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResearchCandidateInsightPlan {
    pub statement: String,
    pub supporting_refs: Vec<String>,
    pub suggested_command: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResearchIngestPlanReport {
    pub valid: bool,
    pub writes: bool,
    pub report_id: String,
    pub title: String,
    pub source_count: usize,
    pub finding_count: usize,
    pub candidate_insight_count: usize,
    pub planned_evidence_count: usize,
    pub created_count: usize,
    pub skipped_count: usize,
    pub errors: Vec<String>,
    pub evidence_drawers: Vec<ResearchEvidenceDrawerPlan>,
    pub candidate_insights: Vec<ResearchCandidateInsightPlan>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeAdoptionRecordPlanInput {
    pub id: Option<String>,
    pub track: String,
    pub signal: String,
    pub feature: String,
    pub query: Option<String>,
    pub context_hash: Option<String>,
    pub card_id: Option<String>,
    pub evaluator_id: Option<String>,
    pub research_report_id: Option<String>,
    pub note: Option<String>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeAdoptionCaptureInput {
    pub id: Option<String>,
    pub surface: String,
    pub outcome: String,
    pub query: Option<String>,
    pub context_hash: Option<String>,
    pub card_id: Option<String>,
    pub evaluator_id: Option<String>,
    pub research_report_id: Option<String>,
    pub note: Option<String>,
    pub metadata: Option<Value>,
}

pub fn runtime_adoption_guidance() -> RuntimeAdoptionGuidance {
    RuntimeAdoptionGuidance {
        version: 1,
        recording_rule: "record only concrete runtime outcomes, not speculation".to_string(),
        required_fields: vec![
            "track".to_string(),
            "signal".to_string(),
            "feature".to_string(),
        ],
        optional_fields: vec![
            "query".to_string(),
            "context_hash".to_string(),
            "card_id".to_string(),
            "evaluator_id".to_string(),
            "research_report_id".to_string(),
            "note".to_string(),
            "metadata".to_string(),
        ],
        signals: vec![
            RuntimeAdoptionSignalGuidance {
                signal: "used".to_string(),
                when: "record when guidance was actually consumed during a task".to_string(),
            },
            RuntimeAdoptionSignalGuidance {
                signal: "accepted".to_string(),
                when: "record when the consumed guidance materially helped the outcome".to_string(),
            },
            RuntimeAdoptionSignalGuidance {
                signal: "rejected".to_string(),
                when: "record when guidance was considered and intentionally not followed"
                    .to_string(),
            },
            RuntimeAdoptionSignalGuidance {
                signal: "miss".to_string(),
                when: "record when useful guidance should have appeared but did not".to_string(),
            },
            RuntimeAdoptionSignalGuidance {
                signal: "rollback".to_string(),
                when: "record when behavior was reverted because guidance degraded the outcome"
                    .to_string(),
            },
            RuntimeAdoptionSignalGuidance {
                signal: "contradiction".to_string(),
                when: "record when guidance conflicted with stronger evidence or instructions"
                    .to_string(),
            },
            RuntimeAdoptionSignalGuidance {
                signal: "neutral".to_string(),
                when: "record when guidance was consumed but had no clear outcome impact"
                    .to_string(),
            },
        ],
        tracks: vec![
            RuntimeAdoptionTrackGuidance {
                track: "runtime_adoption".to_string(),
                when: "general agent-runtime behavior evidence".to_string(),
                feature_examples: vec!["context_pack".to_string(), "skill_selection".to_string()],
            },
            RuntimeAdoptionTrackGuidance {
                track: "card_context".to_string(),
                when: "card-aware context affected or should have affected behavior".to_string(),
                feature_examples: vec!["include_cards".to_string()],
            },
            RuntimeAdoptionTrackGuidance {
                track: "card_embedding".to_string(),
                when: "linked-evidence card retrieval missed statement-level matches".to_string(),
                feature_examples: vec!["card_statement_recall".to_string()],
            },
            RuntimeAdoptionTrackGuidance {
                track: "evaluator".to_string(),
                when: "evaluator advice affected or should have affected a lifecycle decision"
                    .to_string(),
                feature_examples: vec!["advisory_gate".to_string()],
            },
            RuntimeAdoptionTrackGuidance {
                track: "research_adapter".to_string(),
                when: "external research report validation or ingestion planning affected behavior"
                    .to_string(),
                feature_examples: vec![
                    "research_validate_plan".to_string(),
                    "research_ingest_plan".to_string(),
                ],
            },
        ],
    }
}

pub fn prepare_runtime_adoption_record(
    input: RuntimeAdoptionRecordPlanInput,
) -> RuntimeAdoptionRecordPlan {
    let mut command = vec![
        "mempal".to_string(),
        "phase3".to_string(),
        "adoption".to_string(),
        "record".to_string(),
    ];
    push_command_arg(&mut command, "--track", &input.track);
    push_command_arg(&mut command, "--signal", &input.signal);
    push_command_arg(&mut command, "--feature", &input.feature);
    if let Some(value) = input.query.as_deref() {
        push_command_arg(&mut command, "--query", value);
    }
    if let Some(value) = input.context_hash.as_deref() {
        push_command_arg(&mut command, "--context-hash", value);
    }
    if let Some(value) = input.card_id.as_deref() {
        push_command_arg(&mut command, "--card-id", value);
    }
    if let Some(value) = input.evaluator_id.as_deref() {
        push_command_arg(&mut command, "--evaluator-id", value);
    }
    if let Some(value) = input.research_report_id.as_deref() {
        push_command_arg(&mut command, "--research-report-id", value);
    }
    if let Some(value) = input.note.as_deref() {
        push_command_arg(&mut command, "--note", value);
    }
    if let Some(value) = input.id.as_deref() {
        push_command_arg(&mut command, "--id", value);
    }
    if let Some(metadata) = input.metadata.as_ref() {
        push_command_arg(&mut command, "--metadata-json", &metadata.to_string());
    }

    let mut payload = Map::new();
    payload.insert("action".to_string(), Value::String("record".to_string()));
    insert_payload_string(&mut payload, "id", input.id);
    insert_payload_string(&mut payload, "track", Some(input.track));
    insert_payload_string(&mut payload, "signal", Some(input.signal));
    insert_payload_string(&mut payload, "feature", Some(input.feature));
    insert_payload_string(&mut payload, "query", input.query);
    insert_payload_string(&mut payload, "context_hash", input.context_hash);
    insert_payload_string(&mut payload, "card_id", input.card_id);
    insert_payload_string(&mut payload, "evaluator_id", input.evaluator_id);
    insert_payload_string(&mut payload, "research_report_id", input.research_report_id);
    insert_payload_string(&mut payload, "note", input.note);
    if let Some(metadata) = input.metadata {
        payload.insert("metadata".to_string(), metadata);
    }

    RuntimeAdoptionRecordPlan {
        writes: false,
        record_command: command,
        record_payload: Value::Object(payload),
    }
}

pub fn capture_runtime_adoption_record_input(
    input: RuntimeAdoptionCaptureInput,
) -> Result<RuntimeAdoptionRecordPlanInput, String> {
    let (track, feature) = capture_surface_mapping(&input.surface)?;
    let signal = capture_outcome_signal(&input.outcome)?;
    Ok(RuntimeAdoptionRecordPlanInput {
        id: input.id,
        track,
        signal,
        feature,
        query: input.query,
        context_hash: input.context_hash,
        card_id: input.card_id,
        evaluator_id: input.evaluator_id,
        research_report_id: input.research_report_id,
        note: input.note,
        metadata: input.metadata,
    })
}

pub fn prepare_runtime_adoption_capture(
    surface: String,
    outcome: String,
    execute: bool,
    record_input: RuntimeAdoptionRecordPlanInput,
) -> RuntimeAdoptionCaptureReport {
    let record_plan = prepare_runtime_adoption_record(record_input.clone());
    let record_quality = check_runtime_adoption_record(&record_input);
    RuntimeAdoptionCaptureReport {
        writes: false,
        execute,
        surface,
        outcome,
        record_plan,
        record_quality,
        record_checked: None,
    }
}

pub fn evaluator_advice(input: EvaluatorAdviceInput) -> Result<EvaluatorAdviceReport, String> {
    if is_blank(&input.evaluator_id) {
        return Err("evaluator-id is required".to_string());
    }
    if is_blank(&input.subject_kind) {
        return Err("subject-kind is required".to_string());
    }
    if is_blank(&input.subject_id) {
        return Err("subject-id is required".to_string());
    }
    if is_blank(&input.proposed_action) {
        return Err("proposed-action is required".to_string());
    }

    let evidence_refs = normalize_non_empty(input.evidence_refs);
    let counterexample_refs = normalize_non_empty(input.counterexample_refs);
    let risk_notes = normalize_non_empty(input.risk_notes);
    let subject_kind = input.subject_kind.trim().to_string();
    let proposed_action = input.proposed_action.trim().to_string();
    let requires_human_review = subject_kind == "dao_tian" && proposed_action.contains("canonical");

    let mut reasons = Vec::new();
    reasons.push("evaluator output is advisory-only and has no lifecycle authority".to_string());
    reasons
        .push("deterministic promotion gates remain required before lifecycle changes".to_string());
    if requires_human_review {
        reasons.push("dao_tian canonicalization requires human review".to_string());
    }

    let recommendation = if !counterexample_refs.is_empty() || !risk_notes.is_empty() {
        reasons
            .push("counterexample refs or risk notes block supportive recommendation".to_string());
        "do_not_promote"
    } else if evidence_refs.is_empty() {
        reasons.push("supporting evidence refs are required before promotion advice".to_string());
        "needs_evidence"
    } else if requires_human_review {
        "requires_human_review"
    } else {
        reasons.push(
            "supporting evidence refs are present and no risk blockers were supplied".to_string(),
        );
        "advisory_support"
    }
    .to_string();

    let adoption_capture = prepare_runtime_adoption_record(RuntimeAdoptionRecordPlanInput {
        id: None,
        track: "evaluator".to_string(),
        signal: "used".to_string(),
        feature: "advisory_gate".to_string(),
        query: Some(format!(
            "{subject_kind}:{} {proposed_action}",
            input.subject_id.trim()
        )),
        context_hash: None,
        card_id: None,
        evaluator_id: Some(input.evaluator_id.trim().to_string()),
        research_report_id: None,
        note: input.note,
        metadata: Some(evaluator_advice_metadata(
            &subject_kind,
            input.subject_id.trim(),
            &proposed_action,
            &recommendation,
        )),
    });

    Ok(EvaluatorAdviceReport {
        writes: false,
        evaluator_id: input.evaluator_id.trim().to_string(),
        subject_kind,
        subject_id: input.subject_id.trim().to_string(),
        proposed_action,
        recommendation,
        lifecycle_authority: false,
        deterministic_gate_required: true,
        requires_human_review,
        evidence_refs,
        counterexample_refs,
        risk_notes,
        reasons,
        adoption_capture,
    })
}

pub fn check_runtime_adoption_record(
    input: &RuntimeAdoptionRecordPlanInput,
) -> RuntimeAdoptionRecordQualityReport {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    if is_blank(&input.feature) {
        errors.push("feature must not be empty".to_string());
    }

    if requires_outcome_context(&input.signal)
        && input.query.as_deref().is_none_or(is_blank)
        && input.note.as_deref().is_none_or(is_blank)
    {
        warnings.push(
            "missing concrete outcome context: provide note or query for this signal".to_string(),
        );
    }

    match input.track.as_str() {
        "card_context" | "card_embedding" if input.card_id.as_deref().is_none_or(is_blank) => {
            warnings.push("missing card_id for card-related adoption evidence".to_string());
        }
        "evaluator" if input.evaluator_id.as_deref().is_none_or(is_blank) => {
            warnings.push("missing evaluator_id for evaluator adoption evidence".to_string());
        }
        "research_adapter" if input.research_report_id.as_deref().is_none_or(is_blank) => {
            warnings.push(
                "missing research_report_id for research adapter adoption evidence".to_string(),
            );
        }
        _ => {}
    }

    let quality = if !errors.is_empty() {
        "invalid"
    } else if !warnings.is_empty() {
        "warning"
    } else {
        "ready"
    }
    .to_string();

    RuntimeAdoptionRecordQualityReport {
        writes: false,
        valid: errors.is_empty(),
        quality,
        errors,
        warnings,
    }
}

fn normalize_non_empty(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

fn evaluator_advice_metadata(
    subject_kind: &str,
    subject_id: &str,
    proposed_action: &str,
    recommendation: &str,
) -> Value {
    let mut metadata = Map::new();
    metadata.insert(
        "subject_kind".to_string(),
        Value::String(subject_kind.to_string()),
    );
    metadata.insert(
        "subject_id".to_string(),
        Value::String(subject_id.to_string()),
    );
    metadata.insert(
        "proposed_action".to_string(),
        Value::String(proposed_action.to_string()),
    );
    metadata.insert(
        "recommendation".to_string(),
        Value::String(recommendation.to_string()),
    );
    Value::Object(metadata)
}

fn capture_surface_mapping(surface: &str) -> Result<(String, String), String> {
    match surface.trim() {
        "card-context" | "card_context" => {
            Ok(("card_context".to_string(), "include_cards".to_string()))
        }
        "card-embedding" | "card_embedding" => Ok((
            "card_embedding".to_string(),
            "card_statement_recall".to_string(),
        )),
        "research-adapter" | "research_adapter" => Ok((
            "research_adapter".to_string(),
            "research_ingest_plan".to_string(),
        )),
        "evaluator" => Ok(("evaluator".to_string(), "advisory_gate".to_string())),
        "runtime-context" | "runtime_context" => {
            Ok(("runtime_adoption".to_string(), "context_pack".to_string()))
        }
        "skill-selection" | "skill_selection" => Ok((
            "runtime_adoption".to_string(),
            "skill_selection".to_string(),
        )),
        other => Err(format!("unsupported adoption capture surface: {other}")),
    }
}

fn capture_outcome_signal(outcome: &str) -> Result<String, String> {
    match outcome.trim() {
        "used" | "accepted" | "rejected" | "miss" | "rollback" | "contradiction" | "neutral" => {
            Ok(outcome.trim().to_string())
        }
        other => Err(format!("unsupported adoption capture outcome: {other}")),
    }
}

pub fn should_write_checked_record(
    quality: &RuntimeAdoptionRecordQualityReport,
    allow_warnings: bool,
) -> bool {
    quality.valid
        && (quality.quality == "ready" || (allow_warnings && quality.quality == "warning"))
}

pub fn review_runtime_adoption_events(
    events: &[RuntimeAdoptionEvent],
    filters: RuntimeAdoptionReviewFilters,
) -> RuntimeAdoptionReviewReport {
    let filtered_events = events
        .iter()
        .filter(|event| {
            filters
                .signal
                .as_deref()
                .is_none_or(|signal| runtime_adoption_signal_slug(&event.signal) == signal)
        })
        .collect::<Vec<_>>();

    let stats = RuntimeAdoptionSignalCounts::from_events(filtered_events.iter().copied());
    let mut by_feature = BTreeMap::<String, Vec<&RuntimeAdoptionEvent>>::new();
    for event in &filtered_events {
        by_feature
            .entry(event.feature.clone())
            .or_default()
            .push(event);
    }
    let features = by_feature
        .into_iter()
        .map(|(feature, events)| RuntimeAdoptionFeatureReview {
            feature,
            stats: RuntimeAdoptionSignalCounts::from_events(events),
        })
        .collect::<Vec<_>>();

    let (conclusion, reasons) = review_conclusion(&stats);

    RuntimeAdoptionReviewReport {
        writes: false,
        filters,
        total: stats.total,
        stats,
        features,
        conclusion,
        reasons,
    }
}

impl RuntimeAdoptionSignalCounts {
    fn from_events<'a>(events: impl IntoIterator<Item = &'a RuntimeAdoptionEvent>) -> Self {
        let mut stats = Self::default();
        for event in events {
            stats.total += 1;
            match event.signal {
                RuntimeAdoptionSignal::Used => stats.used += 1,
                RuntimeAdoptionSignal::Accepted => stats.accepted += 1,
                RuntimeAdoptionSignal::Rejected => stats.rejected += 1,
                RuntimeAdoptionSignal::Miss => stats.misses += 1,
                RuntimeAdoptionSignal::Rollback => stats.rollbacks += 1,
                RuntimeAdoptionSignal::Contradiction => stats.contradictions += 1,
                RuntimeAdoptionSignal::Neutral => stats.neutral += 1,
            }
        }
        stats
    }
}

fn review_conclusion(stats: &RuntimeAdoptionSignalCounts) -> (String, Vec<String>) {
    if stats.total == 0 {
        return (
            "no_evidence".to_string(),
            vec!["no runtime adoption events match the requested filters".to_string()],
        );
    }
    if stats.rollbacks > 0 {
        return (
            "rollback_risk".to_string(),
            vec!["rollback evidence exists for this adoption surface".to_string()],
        );
    }
    if stats.accepted > 0 && stats.accepted >= stats.rejected + stats.misses + stats.contradictions
    {
        return (
            "positive".to_string(),
            vec!["accepted evidence is at least as strong as negative evidence".to_string()],
        );
    }
    (
        "mixed".to_string(),
        vec!["evidence is present but not clearly positive".to_string()],
    )
}

pub fn card_context_default_readiness(events: &[RuntimeAdoptionEvent]) -> Phase3ReadinessReport {
    let review = review_runtime_adoption_events(
        events,
        RuntimeAdoptionReviewFilters {
            track: Some("card_context".to_string()),
            feature: Some("include_cards".to_string()),
            signal: None,
            limit: 10_000,
        },
    );
    let stats = &review.stats;
    let mut reasons = Vec::new();
    if stats.accepted < 3 {
        reasons.push("insufficient accepted evidence for include_cards default".to_string());
    }
    if stats.rollbacks > 0 {
        reasons.push("rollback evidence blocks card context default readiness".to_string());
    }
    if stats.contradictions > 0 {
        reasons.push("contradiction evidence requires review before defaulting".to_string());
    }
    if stats.accepted < stats.rejected + stats.misses {
        reasons.push("negative evidence outweighs accepted evidence".to_string());
    }

    let ready = reasons.is_empty();
    if ready {
        reasons.push("card context default readiness threshold satisfied".to_string());
    }

    Phase3ReadinessReport {
        writes: false,
        candidate: "card-context-default".to_string(),
        ready,
        decision: if ready {
            "eligible_for_future_default_spec"
        } else {
            "keep_opt_in"
        }
        .to_string(),
        required_track: "card_context".to_string(),
        required_feature: "include_cards".to_string(),
        review,
        reasons,
    }
}

pub fn build_research_ingest_plan_from_value(value: &Value) -> ResearchIngestPlanReport {
    let mut errors = Vec::new();
    let report_id = required_json_string(value, "report_id", &mut errors);
    let title = required_json_string(value, "title", &mut errors);
    let source_count = value
        .get("sources")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    if source_count == 0 {
        errors.push("sources must contain at least one item".to_string());
    }
    let findings = value
        .get("findings")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if findings.is_empty() {
        errors.push("findings must contain at least one item".to_string());
    }
    let candidate_values = value
        .get("candidate_insights")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let evidence_drawers = findings
        .iter()
        .enumerate()
        .map(|(index, finding)| {
            let content = research_evidence_content_from_value(
                report_id.as_str(),
                title.as_str(),
                value.get("sources"),
                finding,
                index,
            );
            let drawer_id = research_evidence_drawer_id(&content);
            ResearchEvidenceDrawerPlan {
                drawer_id,
                finding_index: index,
                source_file: format!("research://{}#finding-{index}", report_id),
                created: false,
                skipped: false,
                content,
            }
        })
        .collect::<Vec<_>>();
    let supporting_refs = evidence_drawers
        .iter()
        .map(|plan| plan.drawer_id.clone())
        .collect::<Vec<_>>();
    let candidate_insights = candidate_values
        .iter()
        .filter_map(|candidate| candidate_statement(candidate).map(str::to_string))
        .map(|statement| ResearchCandidateInsightPlan {
            suggested_command: research_distill_command(&statement, &supporting_refs),
            statement,
            supporting_refs: supporting_refs.clone(),
        })
        .collect::<Vec<_>>();

    ResearchIngestPlanReport {
        valid: errors.is_empty(),
        writes: false,
        report_id,
        title,
        source_count,
        finding_count: findings.len(),
        candidate_insight_count: candidate_values.len(),
        planned_evidence_count: evidence_drawers.len(),
        created_count: 0,
        skipped_count: 0,
        errors,
        evidence_drawers,
        candidate_insights,
    }
}

fn research_evidence_content_from_value(
    report_id: &str,
    title: &str,
    sources: Option<&Value>,
    finding: &Value,
    finding_index: usize,
) -> String {
    let summary = finding_summary(finding);
    let sources = sources
        .map(|value| serde_json::to_string_pretty(value).unwrap_or_else(|_| "[]".to_string()))
        .unwrap_or_else(|| "[]".to_string());
    let raw_finding = serde_json::to_string_pretty(finding).unwrap_or_else(|_| finding.to_string());
    format!(
        "# Research finding: {title}\n\nreport_id: {report_id}\nfinding_index: {finding_index}\n\nsummary: {summary}\n\nsources:\n```json\n{sources}\n```\n\nraw_finding:\n```json\n{raw_finding}\n```\n"
    )
}

fn finding_summary(finding: &Value) -> String {
    if let Some(raw) = finding.as_str() {
        return raw.trim().to_string();
    }
    for field in ["summary", "statement", "content", "text"] {
        if let Some(raw) = finding.get(field).and_then(Value::as_str)
            && !raw.trim().is_empty()
        {
            return raw.trim().to_string();
        }
    }
    serde_json::to_string(finding).unwrap_or_else(|_| finding.to_string())
}

fn candidate_statement(candidate: &Value) -> Option<&str> {
    if let Some(raw) = candidate.as_str()
        && !raw.trim().is_empty()
    {
        return Some(raw.trim());
    }
    for field in ["statement", "summary", "content", "text"] {
        if let Some(raw) = candidate.get(field).and_then(Value::as_str)
            && !raw.trim().is_empty()
        {
            return Some(raw.trim());
        }
    }
    None
}

fn research_evidence_drawer_id(content: &str) -> String {
    let memory_kind = MemoryKind::Evidence;
    let domain = MemoryDomain::Project;
    let anchor_kind = AnchorKind::Repo;
    let provenance = Provenance::Research;
    let empty_refs: &[String] = &[];
    build_bootstrap_drawer_id_from_parts(
        "mempal",
        Some("research"),
        content,
        BootstrapIdentityParts {
            memory_kind: &memory_kind,
            domain: &domain,
            field: "research",
            anchor_kind: &anchor_kind,
            anchor_id: anchor::LEGACY_REPO_ANCHOR_ID,
            parent_anchor_id: None,
            provenance: Some(&provenance),
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
    )
}

fn research_distill_command(statement: &str, supporting_refs: &[String]) -> Vec<String> {
    let mut command = vec![
        "mempal".to_string(),
        "knowledge".to_string(),
        "distill".to_string(),
        "--tier".to_string(),
        "qi".to_string(),
        "--statement".to_string(),
        statement.to_string(),
        "--content".to_string(),
        statement.to_string(),
        "--field".to_string(),
        "research".to_string(),
    ];
    for supporting_ref in supporting_refs {
        push_command_arg(&mut command, "--supporting-ref", supporting_ref);
    }
    command
}

fn required_json_string(value: &Value, field: &'static str, errors: &mut Vec<String>) -> String {
    match value.get(field).and_then(Value::as_str) {
        Some(raw) if !raw.trim().is_empty() => raw.trim().to_string(),
        _ => {
            errors.push(format!("{field} is required"));
            String::new()
        }
    }
}

fn requires_outcome_context(signal: &str) -> bool {
    matches!(
        signal,
        "accepted" | "rejected" | "miss" | "rollback" | "contradiction"
    )
}

fn is_blank(value: &str) -> bool {
    value.trim().is_empty()
}

fn runtime_adoption_signal_slug(signal: &RuntimeAdoptionSignal) -> &'static str {
    match signal {
        RuntimeAdoptionSignal::Used => "used",
        RuntimeAdoptionSignal::Accepted => "accepted",
        RuntimeAdoptionSignal::Rejected => "rejected",
        RuntimeAdoptionSignal::Miss => "miss",
        RuntimeAdoptionSignal::Rollback => "rollback",
        RuntimeAdoptionSignal::Contradiction => "contradiction",
        RuntimeAdoptionSignal::Neutral => "neutral",
    }
}

fn push_command_arg(command: &mut Vec<String>, name: &str, value: &str) {
    command.push(name.to_string());
    command.push(value.to_string());
}

fn insert_payload_string(payload: &mut Map<String, Value>, key: &str, value: Option<String>) {
    if let Some(value) = value {
        payload.insert(key.to_string(), Value::String(value));
    }
}
