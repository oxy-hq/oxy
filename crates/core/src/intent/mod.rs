//! Intent Classification Module
//!
//! This module provides unsupervised intent classification for Oxy agents.
//! It uses embedding + clustering to automatically discover intent categories
//! from user questions, inspired by Langfuse's approach.
//!
//! ## Architecture
//!
//! 1. **Batch Pipeline** (offline): Collect questions → Embed → Cluster (HDBSCAN) → LLM labels → Store
//! 2. **Real-time Classification**: New question → Embed → Find nearest cluster → Return intent
//!
//! ## Usage
//!
//! ```rust,ignore
//! use oxy::intent::IntentClassifier;
//!
//! let classifier = IntentClassifier::new(config).await?;
//!
//! // Run batch clustering pipeline
//! classifier.run_pipeline().await?;
//!
//! // Classify a new question
//! let result = classifier.classify("how many users signed up?").await?;
//! println!("Intent: {} (confidence: {})", result.intent_name, result.confidence);
//!
//! // Classify with incremental learning
//! let (result, added_to_pool) = classifier.classify_with_learning("trace-123", "new question?").await?;
//! ```

mod classifier;
mod clustering;
mod embedding;
pub mod storage;
pub mod types;

// Re-export migrations from the new location for backward compatibility
pub mod migrations {
    pub use crate::storage::clickhouse::migrations::*;
}

pub use classifier::IntentClassifier;
pub use types::{
    Cluster, IncrementalResult, IntentAnalytics, IntentClassification, IntentCluster, IntentConfig,
    PendingItem, PipelineResult, QuestionEmbedding,
};
