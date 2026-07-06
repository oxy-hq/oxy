//! Cross-crate hook for executing metric-tree queries against a real warehouse.
//!
//! The metric-tree ops in `airlayer::engine::metric_tree_ops` (`explain`,
//! `opportunity`) compile airlayer `QueryRequest`s into SQL and need to run
//! that SQL against an actual database. The analytics domain doesn't know
//! how to reach Oxy's connector pool — that wiring lives in `oxy-app`
//! (`OxyProjectContext::build_connector_for`). Mirror the `SubrunRunner`
//! pattern: define a trait here, inject a concrete impl from
//! `agentic-pipeline` / `oxy-app`.
//!
//! Construction is async because building a connector talks to the
//! configured warehouse (auth, dialect resolution); execution returns the
//! synchronous `QueryExecutor` closure that airlayer's ops expect.

use airlayer::DatabaseConfig;
use airlayer::SemanticLayer;
use airlayer::engine::EngineError;
use airlayer::engine::metric_tree::MetricTree;
use airlayer::engine::metric_tree_ops::{ExplainConfig, ExplainResult, OpportunityResult};
use airlayer::engine::query::QueryFilter;
use std::sync::Arc;

/// Run airlayer metric-tree ops against a real warehouse.
///
/// The query-executing ops (`explain`, `opportunity`) need both:
/// - a synchronous `QueryExecutor` closure that talks to the connector pool, and
/// - a `spawn_blocking` wrapper so the airlayer algorithm (which is sync and
///   issues 100+ queries per call) doesn't block the runtime thread.
///
/// airlayer's `QueryExecutor` type is `dyn Fn(...)`, not `Send`, so the
/// executor must be constructed *inside* `spawn_blocking`. The trait therefore
/// exposes `run_explain` / `run_opportunity` directly rather than handing the
/// executor across an `.await` point. Pure ops (`sensitivity`, `predict`)
/// operate on the in-memory tree and only need `load_layer`.
///
/// Concrete impl: `OxyMetricTreeRunner` in `crates/app/src/agentic_wiring/`.
#[async_trait::async_trait]
pub trait MetricTreeRunner: Send + Sync {
    /// Load the workspace's semantic layer (the same scan path used by the
    /// analytics solver's catalog).
    async fn load_layer(&self) -> Result<SemanticLayer, MetricTreeRunnerError>;

    /// List configured warehouse databases (used by airlayer to resolve dialects).
    async fn list_databases(&self) -> Vec<DatabaseConfig>;

    /// Period-over-period root cause analysis. The implementor builds the
    /// `QueryExecutor` and runs `airlayer::explain` inside `spawn_blocking`.
    async fn run_explain(
        &self,
        target: String,
        time_dimension: String,
        current_period: (String, String),
        previous_period: (String, String),
        config: ExplainConfig,
    ) -> Result<ExplainResult, MetricTreeRunnerError>;

    /// Segment opportunity sizing. Same `spawn_blocking` contract as
    /// [`Self::run_explain`].
    async fn run_opportunity(
        &self,
        target: String,
        time_dimension: String,
        period: (String, String),
    ) -> Result<OpportunityResult, MetricTreeRunnerError>;

    /// Return the distinct non-null values of `dimension` observed for
    /// `measure` in the last `since_days` days. Used by the anomaly scanner
    /// to fan out a `group_by` monitor without enumerating segments in YAML.
    async fn get_dimension_values(
        &self,
        dimension: String,
        measure: String,
        since_days: u32,
    ) -> Result<Vec<String>, MetricTreeRunnerError>;

    /// Fetch a single measure's value broken down by a time dimension at the
    /// given granularity. Used by anomaly monitoring to pull the input series
    /// for the detector. Returns `(timestamp_string, value)` rows in
    /// time-ascending order.
    ///
    /// `granularity` is the airlayer string: `"day"` | `"week"` | `"month"` |
    /// `"quarter"` | `"year"`.
    ///
    /// `filters` are additional `equals` dimension filters applied before
    /// aggregation — used for per-store / per-segment monitors.
    async fn run_time_series(
        &self,
        measure: String,
        time_dimension: String,
        granularity: String,
        period: (String, String),
        filters: Vec<QueryFilter>,
    ) -> Result<Vec<(String, f64)>, MetricTreeRunnerError>;

    /// Execute a full airlayer query (window already injected by the caller as a
    /// `time_dimensions` date range) and reduce it to ONE scalar — the value of
    /// the request's single measure in the single aggregated row. Correct for
    /// any measure type (sum, average, count-distinct). Used by external-source
    /// reconciliation. Defaults to unsupported so existing runners need no change.
    async fn run_query_scalar(
        &self,
        request: airlayer::engine::query::QueryRequest,
    ) -> Result<f64, MetricTreeRunnerError> {
        let _ = request;
        Err(MetricTreeRunnerError::Op(
            "run_query_scalar not supported by this runner".to_string(),
        ))
    }
}

/// Failure modes for runner construction. Op-level errors come back as
/// `airlayer::engine::EngineError` from the executor itself.
#[derive(Debug)]
pub enum MetricTreeRunnerError {
    LayerLoad(String),
    ExecutorBuild(String),
    Op(String),
}

impl std::fmt::Display for MetricTreeRunnerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LayerLoad(e) => write!(f, "failed to load semantic layer: {e}"),
            Self::ExecutorBuild(e) => write!(f, "failed to construct query executor: {e}"),
            Self::Op(e) => write!(f, "metric-tree op failed: {e}"),
        }
    }
}

impl std::error::Error for MetricTreeRunnerError {}

impl From<MetricTreeRunnerError> for EngineError {
    fn from(value: MetricTreeRunnerError) -> Self {
        EngineError::QueryError(value.to_string())
    }
}

/// Convenience: build a `MetricTree` for the runner's current semantic layer.
/// Wrapped here so callers don't have to depend on `oxy_semantic` directly.
pub async fn load_tree(
    runner: &Arc<dyn MetricTreeRunner>,
) -> Result<(SemanticLayer, MetricTree), MetricTreeRunnerError> {
    let layer = runner.load_layer().await?;
    let tree = airlayer::engine::metric_tree::MetricTree::build(&layer);
    Ok((layer, tree))
}
