//! Validation rules for the **Solve** stage.
//!
//! | Rule | Name |
//! |---|---|
//! | [`SqlSyntaxRule`]                | `sql_syntax` |
//! | [`TablesExistRule`]              | `tables_exist_in_catalog` |
//! | [`SpecTablesPresentRule`]        | `spec_tables_present` |
//! | [`ColumnRefsValidRule`]          | `column_refs_valid` |
//! | [`TimeseriesOrderByCheckRule`]   | `timeseries_order_by_check` |

use std::ops::ControlFlow;

use serde_json::Value;
use sqlparser::ast::{Query, SetExpr, Statement, visit_relations};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

use crate::semantic::SemanticCatalog;
use crate::{AnalyticsError, QuerySpec, ResultShape};

use super::config::SqlSyntaxParams;
use super::registry::RegistryError;
use super::rule::{SolvableCtx, SolvableRule};

// ---------------------------------------------------------------------------
// Shared helper
// ---------------------------------------------------------------------------

/// Return deduplicated, lowercased table names referenced in a parsed SQL
/// statement list, excluding CTE aliases (which are not real catalog tables).
fn extract_query_tables(stmts: &[Statement]) -> Vec<String> {
    let cte_names: Vec<String> = stmts
        .iter()
        .filter_map(|s| match s {
            Statement::Query(q) => q.with.as_ref(),
            _ => None,
        })
        .flat_map(|with| with.cte_tables.iter())
        .map(|cte| cte.alias.name.value.to_lowercase())
        .collect();

    let mut tables = Vec::new();
    for stmt in stmts {
        let _ = visit_relations(stmt, |rel| {
            if let Some(last) = rel.0.last()
                && let Some(ident) = last.as_ident()
            {
                let n = ident.value.to_lowercase();
                if !cte_names.contains(&n) {
                    tables.push(n);
                }
            }
            ControlFlow::<()>::Continue(())
        });
    }
    tables.sort();
    tables.dedup();
    tables
}

/// Parse the SQL string with `GenericDialect`, returning the statement list
/// or a [`AnalyticsError::SyntaxError`].
fn parse_sql(sql: &str) -> Result<Vec<Statement>, AnalyticsError> {
    let dialect = GenericDialect {};
    Parser::parse_sql(&dialect, sql.trim()).map_err(|e| AnalyticsError::SyntaxError {
        query: sql.to_string(),
        message: e.to_string(),
    })
}

// ---------------------------------------------------------------------------
// Rule: sql_syntax
// ---------------------------------------------------------------------------

/// Rule: `sql_syntax`
///
/// Checks that the generated SQL string is parseable by `sqlparser`.
///
/// **Stage:** `solvable`
/// **Errors:** [`AnalyticsError::SyntaxError`]
/// **Params:**
/// - `dialect` (`String`, default `"generic"`) — SQL dialect hint.
///   Currently only `"generic"` is used internally; other values are
///   accepted but not yet wired to distinct dialect types.
pub struct SqlSyntaxRule {
    _dialect: String,
}

impl SqlSyntaxRule {
    pub fn from_params(params: &Value) -> Result<Box<dyn SolvableRule>, RegistryError> {
        let p: SqlSyntaxParams = if params.is_null() {
            SqlSyntaxParams::default()
        } else {
            serde_json::from_value(params.clone()).map_err(|e| RegistryError::InvalidParams {
                name: "sql_syntax".into(),
                reason: e.to_string(),
            })?
        };
        Ok(Box::new(Self {
            _dialect: p.dialect,
        }))
    }
}

impl SolvableRule for SqlSyntaxRule {
    fn name(&self) -> &'static str {
        "sql_syntax"
    }

    fn description(&self) -> &'static str {
        "The generated SQL must be syntactically valid."
    }

    fn check(&self, ctx: &SolvableCtx<'_>) -> Result<(), AnalyticsError> {
        let stmts = parse_sql(ctx.sql)?;
        if stmts.is_empty() {
            return Err(AnalyticsError::SyntaxError {
                query: ctx.sql.to_string(),
                message: "no SQL statement found".to_string(),
            });
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Rule: tables_exist_in_catalog
// ---------------------------------------------------------------------------

/// Rule: `tables_exist_in_catalog`
///
/// Every table referenced in `FROM` / `JOIN` clauses of the SQL must exist
/// in the schema catalog.  CTE aliases are excluded.
///
/// **Stage:** `solvable`
/// **Errors:** [`AnalyticsError::SyntaxError`]
/// **Params:** none
pub struct TablesExistRule;

impl TablesExistRule {
    pub fn from_params(_params: &Value) -> Result<Box<dyn SolvableRule>, RegistryError> {
        Ok(Box::new(Self))
    }
}

impl SolvableRule for TablesExistRule {
    fn name(&self) -> &'static str {
        "tables_exist_in_catalog"
    }

    fn description(&self) -> &'static str {
        "Every FROM/JOIN table must exist in the schema catalog."
    }

    fn check(&self, ctx: &SolvableCtx<'_>) -> Result<(), AnalyticsError> {
        let stmts = parse_sql(ctx.sql)?;
        let sql_tables = extract_query_tables(&stmts);
        for table in &sql_tables {
            // SemanticCatalog.table_exists checks both schema and semantic views.
            if !ctx.catalog.table_exists(table) {
                return Err(AnalyticsError::SyntaxError {
                    query: ctx.sql.to_string(),
                    message: format!("table `{table}` does not exist in the schema catalog"),
                });
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Rule: spec_tables_present
// ---------------------------------------------------------------------------

/// Rule: `spec_tables_present`
///
/// Every table in `spec.resolved_tables` must appear somewhere in the
/// generated SQL.
///
/// **Stage:** `solvable`
/// **Errors:** [`AnalyticsError::SyntaxError`]
/// **Params:** none
pub struct SpecTablesPresentRule;

impl SpecTablesPresentRule {
    pub fn from_params(_params: &Value) -> Result<Box<dyn SolvableRule>, RegistryError> {
        Ok(Box::new(Self))
    }
}

impl SolvableRule for SpecTablesPresentRule {
    fn name(&self) -> &'static str {
        "spec_tables_present"
    }

    fn description(&self) -> &'static str {
        "Every table listed in spec.resolved_tables must appear in the generated SQL."
    }

    fn check(&self, ctx: &SolvableCtx<'_>) -> Result<(), AnalyticsError> {
        let stmts = parse_sql(ctx.sql)?;
        let sql_tables = extract_query_tables(&stmts);
        for spec_table in &ctx.spec.resolved_tables {
            let lower = spec_table.to_lowercase();
            if !sql_tables.contains(&lower) {
                return Err(AnalyticsError::SyntaxError {
                    query: ctx.sql.to_string(),
                    message: format!(
                        "spec requires table `{spec_table}` but it is absent from the query"
                    ),
                });
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Rule: column_refs_valid
// ---------------------------------------------------------------------------

/// Rule: `column_refs_valid`
///
/// Every qualified `table.column` reference found in the SQL string must
/// point to a column that exists in the catalog for that table.
///
/// Table aliases (e.g. `o.revenue` after `FROM orders AS o`) are not
/// currently tracked — alias names are not catalog table names so the check
/// skips them.
///
/// **Stage:** `solvable`
/// **Errors:** [`AnalyticsError::SyntaxError`]
/// **Params:** none
pub struct ColumnRefsValidRule;

impl ColumnRefsValidRule {
    pub fn from_params(_params: &Value) -> Result<Box<dyn SolvableRule>, RegistryError> {
        Ok(Box::new(Self))
    }
}

impl SolvableRule for ColumnRefsValidRule {
    fn name(&self) -> &'static str {
        "column_refs_valid"
    }

    fn description(&self) -> &'static str {
        "Every table.column reference in the SQL must exist in the schema catalog."
    }

    fn check(&self, ctx: &SolvableCtx<'_>) -> Result<(), AnalyticsError> {
        let col_refs = super::extract_table_column_refs(ctx.sql.trim());
        for (table, column) in &col_refs {
            // SemanticCatalog.table_exists / column_exists check both schema
            // and semantic views.
            if ctx.catalog.table_exists(table) && !ctx.catalog.column_exists(table, column) {
                return Err(AnalyticsError::SyntaxError {
                    query: ctx.sql.to_string(),
                    message: format!("column `{table}.{column}` does not exist"),
                });
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helper: recursive ORDER BY check
// ---------------------------------------------------------------------------

/// Returns `true` if any level of the query tree contains an ORDER BY clause.
/// Checks: the query itself, CTE definitions, and subqueries in FROM.
fn query_has_order_by(q: &Query) -> bool {
    // 1. Outermost query
    if q.order_by.is_some() {
        return true;
    }
    // 2. CTE bodies
    if let Some(with) = &q.with {
        for cte in &with.cte_tables {
            if query_has_order_by(&cte.query) {
                return true;
            }
        }
    }
    // 3. Subqueries in the SET body (e.g. `SELECT * FROM (SELECT ... ORDER BY ...)`)
    if let SetExpr::Select(select) = q.body.as_ref() {
        for from in &select.from {
            if let sqlparser::ast::TableFactor::Derived { subquery, .. } = &from.relation
                && query_has_order_by(subquery)
            {
                return true;
            }
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Rule: timeseries_order_by_check
// ---------------------------------------------------------------------------

/// Rule: `timeseries_order_by_check`
///
/// When the expected shape is `TimeSeries`, verifies that the outermost
/// `SELECT` statement contains an `ORDER BY` clause.  Missing `ORDER BY`
/// produces out-of-order dates, which is always a SQL-generation bug —
/// catching it here is cheaper than detecting it after execution.
///
/// **Stage:** `solvable`
/// **Errors:** [`AnalyticsError::SyntaxError`]
/// **Params:** none
pub struct TimeseriesOrderByCheckRule;

impl TimeseriesOrderByCheckRule {
    pub fn from_params(_params: &Value) -> Result<Box<dyn SolvableRule>, RegistryError> {
        Ok(Box::new(Self))
    }
}

impl SolvableRule for TimeseriesOrderByCheckRule {
    fn name(&self) -> &'static str {
        "timeseries_order_by_check"
    }

    fn description(&self) -> &'static str {
        "TimeSeries queries must have an ORDER BY clause to ensure chronological ordering."
    }

    fn check(&self, ctx: &SolvableCtx<'_>) -> Result<(), AnalyticsError> {
        if ctx.spec.expected_result_shape != ResultShape::TimeSeries {
            return Ok(());
        }

        let stmts = parse_sql(ctx.sql)?;
        // Check the outermost query, its CTEs, and subqueries for ORDER BY.
        let has_order_by = stmts.iter().any(|stmt| match stmt {
            Statement::Query(q) => query_has_order_by(q),
            _ => false,
        });

        if !has_order_by {
            return Err(AnalyticsError::SyntaxError {
                query: ctx.sql.to_string(),
                message: "TimeSeries query is missing an ORDER BY clause — results may not be \
                          chronologically ordered"
                    .to_string(),
            });
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Backward-compatible free function
// ---------------------------------------------------------------------------

/// Validate that a SQL string is syntactically valid and references only
/// tables that exist in the catalog and that the spec requires.
#[allow(dead_code)]
pub fn validate_solvable(
    sql: &str,
    spec: &QuerySpec,
    catalog: &SemanticCatalog,
) -> Result<(), AnalyticsError> {
    let ctx = SolvableCtx { sql, spec, catalog };
    SqlSyntaxRule {
        _dialect: "generic".into(),
    }
    .check(&ctx)?;
    TablesExistRule.check(&ctx)?;
    SpecTablesPresentRule.check(&ctx)?;
    ColumnRefsValidRule.check(&ctx)?;
    TimeseriesOrderByCheckRule.check(&ctx)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
