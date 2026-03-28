use crate::events::AnalyticsEvent;
use agentic_core::events::{Event, EventStream};

// ── Event helpers ─────────────────────────────────────────────────────────────

pub async fn emit_domain(tx: &Option<EventStream<AnalyticsEvent>>, event: AnalyticsEvent) {
    if let Some(tx) = tx {
        let _ = tx.send(Event::Domain(event)).await;
    }
}

pub async fn emit_core(tx: &Option<EventStream<AnalyticsEvent>>, event: agentic_core::CoreEvent) {
    if let Some(tx) = tx {
        let _ = tx.send(agentic_core::Event::Core(event)).await;
    }
}

// ── JSON fence stripping ──────────────────────────────────────────────────────

/// Strip markdown JSON fences and whitespace from LLM output.
pub fn strip_json_fences(raw: &str) -> &str {
    let s = raw.trim();
    let s = s
        .strip_prefix("```json")
        .or_else(|| s.strip_prefix("```"))
        .unwrap_or(s);
    let s = s.strip_suffix("```").unwrap_or(s);
    s.trim()
}

// ── Shape helpers ─────────────────────────────────────────────────────────────

/// Format a [`crate::types::ResultShape`] as a compact human-readable string.
pub fn fmt_result_shape(shape: &crate::types::ResultShape) -> String {
    match shape {
        crate::types::ResultShape::Scalar => "Scalar".to_string(),
        crate::types::ResultShape::Series => "Series".to_string(),
        crate::types::ResultShape::TimeSeries => "TimeSeries".to_string(),
        crate::types::ResultShape::Table { columns } if columns.is_empty() => "Table".to_string(),
        crate::types::ResultShape::Table { columns } => format!("Table[{}]", columns.join(", ")),
    }
}

// ── Airlayer error classification ────────────────────────────────────────────

/// Classify an airlayer [`EngineError`] as retryable or not.
///
/// **Retryable** errors mean the LLM picked a wrong member name (measure,
/// dimension, segment) — it should retry Specify with the error message so
/// it can correct its output.
///
/// **Non-retryable** errors are structural (join graph limitations, cross-
/// dialect queries, SQL generation bugs) — the query cannot be expressed in
/// airlayer's model, so we fall back to LLM SQL generation.
pub fn is_retryable_compile_error(e: &airlayer::engine::EngineError) -> bool {
    use airlayer::engine::EngineError;
    match e {
        // QueryError: "Measure 'X' not found", "Dimension 'X' not found",
        // "Invalid member path", "Segment 'X' not found" — all fixable by
        // the LLM choosing a different name.
        //
        // Exception: "spans multiple dialects" / "Cross-database" — structural,
        // not fixable by renaming.
        EngineError::QueryError(msg) => {
            !msg.contains("multiple dialects") && !msg.contains("Cross-database")
        }
        // JoinError, SchemaError, SqlGenerationError — structural, not retryable.
        EngineError::JoinError(_)
        | EngineError::SchemaError(_)
        | EngineError::SqlGenerationError(_) => false,
    }
}

/// Build a default [`crate::types::ResultShape`] for a query spec.
///
/// Always returns `Table { columns: vec![] }`.  Shape validation
/// (`shape_match`) has been removed from the default rule set because
/// the expected shape cannot be reliably inferred — `resolved_metrics`
/// contains SQL expressions and `dimensions` may be qualified names,
/// neither of which matches actual output column names.
pub fn infer_result_shape(_dims: &[String], _metrics: &[String]) -> crate::types::ResultShape {
    crate::types::ResultShape::Table { columns: vec![] }
}
