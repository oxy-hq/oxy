use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Execution type enum matching frontend ExecutionType
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionType {
    SemanticQuery,
    OmniQuery,
    SqlGenerated,
    Workflow,
    AgentTool,
}

impl ExecutionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExecutionType::SemanticQuery => "semantic_query",
            ExecutionType::OmniQuery => "omni_query",
            ExecutionType::SqlGenerated => "sql_generated",
            ExecutionType::Workflow => "workflow",
            ExecutionType::AgentTool => "agent_tool",
        }
    }

    pub fn is_verified(&self) -> bool {
        matches!(
            self,
            ExecutionType::SemanticQuery | ExecutionType::OmniQuery | ExecutionType::Workflow
        )
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "semantic_query" => Some(ExecutionType::SemanticQuery),
            "omni_query" => Some(ExecutionType::OmniQuery),
            "sql_generated" => Some(ExecutionType::SqlGenerated),
            "workflow" => Some(ExecutionType::Workflow),
            "agent_tool" => Some(ExecutionType::AgentTool),
            _ => None,
        }
    }
}

/// Source type (agent or workflow)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    Agent,
    Workflow,
}

impl SourceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            SourceType::Agent => "agent",
            SourceType::Workflow => "workflow",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "agent" => Some(SourceType::Agent),
            "workflow" => Some(SourceType::Workflow),
            _ => None,
        }
    }
}

/// Summary statistics for execution analytics
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionSummary {
    pub total_executions: u64,
    pub verified_count: u64,
    pub generated_count: u64,
    pub verified_percent: f64,
    pub generated_percent: f64,
    pub success_rate_verified: f64,
    pub success_rate_generated: f64,
    pub most_executed_type: String,
    // Breakdown by type
    pub semantic_query_count: u64,
    pub omni_query_count: u64,
    pub sql_generated_count: u64,
    pub workflow_count: u64,
    pub agent_tool_count: u64,
}

/// Time bucket for time series data
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionTimeBucket {
    pub timestamp: String,
    pub verified_count: u64,
    pub generated_count: u64,
    // Optional detailed breakdown
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_query_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub omni_query_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sql_generated_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_tool_count: Option<u64>,
}

/// Per-agent execution statistics
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentExecutionStats {
    pub agent_ref: String,
    pub total_executions: u64,
    pub verified_count: u64,
    pub generated_count: u64,
    pub verified_percent: f64,
    pub most_executed_type: String,
    pub success_rate: f64,
}

/// A p50/p95/p99 latency triple (milliseconds).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LatencyTriple {
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
}

/// One daily latency-percentile point.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LatencyPercentilePoint {
    pub date: String,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
}

/// Latency percentiles: overall window plus a daily series.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LatencyPercentilesResponse {
    pub overall: LatencyTriple,
    pub series: Vec<LatencyPercentilePoint>,
}

/// One latency-histogram bucket (`upper_ms` = inclusive upper bound, ms).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HistogramBucket {
    pub upper_ms: f64,
    pub count: u64,
}

/// Latency histogram plus the p50/p95/p99 markers to overlay.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LatencyHistogramResponse {
    pub buckets: Vec<HistogramBucket>,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
}

/// Per-model token usage and estimated cost.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelCost {
    pub model: String,
    pub calls: u64,
    pub tokens: u64,
    pub cost_usd: f64,
    pub p95_ms: f64,
}

/// Aggregate LLM cost across models.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionCostResponse {
    pub total_cost_usd: f64,
    pub total_tokens: u64,
    pub by_model: Vec<ModelCost>,
}

/// Approximate USD price per **million** tokens `(input, output)` for known
/// model families. Unknown models price at `(0, 0)` — but a silent `$0` reads
/// as "free" on the cost dashboard and hides real spend, so the unknown case is
/// logged (`warn!`, once per model id) to make the under-report observable.
/// Newer families that share a family name (`claude-opus-5`, `claude-sonnet-5`,
/// …) already resolve via the substring match; genuinely new ids (e.g.
/// `gpt-5.x`, `claude-fable-5`) fall through until priced here — deliberately
/// not fabricated.
pub fn model_price_per_mtok(model: &str) -> (f64, f64) {
    let m = model.to_ascii_lowercase();
    if m.contains("opus") {
        (15.0, 75.0)
    } else if m.contains("sonnet") {
        (3.0, 15.0)
    } else if m.contains("haiku") {
        (0.80, 4.0)
    } else if m.contains("4o-mini") || m.contains("gpt-4.1-mini") || m.contains("mini") {
        (0.15, 0.60)
    } else if m.contains("4o") || m.contains("gpt-4") {
        (2.50, 10.0)
    } else {
        warn_unpriced_model_once(model);
        (0.0, 0.0)
    }
}

/// Model ids already warned about by [`warn_unpriced_model_once`]. Bounded by
/// the count of distinct unpriced models a process sees — a handful at most.
static UNPRICED_MODELS_WARNED: LazyLock<Mutex<HashSet<String>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

/// Records `model` as unpriced, returning `true` only the first time (per
/// process) it is seen. This is the gate that stops the "no pricing entry"
/// `warn!` from re-firing: `model_price_per_mtok` runs on every cost-dashboard
/// request, so without it an unpriced model would log a warning on every refresh.
fn record_unpriced_model(model: &str) -> bool {
    // On a poisoned lock, fall back to `true` (warn) rather than go silent.
    UNPRICED_MODELS_WARNED
        .lock()
        .map(|mut seen| seen.insert(model.to_string()))
        .unwrap_or(true)
}

/// Warn — at most once per process, per model id — that `model` has no pricing
/// entry and its spend will under-report as `$0`.
fn warn_unpriced_model_once(model: &str) {
    if record_unpriced_model(model) {
        tracing::warn!(
            model,
            "no pricing entry for model — run cost will under-report as $0; add it to model_price_per_mtok"
        );
    }
}

/// Estimated USD cost for a model's token counts, via [`model_price_per_mtok`].
pub fn model_cost_usd(model: &str, input_tokens: u64, output_tokens: u64) -> f64 {
    let (price_in, price_out) = model_price_per_mtok(model);
    (input_tokens as f64 / 1_000_000.0) * price_in
        + (output_tokens as f64 / 1_000_000.0) * price_out
}

/// Detailed execution record
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionDetail {
    pub trace_id: String,
    pub span_id: String,
    pub timestamp: String,
    pub execution_type: String,
    pub is_verified: bool,
    // Source information
    pub source_type: String,
    pub source_ref: String,
    // Common fields
    pub status: String,
    pub duration_ms: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    // Type-specific fields
    // For semantic queries
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_query_params: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_sql: Option<String>,
    // For omni queries
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integration: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    // For SQL queries (verified)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sql: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sql_ref: Option<String>,
    // For SQL queries (generated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_question: Option<String>,
    // For workflows
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_ref: Option<String>,
    // For agent tools
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_input: Option<String>,
}

/// Paginated response for execution details
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionListResponse {
    pub executions: Vec<ExecutionDetail>,
    pub total: u64,
    pub limit: usize,
    pub offset: usize,
}

#[cfg(test)]
mod pricing_tests {
    use super::*;

    #[test]
    fn current_claude_families_are_priced() {
        // Family-name substring keeps the current generation (opus/sonnet/haiku,
        // any version suffix) priced rather than silently $0.
        for (model, expected_in) in [
            ("claude-opus-4-8", 15.0),
            ("claude-sonnet-4-6", 3.0),
            ("claude-sonnet-5", 3.0),
            ("claude-haiku-4-5-20251001", 0.80),
        ] {
            let (input, _) = model_price_per_mtok(model);
            assert_eq!(input, expected_in, "unexpected input price for {model}");
        }
    }

    #[test]
    fn unknown_model_prices_zero() {
        // Documents the deliberate gap: genuinely-new ids fall through to $0
        // (logged as a warning) until priced, rather than a fabricated figure.
        assert_eq!(model_price_per_mtok("gpt-5.4"), (0.0, 0.0));
        assert_eq!(model_cost_usd("gpt-5.4", 1_000_000, 1_000_000), 0.0);
    }

    #[test]
    fn unpriced_model_warns_once_per_id() {
        // Unique ids so parallel tests sharing the process-global set can't
        // interfere. The first sighting of an unpriced id warns; repeats are
        // deduped; a distinct id warns again on its own first sighting.
        let model = "test-unpriced-alpha-9f3c";
        assert!(record_unpriced_model(model), "first sighting should warn");
        assert!(!record_unpriced_model(model), "repeat sighting is deduped");
        assert!(
            !record_unpriced_model(model),
            "still deduped on further calls"
        );

        let other = "test-unpriced-beta-9f3c";
        assert!(record_unpriced_model(other), "a distinct id warns once");
    }
}
