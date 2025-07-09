use std::sync::Arc;
use arrow::{
    array::{RecordBatch, RecordBatchIterator, StringArray, FixedSizeListArray, Float32Array},
    datatypes::{Float32Type},
};
use async_openai::types::{CreateEmbeddingRequestArgs, EmbeddingInput};

use crate::{
    adapters::openai::OpenAIClient, 
    config::{
        constants::{RETRIEVAL_EXCLUSION_BUFFER_MULTIPLIER, RETRIEVAL_DEFAULT_INCLUSION_RADIUS},
        model::EmbeddingConfig
    }, 
    errors::OxyError
};

use super::super::types::{Document, RetrievalContent};
use super::{
    math::MathUtils,
    serialization::SerializationUtils,
    table::TableManager,
};

pub(super) struct EmbeddingManager {
    client: OpenAIClient,
    embedding_config: EmbeddingConfig,
    table_manager: Arc<TableManager>,
}

impl EmbeddingManager {
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

    pub(super) async fn embed(&self, documents: &Vec<Document>) -> Result<(), OxyError> {
        let table = self
            .table_manager
            .get_or_create_table(&self.embedding_config.table)
            .await?;
        let schema = table.schema().await?;        
        let contents = Arc::new(StringArray::from_iter_values(
            documents.iter().map(|doc| doc.content.clone()),
        ));
        let source_types = Arc::new(StringArray::from_iter_values(
            documents.iter().map(|doc| doc.source_type.clone()),
        ));
        let source_identifiers = Arc::new(StringArray::from_iter_values(
            documents.iter().map(|doc| doc.source_identifier.clone()),
        ));

        let mut all_retrieval_inclusions: Vec<Vec<RetrievalContent>> = Vec::new();
        let mut all_retrieval_exclusions: Vec<Vec<RetrievalContent>> = Vec::new();
        let mut all_inclusion_midpoints: Vec<Vec<f32>> = Vec::new();
        let mut all_inclusion_radii: Vec<f32> = Vec::new();

        for (_i, doc) in documents.iter().enumerate() {            
            let inclusions = self.embed_retrieval_content(&doc.retrieval_inclusions).await?;
            all_retrieval_inclusions.push(inclusions.clone());

            let exclusions = self.embed_retrieval_content(&doc.retrieval_exclusions).await?;
            all_retrieval_exclusions.push(exclusions.clone());

            let inclusion_embeddings: Vec<Vec<f32>> = inclusions
                .iter()
                .filter_map(|inc| {
                    if !inc.embeddings.is_empty() {
                        Some(inc.embeddings.clone())
                    } else {
                        None
                    }
                })
                .collect();

            let inclusion_midpoint = if !inclusion_embeddings.is_empty() {
                MathUtils::calculate_centroid(&inclusion_embeddings, self.embedding_config.n_dims)
            } else {
                vec![0.0; self.embedding_config.n_dims]
            };

            let exclusion_embeddings: Vec<Vec<f32>> = exclusions
                .iter()
                .filter_map(|exc| {
                    if !exc.embeddings.is_empty() {
                        Some(exc.embeddings.clone())
                    } else {
                        None
                    }
                })
                .collect();

            let inclusion_radius = match MathUtils::find_min_distance(&inclusion_midpoint, &exclusion_embeddings) {
                Ok(Some(nearest_exclusion_distance)) => {
                    nearest_exclusion_distance * RETRIEVAL_EXCLUSION_BUFFER_MULTIPLIER
                },
                Ok(None) => {
                    RETRIEVAL_DEFAULT_INCLUSION_RADIUS
                },
                Err(e) => {
                    return Err(OxyError::RuntimeError(format!("Vector dimension error: {}", e)));
                }
            };

            all_inclusion_midpoints.push(inclusion_midpoint);
            all_inclusion_radii.push(inclusion_radius);            
        }

        tracing::info!("Total documents to embed: {:?}", documents.len());

        table.delete("true").await?;

        let retrieval_inclusions_array = SerializationUtils::create_retrieval_content_array(&all_retrieval_inclusions, self.embedding_config.n_dims)?;
        let retrieval_exclusions_array = SerializationUtils::create_retrieval_content_array(&all_retrieval_exclusions, self.embedding_config.n_dims)?;
        let inclusion_midpoint_array = Arc::new(FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
            all_inclusion_midpoints.iter().map(|midpoint| {
                Some(midpoint.iter().map(|&v| Some(v)).collect::<Vec<_>>())
            }),
            self.embedding_config.n_dims.try_into().unwrap(),
        ));
        let inclusion_radius_array = Arc::new(Float32Array::from_iter_values(
            all_inclusion_radii.iter().copied(),
        ));
        let record_batch = match RecordBatch::try_new(
            schema.clone(),
            vec![
                contents,
                source_types,
                source_identifiers,
                retrieval_inclusions_array,
                retrieval_exclusions_array,
                inclusion_midpoint_array,
                inclusion_radius_array,
            ],
        ) {
            Ok(batch) => {
                batch
            },
            Err(e) => {
                return Err(OxyError::RuntimeError(format!("Failed to create RecordBatch: {:?}", e)));
            }
        };
                
        let batches = RecordBatchIterator::new(
            vec![record_batch].into_iter().map(Ok),
            schema.clone(),
        );
        self.table_manager.add_batches(&table, Box::new(batches)).await?;
        tracing::info!("{} documents embedded!", documents.len());
        Ok(())
    }

    async fn embed_retrieval_content(
        &self,
        retrieval_content_list: &Vec<RetrievalContent>,
    ) -> Result<Vec<RetrievalContent>, OxyError> {
        if retrieval_content_list.is_empty() {
            return Ok(vec![]);
        }

        let embedding_contents = retrieval_content_list
            .iter()
            .map(|content| content.embedding_content.clone())
            .collect::<Vec<String>>();
        
        let embeddings_request = CreateEmbeddingRequestArgs::default()
            .model(self.embedding_config.embed_model.clone())
            .input(EmbeddingInput::StringArray(embedding_contents))
            .dimensions(self.embedding_config.n_dims as u32)
            .build()
            .map_err(|e| OxyError::RuntimeError(format!("Failed to build embedding request: {}", e)))?;
        
        let embeddings_response = self.client.embeddings().create(embeddings_request).await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to create embeddings: {}", e)))?;
        
        let mut result = Vec::new();
        for (content, embedding_data) in retrieval_content_list.iter().zip(embeddings_response.data.iter()) {
            result.push(RetrievalContent {
                embedding_content: content.embedding_content.clone(),
                embeddings: embedding_data.embedding.clone(),
            });
        }
        
        Ok(result)
    }
} 
