//! Model2Vec embedding backend — static distilled models, zero native deps.
//!
//! Default model: `minishlab/potion-multilingual-128M` (BGE-M3 distilled, 1024-dim).
//! Falls back to `minishlab/potion-base-8M` (256-dim) if the multilingual model
//! is not available.

use super::{EmbedError, Embedder, Result};

const DEFAULT_MODEL: &str = "minishlab/potion-multilingual-128M";

pub struct Model2VecEmbedder {
    model: model2vec_rs::model::StaticModel,
    dimensions: usize,
    model_name: String,
}

impl Model2VecEmbedder {
    pub fn new(model_id: &str) -> Result<Self> {
        let model = model2vec_rs::model::StaticModel::from_pretrained(model_id, None, None, None)
            .map_err(|e| {
            EmbedError::Runtime(format!("failed to load model2vec model '{model_id}': {e}"))
        })?;
        // Probe embedding dimensions by encoding a test string
        let probe = model.encode_single("dim_probe");
        let dimensions = probe.len();
        Ok(Self {
            model,
            dimensions,
            model_name: model_id.to_string(),
        })
    }

    pub fn new_default() -> Result<Self> {
        Self::new(DEFAULT_MODEL)
    }
}

#[async_trait::async_trait]
impl Embedder for Model2VecEmbedder {
    async fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let sentences: Vec<String> = texts.iter().map(|t| t.to_string()).collect();
        let embeddings = self.model.encode(&sentences);
        Ok(embeddings)
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn name(&self) -> &str {
        &self.model_name
    }
}
