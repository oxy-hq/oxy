use agentic_core::tools::ToolError;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::anomaly_store::{AnomalyFilter, AnomalyRecord, AnomalyStore};
use crate::metric_tree_runner::MetricTreeRunner;

use super::{emit_tool_error, emit_tool_input, emit_tool_output};

pub struct AnomalyToolContext<'a> {
    pub workspace_id: Uuid,
    pub store: &'a dyn AnomalyStore,
    pub runner: &'a dyn MetricTreeRunner,
}

pub async fn execute_anomaly_tool(
    name: &str,
    params: Value,
    ctx: &AnomalyToolContext<'_>,
) -> Result<Value, ToolError> {
    emit_tool_input(name, &params);
    let result = execute_inner(name, params, ctx).await;
    match &result {
        Ok(v) => emit_tool_output(v),
        Err(e) => emit_tool_error(e),
    }
    result
}

async fn execute_inner(
    name: &str,
    params: Value,
    ctx: &AnomalyToolContext<'_>,
) -> Result<Value, ToolError> {
    match name {
        "list_anomalies" => list_anomalies(params, ctx).await,
        "detect_anomalies" => detect_anomalies(params, ctx).await,
        "explain_anomaly" => explain_anomaly(params, ctx).await,
        _ => Err(ToolError::UnknownTool(name.into())),
    }
}

// ── list_anomalies ──────────────────────────────────────────────────────────

async fn list_anomalies(params: Value, ctx: &AnomalyToolContext<'_>) -> Result<Value, ToolError> {
    let measure = string_param(&params, "measure")?;
    let time_dimension = string_param(&params, "time_dimension")?;
    let granularity = string_param(&params, "granularity")?;
    let period_start = string_param(&params, "period_start")?;
    let period_end = string_param(&params, "period_end")?;

    let filter = AnomalyFilter {
        measure: Some(measure),
        time_dimension: Some(time_dimension),
        granularity: Some(granularity),
        period_start_gte: Some(period_start),
        period_end_lte: Some(period_end),
    };

    let anomalies = ctx
        .store
        .list(ctx.workspace_id, filter)
        .await
        .map_err(|e| ToolError::Execution(e.to_string()))?;

    Ok(json!({
        "count": anomalies.len(),
        "anomalies": anomalies.iter().map(record_to_json).collect::<Vec<_>>(),
    }))
}

// ── detect_anomalies ────────────────────────────────────────────────────────

async fn detect_anomalies(params: Value, ctx: &AnomalyToolContext<'_>) -> Result<Value, ToolError> {
    let measure = string_param(&params, "measure")?;
    let time_dimension = string_param(&params, "time_dimension")?;
    let granularity = string_param(&params, "granularity")?;
    let period_start = string_param(&params, "period_start")?;
    let period_end = string_param(&params, "period_end")?;

    let observations = ctx
        .runner
        .run_time_series(
            measure.clone(),
            time_dimension.clone(),
            granularity.clone(),
            (period_start, period_end),
            vec![],
        )
        .await
        .map_err(|e| ToolError::Execution(e.to_string()))?;

    let result = ctx
        .store
        .detect_and_upsert(
            ctx.workspace_id,
            &measure,
            &time_dimension,
            &granularity,
            observations,
        )
        .await
        .map_err(|e| ToolError::Execution(e.to_string()))?;

    let mut out = json!({
        "count": result.anomalies.len(),
        "total_observations": result.total_observations,
        "anomalies": result.anomalies.iter().map(record_to_json).collect::<Vec<_>>(),
    });
    if let Some(msg) = result.message {
        out["message"] = json!(msg);
    }
    Ok(out)
}

// ── explain_anomaly ─────────────────────────────────────────────────────────

async fn explain_anomaly(params: Value, ctx: &AnomalyToolContext<'_>) -> Result<Value, ToolError> {
    let id_str = string_param(&params, "anomaly_id")?;
    let id = id_str
        .parse::<Uuid>()
        .map_err(|_| ToolError::BadParams(format!("invalid anomaly_id: {id_str}")))?;

    // Return cached result if available.
    if let Some(cached) = ctx
        .store
        .get_explain_cache(id, ctx.workspace_id)
        .await
        .map_err(|e| ToolError::Execution(e.to_string()))?
    {
        return Ok(json!({ "cached": true, "explanation": cached }));
    }

    let record = ctx
        .store
        .get(id, ctx.workspace_id)
        .await
        .map_err(|e| ToolError::Execution(e.to_string()))?
        .ok_or_else(|| ToolError::Execution(format!("anomaly {id} not found")))?;

    let current_period = (record.period_start.clone(), record.period_end.clone());
    let previous_period = shift_period_back(&record.period_start, &record.period_end)
        .ok_or_else(|| ToolError::Execution("could not compute comparison period".to_string()))?;

    let explain_result = ctx
        .runner
        .run_explain(
            record.measure.clone(),
            record.time_dimension.clone(),
            current_period,
            previous_period,
            Default::default(),
        )
        .await
        .map_err(|e| ToolError::Execution(e.to_string()))?;

    let json_result =
        serde_json::to_value(&explain_result).map_err(|e| ToolError::Execution(e.to_string()))?;

    ctx.store
        .set_explain_cache(id, ctx.workspace_id, json_result.clone())
        .await
        .map_err(|e| ToolError::Execution(e.to_string()))?;

    Ok(json!({ "cached": false, "explanation": json_result }))
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn string_param(params: &Value, key: &str) -> Result<String, ToolError> {
    params[key]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| ToolError::BadParams(format!("missing '{key}'")))
}

fn record_to_json(r: &AnomalyRecord) -> Value {
    json!({
        "id": r.id,
        "measure": r.measure,
        "time_dimension": r.time_dimension,
        "granularity": r.granularity,
        "period_start": r.period_start,
        "period_end": r.period_end,
        "observed": r.observed,
        "expected": r.expected,
        "lower": r.lower,
        "upper": r.upper,
        "z_score": r.z_score,
        "severity": r.severity,
        "status": r.status,
    })
}

/// Shift (period_start, period_end) back by the same duration.
fn shift_period_back(start: &str, end: &str) -> Option<(String, String)> {
    use chrono::{DateTime, FixedOffset};
    let s: DateTime<FixedOffset> = start.parse().ok()?;
    let e: DateTime<FixedOffset> = end.parse().ok()?;
    let duration = e.signed_duration_since(s);
    let prev_start = s - duration;
    let prev_end = s - chrono::Duration::seconds(1);
    Some((prev_start.to_rfc3339(), prev_end.to_rfc3339()))
}
