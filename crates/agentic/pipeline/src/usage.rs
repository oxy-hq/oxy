//! Compose the runtime's per-run token aggregation with the LLM-layer
//! pricing table into a single coordinator-facing report.
//!
//! Lives in the pipeline crate because that's the only layer allowed to
//! depend on both `agentic-runtime` (token summing) and `agentic-llm`
//! (pricing). HTTP handlers call into this rather than reaching across
//! the layering boundary themselves.

use std::collections::HashMap;

use sea_orm::{DatabaseConnection, DbErr};

use agentic_llm::pricing::cost_for_call;
use agentic_runtime::crud::{
    LlmTokenSummary, LlmTokenSummaryByRun, llm_usage_for_run, llm_usage_for_runs,
};

/// Per-run LLM usage with cost, ready for the dashboard.
///
/// Cost is `None` only when *every* model on the run is missing from the
/// pricing table — better to show `—` than fabricate a number. A
/// partially-priced run (one known model, one new one) reports the cost
/// of the priced portion and lists the unknown model alongside.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LlmUsageReport {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
    /// Distinct models seen, in order of first appearance.
    pub models: Vec<String>,
    /// Number of completed LLM HTTP rounds (one per `llm_end`).
    pub call_count: u64,
    /// USD cost of the priced portion of the call set. `None` when no
    /// model in `models` is in the pricing table.
    pub cost_usd: Option<f64>,
}

/// Pull the token sum for a run from the events table, multiply by the
/// per-model rates, and roll it into one report. Returns `None` for
/// runs that never invoked the LLM (workflows, airway pipelines, agent
/// runs whose first action errored before any call).
pub async fn usage_report_for_run(
    db: &DatabaseConnection,
    run_id: &str,
) -> Result<Option<LlmUsageReport>, DbErr> {
    let Some(summary) = llm_usage_for_run(db, run_id).await? else {
        return Ok(None);
    };
    Ok(Some(build_report(&summary)))
}

/// Batched variant — one SQL trip + one pricing pass for an entire page
/// of runs. Returned map only contains entries for runs that actually
/// invoked the LLM; workflow / airway runs are silently absent.
pub async fn usage_reports_for_runs(
    db: &DatabaseConnection,
    run_ids: &[String],
) -> Result<HashMap<String, LlmUsageReport>, DbErr> {
    let raw = llm_usage_for_runs(db, run_ids).await?;
    Ok(raw
        .into_iter()
        .map(|(id, summary)| (id, build_report_from_batched(&summary)))
        .collect())
}

fn build_report_from_batched(s: &LlmTokenSummaryByRun) -> LlmUsageReport {
    let summary = LlmTokenSummary {
        input_tokens: s.input_tokens,
        output_tokens: s.output_tokens,
        cache_creation_input_tokens: s.cache_creation_input_tokens,
        cache_read_input_tokens: s.cache_read_input_tokens,
        models: s.models.clone(),
        call_count: s.call_count,
    };
    build_report(&summary)
}

fn build_report(summary: &LlmTokenSummary) -> LlmUsageReport {
    let models: Vec<String> = summary
        .models
        .as_deref()
        .map(|s| s.split(',').map(|m| m.trim().to_string()).collect())
        .unwrap_or_default();

    let input = summary.input_tokens.max(0) as u64;
    let output = summary.output_tokens.max(0) as u64;
    let cache_creation = summary.cache_creation_input_tokens.max(0) as u64;
    let cache_read = summary.cache_read_input_tokens.max(0) as u64;
    let calls = summary.call_count.max(0) as u64;

    // Cost is approximated by attributing the full token totals to each
    // distinct model in turn and taking the *priced average*. This is
    // wrong in the multi-model case — but multi-model runs are rare,
    // and the alternative (per-call cost stored at write time) requires
    // schema work for a small gain. When everyone's on one model the
    // result is exact.
    let priced_costs: Vec<f64> = models
        .iter()
        .filter_map(|m| cost_for_call(m, input, output, cache_creation, cache_read))
        .collect();
    let cost_usd = if priced_costs.is_empty() {
        None
    } else {
        Some(priced_costs.iter().sum::<f64>() / priced_costs.len() as f64)
    };

    LlmUsageReport {
        input_tokens: input,
        output_tokens: output,
        cache_creation_input_tokens: cache_creation,
        cache_read_input_tokens: cache_read,
        models,
        call_count: calls,
        cost_usd,
    }
}
