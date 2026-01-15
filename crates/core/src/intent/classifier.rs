//! Main intent classifier that orchestrates the pipeline

use async_openai::{
    Client,
    config::OpenAIConfig,
    types::chat::{
        ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
        ChatCompletionRequestSystemMessageContent, ChatCompletionRequestUserMessage,
        ChatCompletionRequestUserMessageContent, CreateChatCompletionRequestArgs,
    },
};
use tracing::{debug, info};

use oxy_shared::errors::OxyError;

use super::{
    clustering::{cluster_embeddings, extract_clusters},
    embedding::{EmbeddingService, cosine_similarity},
    storage::IntentStorage,
    types::{
        Cluster, IncrementalResult, IntentAnalytics, IntentClassification, IntentCluster,
        IntentConfig, PipelineResult, UNKNOWN_CLUSTER_ID,
    },
};

/// Intent classifier for discovering and classifying question intents
#[derive(Debug)]
pub struct IntentClassifier {
    config: IntentConfig,
    embedding_service: EmbeddingService,
    storage: IntentStorage,
    llm_client: Client<OpenAIConfig>,
}

impl IntentClassifier {
    /// Create a new intent classifier
    pub async fn new(config: IntentConfig) -> Result<Self, OxyError> {
        let embedding_service = EmbeddingService::new(&config)?;
        let storage = IntentStorage::new(&config);

        // Create LLM client for labeling
        let openai_config = OpenAIConfig::new().with_api_key(&config.openai_api_key);
        let llm_client = Client::with_config(openai_config);

        let classifier = Self {
            config,
            embedding_service,
            storage,
            llm_client,
        };

        // Ensure the unknown cluster exists for outlier classifications
        classifier.ensure_unknown_cluster().await?;

        Ok(classifier)
    }

    /// Run the full clustering pipeline
    ///
    /// 1. Fetch unprocessed questions from traces
    /// 2. Generate embeddings
    /// 3. Store classifications with embeddings
    /// 4. Load all embeddings for clustering
    /// 5. Cluster embeddings with HDBSCAN
    /// 6. Label clusters with LLM
    /// 7. Store results
    pub async fn run_pipeline(&mut self, limit: usize) -> Result<PipelineResult, OxyError> {
        info!("Starting intent classification pipeline...");

        // Step 1: Fetch new questions
        let questions = self.storage.fetch_questions(limit).await?;
        let new_questions_count = questions.len();

        if !questions.is_empty() {
            info!("Fetched {} new questions", questions.len());

            // Step 2: Generate embeddings for new questions
            let texts: Vec<String> = questions.iter().map(|(_, q, _)| q.clone()).collect();
            let embeddings = self.embedding_service.embed_batch(&texts).await?;
            info!("Generated {} embeddings", embeddings.len());

            // Step 3: Store initial classifications (unknown) with embeddings
            for ((trace_id, question, agent_ref), embedding) in
                questions.iter().zip(embeddings.iter())
            {
                let unknown_classification = IntentClassification::unknown();
                self.storage
                    .store_classification(
                        trace_id,
                        question,
                        &unknown_classification,
                        embedding,
                        "agent",
                        agent_ref,
                    )
                    .await?;
            }
        } else {
            info!("No new questions to embed");
        }

        // Step 4: Load all embeddings for clustering
        let all_embeddings = self.storage.load_embeddings().await?;
        if all_embeddings.is_empty() {
            info!("No embeddings available for clustering");
            return Ok(PipelineResult {
                questions_processed: 0,
                clusters_created: 0,
                outliers_count: 0,
            });
        }

        let all_texts: Vec<String> = all_embeddings
            .iter()
            .map(|(_, q, _, _, _)| q.clone())
            .collect();
        let all_vecs: Vec<Vec<f32>> = all_embeddings
            .iter()
            .map(|(_, _, e, _, _)| e.clone())
            .collect();
        info!("Clustering {} total embeddings", all_vecs.len());

        // Step 5: Cluster embeddings
        let clustering_result = cluster_embeddings(&all_vecs, self.config.min_cluster_size);
        info!(
            "Found {} clusters, {} outliers",
            clustering_result.num_clusters,
            clustering_result
                .labels
                .iter()
                .filter(|&&l| l == -1)
                .count()
        );

        // Step 6: Extract and label clusters
        let clusters = extract_clusters(&clustering_result.labels, &all_vecs, &all_texts);

        let mut intent_clusters = Vec::with_capacity(clusters.len());
        for cluster in &clusters {
            let (intent_name, intent_description) = self.label_cluster(&cluster.questions).await?;

            let sample_questions: Vec<String> = cluster.questions.iter().take(5).cloned().collect();

            intent_clusters.push(IntentCluster {
                cluster_id: cluster.id as u32,
                intent_name,
                intent_description,
                centroid: cluster.centroid.clone(),
                sample_questions,
            });
        }

        // Step 7: Store clusters
        self.storage.store_clusters(&intent_clusters).await?;

        // Step 8: Update classifications with new cluster assignments
        for (idx, (trace_id, question, embedding, source_type, source)) in
            all_embeddings.iter().enumerate()
        {
            let cluster_label = clustering_result.labels[idx];
            let classification = if cluster_label == -1 {
                // Outlier - keep as unknown
                IntentClassification::unknown()
            } else {
                // Find the corresponding intent cluster
                if let Some(intent_cluster) = intent_clusters
                    .iter()
                    .find(|c| c.cluster_id == cluster_label as u32)
                {
                    IntentClassification {
                        intent_name: intent_cluster.intent_name.clone(),
                        intent_description: intent_cluster.intent_description.clone(),
                        confidence: 0.8, // High confidence for clustered items
                        cluster_id: intent_cluster.cluster_id,
                    }
                } else {
                    IntentClassification::unknown()
                }
            };

            self.storage
                .update_classification(
                    trace_id,
                    question,
                    &classification,
                    embedding,
                    source_type,
                    source,
                )
                .await?;
        }

        // Step 9: Ensure unknown cluster exists for outlier classifications
        self.ensure_unknown_cluster().await?;

        let outliers_count = clustering_result
            .labels
            .iter()
            .filter(|&&l| l == -1)
            .count();

        Ok(PipelineResult {
            questions_processed: new_questions_count,
            clusters_created: clustering_result.num_clusters,
            outliers_count,
        })
    }

    /// Classify a single question
    pub async fn classify(&self, question: &str) -> Result<IntentClassification, OxyError> {
        // Generate embedding for the question
        let embedding = self.embedding_service.embed(question).await?;

        self.classify_embedding(&embedding).await
    }

    /// Classify based on pre-computed embedding
    async fn classify_embedding(
        &self,
        embedding: &[f32],
    ) -> Result<IntentClassification, OxyError> {
        let clusters = self.storage.load_clusters().await?;
        if clusters.is_empty() {
            return Ok(IntentClassification::unknown());
        }

        // Find nearest cluster
        let mut best_cluster: Option<&IntentCluster> = None;
        let mut best_similarity = f32::NEG_INFINITY;

        for cluster in &clusters {
            let similarity = cosine_similarity(embedding, &cluster.centroid);
            if similarity > best_similarity {
                best_similarity = similarity;
                best_cluster = Some(cluster);
            }
        }

        match best_cluster {
            Some(cluster) if best_similarity > 0.5 => Ok(IntentClassification {
                intent_name: cluster.intent_name.clone(),
                intent_description: cluster.intent_description.clone(),
                confidence: best_similarity,
                cluster_id: cluster.cluster_id,
            }),
            _ => Ok(IntentClassification::unknown()),
        }
    }

    /// Classify with incremental learning
    ///
    /// If confidence is below threshold or the result is unknown, it will trigger
    /// incremental clustering when enough unknown questions accumulate.
    pub async fn classify_with_learning(
        &self,
        trace_id: &str,
        question: &str,
        source_type: &str,
        source: &str,
    ) -> Result<(IntentClassification, bool), OxyError> {
        // Generate embedding
        let embedding = self.embedding_service.embed(question).await?;

        // Classify
        let classification = self.classify_embedding(&embedding).await?;

        // Store classification with embedding
        self.storage
            .store_classification(
                trace_id,
                question,
                &classification,
                &embedding,
                source_type,
                source,
            )
            .await?;

        // Check if we should trigger incremental clustering
        let should_cluster = classification.confidence < self.config.learning_confidence_threshold
            || classification.intent_name == "unknown";

        if should_cluster {
            info!(
                "Low confidence or unknown classification (confidence: {:.2})",
                classification.confidence
            );

            // Check if we should trigger incremental clustering
            let unknown_count = self.storage.get_unknown_count().await?;
            if unknown_count >= self.config.learning_pool_threshold {
                info!(
                    "Unknown count reached threshold ({}), triggering incremental clustering",
                    unknown_count
                );
                let result = self.run_incremental_clustering().await?;
                info!(
                    "Incremental clustering complete: {} new clusters, {} merged, {} outliers",
                    result.new_clusters, result.merged_count, result.outliers_count
                );
            }
        }

        Ok((classification, should_cluster))
    }

    /// Classify and store the result
    pub async fn classify_and_store(
        &self,
        trace_id: &str,
        question: &str,
        source_type: &str,
        source: &str,
    ) -> Result<IntentClassification, OxyError> {
        // Generate embedding
        let embedding = self.embedding_service.embed(question).await?;
        let classification = self.classify_embedding(&embedding).await?;

        self.storage
            .store_classification(
                trace_id,
                question,
                &classification,
                &embedding,
                source_type,
                source,
            )
            .await?;
        Ok(classification)
    }

    /// Get intent analytics for the last N days
    pub async fn get_analytics(&self, days: u32) -> Result<Vec<IntentAnalytics>, OxyError> {
        self.storage.get_analytics(days).await
    }

    /// Get outlier questions
    pub async fn get_outliers(&self, limit: usize) -> Result<Vec<(String, String)>, OxyError> {
        self.storage.get_outliers(limit).await
    }

    /// Get current clusters
    pub async fn get_clusters(&self) -> Result<Vec<IntentCluster>, OxyError> {
        self.storage.load_clusters().await
    }

    /// Get unknown classifications count
    pub async fn get_unknown_count(&self) -> Result<usize, OxyError> {
        self.storage.get_unknown_count().await
    }

    /// Ensure the unknown cluster exists in storage
    ///
    /// This cluster is used for all outlier/unclassified questions
    async fn ensure_unknown_cluster(&self) -> Result<(), OxyError> {
        let clusters = self.storage.load_clusters().await?;
        let has_unknown = clusters.iter().any(|c| c.cluster_id == UNKNOWN_CLUSTER_ID);

        if !has_unknown {
            info!("Creating unknown cluster for outlier classifications");
            let unknown_cluster = IntentCluster::unknown(self.config.embed_dims);
            self.storage.update_cluster(&unknown_cluster).await?;
        }

        Ok(())
    }

    /// Run incremental clustering on unknown classifications
    ///
    /// This method:
    /// 1. Loads unknown classifications
    /// 2. Runs mini-clustering on them
    /// 3. For each new cluster, checks if it should merge with existing clusters
    /// 4. Creates new clusters or merges into existing ones
    /// 5. Reclassifies all unknown items with the updated clusters
    pub async fn run_incremental_clustering(&self) -> Result<IncrementalResult, OxyError> {
        // Load unknown classifications
        let unknown_items: Vec<(String, String, Vec<f32>, String)> =
            self.storage.load_unknown_classifications().await?;
        if unknown_items.is_empty() {
            return Ok(IncrementalResult {
                items_processed: 0,
                new_clusters: 0,
                merged_count: 0,
                outliers_count: 0,
            });
        }

        let items_count = unknown_items.len();
        info!(
            "Running incremental clustering on {} unknown items",
            items_count
        );

        // Load existing clusters from storage
        let mut clusters = self.storage.load_clusters().await?;

        // Extract embeddings and questions (keep original data for reclassification)
        let embeddings: Vec<Vec<f32>> =
            unknown_items.iter().map(|(_, _, e, _)| e.clone()).collect();
        let questions: Vec<String> = unknown_items.iter().map(|(_, q, _, _)| q.clone()).collect();

        // Run mini-clustering with smaller min_cluster_size for incremental updates
        let mini_cluster_size = (self.config.min_cluster_size / 2).max(2);
        let clustering_result = cluster_embeddings(&embeddings, mini_cluster_size);

        // Extract clusters
        let new_clusters = extract_clusters(&clustering_result.labels, &embeddings, &questions);

        let mut new_cluster_count = 0;
        let mut merged_count = 0;

        // Process each new cluster
        for cluster in new_clusters {
            // Check if this cluster should merge with an existing one
            if let Some((existing_idx, similarity)) =
                Self::find_similar_cluster(&clusters, &cluster.centroid)
            {
                if similarity >= self.config.cluster_merge_threshold {
                    // Merge into existing cluster
                    info!(
                        "Merging new cluster into existing cluster {} (similarity: {:.2})",
                        clusters[existing_idx].cluster_id, similarity
                    );
                    self.merge_into_cluster(&mut clusters[existing_idx], &cluster)
                        .await?;
                    merged_count += cluster.questions.len();
                    continue;
                }
            }

            // Create a new cluster
            let next_id = self.storage.get_next_cluster_id().await?;
            let (intent_name, intent_description) = self.label_cluster(&cluster.questions).await?;

            let new_intent_cluster = IntentCluster {
                cluster_id: next_id,
                intent_name: intent_name.clone(),
                intent_description: intent_description.clone(),
                centroid: cluster.centroid.clone(),
                sample_questions: cluster.questions.iter().take(5).cloned().collect(),
            };

            info!(
                "Created new cluster {}: {} ({} questions)",
                next_id,
                intent_name,
                cluster.questions.len()
            );

            // Store the new cluster
            self.storage.update_cluster(&new_intent_cluster).await?;
            clusters.push(new_intent_cluster);
            new_cluster_count += 1;
        }

        // Reclassify all unknown items with the updated clusters
        info!(
            "Reclassifying {} unknown items with updated clusters",
            unknown_items.len()
        );

        let mut outliers_count = 0;

        for (trace_id, question, embedding, agent_ref) in &unknown_items {
            let classification = Self::classify_embedding_with_clusters(&clusters, embedding);

            // Update the classification in the database
            self.storage
                .update_classification(
                    trace_id,
                    question,
                    &classification,
                    embedding,
                    "agent",
                    agent_ref,
                )
                .await?;

            // Check if this is still an outlier (unknown intent)
            if classification.intent_name == "unknown" {
                outliers_count += 1;
                info!("Question remains outlier: {}", question);
            }
        }

        Ok(IncrementalResult {
            items_processed: items_count,
            new_clusters: new_cluster_count,
            merged_count,
            outliers_count,
        })
    }

    /// Classify embedding against provided clusters (no storage access)
    fn classify_embedding_with_clusters(
        clusters: &[IntentCluster],
        embedding: &[f32],
    ) -> IntentClassification {
        if clusters.is_empty() {
            return IntentClassification::unknown();
        }

        let mut best_cluster: Option<&IntentCluster> = None;
        let mut best_similarity = f32::NEG_INFINITY;

        for cluster in clusters {
            let similarity = cosine_similarity(embedding, &cluster.centroid);
            if similarity > best_similarity {
                best_similarity = similarity;
                best_cluster = Some(cluster);
            }
        }

        match best_cluster {
            Some(cluster) if best_similarity > 0.5 => IntentClassification {
                intent_name: cluster.intent_name.clone(),
                intent_description: cluster.intent_description.clone(),
                confidence: best_similarity,
                cluster_id: cluster.cluster_id,
            },
            _ => IntentClassification::unknown(),
        }
    }

    /// Find an existing cluster that's similar to the given centroid
    fn find_similar_cluster(clusters: &[IntentCluster], centroid: &[f32]) -> Option<(usize, f32)> {
        let mut best_idx = None;
        let mut best_similarity = f32::NEG_INFINITY;

        for (idx, cluster) in clusters.iter().enumerate() {
            let similarity = cosine_similarity(centroid, &cluster.centroid);
            if similarity > best_similarity {
                best_similarity = similarity;
                best_idx = Some(idx);
            }
        }

        best_idx.map(|idx| (idx, best_similarity))
    }

    /// Merge a new cluster into an existing one
    async fn merge_into_cluster(
        &self,
        existing: &mut IntentCluster,
        new_cluster: &Cluster,
    ) -> Result<(), OxyError> {
        // Update centroid (weighted average based on sample questions count)
        let existing_weight = existing.sample_questions.len().max(1) as f32;
        let new_weight = new_cluster.questions.len() as f32;
        let total_weight = existing_weight + new_weight;

        let mut new_centroid = vec![0.0f32; existing.centroid.len()];
        for (i, (e, n)) in existing
            .centroid
            .iter()
            .zip(new_cluster.centroid.iter())
            .enumerate()
        {
            new_centroid[i] = (e * existing_weight + n * new_weight) / total_weight;
        }

        existing.centroid = new_centroid;

        // Add new sample questions (keep up to 10)
        for q in &new_cluster.questions {
            if existing.sample_questions.len() < 10 && !existing.sample_questions.contains(q) {
                existing.sample_questions.push(q.clone());
            }
        }

        // Store updated cluster
        self.storage.update_cluster(existing).await?;

        Ok(())
    }

    /// Label a cluster using LLM
    async fn label_cluster(&self, questions: &[String]) -> Result<(String, String), OxyError> {
        // Take sample questions for the prompt
        let sample_questions: Vec<&String> = questions.iter().take(10).collect();
        let questions_text = sample_questions
            .iter()
            .enumerate()
            .map(|(i, q)| format!("{}. {}", i + 1, q))
            .collect::<Vec<_>>()
            .join("\n");

        let system_prompt = r#"You are an expert at analyzing user questions and identifying their intent.
Your task is to identify the common intent shared by a group of similar questions.

Respond with a JSON object containing:
- "intent_name": A short snake_case identifier (e.g., "data_query", "trend_analysis", "schema_exploration")
- "intent_description": A brief description of what these questions are trying to accomplish

Respond ONLY with the JSON object, no other text."#;

        let user_prompt = format!(
            "These questions are grouped together because they are semantically similar. What intent do they share?\n\n{}",
            questions_text
        );

        let messages: Vec<ChatCompletionRequestMessage> = vec![
            ChatCompletionRequestMessage::System(ChatCompletionRequestSystemMessage {
                content: ChatCompletionRequestSystemMessageContent::Text(system_prompt.to_string()),
                name: None,
            }),
            ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessage {
                content: ChatCompletionRequestUserMessageContent::Text(user_prompt),
                name: None,
            }),
        ];

        let request = CreateChatCompletionRequestArgs::default()
            .model(&self.config.labeling_model)
            .messages(messages)
            .build()
            .map_err(|e| OxyError::RuntimeError(format!("Failed to build LLM request: {e}")))?;

        let response = self
            .llm_client
            .chat()
            .create(request)
            .await
            .map_err(|e| OxyError::RuntimeError(format!("LLM request failed: {e}")))?;

        let content = response
            .choices
            .first()
            .and_then(|c| c.message.content.as_ref())
            .ok_or_else(|| OxyError::RuntimeError("No response from LLM".to_string()))?;

        // Parse the JSON response
        self.parse_label_response(content)
    }

    /// Parse the LLM's JSON response for intent label
    fn parse_label_response(&self, content: &str) -> Result<(String, String), OxyError> {
        // Try to extract JSON from the response
        let json_str = if content.contains('{') {
            let start = content.find('{').unwrap();
            let end = content.rfind('}').unwrap_or(content.len() - 1) + 1;
            &content[start..end]
        } else {
            content
        };

        #[derive(serde::Deserialize)]
        struct LabelResponse {
            intent_name: String,
            intent_description: String,
        }

        match serde_json::from_str::<LabelResponse>(json_str) {
            Ok(response) => Ok((response.intent_name, response.intent_description)),
            Err(e) => {
                debug!("Failed to parse LLM response: {} - content: {}", e, content);
                // Fallback: generate a generic label
                Ok((
                    "unlabeled".to_string(),
                    "Cluster could not be automatically labeled".to_string(),
                ))
            }
        }
    }
}
