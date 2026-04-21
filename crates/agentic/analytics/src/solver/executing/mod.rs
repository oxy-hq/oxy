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
#[cfg(test)]
use crate::procedure::ProcedureOutput;
use crate::types::{SolutionPayload, SolutionSource};

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
        // Procedure solutions are intercepted before this code path runs.
        SolutionSource::Procedure { .. } => ("sql_generated", false),
    }
}

// ---------------------------------------------------------------------------
// Compact result formatter (for retry context)
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// execute_solution body
// ---------------------------------------------------------------------------

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
            SolutionSource::VendorEngine(_) => QuerySource::Vendor,
            // Procedure solutions are intercepted by `build_executing_handler` before
            // `execute_solution` is ever called, so this arm is unreachable for that
            // variant. Kept in the pattern only to satisfy exhaustiveness.
            SolutionSource::LlmWithSemanticContext | SolutionSource::Procedure { .. } => {
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

                let connector = self
                    .connectors
                    .get(&solution.connector_name)
                    .or_else(|| self.connectors.get(&self.default_connector))
                    .or_else(|| self.connectors.values().next())
                    .expect("AnalyticsSolver must have at least one connector")
                    .clone();
                let sql = sql.clone();
                // Child `tool_call` span so this execution shows up in the
                // Execution Analytics tab alongside classic agent tool calls.
                let (execution_type, is_verified) = execution_type_for(&solution.solution_source);
                let tool_span = tracing::info_span!(
                    "analytics.tool_call",
                    oxy.name = "analytics.tool_call",
                    oxy.span_type = "tool_call",
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
        }
    }
}

// ---------------------------------------------------------------------------
// State handler
// ---------------------------------------------------------------------------

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

                    // ── Procedure path — delegate via coordinator ──────────────
                    if let SolutionSource::Procedure { ref file_path } = solution_source {
                        // Store suspension data so the orchestrator can
                        // resume from the Executing stage after the
                        // coordinator runs the workflow as a child task.
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
                                question: format!("Execute procedure: {}", file_path.display()),
                                suggestions: vec![],
                            });
                        return TransitionResult::diagnosing(ProblemState::Diagnosing {
                            error: AnalyticsError::SyntaxError {
                                query: file_path.display().to_string(),
                                message: format!(
                                    "delegating procedure execution: {}",
                                    file_path.display()
                                ),
                            },
                            back: BackTarget::Suspend {
                                reason: agentic_core::delegation::SuspendReason::Delegation {
                                    target: agentic_core::delegation::DelegationTarget::Workflow {
                                        workflow_ref: file_path.to_string_lossy().to_string(),
                                    },
                                    request: format!("Execute procedure {}", file_path.display()),
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
                                        SolutionSource::SemanticLayer
                                        | SolutionSource::Procedure { .. }
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
                                            let spec = run_ctx.spec.clone().expect(
                                                "run_ctx.spec must be set before executing",
                                            );
                                            BackTarget::Solve(spec, hint)
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
                            let back = match solution_source {
                                SolutionSource::SemanticLayer
                                | SolutionSource::Procedure { .. }
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
                                        let intent = run_ctx
                                            .intent
                                            .clone()
                                            .expect("run_ctx.intent must be set before executing");
                                        BackTarget::Clarify(intent, hint)
                                    }
                                }
                                SolutionSource::LlmWithSemanticContext => {
                                    let spec = run_ctx.spec.clone().expect(
                                        "run_ctx.spec must be set for LlmWithSemanticContext path",
                                    );
                                    BackTarget::Solve(spec, hint)
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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert a [`ProcedureOutput`] into an [`AnalyticsResult`] that the
/// Interpreting stage can consume.
///
/// Each procedure step becomes its own `QueryResultSet`. Table steps carry
/// real columns and rows; non-table steps are wrapped in a single-cell table.
#[cfg(test)]
fn procedure_output_to_result(output: ProcedureOutput) -> AnalyticsResult {
    use crate::types::QueryResultSet;
    use agentic_core::result::{CellValue, QueryResult, QueryRow};

    if output.steps.is_empty() {
        return AnalyticsResult::single(
            QueryResult {
                columns: vec!["result".to_string()],
                rows: vec![QueryRow(vec![CellValue::Text(
                    "(procedure produced no output)".to_string(),
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
