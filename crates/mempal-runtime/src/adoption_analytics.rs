use std::collections::BTreeMap;

use serde::Serialize;

use crate::core::types::{RuntimeAdoptionEvent, RuntimeAdoptionSignal, RuntimeAdoptionTrack};

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeAdoptionAnalyticsReport {
    pub writes: bool,
    pub total_events: usize,
    pub groups: Vec<RuntimeAdoptionAnalyticsGroup>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeAdoptionAnalyticsGroup {
    pub track: String,
    pub feature: String,
    pub total: usize,
    pub used: usize,
    pub accepted: usize,
    pub rejected: usize,
    pub misses: usize,
    pub rollbacks: usize,
    pub contradictions: usize,
    pub neutral: usize,
    pub recommendation: String,
}

pub fn build_runtime_adoption_analytics(
    events: &[RuntimeAdoptionEvent],
) -> RuntimeAdoptionAnalyticsReport {
    let mut groups = BTreeMap::<(String, String), RuntimeAdoptionAnalyticsGroup>::new();
    for event in events {
        let track = runtime_adoption_track_slug(&event.track).to_string();
        let feature = event.feature.clone();
        let group = groups.entry((track.clone(), feature.clone())).or_insert(
            RuntimeAdoptionAnalyticsGroup {
                track,
                feature,
                total: 0,
                used: 0,
                accepted: 0,
                rejected: 0,
                misses: 0,
                rollbacks: 0,
                contradictions: 0,
                neutral: 0,
                recommendation: String::new(),
            },
        );
        group.total += 1;
        match event.signal {
            RuntimeAdoptionSignal::Used => group.used += 1,
            RuntimeAdoptionSignal::Accepted => group.accepted += 1,
            RuntimeAdoptionSignal::Rejected => group.rejected += 1,
            RuntimeAdoptionSignal::Miss => group.misses += 1,
            RuntimeAdoptionSignal::Rollback => group.rollbacks += 1,
            RuntimeAdoptionSignal::Contradiction => group.contradictions += 1,
            RuntimeAdoptionSignal::Neutral => group.neutral += 1,
        }
    }

    let mut groups = groups.into_values().collect::<Vec<_>>();
    for group in &mut groups {
        group.recommendation = recommendation_for(group).to_string();
    }

    RuntimeAdoptionAnalyticsReport {
        writes: false,
        total_events: events.len(),
        groups,
    }
}

fn recommendation_for(group: &RuntimeAdoptionAnalyticsGroup) -> &'static str {
    if group.rollbacks > 0 || group.contradictions > 0 {
        return "review_before_default_change";
    }
    if group.misses > group.accepted {
        return "investigate_misses";
    }
    if group.rejected > group.accepted {
        return "observe_or_rollback";
    }
    if group.accepted >= 3 && group.rejected <= group.accepted {
        return "candidate_for_workflow_hardening";
    }
    "observe_more_evidence"
}

fn runtime_adoption_track_slug(track: &RuntimeAdoptionTrack) -> &'static str {
    match track {
        RuntimeAdoptionTrack::RuntimeAdoption => "runtime_adoption",
        RuntimeAdoptionTrack::CardContext => "card_context",
        RuntimeAdoptionTrack::CardEmbedding => "card_embedding",
        RuntimeAdoptionTrack::Evaluator => "evaluator",
        RuntimeAdoptionTrack::ResearchAdapter => "research_adapter",
    }
}
