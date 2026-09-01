//! Async executor for the four metric-tree tools.
//!
//! The actual algorithms live in `oxy_airlayer_compat::engine::metric_tree_ops`. This
//! module is the bridge: parse JSON parameters, load the layer + tree via the
//! injected [`MetricTreeRunner`], build the synchronous `QueryExecutor`,
//! and run the op inside `spawn_blocking` (the airlayer op is sync and may
//! issue 100+ warehouse queries — never block the runtime thread).
//!
//! Returns shape:
//! - `explain_metric` → serialized [`oxy_airlayer_compat::engine::metric_tree_ops::ExplainResult`]
//! - `find_opportunities` → [`OpportunityResult`]
//! - `metric_sensitivity` → [`SensitivityResult`]
//! - `predict_impact` → [`PredictResult`]
//!
//! Errors map to `ToolError::Execution` with the airlayer message,
//! so the LLM sees a single sentence rather than a multi-line panic.

use std::sync::Arc;

use agentic_core::tools::ToolError;
use oxy_airlayer_compat::engine::metric_tree_ops::{self, ExplainConfig};
use serde_json::{Value, json};

use crate::metric_tree_runner::MetricTreeRunner;

use super::{emit_tool_error, emit_tool_input, emit_tool_output};

/// Dispatch a metric-tree tool call. Tool name must be one of
/// [`super::METRIC_TREE_TOOL_NAMES`].
#[tracing::instrument(
    skip(runner, params),
    fields(oxy.name = "analytics.tool", oxy.span_type = "analytics", tool = %name)
)]
pub async fn execute_metric_tree_tool(
    name: &str,
    params: Value,
    runner: Arc<dyn MetricTreeRunner>,
) -> Result<Value, ToolError> {
    emit_tool_input(name, &params);
    let result = execute_inner(name, params, runner).await;
    match &result {
        Ok(v) => emit_tool_output(v),
        Err(e) => emit_tool_error(e),
    }
    result
}

async fn execute_inner(
    name: &str,
    params: Value,
    runner: Arc<dyn MetricTreeRunner>,
) -> Result<Value, ToolError> {
    match name {
        "metric_sensitivity" => run_sensitivity(params, runner).await,
        "predict_impact" => run_predict(params, runner).await,
        "explain_metric" => run_explain(params, runner).await,
        "find_opportunities" => run_opportunity(params, runner).await,
        _ => Err(ToolError::UnknownTool(name.into())),
    }
}

// ── metric_sensitivity ──────────────────────────────────────────────────────

async fn run_sensitivity(
    params: Value,
    runner: Arc<dyn MetricTreeRunner>,
) -> Result<Value, ToolError> {
    let target = required_str(&params, "target")?;
    let layer = runner
        .load_layer()
        .await
        .map_err(|e| ToolError::Execution(e.to_string()))?;
    let tree = oxy_airlayer_compat::engine::metric_tree::MetricTree::build(&layer);
    let result = metric_tree_ops::sensitivity(&tree, target)
        .map_err(|e| ToolError::Execution(e.to_string()))?;
    serde_json::to_value(result).map_err(|e| ToolError::Execution(e.to_string()))
}

// ── predict_impact ──────────────────────────────────────────────────────────

async fn run_predict(params: Value, runner: Arc<dyn MetricTreeRunner>) -> Result<Value, ToolError> {
    let changes_value = params
        .get("changes")
        .ok_or_else(|| ToolError::BadParams("missing 'changes' array".into()))?;
    let changes_array = changes_value
        .as_array()
        .ok_or_else(|| ToolError::BadParams("'changes' must be an array".into()))?;
    let changes: Result<Vec<(String, f64)>, ToolError> = changes_array
        .iter()
        .map(|entry| {
            let measure = entry
                .get("measure")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::BadParams("change.measure must be a string".into()))?
                .to_string();
            let delta = entry
                .get("delta")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| ToolError::BadParams("change.delta must be a number".into()))?;
            Ok((measure, delta))
        })
        .collect();
    let changes = changes?;

    let layer = runner
        .load_layer()
        .await
        .map_err(|e| ToolError::Execution(e.to_string()))?;
    let tree = oxy_airlayer_compat::engine::metric_tree::MetricTree::build(&layer);
    let result = metric_tree_ops::predict(&tree, &changes)
        .map_err(|e| ToolError::Execution(e.to_string()))?;
    serde_json::to_value(result).map_err(|e| ToolError::Execution(e.to_string()))
}

// ── explain_metric ──────────────────────────────────────────────────────────

async fn run_explain(params: Value, runner: Arc<dyn MetricTreeRunner>) -> Result<Value, ToolError> {
    let target = required_str(&params, "target")?.to_string();
    let time_dimension = required_str(&params, "time_dimension")?.to_string();
    let cur_start = required_str(&params, "current_period_start")?.to_string();
    let cur_end = required_str(&params, "current_period_end")?.to_string();
    let prev_start = required_str(&params, "previous_period_start")?.to_string();
    let prev_end = required_str(&params, "previous_period_end")?.to_string();
    let deep = params
        .get("deep")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let mut config = ExplainConfig::default();
    config.deep = deep;

    let result = runner
        .run_explain(
            target,
            time_dimension,
            (cur_start, cur_end),
            (prev_start, prev_end),
            vec![],
            config,
        )
        .await
        .map_err(|e| ToolError::Execution(e.to_string()))?;

    serde_json::to_value(result).map_err(|e| ToolError::Execution(e.to_string()))
}

// ── find_opportunities ──────────────────────────────────────────────────────

async fn run_opportunity(
    params: Value,
    runner: Arc<dyn MetricTreeRunner>,
) -> Result<Value, ToolError> {
    let target = required_str(&params, "target")?.to_string();
    let time_dimension = required_str(&params, "time_dimension")?.to_string();
    let period_start = required_str(&params, "period_start")?.to_string();
    let period_end = required_str(&params, "period_end")?.to_string();

    let result = runner
        .run_opportunity(target, time_dimension, (period_start, period_end))
        .await
        .map_err(|e| ToolError::Execution(e.to_string()))?;

    serde_json::to_value(result).map_err(|e| ToolError::Execution(e.to_string()))
}

// ── helpers ─────────────────────────────────────────────────────────────────

fn required_str<'a>(params: &'a Value, key: &str) -> Result<&'a str, ToolError> {
    params
        .get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::BadParams(format!("missing '{key}' string")))
}

/// Stub result returned by a metric-tree tool when the workspace has no
/// runner wired up. Tools should not be exposed in that case, but the
/// safety net keeps the agent honest if the LLM hallucinates a call.
pub fn no_runner_error() -> ToolError {
    ToolError::Execution(
        "metric-tree tools are not available: no metric_tree_runner is \
         configured for this workspace"
            .into(),
    )
}

/// JSON payload returned when a metric-tree call surfaces a non-error result
/// that the LLM should still treat as a soft failure (empty tree, no drivers,
/// etc.). Kept here so handlers stay terse.
#[allow(dead_code)]
pub fn empty_result(reason: &str) -> Value {
    json!({ "ok": false, "reason": reason })
}
