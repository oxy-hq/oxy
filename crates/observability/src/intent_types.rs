//! Intent classification types shared between the classifier (in core) and
//! observability backends.
//!
//! Only the persistence-facing types live here; configuration and transient
//! types (e.g. `IntentConfig`, `Cluster`, `PendingItem`) remain in
//! `oxy::intent::types` where they belong with the classifier logic.

use serde::{Deserialize, Serialize};

/// Special cluster ID for the "unknown" cluster.
pub const UNKNOWN_CLUSTER_ID: u32 = 0;

/// An intent cluster stored in an observability backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentCluster {
    pub cluster_id: u32,
    pub intent_name: String,
    pub intent_description: String,
    pub centroid: Vec<f32>,
    pub sample_questions: Vec<String>,
}

impl IntentCluster {
    /// Create an "unknown" cluster for outlier questions.
    pub fn unknown(embed_dims: usize) -> Self {
        Self {
            cluster_id: UNKNOWN_CLUSTER_ID,
            intent_name: "unknown".to_string(),
            intent_description: "Could not classify this question".to_string(),
            centroid: vec![0.0; embed_dims],
            sample_questions: vec![],
        }
    }

    /// Check if this is the unknown cluster.
    pub fn is_unknown(&self) -> bool {
        self.cluster_id == UNKNOWN_CLUSTER_ID
    }
}

/// Result of classifying a question against known clusters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentClassification {
    pub intent_name: String,
    pub intent_description: String,
    pub confidence: f32,
    pub cluster_id: u32,
}

impl IntentClassification {
    /// Create an "unknown" classification for outliers.
    pub fn unknown() -> Self {
        Self {
            intent_name: "unknown".to_string(),
            intent_description: "Could not classify this question".to_string(),
            confidence: 0.0,
            cluster_id: UNKNOWN_CLUSTER_ID,
        }
    }

    /// Check if this is an unknown classification.
    pub fn is_unknown(&self) -> bool {
        self.cluster_id == UNKNOWN_CLUSTER_ID
    }
}

/// Analytics data for intent distribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentAnalytics {
    pub intent_name: String,
    pub count: u64,
    pub percentage: f64,
}
