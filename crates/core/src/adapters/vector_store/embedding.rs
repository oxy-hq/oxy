use async_openai::types::embeddings::{CreateEmbeddingRequestArgs, EmbeddingInput};

use crate::{
    adapters::{openai::OpenAIClient, vector_store::types::Embedding},
    config::{constants::RETRIEVAL_EMBEDDINGS_BATCH_SIZE, model::EmbeddingConfig},
    errors::OxyError,
};

/// Create embeddings for the provided contents in fixed-size batches.
/// Batches are processed sequentially rather than in parallel to avoid Send/lifetime issues in async closures.
pub async fn create_embeddings_batched(
    client: &OpenAIClient,
    embedding_config: &EmbeddingConfig,
    contents: &Vec<String>,
) -> Result<Vec<Embedding>, OxyError> {
    if contents.is_empty() {
        return Ok(vec![]);
    }

    let mut all_embeddings: Vec<Embedding> = Vec::with_capacity(contents.len());

    for chunk in contents.chunks(RETRIEVAL_EMBEDDINGS_BATCH_SIZE) {
        let embeddings_request = CreateEmbeddingRequestArgs::default()
            .model(embedding_config.embed_model.clone())
            .input(EmbeddingInput::StringArray(chunk.to_vec()))
            .dimensions(embedding_config.n_dims as u32)
            .build()
            .map_err(|e| {
                OxyError::RuntimeError(format!("Failed to build embedding request: {e}"))
            })?;

        let embeddings_response = client
            .embeddings()
            .create(embeddings_request)
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to create embeddings: {e}")))?;

        let mut batch_embeddings: Vec<Embedding> = embeddings_response
            .data
            .into_iter()
            .map(|data| data.embedding)
            .collect();

        all_embeddings.append(&mut batch_embeddings);
    }

    Ok(all_embeddings)
}
