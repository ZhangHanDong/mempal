use crate::core::config::Config;
use async_trait::async_trait;

use super::{EmbedError, Embedder, Result, api::ApiEmbedder};

/// Default embedding dimensions (used by API backend when no model is loaded).
pub const DEFAULT_API_DIMENSIONS: usize = 384;

#[async_trait]
pub trait EmbedderFactory: Send + Sync {
    async fn build(&self) -> Result<Box<dyn Embedder>>;
}

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
                Ok(Box::new(super::model2vec::Model2VecEmbedder::new(
                    model_id,
                )?))
            }
            #[cfg(feature = "onnx")]
            "onnx" => Ok(Box::new(
                super::onnx::OnnxEmbedder::new_or_download().await?,
            )),
            "api" => Ok(Box::new(ApiEmbedder::new(
                self.config
                    .embed
                    .api_endpoint
                    .clone()
                    .unwrap_or_else(|| "http://localhost:11434/api/embeddings".to_string()),
                self.config.embed.api_model.clone(),
                DEFAULT_API_DIMENSIONS,
            ))),
            backend => Err(EmbedError::UnsupportedBackend(backend.to_string())),
        }
    }
}
