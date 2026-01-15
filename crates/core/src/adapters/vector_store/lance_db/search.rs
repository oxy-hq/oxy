use arrow::array::RecordBatch;
use futures::TryStreamExt;
use lancedb::{
    DistanceType,
    query::{ExecutableQuery, QueryBase},
};

use crate::{
    adapters::{
        openai::OpenAIClient,
        vector_store::types::{Embedding, RetrievalItem, SearchRecord},
    },
    config::{
        constants::{RETRIEVAL_EMBEDDINGS_COLUMN, RETRIEVAL_INCLUSIONS_TABLE},
        model::EmbeddingConfig,
    },
    service::retrieval::EnumIndexManager,
};
use oxy_shared::errors::OxyError;

use async_openai::types::embeddings::{CreateEmbeddingRequestArgs, EmbeddingInput};
use std::{collections::HashMap, sync::Arc};

use super::ingestion::IngestionManager;
use super::{serialization::SerializationUtils, table::TableManager};

pub(super) struct SearchManager {
    client: OpenAIClient,
    embedding_config: EmbeddingConfig,
    table_manager: Arc<TableManager>,
    enum_index_manager: Arc<EnumIndexManager>,
}

impl SearchManager {
    pub(super) fn new(
        embedding_config: EmbeddingConfig,
        client: OpenAIClient,
        table_manager: Arc<TableManager>,
        enum_index_manager: Arc<EnumIndexManager>,
    ) -> Self {
        Self {
            client,
            embedding_config,
            table_manager,
            enum_index_manager,
        }
    }

    pub(super) async fn search(&self, query: &str) -> Result<Vec<SearchRecord>, OxyError> {
        let manager = IngestionManager::new(
            self.client.clone(),
            self.embedding_config.clone(),
            self.table_manager.clone(),
        );
        manager
            .ingest_parameterized_retrieval_objects_for_query(&self.enum_index_manager, query)
            .await?;

        tracing::info!("Embedding search query: {}", query);
        let query_vector = self.embed_query(query).await?;
        let retrieval_table = self
            .table_manager
            .get_or_create_table(RETRIEVAL_INCLUSIONS_TABLE)
            .await?;
        let row_count = retrieval_table.count_rows(None).await?;
        if row_count == 0 {
            tracing::info!("No inclusions found in table, returning empty results");
            return Ok(vec![]);
        }

        let mut stream = retrieval_table
            .vector_search(query_vector.as_slice())?
            .column(RETRIEVAL_EMBEDDINGS_COLUMN)
            .distance_type(DistanceType::Cosine)
            .limit(self.embedding_config.top_k * self.embedding_config.factor)
            .execute()
            .await?;

        let mut retrieval_items: Vec<(RetrievalItem, f32)> = vec![];
        let mut schema_validated = false;

        while let Some(record_batch) = stream.try_next().await? {
            // Validate schema once before processing any batches
            if !schema_validated {
                self.validate_inclusion_search_result_schema(&record_batch)?;
                schema_validated = true;
            }

            let retrieval_items_batch =
                SerializationUtils::deserialize_search_records(&record_batch)?;
            retrieval_items.extend(retrieval_items_batch);
        }

        for (item, distance) in retrieval_items.iter() {
            tracing::info!(
                "Candidate: {} text: {} radius: {} distance: {}",
                item.source_identifier,
                item.embedding_content,
                item.radius,
                distance
            );
        }

        let filtered = self.filter_by_inclusion_radius(retrieval_items);
        let deduplicated = self.deduplicate_by_source_identifier(filtered);
        let final_results = self.finalize_search_results(deduplicated);

        tracing::info!(
            "Search completed with {} unique results",
            final_results.len()
        );

        Ok(final_results)
    }

    async fn embed_query(&self, query: &str) -> anyhow::Result<Embedding> {
        let embeddings_request = CreateEmbeddingRequestArgs::default()
            .model(self.embedding_config.embed_model.clone())
            .input(EmbeddingInput::String(query.to_string()))
            .dimensions(self.embedding_config.n_dims as u32)
            .build()?;
        let embeddings_response = self.client.embeddings().create(embeddings_request).await?;
        Ok(embeddings_response.data[0].embedding.clone())
    }

    fn validate_inclusion_search_result_schema(
        &self,
        record_batch: &RecordBatch,
    ) -> Result<(), OxyError> {
        let schema = record_batch.schema();

        // Only need to validate _distance since table schema is already validated
        // _distance is the only column added by the vector search operation
        if schema.column_with_name("_distance").is_none() {
            return Err(OxyError::RuntimeError(
                "Missing '_distance' column in inclusion search results - vector search may have failed"
                    .to_string(),
            ));
        }

        tracing::debug!(
            "Inclusion search result schema validation passed - _distance column present"
        );
        Ok(())
    }

    /// Filter candidates by inclusion radius (epsilon ball filtering)
    fn filter_by_inclusion_radius(
        &self,
        retrieval_items: Vec<(RetrievalItem, f32)>,
    ) -> Vec<(RetrievalItem, f32)> {
        tracing::info!(
            "Found {} candidates, applying epsilon ball filtering",
            retrieval_items.len()
        );

        let filtered: Vec<_> = retrieval_items
            .into_iter()
            .filter(|(item, distance)| {
                let radius = item.radius;
                let within_radius = *distance <= radius;
                tracing::debug!(
                    "Inclusion '{}:{}' is {} epsilon ball (distance: {:.3}, radius: {:.3})",
                    item.source_identifier,
                    item.embedding_content,
                    if within_radius { "within" } else { "outside" },
                    distance,
                    radius
                );
                within_radius
            })
            .collect();

        tracing::info!(
            "Epsilon ball filtering completed: {} candidates passed",
            filtered.len()
        );
        filtered
    }

    /// Deduplicate by source identifier, keeping the best match per source
    fn deduplicate_by_source_identifier(
        &self,
        retrieval_items: Vec<(RetrievalItem, f32)>,
    ) -> Vec<(RetrievalItem, f32)> {
        tracing::info!(
            "Deduplicating {} inclusions by source_identifier",
            retrieval_items.len()
        );

        let deduplicated: Vec<_> = retrieval_items
            .into_iter()
            .fold(HashMap::new(), |mut best_by_source: HashMap<String, (RetrievalItem, f32)>, (item, distance)| {
                let source_id = item.source_identifier.clone();

                match best_by_source.get(&source_id) {
                    Some((_, existing_distance)) if distance < *existing_distance => {
                        tracing::debug!(
                            "Replacing candidate for '{}' (new distance: {:.3} < existing: {:.3})",
                            source_id,
                            distance,
                            existing_distance
                        );
                        best_by_source.insert(source_id, (item, distance));
                    }
                    None => {
                        best_by_source.insert(source_id, (item, distance));
                    }
                    _ => {} // Keep existing better match
                }
                best_by_source
            })
            .into_values()
            .collect();

        tracing::info!(
            "Deduplication completed: {} unique sources",
            deduplicated.len()
        );
        deduplicated
    }

    /// Convert filtered retrieval items to SearchRecord objects, sort by relevance, and truncate to top_k
    fn finalize_search_results(
        &self,
        retrieval_items: Vec<(RetrievalItem, f32)>,
    ) -> Vec<SearchRecord> {
        let mut search_records: Vec<SearchRecord> = retrieval_items
            .into_iter()
            .map(|(item, distance)| {
                let radius = item.radius;
                let relevance_score = if radius > 0.0 {
                    1.0 - (distance / radius)
                } else {
                    1.0
                };

                SearchRecord {
                    retrieval_item: item,
                    distance,
                    score: None,
                    relevance_score: Some(relevance_score),
                }
            })
            .collect();

        search_records.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        search_records.truncate(self.embedding_config.top_k);

        tracing::info!(
            "Processing pipeline completed: {} final results",
            search_records.len()
        );

        search_records
    }
}
