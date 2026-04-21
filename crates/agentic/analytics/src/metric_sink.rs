//! Port for recording analytics metric usage.
//!
//! The domain crate defines what it wants to record (agent, question,
//! measures + dimensions from the resolved semantic query, SQL text).
//! The adapter that writes into `oxy-observability`'s
//! `metric_usage` table lives in the composition layer (`oxy-app`) so
//! this crate stays free of infrastructure deps.
//!
//! Contract:
//! - Implementations must be fire-and-forget — the method returns
//!   synchronously and any I/O must be dispatched async without
//!   blocking the analytics pipeline.
//! - Implementations resolve `trace_id` from the current tracing span
//!   context (`oxy_observability::current_trace_id`). The analytics
//!   solver calls this while the `analytics.tool_call` span is
//!   entered, so `Span::current()` points at a span whose parent chain
//!   reaches `analytics.run`.
//! - Errors must be logged and swallowed, not propagated, so metrics
//!   failures never break a run.

use std::sync::Arc;

/// Shared handle passed to the [`AnalyticsSolver`] and
/// [`AnalyticsFanoutWorker`] so every connector execution can record
/// which measures and dimensions were touched.
///
/// [`AnalyticsSolver`]: crate::solver
/// [`AnalyticsFanoutWorker`]: crate::solver
pub type SharedMetricSink = Arc<dyn AnalyticsMetricSink>;

pub trait AnalyticsMetricSink: Send + Sync + std::fmt::Debug {
    /// Record Tier 1 metric usage (the explicit measures and dimensions
    /// from a resolved semantic query) for a single analytics query
    /// execution. Fire-and-forget — see the module-level contract.
    fn record_analytics_query(
        &self,
        agent_id: &str,
        question: &str,
        measures: &[String],
        dimensions: &[String],
        sql: &str,
    );
}
