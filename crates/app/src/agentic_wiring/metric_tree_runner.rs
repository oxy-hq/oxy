//! Concrete `MetricTreeRunner` for Oxy.
//!
//! Loads the workspace's semantic layer from disk, compiles airlayer
//! `QueryRequest`s through `SemanticEngine`, and executes the resulting SQL
//! through Oxy's connector pool. Used by both the HTTP `/semantic/metric-tree`
//! handlers and the agentic analytics tools — same code path, identical
//! resolution semantics.
//!
//! Per-run cost notes (see `metric-tree.md` for the algorithmic context): a
//! single `explain` call can fire 100+ queries against the warehouse. Each
//! query goes through `Handle::block_on` because airlayer's `QueryExecutor`
//! type is synchronous; the wrapping `spawn_blocking` is provided by the
//! caller (HTTP handler) so the runtime thread isn't blocked.

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use agentic_analytics::{MetricTreeRunner, MetricTreeRunnerError};
use agentic_semantic::refresh_key_cache::RefreshKeyCache;
use airlayer::DatabaseConfig;
use airlayer::SemanticLayer;
use airlayer::engine::EngineError;
use airlayer::engine::metric_tree::MetricTree;
use airlayer::engine::metric_tree_ops::{
    self, ExplainConfig, ExplainResult, OpportunityResult, QueryExecutor,
};
use airlayer::engine::query::QueryRequest;
use async_trait::async_trait;
use entity::workspace_members::WorkspaceRole;
use oxy::adapters::workspace::manager::WorkspaceManager;
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::agentic_wiring::OxyProjectContext;

/// Adapter that exposes Oxy's semantic-layer + connector pool as a
/// [`MetricTreeRunner`].
///
/// Holds the smallest set of fields needed to (a) load the on-disk semantic
/// layer and (b) execute SQL through `run_via_agentic_connector`. Built once
/// per pipeline run from [`crate::agentic_wiring::OxyProjectContext`].
pub struct OxyMetricTreeRunner {
    workspace_manager: WorkspaceManager,
    user_id: Uuid,
    role: WorkspaceRole,
    preagg_cache: Option<Arc<RwLock<RefreshKeyCache>>>,
    preagg_renewal_threshold_secs: u64,
}

impl OxyMetricTreeRunner {
    pub fn new(workspace_manager: WorkspaceManager, user_id: Uuid, role: WorkspaceRole) -> Self {
        Self {
            workspace_manager,
            user_id,
            role,
            preagg_cache: None,
            preagg_renewal_threshold_secs: 120,
        }
    }

    pub fn with_preagg(
        mut self,
        cache: Option<Arc<RwLock<RefreshKeyCache>>>,
        renewal_threshold_secs: u64,
    ) -> Self {
        self.preagg_cache = cache;
        self.preagg_renewal_threshold_secs = renewal_threshold_secs;
        self
    }

    /// Load the workspace's semantic layer from disk. Same scan path the
    /// analytics catalog uses, so the agent and the metric-tree ops see the
    /// same set of views.
    pub fn load_layer_sync(
        workspace_manager: &WorkspaceManager,
    ) -> Result<SemanticLayer, MetricTreeRunnerError> {
        let scan_path = workspace_manager.config_manager.semantics_scan_path();
        oxy_airlayer_compat::load_layer_from_dir(&scan_path)
            .map_err(|e| MetricTreeRunnerError::LayerLoad(e.to_string()))
    }

    /// List configured databases as airlayer `DatabaseConfig`s. The
    /// `DatasourceDialectMap` is built from these by the caller.
    pub fn list_databases_sync(workspace_manager: &WorkspaceManager) -> Vec<DatabaseConfig> {
        workspace_manager
            .config_manager
            .list_databases()
            .iter()
            .map(|db| DatabaseConfig {
                name: db.name.clone(),
                db_type: db.database_type.to_string(),
            })
            .collect()
    }
}

#[async_trait]
impl MetricTreeRunner for OxyMetricTreeRunner {
    async fn load_layer(&self) -> Result<SemanticLayer, MetricTreeRunnerError> {
        Self::load_layer_sync(&self.workspace_manager)
    }

    async fn list_databases(&self) -> Vec<DatabaseConfig> {
        Self::list_databases_sync(&self.workspace_manager)
    }

    async fn run_explain(
        &self,
        target: String,
        time_dimension: String,
        current_period: (String, String),
        previous_period: (String, String),
        config: ExplainConfig,
    ) -> Result<ExplainResult, MetricTreeRunnerError> {
        let inputs = self.snapshot_for_blocking().await?;
        let total_start = std::time::Instant::now();
        let target_for_log = target.clone();
        // QueryExecutor isn't Send, so build + consume entirely inside
        // spawn_blocking. All inputs (engine, databases, workspace_manager,
        // tokio handle, the period tuples) are Send.
        let result = tokio::task::spawn_blocking(move || {
            let RunInputs {
                layer,
                databases,
                engine,
                workspace_manager,
                user_id,
                role,
                handle,
                scan_path,
                preagg_cache,
                preagg_renewal_threshold_secs,
            } = inputs;
            // Strip dim-split candidates that aren't useful before passing
            // the layer to airlayer's explain. See [`prune_dims_for_explain`]
            // for the policy.
            let pruned = prune_dims_for_explain(layer);
            let tree = MetricTree::build(&pruned);
            let executor = build_query_executor(
                &target,
                engine,
                databases,
                workspace_manager,
                user_id,
                role,
                handle,
                scan_path,
                preagg_cache,
                preagg_renewal_threshold_secs,
            );
            metric_tree_ops::explain(
                &tree,
                &pruned,
                &target,
                &time_dimension,
                (current_period.0.as_str(), current_period.1.as_str()),
                (previous_period.0.as_str(), previous_period.1.as_str()),
                &config,
                &*executor,
            )
            .map_err(|e| MetricTreeRunnerError::Op(e.to_string()))
        })
        .await
        .map_err(|e| MetricTreeRunnerError::Op(format!("explain task panicked: {e}")))?;
        tracing::info!(
            target: "metric_tree.explain",
            measure = %target_for_log,
            total_ms = total_start.elapsed().as_millis() as u64,
            ok = result.is_ok(),
            "explain done"
        );
        result
    }

    async fn get_dimension_values(
        &self,
        dimension: String,
        measure: String,
        since_days: u32,
    ) -> Result<Vec<String>, MetricTreeRunnerError> {
        use airlayer::engine::query::{QueryRequest, TimeDimensionQuery};
        let inputs = self.snapshot_for_blocking().await?;
        // Derive the time dimension from the measure's view: take the view
        // prefix from `measure` (e.g. "sales_daily") and append the first
        // available date-type dimension for the lookback filter. We send a
        // dimensions-only query (no measures) so airlayer returns raw rows
        // grouped by the requested dimension. The date range prunes stale
        // segments (closed locations etc.) so only active values come back.
        let since = chrono::Utc::now() - chrono::Duration::days(since_days as i64);
        let since_str = since.format("%Y-%m-%d").to_string();
        let now_str = chrono::Utc::now().format("%Y-%m-%d").to_string();

        // Infer the view's primary time dimension from the measure id.
        // Convention: `view_name.measure` → time dim is `view_name.business_date`
        // or `view_name.created_at`. We try `business_date` first (Toast views),
        // then fall back to omitting the date filter (returns all-time values).
        let view = measure.split('.').next().unwrap_or("").to_string();
        let time_dim_candidate = format!("{view}.business_date");

        let dim_alias = dimension.replace('.', "__");

        let request_with_date = QueryRequest {
            dimensions: vec![dimension.clone()],
            time_dimensions: vec![TimeDimensionQuery {
                dimension: time_dim_candidate.clone(),
                granularity: None,
                date_range: Some(vec![since_str.clone(), now_str.clone()]),
            }],
            limit: Some(10_000),
            ..QueryRequest::new()
        };
        let request_no_date = QueryRequest {
            dimensions: vec![dimension.clone()],
            limit: Some(10_000),
            ..QueryRequest::new()
        };

        tokio::task::spawn_blocking(move || {
            let RunInputs {
                databases,
                engine,
                workspace_manager,
                user_id,
                role,
                handle,
                scan_path,
                preagg_cache,
                preagg_renewal_threshold_secs,
                ..
            } = inputs;
            let executor = build_query_executor(
                &measure,
                engine,
                databases,
                workspace_manager,
                user_id,
                role,
                handle,
                scan_path,
                preagg_cache,
                preagg_renewal_threshold_secs,
            );
            // Try with date filter first; fall back to unfiltered on error
            // (some views don't have a `business_date` dimension).
            let rows = (executor)(&request_with_date)
                .or_else(|_| (executor)(&request_no_date))
                .map_err(|e| MetricTreeRunnerError::Op(e.to_string()))?;
            let mut out: Vec<String> = Vec::with_capacity(rows.len());
            for row in rows {
                if let Some(v) = row.get(&dim_alias).and_then(|v| {
                    v.as_str()
                        .map(String::from)
                        .or_else(|| Some(v.to_string().trim_matches('"').to_string()))
                }) && !v.is_empty()
                    && v != "null"
                {
                    out.push(v);
                }
            }
            out.sort();
            out.dedup();
            Ok(out)
        })
        .await
        .map_err(|e| MetricTreeRunnerError::Op(format!("dimension-values task panicked: {e}")))?
    }

    async fn run_time_series(
        &self,
        measure: String,
        time_dimension: String,
        granularity: String,
        period: (String, String),
        filters: Vec<airlayer::engine::query::QueryFilter>,
    ) -> Result<Vec<(String, f64)>, MetricTreeRunnerError> {
        use airlayer::engine::query::{OrderBy, QueryRequest, TimeDimensionQuery};
        let inputs = self.snapshot_for_blocking().await?;
        let dim_alias = format!("{time_dimension}.{granularity}");
        let measure_alias = measure.replace('.', "__");
        let dim_alias_for_extract = dim_alias.replace('.', "__");
        let request = QueryRequest {
            measures: vec![measure.clone()],
            filters,
            time_dimensions: vec![TimeDimensionQuery {
                dimension: time_dimension.clone(),
                granularity: Some(granularity.clone()),
                date_range: Some(vec![period.0.clone(), period.1.clone()]),
            }],
            order: vec![OrderBy {
                id: dim_alias.clone(),
                desc: false,
            }],
            ..QueryRequest::new()
        };
        tokio::task::spawn_blocking(move || {
            let RunInputs {
                databases,
                engine,
                workspace_manager,
                user_id,
                role,
                handle,
                scan_path,
                preagg_cache,
                preagg_renewal_threshold_secs,
                ..
            } = inputs;
            let executor = build_query_executor(
                &measure,
                engine,
                databases,
                workspace_manager,
                user_id,
                role,
                handle,
                scan_path,
                preagg_cache,
                preagg_renewal_threshold_secs,
            );
            let rows =
                (executor)(&request).map_err(|e| MetricTreeRunnerError::Op(e.to_string()))?;
            // airlayer returns column-keyed maps with the measure aliased as
            // `view__measure` and the time bucket as `view__dim.granularity`
            // (dotted). The connector flattens dots in keys, so use the
            // pre-flattened aliases for lookups.
            let mut out: Vec<(String, f64)> = Vec::with_capacity(rows.len());
            for row in rows {
                let ts = row
                    .get(&dim_alias_for_extract)
                    .and_then(|v| v.as_str().map(String::from))
                    .or_else(|| {
                        row.get(&dim_alias_for_extract)
                            .map(|v| v.to_string().trim_matches('"').to_string())
                    })
                    .unwrap_or_default();
                let value = row
                    .get(&measure_alias)
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                if !ts.is_empty() {
                    out.push((ts, value));
                }
            }
            Ok(out)
        })
        .await
        .map_err(|e| MetricTreeRunnerError::Op(format!("time-series task panicked: {e}")))?
    }

    async fn run_scalar(
        &self,
        measure: String,
        time_dimension: String,
        period: (String, String),
        filters: Vec<airlayer::engine::query::QueryFilter>,
    ) -> Result<f64, MetricTreeRunnerError> {
        use airlayer::engine::query::{QueryRequest, TimeDimensionQuery};
        let inputs = self.snapshot_for_blocking().await?;
        let measure_alias = measure.replace('.', "__");
        // No `granularity` ⇒ the time dimension only bounds the window; the
        // warehouse returns a single aggregated row.
        let request = QueryRequest {
            measures: vec![measure.clone()],
            filters,
            time_dimensions: vec![TimeDimensionQuery {
                dimension: time_dimension,
                granularity: None,
                date_range: Some(vec![period.0.clone(), period.1.clone()]),
            }],
            ..QueryRequest::new()
        };
        tokio::task::spawn_blocking(move || {
            let RunInputs {
                databases,
                engine,
                workspace_manager,
                user_id,
                role,
                handle,
                scan_path,
                preagg_cache,
                preagg_renewal_threshold_secs,
                ..
            } = inputs;
            let executor = build_query_executor(
                &measure,
                engine,
                databases,
                workspace_manager,
                user_id,
                role,
                handle,
                scan_path,
                preagg_cache,
                preagg_renewal_threshold_secs,
            );
            let rows =
                (executor)(&request).map_err(|e| MetricTreeRunnerError::Op(e.to_string()))?;
            // Single aggregate row; the measure is aliased `view__measure`. An
            // empty window (no rows) reads as 0.0 — a SUM/COUNT of nothing.
            let value = rows
                .first()
                .and_then(|row| row.get(&measure_alias))
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            Ok(value)
        })
        .await
        .map_err(|e| MetricTreeRunnerError::Op(format!("scalar task panicked: {e}")))?
    }

    async fn run_opportunity(
        &self,
        target: String,
        time_dimension: String,
        period: (String, String),
    ) -> Result<OpportunityResult, MetricTreeRunnerError> {
        let inputs = self.snapshot_for_blocking().await?;
        tokio::task::spawn_blocking(move || {
            let RunInputs {
                layer,
                databases,
                engine,
                workspace_manager,
                user_id,
                role,
                handle,
                scan_path,
                preagg_cache,
                preagg_renewal_threshold_secs,
            } = inputs;
            let tree = MetricTree::build(&layer);
            let executor = build_query_executor(
                &target,
                engine,
                databases,
                workspace_manager,
                user_id,
                role,
                handle,
                scan_path,
                preagg_cache,
                preagg_renewal_threshold_secs,
            );
            metric_tree_ops::opportunity(
                &tree,
                &layer,
                &target,
                &time_dimension,
                (period.0.as_str(), period.1.as_str()),
                &*executor,
            )
            .map_err(|e| MetricTreeRunnerError::Op(e.to_string()))
        })
        .await
        .map_err(|e| MetricTreeRunnerError::Op(format!("opportunity task panicked: {e}")))?
    }
}

/// All the `Send` inputs needed to construct a `QueryExecutor` inside
/// `spawn_blocking`. Kept as a struct so the two query-executing ops can
/// share the same snapshot path without duplicating six trailing locals.
struct RunInputs {
    layer: SemanticLayer,
    databases: Vec<DatabaseConfig>,
    engine: airlayer::SemanticEngine,
    workspace_manager: WorkspaceManager,
    user_id: Uuid,
    role: WorkspaceRole,
    handle: tokio::runtime::Handle,
    scan_path: PathBuf,
    preagg_cache: Option<Arc<RwLock<RefreshKeyCache>>>,
    preagg_renewal_threshold_secs: u64,
}

impl OxyMetricTreeRunner {
    async fn snapshot_for_blocking(&self) -> Result<RunInputs, MetricTreeRunnerError> {
        let layer = self.load_layer().await?;
        let databases = self.list_databases().await;
        let dialects = airlayer::DatasourceDialectMap::from_config_databases(&databases);
        let engine = airlayer::SemanticEngine::from_semantic_layer(layer.clone(), dialects)
            .map_err(|e| MetricTreeRunnerError::ExecutorBuild(e.to_string()))?;
        let scan_path = self
            .workspace_manager
            .config_manager
            .semantics_scan_path()
            .to_path_buf();
        Ok(RunInputs {
            layer,
            databases,
            engine,
            workspace_manager: self.workspace_manager.clone(),
            user_id: self.user_id,
            role: self.role.clone(),
            handle: tokio::runtime::Handle::current(),
            scan_path,
            preagg_cache: self.preagg_cache.clone(),
            preagg_renewal_threshold_secs: self.preagg_renewal_threshold_secs,
        })
    }
}

/// Build a `QueryExecutor` closure: compile each `QueryRequest` to SQL via
/// airlayer, then execute through Oxy's connector. The closure runs on a
/// blocking thread, so `block_on` bridges to the async connector call.
///
/// **Connectors are cached for the lifetime of the closure.** Explain calls
/// fire 10-50 queries against the same database; without the cache each
/// query was building a fresh `OxyProjectContext` and re-opening the
/// DuckDB file. With the cache, the connector is built once on first use
/// and reused across every query, which on the orders fixture cuts an
/// explain run from ~10s down to <2s.
///
/// Extracted here so both `OxyMetricTreeRunner` and the HTTP `/explain` and
/// `/opportunity` handlers go through the exact same code path.
pub fn build_query_executor(
    _target_measure: &str,
    engine: airlayer::SemanticEngine,
    databases: Vec<DatabaseConfig>,
    workspace_manager: WorkspaceManager,
    user_id: Uuid,
    role: WorkspaceRole,
    handle: tokio::runtime::Handle,
    scan_path: PathBuf,
    preagg_cache: Option<Arc<RwLock<RefreshKeyCache>>>,
    preagg_renewal_threshold_secs: u64,
) -> Box<QueryExecutor> {
    use agentic_connector::DatabaseConnector;
    let pool: std::sync::Mutex<std::collections::HashMap<String, Vec<Arc<dyn DatabaseConnector>>>> =
        std::sync::Mutex::new(std::collections::HashMap::new());

    // Per-explain result cache: (database, sql) → rows. airlayer's
    // explain re-queries the same component measures during opposing-
    // offset detection that it already pulled in evaluate_candidates,
    // so the same SQL fires 2-3 times in a typical run. Memoize within
    // a single explain to skip the duplicates. (Upstream airlayer fix
    // tracked in metric-tree.md.)
    let result_cache: std::sync::Mutex<
        std::collections::HashMap<(String, String), Vec<Map<String, Value>>>,
    > = std::sync::Mutex::new(std::collections::HashMap::new());
    // Per-explain query counter for the trace summary. Each call increments
    // it so log lines can be correlated across the recursive search.
    let query_seq = std::sync::atomic::AtomicUsize::new(0);
    Box::new(move |request: &QueryRequest| {
        let compiled = engine.compile_query(request)?;
        // airlayer emits parameterized SQL ($1, $2, …) + a separate params
        // vector. The agentic-connector executor takes a raw SQL string, so
        // inline the params the same way `resolve_and_compile` does.
        let sql = oxy_shared::substitute_params(&compiled.sql, &compiled.params);
        let database = resolve_database(&engine, request, &databases)?;
        let seq = query_seq.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let total_start = std::time::Instant::now();

        // Result-cache hit? Save the round-trip + warehouse work entirely.
        let result_key = (database.clone(), sql.clone());
        if let Some(cached) = result_cache.lock().unwrap().get(&result_key).cloned() {
            tracing::info!(
                target: "metric_tree.explain",
                seq,
                database = %database,
                result_cache_hit = true,
                row_count = cached.len(),
                total_ms = total_start.elapsed().as_millis() as u64,
                "query (cached)"
            );
            return Ok(cached);
        }

        // Preagg path: if a covering rollup exists in the local Parquet manifest,
        // serve from DuckDB instead of the warehouse. Any failure falls through
        // silently to the warehouse path below.
        if let Some(ref preagg) = preagg_cache
            && let Some(agentic_semantic::compile::CompiledQuery::Preaggregation {
                preagg_sql,
                parquet_path,
                ..
            }) = agentic_semantic::compile::try_resolve_local_parquet(
                &scan_path,
                request,
                preagg,
                preagg_renewal_threshold_secs,
                &sql,
                &database,
            )
        {
            match execute_preagg_and_convert(&preagg_sql, &parquet_path) {
                Ok(rows) => {
                    tracing::info!(
                        target: "metric_tree.explain",
                        seq,
                        database = %database,
                        preagg = true,
                        row_count = rows.len(),
                        total_ms = total_start.elapsed().as_millis() as u64,
                        "query (preagg)"
                    );
                    result_cache
                        .lock()
                        .unwrap()
                        .insert(result_key, rows.clone());
                    return Ok(rows);
                }
                Err(e) => {
                    tracing::warn!(
                        target: "metric_tree.explain",
                        seq,
                        %e,
                        "preagg execute failed, falling back to warehouse"
                    );
                }
            }
        }

        // Pop an idle connector from the pool, or build a fresh one.
        // Threads that find the pool empty build in parallel — concurrent
        // first-batch queries each pay the build cost once, not N times.
        let pooled = pool
            .lock()
            .unwrap()
            .get_mut(&database)
            .and_then(|v| v.pop());
        let (connector, pool_hit) = if let Some(c) = pooled {
            (c, true)
        } else {
            let build_start = std::time::Instant::now();
            let ctx = OxyProjectContext::new(workspace_manager.clone())
                .with_subject(user_id)
                .with_role(role.clone());
            let built = handle
                .block_on(ctx.build_connector_for(&database))
                .map_err(|e| EngineError::QueryError(e.to_string()))?;
            tracing::info!(
                target: "metric_tree.explain",
                seq,
                database = %database,
                build_ms = build_start.elapsed().as_millis() as u64,
                "connector built (pool miss)"
            );
            (built, false)
        };

        let exec_start = std::time::Instant::now();
        let stream = handle
            .block_on(connector.execute_query_full(&sql))
            .map_err(|e| EngineError::QueryError(format!("{e:?}")))?;
        let exec_ms = exec_start.elapsed().as_millis() as u64;
        let decode_start = std::time::Instant::now();
        let rows = handle
            .block_on(crate::server::api::typed_stream::typed_stream_to_json_array(stream))
            .map_err(|e| EngineError::QueryError(format!("{e:?}")))?;
        let decode_ms = decode_start.elapsed().as_millis() as u64;
        let row_count = rows.len();
        let result = rows_to_maps(rows);
        result_cache
            .lock()
            .unwrap()
            .insert(result_key, result.clone());

        // Return the connector to the pool so the next parallel call can reuse it.
        pool.lock()
            .unwrap()
            .entry(database.clone())
            .or_default()
            .push(connector);

        tracing::info!(
            target: "metric_tree.explain",
            seq,
            database = %database,
            pool_hit,
            measures = ?request.measures,
            dimensions = ?request.dimensions,
            row_count,
            exec_ms,
            decode_ms,
            total_ms = total_start.elapsed().as_millis() as u64,
            sql_len = sql.len(),
            "query"
        );
        Ok(result)
    })
}

/// Resolve which configured database a query targets, from the datasource of
/// the first measure's view, falling back to the first workspace database.
fn resolve_database(
    engine: &airlayer::SemanticEngine,
    request: &QueryRequest,
    databases: &[DatabaseConfig],
) -> Result<String, EngineError> {
    let member = request
        .measures
        .first()
        .or_else(|| request.dimensions.first())
        .ok_or_else(|| EngineError::QueryError("query has no measures".to_string()))?;
    let view_name = member.split('.').next().unwrap_or(member.as_str());
    if let Some(datasource) = engine.view(view_name).and_then(|v| v.datasource.clone()) {
        return Ok(datasource);
    }
    databases
        .first()
        .map(|d| d.name.clone())
        .ok_or_else(|| EngineError::QueryError("no database configured".to_string()))
}

/// Convert a connector JSON response (`[header, ..rows]`, all-string cells)
/// into the column-keyed row maps airlayer's metric-tree ops expect. Numeric
/// cells are parsed back to JSON numbers so delta extraction works.
fn rows_to_maps(rows: Vec<Vec<String>>) -> Vec<Map<String, Value>> {
    let mut iter = rows.into_iter();
    let header = iter.next().unwrap_or_default();
    iter.map(|row| {
        header
            .iter()
            .zip(row)
            .map(|(key, cell)| {
                let value = if cell.is_empty() {
                    Value::Null
                } else if let Ok(n) = cell.parse::<f64>() {
                    serde_json::Number::from_f64(n)
                        .map(Value::Number)
                        .unwrap_or(Value::String(cell))
                } else {
                    Value::String(cell)
                };
                (key.clone(), value)
            })
            .collect()
    })
    .collect()
}

/// Construct an `Arc<dyn MetricTreeRunner>` from a workspace + user + role
/// triple. Used by [`crate::agentic_wiring::OxyProjectContext::metric_tree_runner`]
/// to populate the agentic platform port.
pub fn make_runner(
    workspace_manager: WorkspaceManager,
    user_id: Uuid,
    role: WorkspaceRole,
    preagg_cache: Option<Arc<RwLock<RefreshKeyCache>>>,
    preagg_renewal_threshold_secs: u64,
) -> Arc<dyn MetricTreeRunner> {
    Arc::new(
        OxyMetricTreeRunner::new(workspace_manager, user_id, role)
            .with_preagg(preagg_cache, preagg_renewal_threshold_secs),
    )
}

/// Strip dimensions from every view that are bad split candidates for
/// airlayer's `explain`. Currently drops:
///
/// - **All `Number`-typed dimensions.** Most numeric dims in real schemas
///   are either foreign keys (high cardinality → one breakdown row per
///   entity → minutes-long queries on warehouses without bucketing) or
///   continuous values (price, amount) that need bucketing to be
///   meaningful split candidates. Either way, surfacing them as raw
///   GROUP BY columns produces noisy results at high cost. Numeric
///   measures are unaffected — this only touches the dim list airlayer
///   uses for `evaluate_candidates`.
///
/// Strings + booleans pass through unchanged. The metric tree itself is
/// rebuilt against the pruned layer so component / driver edges stay
/// intact.
///
/// This is a temporary workaround until airlayer either:
/// 1. exposes a `cardinality_cap` / `splittable` flag per dim, or
/// 2. profiles dim cardinality at compile time and self-excludes high-N
///    candidates from `evaluate_candidates`.
fn prune_dims_for_explain(mut layer: SemanticLayer) -> SemanticLayer {
    use airlayer::schema::models::DimensionType;
    for view in &mut layer.views {
        view.dimensions
            .retain(|d| !matches!(d.dimension_type, DimensionType::Number));
    }
    layer
}

/// Convert a pre-aggregation DuckDB result to the column-keyed row maps that
/// airlayer's metric-tree ops expect.
///
/// Returns `Err` only when the Parquet file is missing or DuckDB fails;
/// callers log the error and fall back to the warehouse path.
/// Convert a pre-aggregation DuckDB result to the column-keyed row maps that
/// airlayer's metric-tree ops expect.
///
/// `execute_preagg_sql` returns `{ "columns": [...], "rows": [...], ... }`.
/// Returns `Err` only when the Parquet file is missing or DuckDB fails;
/// callers log the error and fall back to the warehouse path.
fn execute_preagg_and_convert(
    preagg_sql: &str,
    parquet_path: &std::path::Path,
) -> Result<Vec<Map<String, Value>>, String> {
    let json = agentic_semantic::preagg::execute_preagg_sql(preagg_sql, parquet_path)
        .map_err(|e| e.to_string())?;
    let rows = json
        .get("rows")
        .and_then(|r| r.as_array())
        .cloned()
        .unwrap_or_default();
    Ok(rows
        .into_iter()
        .filter_map(|row| match row {
            Value::Object(m) => Some(m),
            _ => None,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn execute_preagg_and_convert_returns_maps() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let parquet_path = dir.path().join("test.parquet");

        let conn = duckdb::Connection::open_in_memory().unwrap();
        conn.execute_batch(&format!(
            "COPY (SELECT 'foo' AS region, 42.0 AS revenue) TO '{}' (FORMAT PARQUET);",
            parquet_path.display()
        ))
        .unwrap();

        let preagg_sql = format!(
            "SELECT region, revenue FROM read_parquet('{}')",
            parquet_path.display()
        );

        let rows = execute_preagg_and_convert(&preagg_sql, &parquet_path)
            .expect("preagg convert should succeed");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get("region"), Some(&json!("foo")));
        let rev = rows[0].get("revenue").and_then(|v| v.as_f64());
        assert_eq!(rev, Some(42.0));
    }

    #[test]
    fn execute_preagg_and_convert_missing_file_returns_err() {
        let result = execute_preagg_and_convert(
            "SELECT 1",
            std::path::Path::new("/nonexistent/path.parquet"),
        );
        assert!(result.is_err(), "missing Parquet should return Err");
    }
}
