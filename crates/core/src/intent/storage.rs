//! ClickHouse storage for intent classification data

use clickhouse::Row;
use serde::{Deserialize, Serialize};

use crate::storage::{ClickHouseConfig, ClickHouseStorage};
use oxy_shared::errors::OxyError;

use super::types::{IntentAnalytics, IntentClassification, IntentCluster, IntentConfig};

/// Storage client for intent classification data
pub struct IntentStorage {
    storage: ClickHouseStorage,
}

impl std::fmt::Debug for IntentStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IntentStorage")
            .field("storage", &self.storage)
            .finish()
    }
}

/// Row type for reading questions from traces
#[derive(Debug, Row, Deserialize)]
struct QuestionRow {
    #[serde(rename = "TraceId")]
    trace_id: String,
    #[serde(rename = "Question")]
    question: String,
    #[serde(rename = "Source")]
    source: String,
}

/// Row type for intent clusters
#[derive(Debug, Row, Serialize, Deserialize)]
struct ClusterRow {
    #[serde(rename = "ClusterId")]
    cluster_id: u32,
    #[serde(rename = "IntentName")]
    intent_name: String,
    #[serde(rename = "IntentDescription")]
    intent_description: String,
    #[serde(rename = "Centroid")]
    centroid: Vec<f32>,
    #[serde(rename = "SampleQuestions")]
    sample_questions: Vec<String>,
}

/// Row type for analytics
#[derive(Debug, Row, Deserialize)]
struct AnalyticsRow {
    #[serde(rename = "IntentName")]
    intent_name: String,
    #[serde(rename = "Count")]
    count: u64,
}

/// Row type for classification (write)
#[derive(Debug, Row, Serialize)]
struct ClassificationWriteRow {
    #[serde(rename = "TraceId")]
    trace_id: String,
    #[serde(rename = "Question")]
    question: String,
    #[serde(rename = "ClusterId")]
    cluster_id: u32,
    #[serde(rename = "IntentName")]
    intent_name: String,
    #[serde(rename = "Confidence")]
    confidence: f32,
    #[serde(rename = "Embedding")]
    embedding: Vec<f32>,
    #[serde(rename = "SourceType")]
    source_type: String,
    #[serde(rename = "Source")]
    source: String,
}

/// Row type for classification (read)
#[derive(Debug, Row, Deserialize)]
struct ClassificationReadRow {
    #[serde(rename = "TraceId")]
    trace_id: String,
    #[serde(rename = "Question")]
    question: String,
    #[serde(rename = "ClusterId")]
    cluster_id: u32,
    #[serde(rename = "IntentName")]
    intent_name: String,
    #[serde(rename = "Confidence")]
    confidence: f32,
    #[serde(rename = "Embedding")]
    embedding: Vec<f32>,
    #[serde(rename = "SourceType")]
    source_type: String,
    #[serde(rename = "Source")]
    source: String,
}

impl IntentStorage {
    /// Create a new storage client
    pub fn new(config: &IntentConfig) -> Self {
        let ch_config = ClickHouseConfig {
            url: config.clickhouse_url.clone(),
            user: config.clickhouse_user.clone(),
            password: config.clickhouse_password.clone(),
            database: config.clickhouse_database.clone(),
        };
        let storage = ClickHouseStorage::new(ch_config);

        Self { storage }
    }

    /// Create from environment variables
    pub fn from_env() -> Self {
        Self {
            storage: ClickHouseStorage::from_env(),
        }
    }

    /// Get a reference to the underlying ClickHouse storage
    pub fn clickhouse_storage(&self) -> &ClickHouseStorage {
        &self.storage
    }

    /// Fetch unprocessed questions from traces
    pub async fn fetch_questions(
        &self,
        limit: usize,
    ) -> Result<Vec<(String, String, String)>, OxyError> {
        let query = format!(
            r#"
            SELECT DISTINCT
                TraceId,
                SpanAttributes['agent.prompt'] as Question,
                SpanAttributes['agent.ref'] as Source
            FROM otel_traces
            WHERE SpanName = 'agent.run_agent'
              AND SpanAttributes['agent.prompt'] != ''
              AND TraceId NOT IN (SELECT TraceId FROM intent_classifications)
            LIMIT {}
            "#,
            limit
        );

        let rows: Vec<QuestionRow> = self
            .storage
            .client()
            .query(&query)
            .fetch_all()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to fetch questions: {e}")))?;

        Ok(rows
            .into_iter()
            .map(|r| (r.trace_id, r.question, r.source))
            .collect())
    }

    /// Load all embeddings from intent_classifications for clustering
    pub async fn load_embeddings(
        &self,
    ) -> Result<Vec<(String, String, Vec<f32>, String, String)>, OxyError> {
        let rows: Vec<ClassificationReadRow> = self
            .storage
            .client()
            .query("SELECT TraceId, Question, ClusterId, IntentName, Confidence, Embedding, SourceType, Source FROM intent_classifications")
            .fetch_all()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to load embeddings: {e}")))?;

        Ok(rows
            .into_iter()
            .map(|r| (r.trace_id, r.question, r.embedding, r.source_type, r.source))
            .collect())
    }

    /// Store intent clusters (replaces existing)
    pub async fn store_clusters(&self, clusters: &[IntentCluster]) -> Result<(), OxyError> {
        if clusters.is_empty() {
            return Ok(());
        }

        // Clear existing clusters first
        self.storage
            .client()
            .query("TRUNCATE TABLE intent_clusters")
            .execute()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to truncate clusters: {e}")))?;

        let mut insert = self
            .storage
            .client()
            .insert("intent_clusters")
            .map_err(|e| OxyError::RuntimeError(format!("Failed to prepare insert: {e}")))?;

        for cluster in clusters {
            insert
                .write(&ClusterRow {
                    cluster_id: cluster.cluster_id,
                    intent_name: cluster.intent_name.clone(),
                    intent_description: cluster.intent_description.clone(),
                    centroid: cluster.centroid.clone(),
                    sample_questions: cluster.sample_questions.clone(),
                })
                .await
                .map_err(|e| OxyError::RuntimeError(format!("Failed to write cluster: {e}")))?;
        }

        insert
            .end()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to finish insert: {e}")))?;

        Ok(())
    }

    /// Load all clusters from storage
    pub async fn load_clusters(&self) -> Result<Vec<IntentCluster>, OxyError> {
        let rows: Vec<ClusterRow> = self
            .storage
            .client()
            .query(
                "SELECT ClusterId, IntentName, IntentDescription, Centroid, SampleQuestions
                 FROM intent_clusters FINAL",
            )
            .fetch_all()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to load clusters: {e}")))?;

        Ok(rows
            .into_iter()
            .map(|r| IntentCluster {
                cluster_id: r.cluster_id,
                intent_name: r.intent_name,
                intent_description: r.intent_description,
                centroid: r.centroid,
                sample_questions: r.sample_questions,
            })
            .collect())
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
        // Use 0 for unknown/outlier classifications (u32::MAX means unknown)
        let cluster_id = if classification.cluster_id == u32::MAX {
            0u32
        } else {
            classification.cluster_id
        };

        let mut insert = self
            .storage
            .client()
            .insert("intent_classifications")
            .map_err(|e| OxyError::RuntimeError(format!("Failed to prepare insert: {e}")))?;

        insert
            .write(&ClassificationWriteRow {
                trace_id: trace_id.to_string(),
                question: question.to_string(),
                cluster_id,
                intent_name: classification.intent_name.clone(),
                confidence: classification.confidence,
                embedding: embedding.to_vec(),
                source_type: source_type.to_string(),
                source: source.to_string(),
            })
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to write classification: {e}")))?;

        insert
            .end()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to finish insert: {e}")))?;

        Ok(())
    }

    /// Update an existing classification (delete old and insert new)
    ///
    /// This is used after incremental clustering to update previously unknown classifications
    pub async fn update_classification(
        &self,
        trace_id: &str,
        question: &str,
        classification: &IntentClassification,
        embedding: &[f32],
        source_type: &str,
        source: &str,
    ) -> Result<(), OxyError> {
        // Delete the old classification for this trace_id
        let delete_query = format!(
            "ALTER TABLE intent_classifications DELETE WHERE TraceId = '{}'",
            trace_id.replace('\'', "\\'")
        );

        self.storage
            .client()
            .query(&delete_query)
            .execute()
            .await
            .map_err(|e| {
                OxyError::RuntimeError(format!("Failed to delete old classification: {e}"))
            })?;

        // Insert the new classification
        self.store_classification(
            trace_id,
            question,
            classification,
            embedding,
            source_type,
            source,
        )
        .await
    }

    /// Get intent analytics for the last N days
    pub async fn get_analytics(&self, days: u32) -> Result<Vec<IntentAnalytics>, OxyError> {
        let query = format!(
            r#"
            SELECT 
                IntentName,
                count() as Count
            FROM intent_classifications
            WHERE ClassifiedAt >= now64() - INTERVAL {} DAY
            GROUP BY IntentName
            ORDER BY Count DESC
            "#,
            days
        );

        let rows: Vec<AnalyticsRow> = self
            .storage
            .client()
            .query(&query)
            .fetch_all()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to get analytics: {e}")))?;

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
        let query = format!(
            r#"
            SELECT TraceId, Question
            FROM intent_classifications
            WHERE IntentName = 'unknown' OR Confidence < 0.5
            LIMIT {}
            "#,
            limit
        );

        let rows: Vec<QuestionRow> = self
            .storage
            .client()
            .query(&query)
            .fetch_all()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to get outliers: {e}")))?;

        Ok(rows.into_iter().map(|r| (r.trace_id, r.question)).collect())
    }

    /// Load unknown classifications for incremental clustering
    pub async fn load_unknown_classifications(
        &self,
    ) -> Result<Vec<(String, String, Vec<f32>, String)>, OxyError> {
        let rows: Vec<ClassificationReadRow> = self
            .storage
            .client()
            .query(
                "SELECT TraceId, Question, ClusterId, IntentName, Confidence, Embedding, SourceType, Source FROM intent_classifications WHERE IntentName = 'unknown'",
            )
            .fetch_all()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to load unknown classifications: {e}")))?;

        Ok(rows
            .into_iter()
            .map(|r| (r.trace_id, r.question, r.embedding, r.source))
            .collect())
    }

    /// Get the count of unknown classifications
    pub async fn get_unknown_count(&self) -> Result<usize, OxyError> {
        #[derive(Debug, Row, Deserialize)]
        struct CountRow {
            count: u64,
        }

        let rows: Vec<CountRow> = self
            .storage
            .client()
            .query(
                "SELECT count() as count FROM intent_classifications WHERE IntentName = 'unknown'",
            )
            .fetch_all()
            .await
            .map_err(|e| {
                OxyError::RuntimeError(format!("Failed to count unknown classifications: {e}"))
            })?;

        Ok(rows.first().map(|r| r.count as usize).unwrap_or(0))
    }

    /// Update an existing cluster (merge new items)
    pub async fn update_cluster(&self, cluster: &IntentCluster) -> Result<(), OxyError> {
        // Use INSERT with ReplacingMergeTree - it will replace based on ClusterId
        let mut insert = self
            .storage
            .client()
            .insert("intent_clusters")
            .map_err(|e| {
                OxyError::RuntimeError(format!("Failed to prepare cluster update: {e}"))
            })?;

        insert
            .write(&ClusterRow {
                cluster_id: cluster.cluster_id,
                intent_name: cluster.intent_name.clone(),
                intent_description: cluster.intent_description.clone(),
                centroid: cluster.centroid.clone(),
                sample_questions: cluster.sample_questions.clone(),
            })
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to write cluster: {e}")))?;

        insert
            .end()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to finish cluster update: {e}")))?;

        Ok(())
    }

    /// Get the next available cluster ID
    pub async fn get_next_cluster_id(&self) -> Result<u32, OxyError> {
        #[derive(Debug, Row, Deserialize)]
        struct MaxRow {
            max_id: u32,
        }

        let rows: Vec<MaxRow> = self
            .storage
            .client()
            .query("SELECT max(ClusterId) as max_id FROM intent_clusters")
            .fetch_all()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to get max cluster ID: {e}")))?;

        let max_id = rows.first().map(|r| r.max_id).unwrap_or(0);

        let next_id = max_id.saturating_add(1);

        Ok(next_id)
    }
}
