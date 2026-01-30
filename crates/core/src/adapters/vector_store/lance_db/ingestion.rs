use std::{collections::HashMap, collections::HashSet, sync::Arc};

use crate::{
    adapters::{
        openai::OpenAIClient,
        vector_store::{
            build_index_key,
            builders::parameterized::build_parameterized_retrieval_objects,
            embedding::create_embeddings_batched,
            types::{Embedding, RetrievalItem, RetrievalObject},
        },
    },
    config::{
        constants::{
            RETRIEVAL_CHILD_INCLUSION_RADIUS, RETRIEVAL_DEFAULT_INCLUSION_RADIUS,
            RETRIEVAL_EMBEDDINGS_COLUMN, RETRIEVAL_EXCLUSION_BUFFER_MULTIPLIER,
            RETRIEVAL_INCLUSIONS_TABLE,
        },
        model::EmbeddingConfig,
    },
    service::retrieval::EnumIndexManager,
};
use oxy_shared::errors::OxyError;

use super::{math::MathUtils, serialization::SerializationUtils, table::TableManager};

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
        reindex: bool,
    ) -> Result<(), OxyError> {
        let retrieval_items = self
            .build_retrieval_items_to_ingest(retrieval_objects)
            .await?;
        let retrieval_batch = if retrieval_items.is_empty() {
            return Ok(());
        } else {
            SerializationUtils::create_retrieval_record_batch(
                &retrieval_items,
                self.embedding_config.n_dims,
            )?
        };
        let retrieval_rows = retrieval_batch.num_rows();

        tracing::info!("Total retrieval items to ingest: {}", retrieval_rows);

        let retrieval_table = self
            .table_manager
            .get_or_create_table(RETRIEVAL_INCLUSIONS_TABLE)
            .await?;
        self.table_manager
            .upsert_batch(&retrieval_table, retrieval_batch)
            .await?;

        if reindex {
            self.table_manager
                .reindex_and_optimize(&retrieval_table, &[RETRIEVAL_EMBEDDINGS_COLUMN])
                .await?;
        }

        Ok(())
    }

    pub(super) async fn ingest_parameterized_retrieval_objects_for_query(
        &self,
        enum_index_manager: &EnumIndexManager,
        query: &str,
    ) -> Result<(), OxyError> {
        let param_objects =
            build_parameterized_retrieval_objects(enum_index_manager, query).await?;
        if param_objects.is_empty() {
            return Ok(());
        }

        // Since this function only runs at query time, we don't reindex to avoid
        // latency - may want to schedule reindexing thru background job eventually
        let reindex = false;
        self.ingest(&param_objects, reindex).await
    }

    async fn build_retrieval_items_to_ingest(
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

        // Use a HashMap to deduplicate items by their upsert_key (source_identifier + embedding_content)
        // This prevents "Ambiguous merge insert" errors when the same item appears multiple times
        let mut retrieval_items_by_key: HashMap<String, RetrievalItem> = HashMap::new();

        for obj in retrieval_objects.iter() {
            let content = obj.determine_content(); // a retrieval object's inclusions all have the same content
            let mut exclusion_embeddings: Vec<Embedding> = Vec::with_capacity(obj.exclusions.len());

            for exclusion_text in obj.exclusions.iter() {
                let embedding =
                    text_to_embedding
                        .get(exclusion_text)
                        .cloned()
                        .ok_or_else(|| {
                            OxyError::RuntimeError(format!(
                                "Embedding not found for exclusion: {exclusion_text}"
                            ))
                        })?;
                exclusion_embeddings.push(embedding.clone());
            }

            for inclusion_text in obj.inclusions.iter() {
                let embedding =
                    text_to_embedding
                        .get(inclusion_text)
                        .cloned()
                        .ok_or_else(|| {
                            OxyError::RuntimeError(format!(
                                "Embedding not found for inclusion: {inclusion_text}"
                            ))
                        })?;
                let max_radius = if obj.is_child {
                    RETRIEVAL_CHILD_INCLUSION_RADIUS
                } else {
                    RETRIEVAL_DEFAULT_INCLUSION_RADIUS
                };

                let radius = if exclusion_embeddings.is_empty() {
                    max_radius
                } else {
                    match MathUtils::find_min_distance(&embedding, &exclusion_embeddings) {
                        Ok(Some(d)) => (d * RETRIEVAL_EXCLUSION_BUFFER_MULTIPLIER).min(max_radius),
                        Ok(None) => max_radius,
                        Err(e) => {
                            return Err(OxyError::RuntimeError(format!(
                                "Vector dimension error: {e}"
                            )));
                        }
                    }
                };

                // Generate the same upsert_key used by serialization
                let upsert_key =
                    build_index_key([obj.source_identifier.as_str(), inclusion_text.as_str()]);

                // If duplicate, later item wins (consistent behavior)
                retrieval_items_by_key.insert(
                    upsert_key,
                    RetrievalItem {
                        source_identifier: obj.source_identifier.clone(),
                        embedding_content: inclusion_text.clone(),
                        embedding,
                        content: content.clone(),
                        source_type: obj.source_type.clone(),
                        radius,
                    },
                );
            }
        }

        Ok(retrieval_items_by_key.into_values().collect())
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
