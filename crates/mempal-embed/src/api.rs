use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{EmbedError, Embedder, Result};

#[derive(Debug, Clone)]
pub struct ApiEmbedder {
    endpoint: String,
    model: Option<String>,
    dimensions: usize,
}

impl ApiEmbedder {
    pub fn new(endpoint: String, model: Option<String>, dimensions: usize) -> Self {
        Self {
            endpoint,
            model,
            dimensions,
        }
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }
}

#[async_trait::async_trait]
impl Embedder for ApiEmbedder {
    async fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        if is_ollama_embeddings_endpoint(self.endpoint()) {
            return self.embed_ollama_compatible(texts).await;
        }

        self.embed_openai_compatible(texts).await
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn name(&self) -> &str {
        "api"
    }
}

impl ApiEmbedder {
    async fn embed_openai_compatible(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let endpoint = self.endpoint().to_string();
        let response = reqwest::Client::new()
            .post(self.endpoint())
            .json(&OpenAiEmbeddingsRequest {
                model: self.model().unwrap_or("text-embedding-3-small"),
                input: texts,
            })
            .send()
            .await
            .map_err(|source| EmbedError::HttpRequest {
                endpoint: endpoint.clone(),
                source,
            })?
            .error_for_status()
            .map_err(|source| EmbedError::HttpStatus {
                endpoint: endpoint.clone(),
                source,
            })?
            .json::<OpenAiEmbeddingsResponse>()
            .await
            .map_err(|source| EmbedError::DecodeResponse { endpoint, source })?;

        let vectors = response
            .data
            .into_iter()
            .map(|item| item.embedding)
            .collect::<Vec<_>>();
        validate_vectors(&vectors, self.dimensions())?;
        Ok(vectors)
    }

    async fn embed_ollama_compatible(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let client = reqwest::Client::new();
        let mut vectors = Vec::with_capacity(texts.len());
        let endpoint = self.endpoint().to_string();

        for text in texts {
            let response = client
                .post(self.endpoint())
                .json(&OllamaEmbeddingsRequest {
                    model: self.model().unwrap_or("nomic-embed-text"),
                    prompt: text,
                })
                .send()
                .await
                .map_err(|source| EmbedError::HttpRequest {
                    endpoint: endpoint.clone(),
                    source,
                })?
                .error_for_status()
                .map_err(|source| EmbedError::HttpStatus {
                    endpoint: endpoint.clone(),
                    source,
                })?
                .json::<Value>()
                .await
                .map_err(|source| EmbedError::DecodeResponse {
                    endpoint: endpoint.clone(),
                    source,
                })?;

            let embedding = response
                .get("embedding")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    EmbedError::InvalidResponse(
                        "embedding response missing embedding array".to_string(),
                    )
                })?
                .iter()
                .map(json_number_to_f32)
                .collect::<Result<Vec<_>>>()?;
            vectors.push(embedding);
        }

        validate_vectors(&vectors, self.dimensions())?;
        Ok(vectors)
    }
}

#[derive(Debug, Serialize)]
struct OpenAiEmbeddingsRequest<'a> {
    model: &'a str,
    input: &'a [&'a str],
}

#[derive(Debug, Deserialize)]
struct OpenAiEmbeddingsResponse {
    data: Vec<OpenAiEmbeddingItem>,
}

#[derive(Debug, Deserialize)]
struct OpenAiEmbeddingItem {
    embedding: Vec<f32>,
}

#[derive(Debug, Serialize)]
struct OllamaEmbeddingsRequest<'a> {
    model: &'a str,
    prompt: &'a str,
}

fn is_ollama_embeddings_endpoint(endpoint: &str) -> bool {
    endpoint.trim_end_matches('/').ends_with("/api/embeddings")
}

fn validate_vectors(vectors: &[Vec<f32>], expected_dimensions: usize) -> Result<()> {
    if vectors.is_empty() {
        return Err(EmbedError::EmptyVectors);
    }

    if let Some(actual) = vectors
        .iter()
        .map(Vec::len)
        .find(|length| *length != expected_dimensions)
    {
        return Err(EmbedError::InvalidDimensions {
            expected: expected_dimensions,
            actual,
        });
    }

    Ok(())
}

fn json_number_to_f32(value: &Value) -> Result<f32> {
    value
        .as_f64()
        .map(|number| number as f32)
        .ok_or_else(|| EmbedError::InvalidResponse("embedding element was not numeric".to_string()))
}
