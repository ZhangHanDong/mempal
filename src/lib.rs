#![warn(clippy::all)]

pub mod aaak;
#[cfg(feature = "rest")]
pub mod api;
pub mod context;
pub mod core;
pub mod cowork;
pub mod embed;
pub mod factcheck;
pub mod ingest;
pub mod mcp;
pub mod search;
