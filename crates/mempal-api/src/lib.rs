#![warn(clippy::all)]

#[cfg(feature = "rest")]
mod handlers;
#[cfg(feature = "rest")]
mod state;

#[cfg(feature = "rest")]
pub use handlers::{DEFAULT_REST_ADDR, router, serve};
#[cfg(feature = "rest")]
pub use state::{ApiState, ConfiguredEmbedderFactory, EmbedderFactory};
