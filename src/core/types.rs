use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceType {
    Project,
    Conversation,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Drawer {
    pub id: String,
    pub content: String,
    pub wing: String,
    pub room: Option<String>,
    pub source_file: Option<String>,
    pub source_type: SourceType,
    pub added_at: String,
    pub chunk_index: Option<i64>,
    /// Importance ranking (0-5). Higher = more important for wake-up context.
    #[serde(default)]
    pub importance: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Triple {
    pub id: String,
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub confidence: f64,
    pub source_drawer: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaxonomyEntry {
    pub wing: String,
    pub room: String,
    pub display_name: Option<String>,
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TripleStats {
    pub total: i64,
    pub active: i64,
    pub expired: i64,
    pub entities: i64,
    pub top_predicates: Vec<(String, i64)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteDecision {
    pub wing: Option<String>,
    pub room: Option<String>,
    pub confidence: f32,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchResult {
    pub drawer_id: String,
    pub content: String,
    pub wing: String,
    pub room: Option<String>,
    pub source_file: String,
    pub similarity: f32,
    pub route: RouteDecision,
    /// Other wings that share this result's room (tunnel hints).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tunnel_hints: Vec<String>,
}
