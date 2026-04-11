#![warn(clippy::all)]

use std::path::PathBuf;

use thiserror::Error;

pub mod api;
pub mod factory;
#[cfg(feature = "model2vec")]
pub mod model2vec;
#[cfg(feature = "onnx")]
pub mod onnx;

pub use factory::{ConfiguredEmbedderFactory, EmbedderFactory};

pub type Result<T> = std::result::Result<T, EmbedError>;

#[derive(Debug, Error)]
pub enum EmbedError {
    #[error("failed to create model directory {path}")]
    CreateModelDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to check whether {path} exists")]
    CheckPathExists {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to download {url}")]
    Download {
        url: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("download returned error status for {url}")]
    DownloadStatus {
        url: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("failed to read download body from {url}")]
    ReadDownloadBody {
        url: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("failed to write {path}")]
    WriteFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to rename {from} to {to}")]
    RenameFile {
        from: PathBuf,
        to: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to initialize ONNX session builder: {0}")]
    SessionBuilder(String),
    #[error("failed to load ONNX model from {path}: {message}")]
    LoadModel { path: PathBuf, message: String },
    #[error("tokenizer error: {0}")]
    Tokenizer(String),
    #[error("embedding runtime error: {0}")]
    Runtime(String),
    #[error("embedding worker panicked")]
    WorkerPanic(#[source] tokio::task::JoinError),
    #[error("failed to call embedding endpoint {endpoint}")]
    HttpRequest {
        endpoint: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("embedding endpoint returned error status {endpoint}")]
    HttpStatus {
        endpoint: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("failed to decode embedding response from {endpoint}")]
    DecodeResponse {
        endpoint: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("invalid embedding response: {0}")]
    InvalidResponse(String),
    #[error("embedding endpoint returned no vectors")]
    EmptyVectors,
    #[error(
        "embedding endpoint returned vectors with unexpected dimensions; expected {expected}, got {actual}"
    )]
    InvalidDimensions { expected: usize, actual: usize },
    #[error("unsupported embed backend: {0}")]
    UnsupportedBackend(String),
}

#[async_trait::async_trait]
pub trait Embedder: Send + Sync {
    async fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
    fn dimensions(&self) -> usize;
    fn name(&self) -> &str;
}
