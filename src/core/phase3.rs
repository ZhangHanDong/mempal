use serde::Serialize;
use serde_json::{Map, Value};

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
                feature_examples: vec!["research_validate_plan".to_string()],
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

fn push_command_arg(command: &mut Vec<String>, name: &str, value: &str) {
    command.push(name.to_string());
    command.push(value.to_string());
}

fn insert_payload_string(payload: &mut Map<String, Value>, key: &str, value: Option<String>) {
    if let Some(value) = value {
        payload.insert(key.to_string(), Value::String(value));
    }
}
