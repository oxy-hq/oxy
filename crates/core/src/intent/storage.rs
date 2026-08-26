//! Storage for intent classification data backed by ObservabilityStore

use std::sync::Arc;

use oxy_observability::ObservabilityStore;
use oxy_shared::errors::OxyError;

use super::types::{IntentAnalytics, IntentClassification, IntentCluster, IntentConfig};

/// Storage client for intent classification data
pub struct IntentStorage {
    storage: Arc<dyn ObservabilityStore>,
}

impl std::fmt::Debug for IntentStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IntentStorage")
            .field("storage", &self.storage)
            .finish()
    }
}

impl IntentStorage {
    /// Create a new storage client.
    /// Returns an error if observability storage has not been initialized —
    /// i.e. `OXY_OBSERVABILITY_BACKEND` is unset or names a removed backend.
    /// (NOT `--enterprise`, which does not gate observability;
    /// [`oxy_observability::global::require_global`] carries the full message.)
    pub fn new(_config: &IntentConfig) -> Result<Self, OxyError> {
        let storage = oxy_observability::global::require_global()?.clone();
        Ok(Self { storage })
    }

    /// Create from the global observability storage singleton.
    /// Returns an error if observability storage has not been initialized.
    pub fn from_env() -> Result<Self, OxyError> {
        let storage = oxy_observability::global::require_global()?.clone();
        Ok(Self { storage })
    }

    /// Fetch unprocessed questions from traces
    pub async fn fetch_questions(
        &self,
        limit: usize,
    ) -> Result<Vec<(String, String, String)>, OxyError> {
        self.storage.fetch_unprocessed_questions(limit).await
    }

    /// Load all embeddings from intent_classifications for clustering
    pub async fn load_embeddings(
        &self,
    ) -> Result<Vec<(String, String, Vec<f32>, String, String)>, OxyError> {
        self.storage.load_embeddings().await
    }

    /// Store intent clusters (replaces existing)
    pub async fn store_clusters(&self, clusters: &[IntentCluster]) -> Result<(), OxyError> {
        self.storage.store_clusters(clusters).await
    }

    pub async fn load_clusters(&self) -> Result<Vec<IntentCluster>, OxyError> {
        self.storage.load_clusters().await
    }

    /// Store a classification result with embedding
    pub async fn store_classification(
        &self,
        trace_id: &str,
        question: &str,
        classification: &IntentClassification,
        embedding: &[f32],
        source_type: &str,
        source: &str,
    ) -> Result<(), OxyError> {
        let cluster_id = if classification.cluster_id == u32::MAX {
            0u32
        } else {
            classification.cluster_id
        };
        self.storage
            .store_classification(
                trace_id,
                question,
                cluster_id,
                &classification.intent_name,
                classification.confidence,
                embedding,
                source_type,
                source,
            )
            .await
    }

    /// Update an existing classification (delete old and insert new)
    pub async fn update_classification(
        &self,
        trace_id: &str,
        question: &str,
        classification: &IntentClassification,
        embedding: &[f32],
        source_type: &str,
        source: &str,
    ) -> Result<(), OxyError> {
        let cluster_id = if classification.cluster_id == u32::MAX {
            0u32
        } else {
            classification.cluster_id
        };
        self.storage
            .update_classification(
                trace_id,
                question,
                cluster_id,
                &classification.intent_name,
                classification.confidence,
                embedding,
                source_type,
                source,
            )
            .await
    }

    pub async fn get_analytics(&self, days: u32) -> Result<Vec<IntentAnalytics>, OxyError> {
        let rows = self.storage.get_intent_analytics(days).await?;

        let total: u64 = rows.iter().map(|r| r.count).sum();
        let total_f = total as f64;

        Ok(rows
            .into_iter()
            .map(|r| IntentAnalytics {
                intent_name: r.intent_name,
                count: r.count,
                percentage: if total > 0 {
                    (r.count as f64 / total_f) * 100.0
                } else {
                    0.0
                },
            })
            .collect())
    }

    /// Get outlier questions (not classified or low confidence)
    pub async fn get_outliers(&self, limit: usize) -> Result<Vec<(String, String)>, OxyError> {
        self.storage.get_outliers(limit).await
    }

    pub async fn load_unknown_classifications(
        &self,
    ) -> Result<Vec<(String, String, Vec<f32>, String)>, OxyError> {
        self.storage.load_unknown_classifications().await
    }

    pub async fn get_unknown_count(&self) -> Result<usize, OxyError> {
        self.storage.get_unknown_count().await
    }

    /// Update an existing cluster (merge new items)
    pub async fn update_cluster(&self, cluster: &IntentCluster) -> Result<(), OxyError> {
        self.storage.update_cluster_record(cluster).await
    }

    pub async fn get_next_cluster_id(&self) -> Result<u32, OxyError> {
        self.storage.get_next_cluster_id().await
    }
}
