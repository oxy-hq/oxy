use arrow::array::RecordBatch;
use futures::TryStreamExt;
use lancedb::{
    DistanceType,
    query::{ExecutableQuery, QueryBase},
};

use crate::{
    adapters::openai::OpenAIClient,
    config::{constants::RETRIEVAL_INCLUSION_MIDPOINT_COLUMN, model::EmbeddingConfig},
    errors::OxyError,
};

use super::super::types::SearchRecord;
use async_openai::types::{CreateEmbeddingRequestArgs, EmbeddingInput};
use std::sync::Arc;

use super::{serialization::SerializationUtils, table::TableManager};

pub(super) struct SearchManager {
    client: OpenAIClient,
    embedding_config: EmbeddingConfig,
    table_manager: Arc<TableManager>,
}

impl SearchManager {
    pub(super) fn new(
        embedding_config: EmbeddingConfig,
        client: OpenAIClient,
        table_manager: Arc<TableManager>,
    ) -> Self {
        Self {
            client,
            embedding_config,
            table_manager,
        }
    }

    pub(super) async fn search(&self, query: &str) -> Result<Vec<SearchRecord>, OxyError> {
        tracing::info!("Embedding search query: {}", query);
        let query_vector = self.embed_query(query).await?;
        let table = self
            .table_manager
            .get_or_create_table(&self.embedding_config.table)
            .await?;
        let row_count = table.count_rows(None).await?;
        if row_count == 0 {
            tracing::info!("No documents found in table, returning empty results");
            return Ok(vec![]);
        }

        let mut stream = table
            .vector_search(query_vector.as_slice())?
            .column(RETRIEVAL_INCLUSION_MIDPOINT_COLUMN)
            .distance_type(DistanceType::Cosine)
            .limit(self.embedding_config.top_k * self.embedding_config.factor)
            .execute()
            .await?;

        let mut candidates = vec![];
        let mut schema_validated = false;

        while let Some(record_batch) = stream.try_next().await? {
            // Validate schema once before processing any batches
            if !schema_validated {
                self.validate_search_result_schema(&record_batch)?;
                schema_validated = true;
            }

            let docs = SerializationUtils::deserialize_search_records(&record_batch)?;
            candidates.extend(docs);
        }

        let filtered_candidates = self.filter_by_epsilon_ball(candidates);
        let final_results = self.finalize_search_results(filtered_candidates);

        tracing::info!("Search completed with {} results", final_results.len());

        Ok(final_results)
    }

    async fn embed_query(&self, query: &str) -> anyhow::Result<Vec<f32>> {
        let embeddings_request = CreateEmbeddingRequestArgs::default()
            .model(self.embedding_config.embed_model.clone())
            .input(EmbeddingInput::String(query.to_string()))
            .dimensions(self.embedding_config.n_dims as u32)
            .build()?;
        let embeddings_response = self.client.embeddings().create(embeddings_request).await?;
        Ok(embeddings_response.data[0].embedding.clone())
    }

    /// Validates that the search result RecordBatch contains the distance column
    /// added by vector search (table schema is already validated by get_or_create_table)
    fn validate_search_result_schema(&self, record_batch: &RecordBatch) -> Result<(), OxyError> {
        let schema = record_batch.schema();

        // Only need to validate _distance since table schema is already validated
        // _distance is the only column added by the vector search operation
        if schema.column_with_name("_distance").is_none() {
            return Err(OxyError::RuntimeError(
                "Missing '_distance' column in search results - vector search may have failed"
                    .to_string(),
            ));
        }

        // Log optional score columns for debugging
        if schema.column_with_name("_score").is_some() {
            tracing::debug!("Optional '_score' column found in search results");
        } else if schema.column_with_name("score").is_some() {
            tracing::debug!("Optional 'score' column found in search results");
        }

        tracing::debug!("Search result schema validation passed - _distance column present");
        Ok(())
    }

    /// Filters search candidates using epsilon ball constraint
    /// Returns candidates with their relevance scores that passed the filter
    fn filter_by_epsilon_ball(&self, candidates: Vec<SearchRecord>) -> Vec<(SearchRecord, f32)> {
        tracing::info!(
            "Found {} candidates, applying epsilon ball filtering",
            candidates.len()
        );

        if candidates.is_empty() {
            return vec![];
        }

        let mut filtered_candidates = Vec::new();
        for candidate in candidates {
            let distance = candidate.distance;

            if distance <= candidate.document.inclusion_radius {
                tracing::info!(
                    "Document '{}' is within epsilon ball (distance: {:.3}, radius: {:.3})",
                    candidate.document.source_identifier,
                    distance,
                    candidate.document.inclusion_radius
                );

                let relevance_score = if candidate.document.inclusion_radius > 0.0 {
                    1.0 - (distance / candidate.document.inclusion_radius)
                } else {
                    1.0
                };

                filtered_candidates.push((candidate, relevance_score));
            } else {
                tracing::info!(
                    "Document '{}' is outside epsilon ball (distance: {:.3}, radius: {:.3})",
                    candidate.document.source_identifier,
                    distance,
                    candidate.document.inclusion_radius
                );
            }
        }

        tracing::info!(
            "Epsilon ball filtering completed: {} candidates passed",
            filtered_candidates.len()
        );
        filtered_candidates
    }

    /// Convert filtered candidates to SearchRecord objects, sort by relevance, and truncate to top_k
    fn finalize_search_results(
        &self,
        filtered_candidates: Vec<(SearchRecord, f32)>,
    ) -> Vec<SearchRecord> {
        let mut search_records: Vec<SearchRecord> = filtered_candidates
            .into_iter()
            .map(|(candidate, relevance_score)| SearchRecord {
                document: candidate.document,
                distance: candidate.distance,
                score: candidate.score,
                relevance_score: Some(relevance_score),
            })
            .collect();

        search_records.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        search_records.truncate(self.embedding_config.top_k);
        search_records
    }
}
