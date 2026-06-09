use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct AnomalyRecord {
    pub id: Uuid,
    pub measure: String,
    pub time_dimension: String,
    pub granularity: String,
    /// ISO 8601 timestamp string
    pub period_start: String,
    /// ISO 8601 timestamp string
    pub period_end: String,
    pub observed: f64,
    pub expected: f64,
    pub lower: f64,
    pub upper: f64,
    pub z_score: f64,
    pub severity: String,
    pub status: String,
}

#[derive(Debug, Default, Clone)]
pub struct AnomalyFilter {
    pub measure: Option<String>,
    pub time_dimension: Option<String>,
    pub granularity: Option<String>,
    pub period_start_gte: Option<String>,
    pub period_end_lte: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DetectAndUpsertResult {
    pub anomalies: Vec<AnomalyRecord>,
    pub total_observations: usize,
    /// Set when there are not enough observations to run detection.
    pub message: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum AnomalyStoreError {
    #[error("database error: {0}")]
    Db(String),
    #[error("detection error: {0}")]
    Detection(String),
    #[error("not found: {0}")]
    NotFound(String),
}

#[async_trait::async_trait]
pub trait AnomalyStore: Send + Sync {
    async fn list(
        &self,
        workspace_id: Uuid,
        filter: AnomalyFilter,
    ) -> Result<Vec<AnomalyRecord>, AnomalyStoreError>;

    async fn get(
        &self,
        id: Uuid,
        workspace_id: Uuid,
    ) -> Result<Option<AnomalyRecord>, AnomalyStoreError>;

    /// Run anomaly detection on raw `(timestamp, value)` observations, persist
    /// any flagged anomalies, and return them. The implementation calls
    /// `metric_monitoring::detect::detect()` internally.
    async fn detect_and_upsert(
        &self,
        workspace_id: Uuid,
        measure: &str,
        time_dimension: &str,
        granularity: &str,
        observations: Vec<(String, f64)>,
    ) -> Result<DetectAndUpsertResult, AnomalyStoreError>;

    async fn get_explain_cache(
        &self,
        id: Uuid,
        workspace_id: Uuid,
    ) -> Result<Option<Value>, AnomalyStoreError>;

    async fn set_explain_cache(
        &self,
        id: Uuid,
        workspace_id: Uuid,
        result: Value,
    ) -> Result<(), AnomalyStoreError>;
}
