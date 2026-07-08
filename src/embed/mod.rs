#![warn(clippy::all)]

use async_trait::async_trait;

use crate::core::config::Config;

#[cfg(feature = "model2vec")]
pub use mempal_embed::model2vec;
#[cfg(feature = "onnx")]
pub use mempal_embed::onnx;
pub use mempal_embed::{EmbedError, Embedder, EmbedderFactory, Result, api};

/// Default embedding dimensions used by the API backend when config omits one.
pub const DEFAULT_API_DIMENSIONS: usize = 384;

#[derive(Clone)]
pub struct ConfiguredEmbedderFactory {
    config: Config,
}

impl ConfiguredEmbedderFactory {
    pub fn new(config: Config) -> Self {
        Self { config }
    }
}

#[async_trait]
impl EmbedderFactory for ConfiguredEmbedderFactory {
    async fn build(&self) -> Result<Box<dyn Embedder>> {
        match self.config.embed.backend.as_str() {
            #[cfg(feature = "model2vec")]
            "model2vec" => {
                let model_id = self
                    .config
                    .embed
                    .model
                    .as_deref()
                    .unwrap_or("minishlab/potion-multilingual-128M");
                Ok(Box::new(model2vec::Model2VecEmbedder::new(model_id)?))
            }
            #[cfg(not(feature = "model2vec"))]
            "model2vec" => Err(EmbedError::UnsupportedBackend("model2vec".to_string())),
            #[cfg(feature = "onnx")]
            "onnx" => Ok(Box::new(onnx::OnnxEmbedder::new_or_download().await?)),
            #[cfg(not(feature = "onnx"))]
            "onnx" => Err(EmbedError::UnsupportedBackend("onnx".to_string())),
            "api" => Ok(Box::new(api::ApiEmbedder::new(
                self.config
                    .embed
                    .api_endpoint
                    .clone()
                    .unwrap_or_else(|| "http://localhost:11434/api/embeddings".to_string()),
                self.config.embed.api_model.clone(),
                self.config
                    .embed
                    .dimensions
                    .unwrap_or(DEFAULT_API_DIMENSIONS),
            ))),
            other => Err(EmbedError::UnsupportedBackend(other.to_string())),
        }
    }
}
