//! **Executing** pipeline stage.
//!
//! Owns:
//! - [`format_compact_result`] — compact result formatter for retry context
//! - [`AnalyticsSolver::execute_solution`] — execute SQL against the connector
//! - [`build_executing_handler`] — `StateHandler` factory with path-aware diagnosis

use std::sync::Arc;

use agentic_core::{
    back_target::BackTarget,
    orchestrator::{RunContext, SessionMemory, StateHandler, TransitionResult},
    result::CellValue,
    state::ProblemState,
};
use tracing::Instrument;

use crate::engine::EngineError;
use crate::events::{AnalyticsEvent, QuerySource};
use crate::types::{SolutionPayload, SolutionSource};
#[cfg(test)]
use agentic_core::subrun::SubrunOutput;

use crate::{AnalyticsDomain, AnalyticsError, AnalyticsResult, AnalyticsSolution};

use super::{AnalyticsSolver, emit_domain};

// Mapping from `SolutionSource` to the `oxy.execution_type` span attribute
// and the `oxy.is_verified` flag consumed by the Execution Analytics tab.
// Kept as a free function (not a method) so the fan-out worker and tests in
// `tests.rs` can reuse it.
pub(crate) fn execution_type_for(source: &SolutionSource) -> (&'static str, bool) {
    match source {
        SolutionSource::SemanticLayer => ("semantic_query", true),
        SolutionSource::VendorEngine(_) => ("omni_query", true),
        SolutionSource::LlmWithSemanticContext => ("sql_generated", false),
        // Automation solutions are intercepted before this code path runs.
        SolutionSource::Automation { .. } => ("sql_generated", false),
        // SQL file queries are pre-verified — badge them as verified.
        SolutionSource::SqlFile { .. } => ("verified_sql", true),
    }
}

// Compact result formatter (for retry context)

/// Format an [`AnalyticsResult`] as a compact single-line summary for
/// back-edge retry context.
pub(super) fn format_compact_result(result: &AnalyticsResult) -> String {
    let primary = result.primary();
    let cols = &primary.data.columns;
    let n_rows = primary.data.total_row_count;
    let n_cols = cols.len();

    let sample: Vec<String> = primary
        .data
        .rows
        .iter()
        .take(3)
        .map(|row| {
            let cells: Vec<String> = row
                .0
                .iter()
                .map(|c| match c {
                    CellValue::Text(s) if s.len() > 20 => format!("{}…", &s[..20]),
                    CellValue::Text(s) => s.clone(),
                    CellValue::Number(n) => n.to_string(),
                    CellValue::Null => "NULL".to_string(),
                })
                .collect();
            cells.join(" | ")
        })
        .collect();

    let cols_str = cols.join(", ");
    let mut out = format!("Result: {n_rows} rows x {n_cols} cols [{cols_str}]");
    if !sample.is_empty() {
        out.push_str(&format!("\n  sample: {}", sample.join("; ")));
    }
    out
}

// execute_solution body

impl AnalyticsSolver {
    /// Execute a SQL solution against the appropriate connector.
    ///
    /// Called by the `DomainSolver::execute` trait delegation and directly
    /// by the executing state handler.
    #[tracing::instrument(
        skip_all,
        fields(
            oxy.name = "analytics.execute",
            oxy.span_type = "analytics",
            connector = tracing::field::Empty,
            solution_source = tracing::field::Empty,
            row_count = tracing::field::Empty,
            duration_ms = tracing::field::Empty,
        )
    )]
    pub(crate) async fn execute_solution(
        &mut self,
        solution: AnalyticsSolution,
    ) -> Result<AnalyticsResult, (AnalyticsError, BackTarget<AnalyticsDomain>)> {
        const DEFAULT_SAMPLE_LIMIT: u64 = 1_000;

        let span = tracing::Span::current();
        span.record("connector", &solution.connector_name);
        span.record("solution_source", format!("{:?}", solution.solution_source));

        let start = std::time::Instant::now();

        let query_source = match &solution.solution_source {
            SolutionSource::SemanticLayer => QuerySource::Semantic,
            SolutionSource::SqlFile { .. } => QuerySource::VerifiedSql,
            SolutionSource::VendorEngine(_) => QuerySource::Vendor,
            // Automation solutions are intercepted by `build_executing_handler` before
            // `execute_solution` is ever called, so this arm is unreachable for that
            // variant. Kept in the pattern only to satisfy exhaustiveness.
            SolutionSource::LlmWithSemanticContext | SolutionSource::Automation { .. } => {
                QuerySource::Llm
            }
        };

        match &solution.payload {
            SolutionPayload::Sql(sql) => {
                tracing::debug!(
                    connector = %solution.connector_name,
                    source = ?solution.solution_source,
                    sql = %sql,
                    "executing SQL"
                );

                // Record the SQL query as a visible event for trace inspection.
                tracing::info!(
                    name: "query.input",
                    is_visible = true,
                    sql = %sql,
                    connector = %solution.connector_name,
                    source = %format!("{:?}", solution.solution_source),
                );

                // `connector_name` carries the routing decision already made:
                // for a semantic solution it is the `datasource:` of the
                // topic's views. Substituting the default when it is missing
                // does not degrade the answer, it changes which warehouse and
                // which SQL dialect answered — silently. Refuse instead.
                let connector = match crate::solver::resolve_solution_connector(
                    &self.connectors,
                    &solution.connector_name,
                    &self.default_connector,
                ) {
                    Ok(connector) => connector,
                    Err(msg) => {
                        tracing::error!("analytics execute: {msg}");
                        return Err((
                            AnalyticsError::NeedsUserInput { prompt: msg },
                            BackTarget::Execute(solution.clone(), Default::default()),
                        ));
                    }
                };
                let sql = sql.clone();
                // Child `tool_call` span so this execution shows up in the
                // Execution Analytics tab alongside classic agent tool calls.
                let (execution_type, is_verified) = execution_type_for(&solution.solution_source);
                let tool_span = tracing::info_span!(
                    "analytics.tool_call",
                    oxy.name = "analytics.tool_call",
                    oxy.span_type = "tool_call",
                    // Denormalized from the run span so the execution rollup MV
                    // is a single-source flatten (no read-time span join).
                    oxy.agent.ref = %self.agent_id,
                    agent.prompt = %self.question,
                    oxy.execution_type = execution_type,
                    oxy.is_verified = is_verified,
                    connector = %solution.connector_name,
                );
                let exec_result = connector
                    .execute_query(&sql, DEFAULT_SAMPLE_LIMIT)
                    .instrument(tool_span.clone())
                    .await;
                match exec_result {
                    Ok(exec) => {
                        let duration_ms = start.elapsed().as_millis() as u64;
                        let columns = exec.result.columns.clone();
                        let rows: Vec<Vec<serde_json::Value>> = exec
                            .result
                            .rows
                            .iter()
                            .map(|row| {
                                row.0
                                    .iter()
                                    .map(|cell| match cell {
                                        CellValue::Text(s) => serde_json::Value::String(s.clone()),
                                        CellValue::Number(n) => serde_json::json!(n),
                                        CellValue::Null => serde_json::Value::Null,
                                    })
                                    .collect()
                            })
                            .collect();

                        // `tool_call.output` event lives on the child
                        // tool_call span so Execution Analytics picks it up.
                        // Record metrics inside the same `in_scope` so any
                        // adapter that resolves `trace_id` from the current
                        // span sees `tool_span` (whose parent chain reaches
                        // `analytics.run`).
                        tool_span.in_scope(|| {
                            tracing::info!(
                                name: "tool_call.output",
                                status = "success",
                                row_count = exec.result.rows.len(),
                                duration_ms = duration_ms,
                            );
                            if let (Some(sink), Some(q)) =
                                (self.metric_sink.as_ref(), &solution.semantic_query)
                            {
                                sink.record_analytics_query(
                                    &self.agent_id,
                                    &self.question,
                                    &q.measures,
                                    &q.dimensions,
                                    &sql,
                                );
                            }
                        });

                        // Record successful result as a visible event.
                        let preview = if rows.len() > 5 {
                            serde_json::to_string(&rows[..5]).unwrap_or_default()
                        } else {
                            serde_json::to_string(&rows).unwrap_or_default()
                        };
                        tracing::info!(
                            name: "query.result",
                            is_visible = true,
                            row_count = exec.result.rows.len(),
                            columns = %serde_json::to_string(&columns).unwrap_or_default(),
                            rows_preview = %preview,
                            duration_ms = duration_ms,
                        );

                        emit_domain(
                            &self.event_tx,
                            AnalyticsEvent::QueryExecuted {
                                query: sql.clone(),
                                row_count: exec.result.rows.len(),
                                duration_ms,
                                success: true,
                                error: None,
                                columns,
                                rows,
                                source: query_source,
                                is_preagg: false,
                                sub_spec_index: None,
                                semantic_query: solution.semantic_query.clone(),
                            },
                        )
                        .await;
                        span.record("row_count", exec.result.rows.len());
                        span.record("duration_ms", duration_ms);
                        Ok(AnalyticsResult::single(exec.result, Some(exec.summary)))
                    }
                    Err(e) => {
                        let duration_ms = start.elapsed().as_millis() as u64;

                        tool_span.in_scope(|| {
                            tracing::info!(
                                name: "tool_call.output",
                                status = "error",
                                "error.message" = %e,
                                duration_ms = duration_ms,
                            );
                        });

                        // Record execution error as a visible event.
                        tracing::info!(
                            name: "query.error",
                            is_visible = true,
                            error = %e,
                            sql = %sql,
                            duration_ms = duration_ms,
                        );

                        emit_domain(
                            &self.event_tx,
                            AnalyticsEvent::QueryExecuted {
                                query: sql.clone(),
                                row_count: 0,
                                duration_ms,
                                success: false,
                                error: Some(e.to_string()),
                                columns: vec![],
                                rows: vec![],
                                source: query_source,
                                is_preagg: false,
                                sub_spec_index: None,
                                semantic_query: solution.semantic_query.clone(),
                            },
                        )
                        .await;
                        Err((
                            AnalyticsError::SyntaxError {
                                query: sql,
                                message: e.to_string(),
                            },
                            BackTarget::Execute(solution, Default::default()),
                        ))
                    }
                }
            }

            SolutionPayload::Vendor(vq) => {
                let vq = vq.clone();
                let vendor_name = match &solution.solution_source {
                    SolutionSource::VendorEngine(n) => n.clone(),
                    _ => "unknown".to_string(),
                };
                let engine = self
                    .engine
                    .as_ref()
                    .expect("VendorEngine path requires engine on solver")
                    .clone();
                let (execution_type, is_verified) = execution_type_for(&solution.solution_source);
                let tool_span = tracing::info_span!(
                    "analytics.tool_call",
                    oxy.name = "analytics.tool_call",
                    oxy.span_type = "tool_call",
                    oxy.agent.ref = %self.agent_id,
                    agent.prompt = %self.question,
                    oxy.execution_type = execution_type,
                    oxy.is_verified = is_verified,
                    vendor = %vendor_name,
                );
                let exec_result = engine.execute(&vq).instrument(tool_span.clone()).await;
                match exec_result {
                    Ok(result) => {
                        let duration_ms = start.elapsed().as_millis() as u64;
                        let columns = result.columns.clone();
                        let rows: Vec<Vec<serde_json::Value>> = result
                            .rows
                            .iter()
                            .map(|row| {
                                row.0
                                    .iter()
                                    .map(|cell| match cell {
                                        CellValue::Text(s) => serde_json::Value::String(s.clone()),
                                        CellValue::Number(n) => serde_json::json!(n),
                                        CellValue::Null => serde_json::Value::Null,
                                    })
                                    .collect()
                            })
                            .collect();
                        tool_span.in_scope(|| {
                            tracing::info!(
                                name: "tool_call.output",
                                status = "success",
                                row_count = result.rows.len(),
                                duration_ms = duration_ms,
                            );
                            if let (Some(sink), Some(q)) =
                                (self.metric_sink.as_ref(), &solution.semantic_query)
                            {
                                sink.record_analytics_query(
                                    &self.agent_id,
                                    &self.question,
                                    &q.measures,
                                    &q.dimensions,
                                    &format!("[vendor:{vendor_name}]"),
                                );
                            }
                        });
                        emit_domain(
                            &self.event_tx,
                            AnalyticsEvent::QueryExecuted {
                                query: format!("[vendor:{vendor_name}]"),
                                row_count: result.rows.len(),
                                duration_ms,
                                success: true,
                                error: None,
                                columns,
                                rows,
                                source: query_source,
                                is_preagg: false,
                                sub_spec_index: None,
                                semantic_query: solution.semantic_query.clone(),
                            },
                        )
                        .await;
                        Ok(AnalyticsResult::single(result, None))
                    }
                    Err(e) => {
                        let duration_ms = start.elapsed().as_millis() as u64;
                        let message = e.to_string();
                        tool_span.in_scope(|| {
                            tracing::info!(
                                name: "tool_call.output",
                                status = "error",
                                "error.message" = %message,
                                duration_ms = duration_ms,
                            );
                        });
                        emit_domain(
                            &self.event_tx,
                            AnalyticsEvent::QueryExecuted {
                                query: format!("[vendor:{vendor_name}]"),
                                row_count: 0,
                                duration_ms,
                                success: false,
                                error: Some(message.clone()),
                                columns: vec![],
                                rows: vec![],
                                source: query_source,
                                is_preagg: false,
                                sub_spec_index: None,
                                semantic_query: solution.semantic_query.clone(),
                            },
                        )
                        .await;
                        let analytics_err = match e {
                            EngineError::ApiError { status, body } => AnalyticsError::VendorError {
                                vendor_name: vendor_name.clone(),
                                message: format!("API error {status}: {body}"),
                            },
                            EngineError::Transport(msg) => AnalyticsError::VendorError {
                                vendor_name: vendor_name.clone(),
                                message: msg,
                            },
                            // Contract: translate()-time only — should never reach here.
                            other => AnalyticsError::VendorError {
                                vendor_name: vendor_name.clone(),
                                message: other.to_string(),
                            },
                        };
                        Err((
                            analytics_err,
                            BackTarget::Execute(solution, Default::default()),
                        ))
                    }
                }
            }

            SolutionPayload::Preaggregation {
                preagg_sql,
                source,
                warehouse_sql,
            } => {
                let preagg_sql = preagg_sql.clone();
                let source = source.clone();
                let warehouse_sql = warehouse_sql.clone();
                tracing::debug!(
                    remote = source.is_remote(),
                    "executing preagg rollup via DuckDB"
                );

                // Emit the warehouse SQL as `query.input` so traces and the
                // execution-analytics tab still surface the logical query,
                // not the DuckDB `read_parquet(...)` rewrite.
                tracing::info!(
                    name: "query.input",
                    is_visible = true,
                    sql = %warehouse_sql,
                    connector = %solution.connector_name,
                    source = "SemanticLayer (preagg)",
                );

                // Both tiers run inside this span, and which one answers isn't
                // known until the rollup read returns — so the attributes
                // Execution Analytics reads are left Empty and recorded below
                // from the tier that actually served. Naming the preagg tier up
                // front counted a warehouse fallback as a pre-aggregated query.
                let tool_span = tracing::info_span!(
                    "analytics.tool_call",
                    oxy.name = "analytics.tool_call",
                    oxy.span_type = "tool_call",
                    oxy.agent.ref = %self.agent_id,
                    agent.prompt = %self.question,
                    oxy.execution_type = tracing::field::Empty,
                    oxy.is_verified = tracing::field::Empty,
                    connector = %solution.connector_name,
                );

                // No wall-clock timeout here, matching the warehouse branch
                // above. On a blob source this is a network scan, so what keeps
                // it from stalling a chat run against an unreachable endpoint
                // is `SET http_timeout` / `SET http_retries` in
                // `oxy_shared::duckdb_s3::s3_setup_sql` — the read is bounded
                // where it happens. Wrapping this in `tokio::time::timeout`
                // would not help: it runs in `spawn_blocking`, where dropping
                // the future detaches the task instead of cancelling it.
                let rollup_result = crate::preagg_exec::execute_rollup(
                    preagg_sql.clone(),
                    source.clone(),
                    DEFAULT_SAMPLE_LIMIT,
                )
                .instrument(tool_span.clone())
                .await;

                // A rollup that won't read is not a query the model got wrong.
                // The same question has a warehouse answer, and the variant
                // carries the SQL for it — so fall back rather than raising a
                // SyntaxError, which sends the FSM back to Execute and asks an
                // LLM to repair SQL over what is usually a missing object.
                //
                // Reachable in normal operation on the blob tier: the rebuild
                // mirrors one rollup's Parquet and then the WHOLE manifest, so
                // a node syncing that manifest can resolve entries whose
                // objects are not in the store yet. Slower, not wrong.
                let (exec_result, served_from_rollup) = match rollup_result {
                    Ok(exec) => (Ok(exec), true),
                    Err(rollup_error) => {
                        tracing::warn!(
                            remote = source.is_remote(),
                            error = %rollup_error,
                            "preagg rollup read failed; falling back to the warehouse"
                        );
                        let connector = match crate::solver::resolve_solution_connector(
                            &self.connectors,
                            &solution.connector_name,
                            &self.default_connector,
                        ) {
                            Ok(connector) => connector,
                            Err(msg) => {
                                tracing::error!("analytics execute (preagg fallback): {msg}");
                                // Stamp the span before returning. The rollup
                                // read already failed, so this run is a
                                // warehouse fallback that could not resolve a
                                // connector — badge it as the fallback tier.
                                // Returning first left both attributes Empty
                                // and the run reached Execution Analytics with
                                // `execution_type = ''`, which reads as a gap
                                // in instrumentation rather than as the failure
                                // it is.
                                let (execution_type, is_verified) =
                                    execution_type_for(&solution.solution_source);
                                tool_span.record("oxy.execution_type", execution_type);
                                tool_span.record("oxy.is_verified", is_verified);
                                return Err((
                                    AnalyticsError::NeedsUserInput { prompt: msg },
                                    BackTarget::Execute(solution.clone(), Default::default()),
                                ));
                            }
                        };
                        (
                            connector
                                .execute_query(&warehouse_sql, DEFAULT_SAMPLE_LIMIT)
                                .instrument(tool_span.clone())
                                .await
                                // Match `execute_rollup`'s error type so both
                                // tiers land in one `match` below.
                                .map_err(|e| e.to_string()),
                            false,
                        )
                    }
                };

                // The rollup tier is `semantic_query_preagg`; a fallback is an
                // ordinary semantic-model warehouse query, and is badged as one.
                let (execution_type, is_verified) = if served_from_rollup {
                    ("semantic_query_preagg", true)
                } else {
                    execution_type_for(&solution.solution_source)
                };
                tool_span.record("oxy.execution_type", execution_type);
                tool_span.record("oxy.is_verified", is_verified);

                match exec_result {
                    Ok(exec) => {
                        let duration_ms = start.elapsed().as_millis() as u64;
                        let columns = exec.result.columns.clone();
                        let rows: Vec<Vec<serde_json::Value>> = exec
                            .result
                            .rows
                            .iter()
                            .map(|row| {
                                row.0
                                    .iter()
                                    .map(|cell| match cell {
                                        CellValue::Text(s) => serde_json::Value::String(s.clone()),
                                        CellValue::Number(n) => serde_json::json!(n),
                                        CellValue::Null => serde_json::Value::Null,
                                    })
                                    .collect()
                            })
                            .collect();

                        tool_span.in_scope(|| {
                            tracing::info!(
                                name: "tool_call.output",
                                status = "success",
                                row_count = exec.result.rows.len(),
                                duration_ms = duration_ms,
                            );
                            if let (Some(sink), Some(q)) =
                                (self.metric_sink.as_ref(), &solution.semantic_query)
                            {
                                sink.record_analytics_query(
                                    &self.agent_id,
                                    &self.question,
                                    &q.measures,
                                    &q.dimensions,
                                    &warehouse_sql,
                                );
                            }
                        });

                        tracing::info!(
                            name: "query.result",
                            is_visible = true,
                            row_count = exec.result.rows.len(),
                            columns = %serde_json::to_string(&columns).unwrap_or_default(),
                            duration_ms = duration_ms,
                            is_preagg = served_from_rollup,
                        );

                        emit_domain(
                            &self.event_tx,
                            AnalyticsEvent::QueryExecuted {
                                query: warehouse_sql.clone(),
                                row_count: exec.result.rows.len(),
                                duration_ms,
                                success: true,
                                error: None,
                                columns,
                                rows,
                                source: query_source,
                                // The badge has to report what actually
                                // answered. A fallback that still claimed
                                // "Pre-aggregated" would be the freshness lie
                                // the badge exists to avoid.
                                is_preagg: served_from_rollup,
                                sub_spec_index: None,
                                semantic_query: solution.semantic_query.clone(),
                            },
                        )
                        .await;
                        span.record("row_count", exec.result.rows.len());
                        span.record("duration_ms", duration_ms);
                        Ok(AnalyticsResult::single(exec.result, Some(exec.summary)))
                    }
                    Err(e) => {
                        let duration_ms = start.elapsed().as_millis() as u64;
                        tool_span.in_scope(|| {
                            tracing::info!(
                                name: "tool_call.output",
                                status = "error",
                                "error.message" = %e,
                                duration_ms = duration_ms,
                            );
                        });
                        emit_domain(
                            &self.event_tx,
                            AnalyticsEvent::QueryExecuted {
                                query: warehouse_sql.clone(),
                                row_count: 0,
                                duration_ms,
                                success: false,
                                error: Some(e.clone()),
                                columns: vec![],
                                rows: vec![],
                                source: query_source,
                                is_preagg: true,
                                sub_spec_index: None,
                                semantic_query: solution.semantic_query.clone(),
                            },
                        )
                        .await;
                        Err((
                            AnalyticsError::SyntaxError {
                                // Both tiers failed. Report the WAREHOUSE SQL:
                                // it is the query the model can actually act
                                // on, where the DuckDB rewrite is a rollup
                                // detail it never wrote and cannot repair.
                                query: warehouse_sql,
                                message: e,
                            },
                            BackTarget::Execute(solution, Default::default()),
                        ))
                    }
                }
            }
        }
    }
}

// SQL-generation mode helper

/// Outcome of `terminate_with_sql_if_enabled` — the executing handler
/// uses these to pick between "terminate the run now" and "fall through
/// to normal execution."
pub(super) enum SqlGenOutcome {
    /// SQL mode is off — proceed with the normal execute path.
    Disabled,
    /// SQL is ready; emit the terminal answer directly. The text is the
    /// generated SQL string; downstream automation steps consume it via
    /// the automation `cache:` block.
    Terminate(crate::AnalyticsAnswer),
    /// SQL was generated by the LLM and the `LIMIT 0` smoke check
    /// failed. Route through Diagnosing → Solve so the LLM can retry.
    SmokeCheckFailed { sql: String, message: String },
    /// The solution path can't produce a portable SQL string (vendor
    /// engine, automation delegation). Automation authors hit this when
    /// `output: { mode: sql }` is combined with an agent context that
    /// doesn't generate raw SQL.
    IncompatiblePath { reason: String },
    /// The database the solution names isn't registered, so the smoke check
    /// has nothing to probe.
    ///
    /// Distinct from [`Self::SmokeCheckFailed`] because the remedy is
    /// completely different: no rewrite of the SQL can register a connector.
    /// Routing this through Solve hands the LLM "database X is not available"
    /// as a syntax hint, burns the retry budget, and reports a routing failure
    /// as a malformed query -- with the SQL likely mangled on the way.
    ///
    /// Carries `sql` so the failed-query panel still shows what would have run.
    /// It is not a repair hint here -- the SQL is fine, its destination is not.
    ConnectorUnavailable { sql: String, message: String },
}

impl AnalyticsSolver {
    /// In SQL-generation mode, terminate the run with the generated SQL
    /// as the answer. Pre-validated paths skip execution entirely;
    /// LLM-generated SQL runs a `LIMIT 0` smoke check first.
    pub(super) async fn terminate_with_sql_if_enabled(
        &self,
        solution: &AnalyticsSolution,
    ) -> SqlGenOutcome {
        if !self.sql_generation_mode {
            return SqlGenOutcome::Disabled;
        }

        // Vendor and automation paths produce no portable SQL string —
        // reject so the automation author sees a clear error rather than
        // an empty cache file.
        match &solution.solution_source {
            SolutionSource::VendorEngine(name) => {
                return SqlGenOutcome::IncompatiblePath {
                    reason: format!(
                        "vendor engine `{name}` produces no portable SQL; \
                         remove `output: {{ mode: sql }}` or switch the agent's \
                         context to a semantic-model or LLM-driven source"
                    ),
                };
            }
            SolutionSource::Automation { file_path } => {
                return SqlGenOutcome::IncompatiblePath {
                    reason: format!(
                        "automation delegation ({}) produces no SQL until the \
                         child automation runs; incompatible with \
                         `output: {{ mode: sql }}`",
                        file_path.display()
                    ),
                };
            }
            SolutionSource::SemanticLayer
            | SolutionSource::SqlFile { .. }
            | SolutionSource::LlmWithSemanticContext => {}
        }

        let sql = match &solution.payload {
            SolutionPayload::Sql(s) => s.clone(),
            // Preagg short-circuit produces a DuckDB `read_parquet(...)`
            // statement that's useless to a downstream automation step
            // configured for `output: { mode: sql }`. Hand back the
            // warehouse-side SQL we stashed alongside it so the cached
            // file remains portable.
            SolutionPayload::Preaggregation { warehouse_sql, .. } => warehouse_sql.clone(),
            SolutionPayload::Vendor(_) => {
                // Already filtered above; defense in depth.
                return SqlGenOutcome::IncompatiblePath {
                    reason: "vendor payload reached SQL-gen mode terminate path".to_string(),
                };
            }
        };

        // LLM-generated SQL gets a `LIMIT 0` smoke check before we
        // commit to terminate. Pre-validated paths trust the source.
        let needs_smoke = matches!(
            solution.solution_source,
            SolutionSource::LlmWithSemanticContext
        );
        if needs_smoke {
            // Resolve first, probe second. Collapsing the two would make an
            // unavailable database indistinguishable from SQL the engine
            // rejected, and only one of those is the LLM's to fix.
            let connector = match crate::solver::resolve_solution_connector(
                &self.connectors,
                &solution.connector_name,
                &self.default_connector,
            ) {
                Ok(connector) => connector,
                Err(message) => return SqlGenOutcome::ConnectorUnavailable { sql, message },
            };
            if let Err(message) = Self::smoke_check_sql(&sql, connector.as_ref()).await {
                return SqlGenOutcome::SmokeCheckFailed { sql, message };
            }
        }

        SqlGenOutcome::Terminate(crate::AnalyticsAnswer {
            text: sql,
            display_blocks: vec![],
            spec_hint: solution.semantic_query.clone(),
        })
    }

    /// Validate SQL by wrapping it in a `LIMIT 0` subquery so the engine
    /// parses and plans but doesn't scan. Cheap smoke check used by
    /// SQL-generation mode to catch malformed LLM output before the
    /// automation caches it to disk.
    ///
    /// Takes the resolved connector rather than a name: smoke-checking against
    /// a different warehouse than the query targets is worse than not checking
    /// at all -- it can pass SQL the real target rejects and reject SQL it
    /// accepts -- and resolution failure is the caller's to classify, not a
    /// probe result.
    async fn smoke_check_sql(
        sql: &str,
        connector: &dyn agentic_connector::DatabaseConnector,
    ) -> Result<(), String> {
        let trimmed = sql.trim().trim_end_matches(';');
        let wrapped = format!("SELECT * FROM ({trimmed}) AS __oxy_smoke LIMIT 0");
        connector
            .execute_query(&wrapped, 0)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

// State handler

/// Build the `StateHandler` for the **executing** state.
///
/// Path-aware diagnosis:
/// - `SemanticLayer` failures → `BackTarget::Specify` (re-enter LLM specifying)
/// - `LlmWithSemanticContext` failures → `BackTarget::Solve` (retry SQL generation)
pub(super) fn build_executing_handler()
-> StateHandler<AnalyticsDomain, AnalyticsSolver, AnalyticsEvent> {
    StateHandler {
        next: "interpreting",
        execute: Arc::new(
            |solver: &mut AnalyticsSolver,
             state,
             _events,
             run_ctx: &RunContext<AnalyticsDomain>,
             _memory: &SessionMemory<AnalyticsDomain>| {
                Box::pin(async move {
                    let solution = match state {
                        ProblemState::Executing(s) => s,
                        _ => unreachable!("executing handler called with wrong state"),
                    };
                    let solution_source = solution.solution_source.clone();

                    // ── SQL-generation mode ────────────────────────────────────
                    // Automations that set `output: { mode: sql }` on the
                    // agent task terminate the FSM here with the SQL as
                    // the answer. Pre-validated paths skip execution
                    // outright; LLM-generated SQL gets a `LIMIT 0` smoke
                    // check first. Vendor/automation paths are
                    // structurally incoherent with SQL-gen mode and are
                    // rejected with a clear message.
                    match solver.terminate_with_sql_if_enabled(&solution).await {
                        SqlGenOutcome::Disabled => {}
                        SqlGenOutcome::Terminate(answer) => {
                            return TransitionResult::ok_to(ProblemState::Done(answer), "done");
                        }
                        SqlGenOutcome::SmokeCheckFailed { sql, message } => {
                            // Route back to Solve so the LLM regenerates
                            // the SQL with the smoke-check error as
                            // context. Mirrors the existing
                            // LlmWithSemanticContext failure path.
                            emit_domain(
                                &solver.event_tx,
                                AnalyticsEvent::ExecutionFailed {
                                    query: sql.clone(),
                                    error: message.clone(),
                                    source: format!("{:?}", solution_source),
                                    will_retry: true,
                                },
                            )
                            .await;
                            let base = run_ctx.retry_ctx.clone().unwrap_or_default();
                            let mut hint = base.advance(format!("Smoke check failed: {message}"));
                            hint.previous_output = Some(format!("Failing SQL: {sql}"));
                            let spec = run_ctx.spec.clone().expect(
                                "run_ctx.spec must be set before executing in SQL-gen mode",
                            );
                            return TransitionResult::diagnosing(ProblemState::Diagnosing {
                                error: AnalyticsError::SyntaxError {
                                    query: sql,
                                    message,
                                },
                                back: BackTarget::Solve(spec, hint),
                            });
                        }
                        SqlGenOutcome::ConnectorUnavailable { sql, message } => {
                            // Fatal, not a retry. `NeedsUserInput` +
                            // `BackTarget::Execute` is what the two execute
                            // paths in this change already use for the same
                            // condition, and `diagnose_impl` escalates it
                            // rather than looping. `will_retry: false` so the
                            // UI does not promise a retry that cannot help.
                            tracing::error!("analytics sql-gen: {message}");
                            emit_domain(
                                &solver.event_tx,
                                AnalyticsEvent::ExecutionFailed {
                                    // The SQL is in scope and the sibling
                                    // branch passes it; an empty string leaves
                                    // the failed-query panel blank for no gain.
                                    query: sql.clone(),
                                    error: message.clone(),
                                    source: format!("{:?}", solution_source),
                                    will_retry: false,
                                },
                            )
                            .await;
                            return TransitionResult::diagnosing(ProblemState::Diagnosing {
                                error: AnalyticsError::NeedsUserInput { prompt: message },
                                back: BackTarget::Execute(solution.clone(), Default::default()),
                            });
                        }
                        SqlGenOutcome::IncompatiblePath { reason } => {
                            // Hard fail — the automation YAML is
                            // misconfigured (vendor / automation solution
                            // paired with `output: { mode: sql }`). Use
                            // `BackTarget::Execute` because
                            // `diagnose_impl` returns the error
                            // unchanged for the SyntaxError + Execute
                            // pair, which escalates to
                            // `OrchestratorError::Fatal`. No back-edge
                            // is reasonable here — the automation author
                            // needs to update the YAML.
                            return TransitionResult::diagnosing(ProblemState::Diagnosing {
                                error: AnalyticsError::SyntaxError {
                                    query: String::new(),
                                    message: format!("SQL-gen mode incompatible: {reason}"),
                                },
                                back: BackTarget::Execute(solution.clone(), Default::default()),
                            });
                        }
                    }

                    // ── Automation path — delegate via coordinator ──────────────
                    if let SolutionSource::Automation { ref file_path } = solution_source {
                        // Store suspension data so the orchestrator can
                        // resume from the Executing stage after the
                        // coordinator runs the automation as a child task.
                        // Store directly on the solver struct — the
                        // DomainSolver trait impl delegates to this field.
                        solver.suspension_data =
                            Some(agentic_core::human_input::SuspendedRunData {
                                from_state: "executing".into(),
                                original_input: run_ctx
                                    .intent
                                    .as_ref()
                                    .map(|i| i.raw_question.clone())
                                    .unwrap_or_default(),
                                trace_id: String::new(), // filled by orchestrator
                                stage_data: serde_json::json!({
                                    "intent": serde_json::to_value(run_ctx.intent.as_ref()).ok(),
                                    "spec": serde_json::to_value(run_ctx.spec.as_ref()).ok(),
                                }),
                                question: format!("Execute automation: {}", file_path.display()),
                                suggestions: vec![],
                            });
                        return TransitionResult::diagnosing(ProblemState::Diagnosing {
                            error: AnalyticsError::SyntaxError {
                                query: file_path.display().to_string(),
                                message: format!(
                                    "delegating automation execution: {}",
                                    file_path.display()
                                ),
                            },
                            back: BackTarget::Suspend {
                                reason: agentic_core::delegation::SuspendReason::Delegation {
                                    target:
                                        agentic_core::delegation::DelegationTarget::Automation {
                                            workflow_ref: file_path.to_string_lossy().to_string(),
                                        },
                                    request: format!("Execute automation {}", file_path.display()),
                                    context: serde_json::json!({}),
                                    policy: None,
                                },
                            },
                        });
                    }

                    match solver.execute_solution(solution).await {
                        Ok(result) => {
                            tracing::info!(
                                "[executing] query succeeded, source={:?}, rows={}",
                                solution_source,
                                result.primary().data.rows.len()
                            );
                            if let Some(spec) = &run_ctx.spec
                                && let Err(err) = solver.validator.validate_solved(&result, spec)
                            {
                                tracing::info!(
                                    "[executing] post-execution validation FAILED source={:?} error={err}",
                                    solution_source,
                                );
                                emit_domain(
                                    &solver.event_tx,
                                    AnalyticsEvent::ExecutionFailed {
                                        query: String::new(),
                                        error: err.to_string(),
                                        source: format!("{:?}", solution_source),
                                        will_retry: true,
                                    },
                                )
                                .await;
                                emit_domain(
                                    &solver.event_tx,
                                    AnalyticsEvent::ValidationFailed {
                                        state: "executing".to_string(),
                                        reason: err.to_string(),
                                        model_response: format!("{result:#?}"),
                                    },
                                )
                                .await;
                                let base = run_ctx.retry_ctx.clone().unwrap_or_default();
                                let compact = format_compact_result(&result);
                                let mut hint = base.advance(err.to_string());
                                hint.previous_output = Some(compact);
                                let back = if matches!(err, AnalyticsError::ValueAnomaly { .. }) {
                                    BackTarget::Interpret(result, hint)
                                } else {
                                    match solution_source {
                                        SolutionSource::SqlFile { .. } => {
                                            // SQL file content is fixed — retrying
                                            // Specify would re-execute the same
                                            // file. Bounce to Clarify so the LLM
                                            // can pick a different path.
                                            let intent = run_ctx
                                                .spec
                                                .as_ref()
                                                .map(|s| s.intent.clone())
                                                .or_else(|| run_ctx.intent.clone())
                                                .expect(
                                                    "run_ctx.intent must be set before executing",
                                                );
                                            BackTarget::Clarify(intent, hint)
                                        }
                                        SolutionSource::SemanticLayer
                                        | SolutionSource::Automation { .. }
                                        | SolutionSource::VendorEngine(_) => {
                                            let intent = run_ctx
                                                .spec
                                                .as_ref()
                                                .map(|s| s.intent.clone())
                                                .expect(
                                                    "run_ctx.spec must be set before executing",
                                                );
                                            BackTarget::Specify(intent, hint)
                                        }
                                        SolutionSource::LlmWithSemanticContext => {
                                            // The Mode 2 fast path
                                            // (`spec_to_executing` calling
                                            // `solve_impl` inline right after a
                                            // failed semantic compile) reaches
                                            // Executing without ever entering
                                            // `ProblemState::Solving`, so on this
                                            // first attempt `run_ctx.spec` is
                                            // still None — only a later retry
                                            // through `BackTarget::Solve`
                                            // populates it. Fall back to
                                            // re-Specifying from the intent
                                            // instead of panicking, mirroring the
                                            // SemanticLayer arm above.
                                            if let Some(spec) = run_ctx.spec.clone() {
                                                BackTarget::Solve(spec, hint)
                                            } else {
                                                let intent = run_ctx.intent.clone().expect(
                                                    "run_ctx.intent must be set before executing",
                                                );
                                                BackTarget::Specify(intent, hint)
                                            }
                                        }
                                    }
                                };
                                return TransitionResult::diagnosing(ProblemState::Diagnosing {
                                    error: err,
                                    back,
                                });
                            }
                            TransitionResult::ok(ProblemState::Interpreting(result))
                        }
                        Err((err, _back)) => {
                            tracing::info!(
                                "[executing] FAILED source={:?} error={err}",
                                solution_source,
                            );
                            let failing_query = match &err {
                                AnalyticsError::SyntaxError { query, .. } => query.clone(),
                                _ => String::new(),
                            };
                            emit_domain(
                                &solver.event_tx,
                                AnalyticsEvent::ExecutionFailed {
                                    query: failing_query,
                                    error: err.to_string(),
                                    source: format!("{:?}", solution_source),
                                    will_retry: true,
                                },
                            )
                            .await;
                            let base = run_ctx.retry_ctx.clone().unwrap_or_default();
                            let failing_sql = match &err {
                                AnalyticsError::SyntaxError { query, .. } => Some(query.clone()),
                                _ => None,
                            };
                            let mut hint = base.advance(err.to_string());
                            if let Some(sql) = failing_sql {
                                hint.previous_output = Some(format!("Failing SQL: {sql}"));
                            }
                            let back =
                                match solution_source {
                                    SolutionSource::SqlFile { .. } => {
                                        // Pre-written SQL files are authoritative — the
                                        // file content won't change between attempts, so
                                        // re-running Specify would just re-execute the
                                        // same failing SQL. Route back to Clarify with
                                        // the error so the LLM can either pick a
                                        // different file or fall through to LLM SQL
                                        // generation, instead of looping on the same
                                        // file until the retry budget is exhausted.
                                        let intent = run_ctx
                                            .spec
                                            .as_ref()
                                            .map(|s| s.intent.clone())
                                            .or_else(|| run_ctx.intent.clone())
                                            .expect("run_ctx.intent must be set before executing");
                                        BackTarget::Clarify(intent, hint)
                                    }
                                    SolutionSource::SemanticLayer
                                    | SolutionSource::Automation { .. }
                                    | SolutionSource::VendorEngine(_) => {
                                        // When spec is available (normal path), retry
                                        // from Specify. When it's None (semantic
                                        // shortcut skipped Specifying/Solving), fall
                                        // back to Clarify via run_ctx.intent.
                                        if let Some(intent) =
                                            run_ctx.spec.as_ref().map(|s| s.intent.clone())
                                        {
                                            BackTarget::Specify(intent, hint)
                                        } else {
                                            let intent = run_ctx.intent.clone().expect(
                                                "run_ctx.intent must be set before executing",
                                            );
                                            BackTarget::Clarify(intent, hint)
                                        }
                                    }
                                    SolutionSource::LlmWithSemanticContext => {
                                        // Same shortcut-skips-Solving gap as the
                                        // validation-failure arm above: the Mode 2
                                        // fast path never populates run_ctx.spec on
                                        // its first attempt, so a first-attempt
                                        // execution failure (e.g. the LLM's
                                        // generated SQL references a table that
                                        // doesn't exist) must not assume spec is
                                        // set. Fall back to Clarify via
                                        // run_ctx.intent instead of panicking —
                                        // this was observed live as `panicked at
                                        // .../executing/mod.rs:985:69: run_ctx.spec
                                        // must be set for LlmWithSemanticContext
                                        // path`.
                                        if let Some(spec) = run_ctx.spec.clone() {
                                            BackTarget::Solve(spec, hint)
                                        } else {
                                            let intent = run_ctx.intent.clone().expect(
                                                "run_ctx.intent must be set before executing",
                                            );
                                            BackTarget::Clarify(intent, hint)
                                        }
                                    }
                                };
                            TransitionResult::diagnosing(ProblemState::Diagnosing {
                                error: err,
                                back,
                            })
                        }
                    }
                })
            },
        ),
        diagnose: None,
    }
}

// Helpers

/// Convert a [`SubrunOutput`] into an [`AnalyticsResult`] that the
/// Interpreting stage can consume.
///
/// Each automation step becomes its own `QueryResultSet`. Table steps carry
/// real columns and rows; non-table steps are wrapped in a single-cell table.
#[cfg(test)]
fn automation_output_to_result(output: SubrunOutput) -> AnalyticsResult {
    use crate::types::QueryResultSet;
    use agentic_core::result::{CellValue, QueryResult, QueryRow};

    if output.steps.is_empty() {
        return AnalyticsResult::single(
            QueryResult {
                columns: vec!["result".to_string()],
                rows: vec![QueryRow(vec![CellValue::Text(
                    "(automation produced no output)".to_string(),
                )])],
                total_row_count: 1,
                truncated: false,
            },
            None,
        );
    }

    AnalyticsResult {
        results: output
            .steps
            .into_iter()
            .map(|step| {
                let rows = step
                    .rows
                    .into_iter()
                    .map(|row| QueryRow(row.into_iter().map(json_to_cell).collect()))
                    .collect();
                QueryResultSet {
                    data: QueryResult {
                        columns: step.columns,
                        rows,
                        total_row_count: step.total_row_count,
                        truncated: step.truncated,
                    },
                    summary: None,
                }
            })
            .collect(),
    }
}

/// Convert a typed JSON value (from `to_typed_rows`) into a [`CellValue`].
#[cfg(test)]
fn json_to_cell(v: serde_json::Value) -> agentic_core::result::CellValue {
    match v {
        serde_json::Value::Number(n) => {
            agentic_core::result::CellValue::Number(n.as_f64().unwrap_or(0.0))
        }
        serde_json::Value::String(s) => agentic_core::result::CellValue::Text(s),
        serde_json::Value::Null => agentic_core::result::CellValue::Null,
        other => agentic_core::result::CellValue::Text(other.to_string()),
    }
}

#[cfg(test)]
mod tests;
