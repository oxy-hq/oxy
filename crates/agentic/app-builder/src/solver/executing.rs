//! **Executing** pipeline stage for the app builder domain.
//!
//! Two-pass execution:
//! - Pass 1: control-source tasks (no template substitution, must run first)
//! - Pass 2: display tasks (default control values substituted into SQL)

use std::collections::HashSet;
use std::sync::Arc;

use agentic_core::{
    back_target::BackTarget,
    orchestrator::{RunContext, SessionMemory, StateHandler, TransitionResult},
    state::ProblemState,
};
use regex::Regex;
use sqlparser::ast::{Expr, GroupByExpr, Query, Select, SelectItem, SetExpr, Statement};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

use crate::events::AppBuilderEvent;
use crate::types::{
    AppBuilderDomain, AppBuilderError, AppResult, AppSolution, ControlPlan, TaskResult,
};

use agentic_core::result::{CellValue, QueryResult};

use super::solver::AppBuilderSolver;

/// Extract up to 5 sample rows as strings for event payloads.
pub(crate) fn query_result_sample_rows(result: &QueryResult) -> Vec<Vec<String>> {
    result
        .rows
        .iter()
        .take(5)
        .map(|row| {
            row.0
                .iter()
                .map(|cell| match cell {
                    CellValue::Text(s) => s.clone(),
                    CellValue::Number(n) => n.to_string(),
                    CellValue::Null => "NULL".to_string(),
                })
                .collect()
        })
        .collect()
}

// ── Template substitution ─────────────────────────────────────────────────────

/// Substitute `{{ controls.X | sqlquote }}` references with the control's
/// default value, properly quoted.
pub(crate) fn substitute_defaults(sql: &str, controls: &[ControlPlan]) -> String {
    let re = Regex::new(r"\{\{\s*controls\.(\w+)\s*\|\s*sqlquote\s*\}\}").unwrap();
    re.replace_all(sql, |caps: &regex::Captures<'_>| {
        let name = &caps[1];
        let default = controls
            .iter()
            .find(|c| c.name == name)
            .map(|c| c.default.as_str())
            .unwrap_or("__default__");
        format!("'{}'", default.replace('\'', "''"))
    })
    .into_owned()
}

/// Check that all `{{ controls.X | sqlquote }}` references in SQL resolve to
/// known control names. Returns unresolved names.
pub(crate) fn validate_control_refs(sql: &str, controls: &[ControlPlan]) -> Vec<String> {
    let re = Regex::new(r"\{\{\s*controls\.(\w+)\s*\|\s*sqlquote\s*\}\}").unwrap();
    let known: HashSet<&str> = controls.iter().map(|c| c.name.as_str()).collect();
    let mut missing = Vec::new();
    for caps in re.captures_iter(sql) {
        let name = caps.get(1).unwrap().as_str();
        if !known.contains(name) {
            missing.push(name.to_string());
        }
    }
    missing.sort();
    missing.dedup();
    missing
}

// ── GROUP BY validation ───────────────────────────────────────────────────────

/// Aggregate function names (lowercase) that are legal in a GROUP BY query
/// without being listed in the GROUP BY clause.
const AGGREGATE_FUNCTIONS: &[&str] = &[
    "count",
    "sum",
    "avg",
    "min",
    "max",
    "stddev",
    "stddev_pop",
    "stddev_samp",
    "variance",
    "var_pop",
    "var_samp",
    "any_value",
    "array_agg",
    "string_agg",
    "group_concat",
    "listagg",
    "approx_count_distinct",
    "median",
    "mode",
    "percentile_cont",
    "percentile_disc",
    "first_value",
    "last_value",
    "nth_value",
];

/// Returns true if `expr` is a top-level call to a known aggregate function.
fn is_aggregate_expr(expr: &Expr) -> bool {
    match expr {
        Expr::Function(f) => {
            let name = f
                .name
                .0
                .last()
                .map(|n| n.value.to_lowercase())
                .unwrap_or_default();
            AGGREGATE_FUNCTIONS.contains(&name.as_str())
        }
        Expr::Nested(inner) => is_aggregate_expr(inner),
        _ => false,
    }
}

/// Replace Jinja template placeholders (`{{ … }}`) with a SQL string literal
/// so that `sqlparser` can parse the skeleton of the query.
fn strip_template_placeholders(sql: &str) -> String {
    let re = Regex::new(r"\{\{[^}]*\}\}").unwrap();
    re.replace_all(sql, "'__placeholder__'").into_owned()
}

/// Validate a parsed `Select` node: every non-aggregate, non-wildcard item in
/// the projection must appear verbatim in the GROUP BY expression list.
fn check_select_group_by(select: &Select) -> Result<(), String> {
    let group_exprs = match &select.group_by {
        GroupByExpr::Expressions(exprs, _) if !exprs.is_empty() => exprs,
        _ => return Ok(()), // no GROUP BY → nothing to check
    };

    let group_strs: Vec<String> = group_exprs
        .iter()
        .map(|e| e.to_string().to_lowercase())
        .collect();

    for item in &select.projection {
        let expr = match item {
            SelectItem::UnnamedExpr(e) => e,
            SelectItem::ExprWithAlias { expr, .. } => expr,
            SelectItem::Wildcard(_) | SelectItem::QualifiedWildcard(..) => continue,
        };

        if is_aggregate_expr(expr) {
            continue;
        }

        let expr_str = expr.to_string().to_lowercase();
        if !group_strs.contains(&expr_str) {
            return Err(format!(
                "Column or expression `{expr}` appears in SELECT but is not in the GROUP BY \
                 clause and is not an aggregate function.\n\
                 Fix options:\n\
                 \x20 1. Add `{expr}` to the GROUP BY clause.\n\
                 \x20 2. Wrap it in an aggregate, e.g. `ANY_VALUE({expr})`.\n\
                 \x20 3. Use a subquery: move the aggregation to an inner query and \
                 select `{expr}` from the outer query."
            ));
        }
    }
    Ok(())
}

/// Recursively validate GROUP BY rules for a `Query` node (handles
/// subqueries in `FROM` and nested `SELECT`s).
fn check_query_group_by(query: &Query) -> Result<(), String> {
    match query.body.as_ref() {
        SetExpr::Select(select) => check_select_group_by(select),
        SetExpr::Query(q) => check_query_group_by(q),
        _ => Ok(()),
    }
}

/// Parse `sql` and verify that every `SELECT` with a `GROUP BY` satisfies the
/// rule that non-aggregate columns must appear in the `GROUP BY` list.
///
/// Template placeholders (`{{ controls.X | sqlquote }}`) are stripped before
/// parsing so the skeleton of the query can be analysed statically.
///
/// Returns `Ok(())` if no violation is found (or if the SQL cannot be parsed —
/// the database engine will surface syntax errors at execution time).
pub(crate) fn validate_group_by_rules(sql: &str) -> Result<(), String> {
    let stripped = strip_template_placeholders(sql);
    let stmts = match Parser::parse_sql(&GenericDialect {}, stripped.trim()) {
        Ok(s) => s,
        Err(_) => return Ok(()), // let the DB handle syntax errors
    };
    for stmt in &stmts {
        if let Statement::Query(query) = stmt {
            check_query_group_by(query)?;
        }
    }
    Ok(())
}

/// Check that expected columns (if any) exist in the actual result columns.
/// Returns `None` if valid, or a `ShapeMismatch` error if columns are missing.
pub(crate) fn check_expected_columns(
    task: &crate::types::ResolvedTask,
    actual_columns: &[String],
) -> Option<AppBuilderError> {
    if task.expected_columns.is_empty() {
        return None;
    }
    let actual_lower: HashSet<String> = actual_columns.iter().map(|c| c.to_lowercase()).collect();
    let missing: Vec<&str> = task
        .expected_columns
        .iter()
        .filter(|ec| !actual_lower.contains(&ec.to_lowercase()))
        .map(|s| s.as_str())
        .collect();
    if missing.is_empty() {
        None
    } else {
        Some(AppBuilderError::ShapeMismatch {
            task_name: task.name.clone(),
            expected: format!("columns [{}]", task.expected_columns.join(", ")),
            actual: format!(
                "columns [{}] (missing: {})",
                actual_columns.join(", "),
                missing.join(", ")
            ),
        })
    }
}

// ---------------------------------------------------------------------------
// execute_impl
// ---------------------------------------------------------------------------

impl AppBuilderSolver {
    /// Emit a `TaskExecutionFailed` event if the event channel is available.
    async fn emit_task_failure(&self, task_name: &str, sql: &str, error: &str) {
        if let Some(tx) = &self.event_tx {
            let _ = tx
                .send(agentic_core::events::Event::Domain(
                    AppBuilderEvent::TaskExecutionFailed {
                        task_name: task_name.to_string(),
                        sql: sql.to_string(),
                        error: error.to_string(),
                        will_retry: true,
                    },
                ))
                .await;
        }
    }

    /// Execute all tasks in two passes: control-source first, then display.
    pub(crate) async fn execute_impl(
        &mut self,
        solution: AppSolution,
    ) -> Result<AppResult, (AppBuilderError, BackTarget<AppBuilderDomain>)> {
        let connector = self
            .connectors
            .get(&solution.connector_name)
            .or_else(|| self.connectors.get(&self.default_connector))
            .or_else(|| self.connectors.values().next())
            .expect("AppBuilderSolver must have at least one connector")
            .clone();

        let mut task_results: Vec<TaskResult> = Vec::new();

        // Pass 1 — control-source tasks (no substitution).
        for task in solution.tasks.iter().filter(|t| t.is_control_source) {
            if let Err(msg) = validate_group_by_rules(&task.sql) {
                self.emit_task_failure(&task.name, &task.sql, &msg).await;
                return Err((
                    AppBuilderError::SyntaxError {
                        query: task.sql.clone(),
                        message: msg,
                    },
                    BackTarget::Execute(solution, Default::default()),
                ));
            }
            match connector.execute_query(&task.sql, 200).await {
                Ok(exec) => {
                    let row_count = exec.result.total_row_count;
                    if row_count == 0 {
                        let msg = format!("task '{}' returned no rows", task.name);
                        self.emit_task_failure(&task.name, &task.sql, &msg).await;
                        return Err((
                            AppBuilderError::EmptyResults {
                                task_name: task.name.clone(),
                            },
                            BackTarget::Execute(solution, Default::default()),
                        ));
                    }
                    if let Some(tx) = &self.event_tx {
                        let _ = tx
                            .send(agentic_core::events::Event::Domain(
                                AppBuilderEvent::TaskExecuted {
                                    task_name: task.name.clone(),
                                    sql: task.sql.clone(),
                                    row_count: row_count as usize,
                                    columns: exec.result.columns.clone(),
                                    sample_rows: query_result_sample_rows(&exec.result),
                                },
                            ))
                            .await;
                    }
                    if let Some(shape_err) = check_expected_columns(task, &exec.result.columns) {
                        self.emit_task_failure(&task.name, &task.sql, &shape_err.to_string())
                            .await;
                        return Err((shape_err, BackTarget::Execute(solution, Default::default())));
                    }
                    let column_types: Vec<Option<String>> = exec
                        .summary
                        .columns
                        .iter()
                        .map(|c| c.data_type.clone())
                        .collect();
                    task_results.push(TaskResult {
                        name: task.name.clone(),
                        sql: task.sql.clone(),
                        columns: exec.result.columns.clone(),
                        column_types,
                        row_count: row_count as usize,
                        is_control_source: true,
                        expected_shape: task.expected_shape.clone(),
                        expected_columns: task.expected_columns.clone(),
                        sample: exec.result,
                    });
                }
                Err(e) => {
                    self.emit_task_failure(&task.name, &task.sql, &e.to_string())
                        .await;
                    return Err((
                        AppBuilderError::SyntaxError {
                            query: task.sql.clone(),
                            message: e.to_string(),
                        },
                        BackTarget::Execute(solution, Default::default()),
                    ));
                }
            }
        }

        // Validate control references in display task SQL before execution.
        let mut all_missing = Vec::new();
        for task in solution.tasks.iter().filter(|t| !t.is_control_source) {
            let missing = validate_control_refs(&task.sql, &solution.controls);
            for name in missing {
                all_missing.push(format!(
                    "task '{}' references unknown control '{name}'",
                    task.name
                ));
            }
        }
        if !all_missing.is_empty() {
            return Err((
                AppBuilderError::InvalidSpec {
                    errors: all_missing,
                },
                BackTarget::Execute(solution, Default::default()),
            ));
        }

        // Pass 2 — display tasks (substitute default control values).
        for task in solution.tasks.iter().filter(|t| !t.is_control_source) {
            // Validate GROUP BY on the raw SQL (template placeholders are stripped internally)
            // so the structural check runs without needing control values resolved.
            if let Err(msg) = validate_group_by_rules(&task.sql) {
                self.emit_task_failure(&task.name, &task.sql, &msg).await;
                return Err((
                    AppBuilderError::SyntaxError {
                        query: task.sql.clone(),
                        message: msg,
                    },
                    BackTarget::Execute(solution, Default::default()),
                ));
            }
            let substituted = substitute_defaults(&task.sql, &solution.controls);
            match connector.execute_query(&substituted, 100).await {
                Ok(exec) => {
                    let row_count = exec.result.total_row_count;
                    if row_count == 0 {
                        let msg = format!("task '{}' returned no rows", task.name);
                        self.emit_task_failure(&task.name, &substituted, &msg).await;
                        return Err((
                            AppBuilderError::EmptyResults {
                                task_name: task.name.clone(),
                            },
                            BackTarget::Execute(solution, Default::default()),
                        ));
                    }
                    if let Some(tx) = &self.event_tx {
                        let _ = tx
                            .send(agentic_core::events::Event::Domain(
                                AppBuilderEvent::TaskExecuted {
                                    task_name: task.name.clone(),
                                    sql: substituted.clone(),
                                    row_count: row_count as usize,
                                    columns: exec.result.columns.clone(),
                                    sample_rows: query_result_sample_rows(&exec.result),
                                },
                            ))
                            .await;
                    }
                    if let Some(shape_err) = check_expected_columns(task, &exec.result.columns) {
                        self.emit_task_failure(&task.name, &substituted, &shape_err.to_string())
                            .await;
                        return Err((shape_err, BackTarget::Execute(solution, Default::default())));
                    }
                    let column_types: Vec<Option<String>> = exec
                        .summary
                        .columns
                        .iter()
                        .map(|c| c.data_type.clone())
                        .collect();
                    task_results.push(TaskResult {
                        name: task.name.clone(),
                        sql: task.sql.clone(),
                        columns: exec.result.columns.clone(),
                        column_types,
                        row_count: row_count as usize,
                        is_control_source: false,
                        expected_shape: task.expected_shape.clone(),
                        expected_columns: task.expected_columns.clone(),
                        sample: exec.result,
                    });
                }
                Err(e) => {
                    self.emit_task_failure(&task.name, &substituted, &e.to_string())
                        .await;
                    return Err((
                        AppBuilderError::SyntaxError {
                            query: task.sql.clone(),
                            message: e.to_string(),
                        },
                        BackTarget::Execute(solution, Default::default()),
                    ));
                }
            }
        }

        Ok(AppResult {
            task_results,
            controls: solution.controls,
            layout: solution.layout,
            connector_name: solution.connector_name,
        })
    }
}

// ---------------------------------------------------------------------------
// State handler
// ---------------------------------------------------------------------------

/// Build the `StateHandler` for the **executing** state.
pub(super) fn build_executing_handler()
-> StateHandler<AppBuilderDomain, AppBuilderSolver, AppBuilderEvent> {
    StateHandler {
        next: "interpreting",
        execute: Arc::new(
            |solver: &mut AppBuilderSolver,
             state,
             _events,
             _run_ctx: &RunContext<AppBuilderDomain>,
             _memory: &SessionMemory<AppBuilderDomain>| {
                Box::pin(async move {
                    let solution = match state {
                        ProblemState::Executing(s) => s,
                        _ => unreachable!("executing handler called with wrong state"),
                    };
                    match solver.execute_impl(solution).await {
                        Ok(result) => TransitionResult::ok(ProblemState::Interpreting(result)),
                        Err((err, back)) => {
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

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_substitute_defaults_basic() {
        let controls = vec![ControlPlan {
            name: "store".into(),
            label: "Store".into(),
            control_type: crate::types::ControlType::Select,
            source_task: None,
            options: vec![],
            default: "All".into(),
        }];
        let sql = "SELECT * FROM sales WHERE store = {{ controls.store | sqlquote }}";
        let result = substitute_defaults(sql, &controls);
        assert_eq!(result, "SELECT * FROM sales WHERE store = 'All'");
    }

    #[test]
    fn test_substitute_defaults_multiple() {
        let controls = vec![
            ControlPlan {
                name: "store".into(),
                label: "Store".into(),
                control_type: crate::types::ControlType::Select,
                source_task: None,
                options: vec![],
                default: "NYC".into(),
            },
            ControlPlan {
                name: "year".into(),
                label: "Year".into(),
                control_type: crate::types::ControlType::Select,
                source_task: None,
                options: vec![],
                default: "2024".into(),
            },
        ];
        let sql = "SELECT * FROM t WHERE store = {{ controls.store | sqlquote }} AND year = {{ controls.year | sqlquote }}";
        let result = substitute_defaults(sql, &controls);
        assert_eq!(
            result,
            "SELECT * FROM t WHERE store = 'NYC' AND year = '2024'"
        );
    }

    #[test]
    fn test_substitute_defaults_escapes_quotes() {
        let controls = vec![ControlPlan {
            name: "name".into(),
            label: "Name".into(),
            control_type: crate::types::ControlType::Select,
            source_task: None,
            options: vec![],
            default: "O'Brien".into(),
        }];
        let sql = "SELECT * FROM t WHERE name = {{ controls.name | sqlquote }}";
        let result = substitute_defaults(sql, &controls);
        assert_eq!(result, "SELECT * FROM t WHERE name = 'O''Brien'");
    }

    #[test]
    fn test_validate_control_refs_valid() {
        let controls = vec![ControlPlan {
            name: "store".into(),
            label: "Store".into(),
            control_type: crate::types::ControlType::Select,
            source_task: None,
            options: vec![],
            default: "All".into(),
        }];
        let sql = "SELECT * FROM t WHERE store = {{ controls.store | sqlquote }}";
        assert!(validate_control_refs(sql, &controls).is_empty());
    }

    #[test]
    fn test_validate_control_refs_missing() {
        let controls = vec![ControlPlan {
            name: "store".into(),
            label: "Store".into(),
            control_type: crate::types::ControlType::Select,
            source_task: None,
            options: vec![],
            default: "All".into(),
        }];
        let sql = "SELECT * FROM t WHERE region = {{ controls.region | sqlquote }}";
        let missing = validate_control_refs(sql, &controls);
        assert_eq!(missing, vec!["region"]);
    }

    #[test]
    fn test_validate_control_refs_no_refs() {
        let controls = vec![];
        let sql = "SELECT * FROM t";
        assert!(validate_control_refs(sql, &controls).is_empty());
    }

    #[test]
    fn test_check_expected_columns_valid() {
        let task = crate::types::ResolvedTask {
            name: "t1".into(),
            sql: "".into(),
            is_control_source: false,
            expected_shape: crate::types::ResultShape::TimeSeries,
            expected_columns: vec!["month".into(), "revenue".into()],
        };
        let actual = vec!["month".into(), "revenue".into(), "extra".into()];
        assert!(check_expected_columns(&task, &actual).is_none());
    }

    #[test]
    fn test_check_expected_columns_missing() {
        let task = crate::types::ResolvedTask {
            name: "t1".into(),
            sql: "".into(),
            is_control_source: false,
            expected_shape: crate::types::ResultShape::TimeSeries,
            expected_columns: vec!["month".into(), "revenue".into()],
        };
        let actual = vec!["month".into(), "sales".into()];
        let err = check_expected_columns(&task, &actual);
        assert!(err.is_some());
        match err.unwrap() {
            AppBuilderError::ShapeMismatch { task_name, .. } => {
                assert_eq!(task_name, "t1");
            }
            _ => panic!("expected ShapeMismatch"),
        }
    }

    #[test]
    fn test_check_expected_columns_empty_expected() {
        let task = crate::types::ResolvedTask {
            name: "t1".into(),
            sql: "".into(),
            is_control_source: false,
            expected_shape: crate::types::ResultShape::TimeSeries,
            expected_columns: vec![],
        };
        let actual = vec!["anything".into()];
        assert!(check_expected_columns(&task, &actual).is_none());
    }

    // ── GROUP BY validation ───────────────────────────────────────────────────

    #[test]
    fn test_group_by_valid_aggregate() {
        // AVG(Weight) is an aggregate → ok even without being in GROUP BY
        let sql = "SELECT date_trunc('week', Date) AS Week, AVG(Weight) AS Avg_Weight \
                   FROM strength \
                   GROUP BY date_trunc('week', Date)";
        assert!(validate_group_by_rules(sql).is_ok());
    }

    #[test]
    fn test_group_by_valid_count_distinct() {
        // COUNT(DISTINCT Date) is an aggregate → ok
        let sql = "SELECT date_trunc('week', Date) AS Week, COUNT(DISTINCT Date) AS Sessions \
                   FROM strength \
                   GROUP BY date_trunc('week', Date)";
        assert!(validate_group_by_rules(sql).is_ok());
    }

    #[test]
    fn test_group_by_missing_column() {
        // Date is in SELECT but not in GROUP BY and not an aggregate → error
        let sql = "SELECT date_trunc('week', Date) AS Week, AVG(Weight) AS Avg_Weight, Date \
                   FROM strength \
                   GROUP BY date_trunc('week', Date)";
        let result = validate_group_by_rules(sql);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("Date"), "error should mention 'Date': {msg}");
        assert!(
            msg.contains("GROUP BY") || msg.contains("aggregate"),
            "error should give a hint: {msg}"
        );
    }

    #[test]
    fn test_group_by_no_group_by_clause() {
        // No GROUP BY → nothing to validate
        let sql = "SELECT Date, Weight FROM strength WHERE Date >= '2024-01-01'";
        assert!(validate_group_by_rules(sql).is_ok());
    }

    #[test]
    fn test_group_by_with_template_placeholders() {
        // Template placeholders should be stripped before parsing
        let sql = "SELECT date_trunc('week', Date) AS Week, COUNT(DISTINCT Date) AS Sessions \
                   FROM strength \
                   WHERE Date >= {{ controls.start_date | sqlquote }}::date \
                   GROUP BY date_trunc('week', Date)";
        assert!(validate_group_by_rules(sql).is_ok());
    }

    #[test]
    fn test_group_by_regression_weight_query() {
        // Regression: the actual failing query pattern from the bug report
        let sql = "SELECT date_trunc('week', Date) AS Week, AVG(Weight) AS Avg_Weight \
                   FROM strength \
                   WHERE Date >= '2024-01-01' AND Date <= '2024-12-31' \
                   GROUP BY date_trunc('week', Date) \
                   ORDER BY Week ASC";
        assert!(validate_group_by_rules(sql).is_ok());
    }

    #[test]
    fn test_group_by_regression_bare_date_selected() {
        // Regression: Date appears standalone in SELECT but GROUP BY has date_trunc(...)
        let sql = "SELECT date_trunc('week', Date) AS Week, AVG(Weight) AS Avg_Weight, Date \
                   FROM strength \
                   GROUP BY date_trunc('week', Date)";
        let result = validate_group_by_rules(sql);
        assert!(result.is_err());
    }

    #[test]
    fn test_group_by_invalid_sql_is_ok() {
        // Unparsable SQL should pass (DB will surface the real error)
        assert!(validate_group_by_rules("NOT VALID SQL !!!").is_ok());
    }

    #[test]
    fn test_check_expected_columns_case_insensitive() {
        let task = crate::types::ResolvedTask {
            name: "t1".into(),
            sql: "".into(),
            is_control_source: false,
            expected_shape: crate::types::ResultShape::TimeSeries,
            expected_columns: vec!["Month".into(), "Revenue".into()],
        };
        let actual = vec!["month".into(), "revenue".into()];
        assert!(check_expected_columns(&task, &actual).is_none());
    }
}
