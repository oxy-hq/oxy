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
        EngineError::QueryError(_msg) => true,
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

// ── Connector routing ─────────────────────────────────────────────────────────

/// Look up a connector by name: exact match first, then case-insensitive.
///
/// THE one place the matching rule lives. It was three independent copies and
/// three exact-only lookups, which is how "a datasource differing only in case
/// is the same datasource" came to be true on the execute path and false on the
/// plan path -- so a `datasource: July_Airhouse` failed at solving before the
/// case-tolerant guard downstream was ever reached.
///
/// Case-insensitive because `airlayer` matches a view's `datasource:` with
/// `eq_ignore_ascii_case`, so a name that resolves for airlayer has to resolve
/// here too. Refusing to reroute is the policy; refusing on letter case alone
/// is a false positive whose error reads as a broken message rather than a
/// name mismatch.
///
/// Returns the REGISTERED key alongside the connector so a caller can report
/// or route by the canonical spelling rather than the caller's.
pub fn lookup_connector<'a>(
    connectors: &'a std::collections::HashMap<
        String,
        std::sync::Arc<dyn agentic_connector::DatabaseConnector>,
    >,
    name: &str,
) -> Option<(
    &'a String,
    &'a std::sync::Arc<dyn agentic_connector::DatabaseConnector>,
)> {
    if let Some(entry) = connectors.get_key_value(name) {
        return Some(entry);
    }
    connectors
        .iter()
        .find(|(registered, _)| registered.eq_ignore_ascii_case(name))
}

/// Resolve the connector a solution should execute against.
///
/// `connector_name` is where the routing decision already landed: for a
/// semantic-layer solution it is the `datasource:` of the topic's views, and
/// running that SQL anywhere else is not a degraded answer, it is a wrong one.
/// A view built for DuckDB whose connector failed to build will happily compile
/// and "succeed" against a ClickHouse default, either erroring on an unknown
/// function or — worse — returning a plausible number from a function that
/// means something different.
///
/// So the empty name is the only case that may fall back. An empty name means
/// no opinion was expressed and the default is genuinely the right answer; a
/// name that is set but unregistered means the database this query belongs to
/// is not available, and the caller needs to be told rather than rerouted.
pub fn resolve_solution_connector(
    connectors: &std::collections::HashMap<
        String,
        std::sync::Arc<dyn agentic_connector::DatabaseConnector>,
    >,
    connector_name: &str,
    default_connector: &str,
) -> Result<std::sync::Arc<dyn agentic_connector::DatabaseConnector>, String> {
    if let Some((_, c)) = lookup_connector(connectors, connector_name) {
        return Ok(c.clone());
    }

    if !connector_name.is_empty() {
        let mut available: Vec<&str> = connectors.keys().map(String::as_str).collect();
        available.sort_unstable();
        return Err(format!(
            "database '{connector_name}' is not available to this agent, so the query \
             cannot run where it belongs. Connected: [{}]. If '{connector_name}' is \
             named by a semantic view's `datasource:`, its connector failed to build — \
             check the run logs for a connector build warning rather than re-running.",
            available.join(", ")
        ));
    }

    // No `.or_else(|| values().next())` tail. HashMap iteration order is
    // nondeterministic, so with a misconfigured `default_connector` and two or
    // more connectors that lands the query in an arbitrary warehouse, and a
    // different one per process -- this function's whole purpose, entered by a
    // different door. A `default_connector` that is not registered is itself a
    // misconfiguration, so say so.
    connectors.get(default_connector).cloned().ok_or_else(|| {
        if connectors.is_empty() {
            "no database connectors are registered for this agent".to_string()
        } else {
            let mut available: Vec<&str> = connectors.keys().map(String::as_str).collect();
            available.sort_unstable();
            format!(
                "the default database '{default_connector}' is not registered, and the query \
                 named none of its own. Connected: [{}]",
                available.join(", ")
            )
        }
    })
}
