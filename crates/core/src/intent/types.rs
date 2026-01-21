//! Types for intent classification

use serde::{Deserialize, Serialize};

/// Configuration for intent classification
#[derive(Debug, Clone)]
pub struct IntentConfig {
    /// ClickHouse connection URL
    pub clickhouse_url: String,
    /// ClickHouse user
    pub clickhouse_user: String,
    /// ClickHouse password
    pub clickhouse_password: String,
    /// ClickHouse database name
    pub clickhouse_database: String,
    /// OpenAI API key for embeddings and LLM labeling
    pub openai_api_key: String,
    /// Embedding model to use (default: text-embedding-3-small)
    pub embed_model: String,
    /// Embedding dimensions (default: 1536)
    pub embed_dims: usize,
    /// Minimum cluster size for HDBSCAN
    pub min_cluster_size: usize,
    /// Model for LLM labeling
    pub labeling_model: String,
    /// Confidence threshold below which questions trigger incremental learning
    pub learning_confidence_threshold: f32,
    /// Number of pending items to trigger mini-clustering
    pub learning_pool_threshold: usize,
    /// Similarity threshold for merging into existing clusters
    pub cluster_merge_threshold: f32,
}

impl Default for IntentConfig {
    fn default() -> Self {
        Self {
            clickhouse_url: "http://localhost:8123".to_string(),
            clickhouse_user: "default".to_string(),
            clickhouse_password: String::new(),
            clickhouse_database: "otel".to_string(),
            openai_api_key: String::new(),
            embed_model: "text-embedding-3-small".to_string(),
            embed_dims: 1536,
            min_cluster_size: 10,
            labeling_model: "gpt-4o-mini".to_string(),
            learning_confidence_threshold: 0.5,
            learning_pool_threshold: 10,
            cluster_merge_threshold: 0.7,
        }
    }
}

impl IntentConfig {
    /// Create config from environment variables
    pub fn from_env() -> Self {
        Self {
            clickhouse_url: std::env::var("OXY_CLICKHOUSE_URL")
                .unwrap_or_else(|_| "http://localhost:8123".to_string()),
            clickhouse_user: std::env::var("OXY_CLICKHOUSE_USER")
                .unwrap_or_else(|_| "default".to_string()),
            clickhouse_password: std::env::var("OXY_CLICKHOUSE_PASSWORD").unwrap_or_default(),
            clickhouse_database: std::env::var("OXY_CLICKHOUSE_DATABASE")
                .unwrap_or_else(|_| "otel".to_string()),
            openai_api_key: std::env::var("OPENAI_API_KEY").unwrap_or_default(),
            embed_model: std::env::var("INTENT_EMBED_MODEL")
                .unwrap_or_else(|_| "text-embedding-3-small".to_string()),
            embed_dims: std::env::var("INTENT_EMBED_DIMS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1536),
            min_cluster_size: std::env::var("INTENT_MIN_CLUSTER_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10),
            labeling_model: std::env::var("INTENT_LABELING_MODEL")
                .unwrap_or_else(|_| "gpt-4o-mini".to_string()),
            learning_confidence_threshold: std::env::var("INTENT_LEARNING_CONFIDENCE_THRESHOLD")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.5),
            learning_pool_threshold: std::env::var("INTENT_LEARNING_POOL_THRESHOLD")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(50),
            cluster_merge_threshold: std::env::var("INTENT_CLUSTER_MERGE_THRESHOLD")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.7),
        }
    }
}

/// A question with its embedding vector
#[derive(Debug, Clone)]
pub struct QuestionEmbedding {
    pub trace_id: String,
    pub question: String,
    pub embedding: Vec<f32>,
}

/// A cluster of similar questions
#[derive(Debug, Clone)]
pub struct Cluster {
    pub id: i32,
    pub embeddings: Vec<Vec<f32>>,
    pub questions: Vec<String>,
    pub centroid: Vec<f32>,
}

impl Cluster {
    /// Calculate the centroid (mean) of all embeddings in the cluster
    pub fn calculate_centroid(embeddings: &[Vec<f32>]) -> Vec<f32> {
        if embeddings.is_empty() {
            return vec![];
        }
        let dims = embeddings[0].len();
        let mut centroid = vec![0.0f32; dims];
        for embedding in embeddings {
            for (i, val) in embedding.iter().enumerate() {
                centroid[i] += val;
            }
        }
        let count = embeddings.len() as f32;
        for val in &mut centroid {
            *val /= count;
        }
        centroid
    }
}

/// An intent cluster stored in ClickHouse
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentCluster {
    pub cluster_id: u32,
    pub intent_name: String,
    pub intent_description: String,
    pub centroid: Vec<f32>,
    pub sample_questions: Vec<String>,
}

/// Special cluster ID for the "unknown" cluster
pub const UNKNOWN_CLUSTER_ID: u32 = 0;

impl IntentCluster {
    /// Create an "unknown" cluster for outlier questions
    pub fn unknown(embed_dims: usize) -> Self {
        Self {
            cluster_id: UNKNOWN_CLUSTER_ID,
            intent_name: "unknown".to_string(),
            intent_description: "Could not classify this question".to_string(),
            centroid: vec![0.0; embed_dims],
            sample_questions: vec![],
        }
    }

    /// Check if this is the unknown cluster
    pub fn is_unknown(&self) -> bool {
        self.cluster_id == UNKNOWN_CLUSTER_ID
    }
}

/// Result of classifying a question
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentClassification {
    pub intent_name: String,
    pub intent_description: String,
    pub confidence: f32,
    pub cluster_id: u32,
}

impl IntentClassification {
    /// Create an "unknown" classification for outliers
    pub fn unknown() -> Self {
        Self {
            intent_name: "unknown".to_string(),
            intent_description: "Could not classify this question".to_string(),
            confidence: 0.0,
            cluster_id: UNKNOWN_CLUSTER_ID,
        }
    }

    /// Check if this is an unknown classification
    pub fn is_unknown(&self) -> bool {
        self.cluster_id == UNKNOWN_CLUSTER_ID
    }
}

/// Analytics data for intent distribution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentAnalytics {
    pub intent_name: String,
    pub count: u64,
    pub percentage: f64,
}

/// Result of running the clustering pipeline
#[derive(Debug, Clone)]
pub struct PipelineResult {
    pub questions_processed: usize,
    pub clusters_created: usize,
    pub outliers_count: usize,
}

/// A pending item waiting for incremental clustering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingItem {
    pub trace_id: String,
    pub question: String,
    pub embedding: Vec<f32>,
    pub created_at: i64,
}

/// Result of incremental learning
#[derive(Debug, Clone)]
pub struct IncrementalResult {
    /// Number of unknown items processed
    pub items_processed: usize,
    /// Number of new clusters created
    pub new_clusters: usize,
    /// Number of items merged into existing clusters
    pub merged_count: usize,
    /// Number of items that remain as outliers
    pub outliers_count: usize,
}
