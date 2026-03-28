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

use crate::engine::EngineError;
use crate::events::AnalyticsEvent;
use crate::procedure::ProcedureOutput;
use crate::types::{SolutionPayload, SolutionSource};

use crate::{AnalyticsDomain, AnalyticsError, AnalyticsResult, AnalyticsSolution};

use super::{emit_domain, AnalyticsSolver};

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
    pub(crate) async fn execute_solution(
        &mut self,
        solution: AnalyticsSolution,
    ) -> Result<AnalyticsResult, (AnalyticsError, BackTarget<AnalyticsDomain>)> {
        const DEFAULT_SAMPLE_LIMIT: u64 = 1_000;

        let start = std::time::Instant::now();

        match &solution.payload {
            SolutionPayload::Sql(sql) => {
                eprintln!(
                    "[executing] running SQL (source={:?}, connector={}):\n{}",
                    solution.solution_source, solution.connector_name, sql
                );
                let connector = self
                    .connectors
                    .get(&solution.connector_name)
                    .or_else(|| self.connectors.get(&self.default_connector))
                    .or_else(|| self.connectors.values().next())
                    .expect("AnalyticsSolver must have at least one connector")
                    .clone();
                let sql = sql.clone();
                match connector.execute_query(&sql, DEFAULT_SAMPLE_LIMIT).await {
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
                            },
                        )
                        .await;
                        Ok(AnalyticsResult::single(exec.result, Some(exec.summary)))
                    }
                    Err(e) => {
                        let duration_ms = start.elapsed().as_millis() as u64;
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
                match engine.execute(&vq).await {
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
                            },
                        )
                        .await;
                        Ok(AnalyticsResult::single(result, None))
                    }
                    Err(e) => {
                        let duration_ms = start.elapsed().as_millis() as u64;
                        let message = e.to_string();
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
pub(super) fn build_executing_handler(
) -> StateHandler<AnalyticsDomain, AnalyticsSolver, AnalyticsEvent> {
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

                    // ── Procedure path ─────────────────────────────────────────
                    if let SolutionSource::Procedure { ref file_path } = solution_source {
                        let intent = run_ctx
                            .spec
                            .as_ref()
                            .map(|s| s.intent.clone())
                            .expect("run_ctx.spec must be set before executing");
                        return match solver.procedure_runner.as_ref() {
                            Some(runner) => match runner.run(file_path).await {
                                Ok(output) => TransitionResult::ok(ProblemState::Interpreting(
                                    procedure_output_to_result(output),
                                )),
                                Err(e) => {
                                    let hint = run_ctx
                                        .retry_ctx
                                        .clone()
                                        .unwrap_or_default()
                                        .advance(e.to_string());
                                    TransitionResult::diagnosing(ProblemState::Diagnosing {
                                        error: AnalyticsError::SyntaxError {
                                            query: file_path.display().to_string(),
                                            message: e.to_string(),
                                        },
                                        back: BackTarget::Specify(intent, hint),
                                    })
                                }
                            },
                            None => {
                                let hint = run_ctx
                                    .retry_ctx
                                    .clone()
                                    .unwrap_or_default()
                                    .advance("no procedure runner configured".to_string());
                                TransitionResult::diagnosing(ProblemState::Diagnosing {
                                    error: AnalyticsError::SyntaxError {
                                        query: file_path.display().to_string(),
                                        message: "no ProcedureRunner registered on this solver"
                                            .into(),
                                    },
                                    back: BackTarget::Specify(intent, hint),
                                })
                            }
                        };
                    }

                    match solver.execute_solution(solution).await {
                        Ok(result) => {
                            eprintln!(
                                "[executing] query succeeded, source={:?}, rows={}",
                                solution_source,
                                result.primary().data.rows.len()
                            );
                            if let Some(spec) = &run_ctx.spec {
                                if let Err(err) = solver.validator.validate_solved(&result, spec) {
                                    eprintln!(
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
                                    let back = if matches!(err, AnalyticsError::ValueAnomaly { .. })
                                    {
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
                                    return TransitionResult::diagnosing(
                                        ProblemState::Diagnosing { error: err, back },
                                    );
                                }
                            }
                            TransitionResult::ok(ProblemState::Interpreting(result))
                        }
                        Err((err, _back)) => {
                            eprintln!(
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
                                    let intent = run_ctx
                                        .spec
                                        .as_ref()
                                        .map(|s| s.intent.clone())
                                        .expect("run_ctx.spec must be set before executing");
                                    BackTarget::Specify(intent, hint)
                                }
                                SolutionSource::LlmWithSemanticContext => {
                                    let spec = run_ctx
                                        .spec
                                        .clone()
                                        .expect("run_ctx.spec must be set before executing");
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
mod procedure_output_tests {
    use super::*;
    use crate::procedure::{ProcedureOutput, ProcedureStepResult};

    fn make_step(
        name: &str,
        cols: Vec<&str>,
        rows: Vec<Vec<serde_json::Value>>,
    ) -> ProcedureStepResult {
        let row_count = rows.len() as u64;
        ProcedureStepResult {
            step_name: name.to_string(),
            columns: cols.into_iter().map(String::from).collect(),
            rows,
            truncated: false,
            total_row_count: row_count,
        }
    }

    #[test]
    fn empty_steps_returns_placeholder() {
        let result = procedure_output_to_result(ProcedureOutput { steps: vec![] });
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].data.columns, vec!["result"]);
    }

    #[test]
    fn single_step_produces_single_result_set() {
        let result = procedure_output_to_result(ProcedureOutput {
            steps: vec![make_step(
                "q1",
                vec!["a", "b"],
                vec![vec![serde_json::json!("x"), serde_json::json!(2)]],
            )],
        });
        assert_eq!(result.results.len(), 1);
        assert!(!result.is_multi());
        assert_eq!(result.results[0].data.columns, vec!["a", "b"]);
        assert_eq!(result.results[0].data.total_row_count, 1);
    }

    #[test]
    fn multiple_steps_produce_multi_result() {
        let result = procedure_output_to_result(ProcedureOutput {
            steps: vec![
                make_step("q1", vec!["x"], vec![vec![serde_json::json!(1)]]),
                make_step("q2", vec!["y"], vec![vec![serde_json::json!(2)]]),
                make_step("q3", vec!["z"], vec![vec![serde_json::json!(3)]]),
            ],
        });
        assert_eq!(result.results.len(), 3);
        assert!(result.is_multi());
    }

    // Numeric JSON values from to_typed_rows must arrive as CellValue::Number
    // so that the chart renderer receives proper JSON numbers, not strings.
    #[test]
    fn numeric_json_cells_become_number_cell_values() {
        use agentic_core::result::CellValue;

        let result = procedure_output_to_result(ProcedureOutput {
            steps: vec![make_step(
                "revenue_by_region",
                vec!["region", "total_revenue"],
                vec![
                    vec![serde_json::json!("North"), serde_json::json!(42000.0)],
                    vec![serde_json::json!("South"), serde_json::json!(31500.5)],
                    vec![serde_json::json!("West"), serde_json::json!(0)],
                ],
            )],
        });
        let rows = &result.results[0].data.rows;
        // String values stay as text.
        assert!(matches!(&rows[0].0[0], CellValue::Text(s) if s == "North"));
        // JSON numbers become CellValue::Number.
        assert!(
            matches!(rows[0].0[1], CellValue::Number(n) if n == 42000.0),
            "expected Number(42000.0), got {:?}",
            rows[0].0[1]
        );
        assert!(
            matches!(rows[1].0[1], CellValue::Number(n) if n == 31500.5),
            "expected Number(31500.5), got {:?}",
            rows[1].0[1]
        );
        assert!(
            matches!(rows[2].0[1], CellValue::Number(n) if n == 0.0),
            "expected Number(0.0), got {:?}",
            rows[2].0[1]
        );
    }
}
