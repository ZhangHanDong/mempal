#![warn(clippy::all)]

pub mod aaak;
pub mod adoption_analytics;
#[cfg(feature = "rest")]
pub mod api;
pub mod brief;
pub mod context;
pub mod core;
pub mod cowork;
pub mod doctor;
pub mod embed;
pub mod factcheck;
pub mod field_taxonomy;
pub mod ingest;
pub mod knowledge_anchor;
pub mod knowledge_card_backfill;
pub mod knowledge_card_lifecycle;
pub mod knowledge_card_retrieval;
pub mod knowledge_distill;
pub mod knowledge_gate;
pub mod knowledge_lifecycle;
pub mod mcp;
pub mod projects;
pub mod search;
