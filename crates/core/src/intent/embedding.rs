//! Embedding service for generating question embeddings

use async_openai::{
    Client,
    config::OpenAIConfig,
    types::embeddings::{CreateEmbeddingRequestArgs, EmbeddingInput},
};

use crate::errors::OxyError;

use super::types::IntentConfig;

/// Service for generating text embeddings
#[derive(Debug)]
pub struct EmbeddingService {
    client: Client<OpenAIConfig>,
    model: String,
    dims: usize,
}

impl EmbeddingService {
    /// Create a new embedding service
    pub fn new(config: &IntentConfig) -> Result<Self, OxyError> {
        if config.openai_api_key.is_empty() {
            return Err(OxyError::ConfigurationError(
                "OpenAI API key is required for intent classification".to_string(),
            ));
        }

        let openai_config = OpenAIConfig::new().with_api_key(&config.openai_api_key);
        let client = Client::with_config(openai_config);

        Ok(Self {
            client,
            model: config.embed_model.clone(),
            dims: config.embed_dims,
        })
    }

    /// Generate embedding for a single text
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>, OxyError> {
        let request = CreateEmbeddingRequestArgs::default()
            .model(&self.model)
            .input(EmbeddingInput::String(text.to_string()))
            .dimensions(self.dims as u32)
            .build()
            .map_err(|e| {
                OxyError::RuntimeError(format!("Failed to build embedding request: {e}"))
            })?;

        let response = self
            .client
            .embeddings()
            .create(request)
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to create embedding: {e}")))?;

        response
            .data
            .into_iter()
            .next()
            .map(|d| d.embedding)
            .ok_or_else(|| OxyError::RuntimeError("No embedding returned".to_string()))
    }

    /// Generate embeddings for multiple texts in batches
    pub async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, OxyError> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        const BATCH_SIZE: usize = 100;
        let mut all_embeddings = Vec::with_capacity(texts.len());

        for chunk in texts.chunks(BATCH_SIZE) {
            let request = CreateEmbeddingRequestArgs::default()
                .model(&self.model)
                .input(EmbeddingInput::StringArray(chunk.to_vec()))
                .dimensions(self.dims as u32)
                .build()
                .map_err(|e| {
                    OxyError::RuntimeError(format!("Failed to build embedding request: {e}"))
                })?;

            let response = self
                .client
                .embeddings()
                .create(request)
                .await
                .map_err(|e| OxyError::RuntimeError(format!("Failed to create embeddings: {e}")))?;

            let mut batch_embeddings: Vec<Vec<f32>> =
                response.data.into_iter().map(|d| d.embedding).collect();
            all_embeddings.append(&mut batch_embeddings);
        }

        Ok(all_embeddings)
    }
}

/// Calculate cosine similarity between two vectors
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let mut dot_product = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;

    for (x, y) in a.iter().zip(b.iter()) {
        dot_product += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot_product / (norm_a.sqrt() * norm_b.sqrt())
}
