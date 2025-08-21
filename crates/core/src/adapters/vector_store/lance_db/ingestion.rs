use arrow::array::RecordBatch;
use std::{collections::HashSet, sync::Arc};

use crate::{
    adapters::{
        openai::OpenAIClient,
        vector_store::types::{Embedding, RetrievalItem, RetrievalObject},
    },
    config::{
        constants::{
            RETRIEVAL_DEFAULT_INCLUSION_RADIUS, RETRIEVAL_EMBEDDINGS_COLUMN,
            RETRIEVAL_EXCLUSION_BUFFER_MULTIPLIER,
        },
        model::EmbeddingConfig,
    },
    errors::OxyError,
};

use super::{
    embedding::create_embeddings_batched, math::MathUtils, serialization::SerializationUtils,
    table::TableManager,
};

pub(super) struct IngestionManager {
    client: OpenAIClient,
    embedding_config: EmbeddingConfig,
    table_manager: Arc<TableManager>,
}

impl IngestionManager {
    pub(super) fn new(
        client: OpenAIClient,
        embedding_config: EmbeddingConfig,
        table_manager: Arc<TableManager>,
    ) -> Self {
        Self {
            client,
            embedding_config,
            table_manager,
        }
    }

    pub(super) async fn ingest(
        &self,
        retrieval_objects: &Vec<RetrievalObject>,
    ) -> Result<(), OxyError> {
        let retrieval_items = self.build_retrieval_items(retrieval_objects).await?;

        tracing::info!("Total retrieval items to ingest: {}", retrieval_items.len());

        let batch: Option<RecordBatch> = if retrieval_items.is_empty() {
            None
        } else {
            Some(SerializationUtils::create_retrieval_record_batch(
                &retrieval_items,
                self.embedding_config.n_dims,
            )?)
        };

        self.table_manager
            .replace_with_batch(batch, RETRIEVAL_EMBEDDINGS_COLUMN)
            .await?;

        tracing::info!("{} retrieval items ingested.", retrieval_items.len());
        Ok(())
    }

    async fn build_retrieval_items(
        &self,
        retrieval_objects: &Vec<RetrievalObject>,
    ) -> Result<Vec<RetrievalItem>, OxyError> {
        let all_texts_to_embed = self.collect_unique_retrieval_strings(retrieval_objects);
        let all_embeddings =
            create_embeddings_batched(&self.client, &self.embedding_config, &all_texts_to_embed)
                .await?;
        let text_to_embedding: std::collections::HashMap<String, Embedding> = all_texts_to_embed
            .into_iter()
            .zip(all_embeddings.into_iter())
            .collect();

        let mut retrieval_items: Vec<RetrievalItem> = Vec::new();
        for obj in retrieval_objects.iter() {
            let content = obj.build_content(); // a retrieval object's inclusions all have the same content
            let mut exclusion_embeddings: Vec<Embedding> = Vec::with_capacity(obj.exclusions.len());

            for exclusion_text in obj.exclusions.iter() {
                let embedding = text_to_embedding
                    .get(exclusion_text)
                    .cloned()
                    .unwrap_or_default();
                exclusion_embeddings.push(embedding.clone());
            }

            for inclusion_text in obj.inclusions.iter() {
                let embedding = text_to_embedding
                    .get(inclusion_text)
                    .cloned()
                    .unwrap_or_default();
                let radius = if exclusion_embeddings.is_empty() {
                    RETRIEVAL_DEFAULT_INCLUSION_RADIUS
                } else {
                    match MathUtils::find_min_distance(&embedding, &exclusion_embeddings) {
                        Ok(Some(d)) => d * RETRIEVAL_EXCLUSION_BUFFER_MULTIPLIER,
                        Ok(None) => RETRIEVAL_DEFAULT_INCLUSION_RADIUS,
                        Err(e) => {
                            return Err(OxyError::RuntimeError(format!(
                                "Vector dimension error: {e}"
                            )));
                        }
                    }
                };

                retrieval_items.push(RetrievalItem {
                    source_identifier: obj.source_identifier.clone(),
                    embedding_content: inclusion_text.clone(),
                    embedding,
                    content: content.clone(),
                    source_type: obj.source_type.clone(),
                    radius,
                });
            }
        }

        Ok(retrieval_items)
    }

    fn collect_unique_retrieval_strings(
        &self,
        retrieval_objects: &Vec<RetrievalObject>,
    ) -> Vec<String> {
        let mut seen: HashSet<&str> = HashSet::new();
        let mut unique_texts: Vec<String> = Vec::new();

        for obj in retrieval_objects.iter() {
            for text in obj.exclusions.iter().chain(obj.inclusions.iter()) {
                if seen.insert(text.as_str()) {
                    unique_texts.push(text.clone());
                }
            }
        }

        unique_texts
    }
}
