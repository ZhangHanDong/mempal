use serde::Serialize;

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
