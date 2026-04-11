#![warn(clippy::all)]

pub mod aaak;
#[cfg(feature = "rest")]
pub mod api;
pub mod core;
pub mod embed;
pub mod ingest;
pub mod mcp;
pub mod search;
