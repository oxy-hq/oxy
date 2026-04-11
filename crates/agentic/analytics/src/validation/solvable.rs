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
            if let Some(last) = rel.0.last() {
                let n = last.value.to_lowercase();
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
    if !q.order_by.is_empty() {
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
mod tests {
    use super::validate_solvable;
    use crate::AnalyticsError;
    use crate::validation::test_fixtures::*;

    // ── validate_solvable: happy path ─────────────────────────────────────────

    #[test]
    fn solvable_happy_path() {
        let sql = "SELECT customers.region, SUM(orders.revenue) \
                   FROM orders \
                   JOIN customers ON orders.customer_id = customers.customer_id \
                   GROUP BY customers.region";
        assert_eq!(
            validate_solvable(sql, &make_spec(), &sample_catalog()),
            Ok(())
        );
    }

    #[test]
    fn solvable_with_subquery_parens() {
        let spec = {
            let mut s = make_spec();
            s.resolved_tables = vec!["orders".into()];
            s.join_path = vec![];
            s
        };
        let sql = "SELECT * FROM orders WHERE revenue > (SELECT AVG(revenue) FROM orders)";
        assert_eq!(validate_solvable(sql, &spec, &sample_catalog()), Ok(()));
    }

    // ── validate_solvable: structural errors ──────────────────────────────────

    #[test]
    fn solvable_no_select() {
        let sql = "FROM orders JOIN customers ON orders.customer_id = customers.customer_id";
        assert!(matches!(
            validate_solvable(sql, &make_spec(), &sample_catalog()),
            Err(AnalyticsError::SyntaxError { .. })
        ));
    }

    #[test]
    fn solvable_no_from() {
        let sql = "SELECT 1 + 1";
        assert!(matches!(
            validate_solvable(sql, &make_spec(), &sample_catalog()),
            Err(AnalyticsError::SyntaxError { .. })
        ));
    }

    #[test]
    fn solvable_unmatched_open_paren() {
        let sql = "SELECT * FROM orders WHERE (revenue > 100";
        assert!(matches!(
            validate_solvable(sql, &make_spec(), &sample_catalog()),
            Err(AnalyticsError::SyntaxError { .. })
        ));
    }

    #[test]
    fn solvable_unmatched_close_paren() {
        let sql = "SELECT * FROM orders WHERE revenue > 100)";
        assert!(matches!(
            validate_solvable(sql, &make_spec(), &sample_catalog()),
            Err(AnalyticsError::SyntaxError { .. })
        ));
    }

    #[test]
    fn solvable_paren_inside_string_literal_ok() {
        let spec = {
            let mut s = make_spec();
            s.resolved_tables = vec!["orders".into()];
            s.join_path = vec![];
            s
        };
        let sql = "SELECT * FROM orders WHERE status = 'pending (review)'";
        assert_eq!(validate_solvable(sql, &spec, &sample_catalog()), Ok(()));
    }

    #[test]
    fn solvable_table_not_in_catalog() {
        let sql = "SELECT * FROM orders JOIN ghost_table ON orders.id = ghost_table.id";
        assert!(matches!(
            validate_solvable(sql, &make_spec(), &sample_catalog()),
            Err(AnalyticsError::SyntaxError { message, .. }) if message.contains("ghost_table")
        ));
    }

    #[test]
    fn solvable_spec_table_absent_from_query() {
        let sql = "SELECT revenue FROM orders"; // customers missing
        assert!(matches!(
            validate_solvable(sql, &make_spec(), &sample_catalog()),
            Err(AnalyticsError::SyntaxError { message, .. }) if message.contains("customers")
        ));
    }

    // ── validate_solvable: sqlparser-specific coverage ────────────────────────

    #[test]
    fn solvable_completely_invalid_sql() {
        assert!(matches!(
            validate_solvable("NOT EVEN SQL !!!", &make_spec(), &sample_catalog()),
            Err(AnalyticsError::SyntaxError { .. })
        ));
    }

    #[test]
    fn solvable_empty_string() {
        assert!(matches!(
            validate_solvable("", &make_spec(), &sample_catalog()),
            Err(AnalyticsError::SyntaxError { .. })
        ));
    }

    #[test]
    fn solvable_aliased_tables_ok() {
        let sql = "SELECT c.region, SUM(o.revenue) \
                   FROM orders AS o \
                   JOIN customers AS c ON o.customer_id = c.customer_id \
                   GROUP BY c.region";
        assert_eq!(
            validate_solvable(sql, &make_spec(), &sample_catalog()),
            Ok(())
        );
    }

    #[test]
    fn solvable_cte_ok() {
        let spec = {
            let mut s = make_spec();
            s.resolved_tables = vec!["orders".into()];
            s.join_path = vec![];
            s
        };
        let sql = "WITH summary AS (SELECT date, SUM(revenue) AS total FROM orders GROUP BY date) \
                   SELECT * FROM summary";
        assert_eq!(validate_solvable(sql, &spec, &sample_catalog()), Ok(()));
    }

    #[test]
    fn solvable_subquery_in_from_ok() {
        let spec = {
            let mut s = make_spec();
            s.resolved_tables = vec!["orders".into()];
            s.join_path = vec![];
            s
        };
        let sql = "SELECT sub.date, sub.total \
                   FROM (SELECT date, SUM(revenue) AS total FROM orders GROUP BY date) AS sub";
        assert_eq!(validate_solvable(sql, &spec, &sample_catalog()), Ok(()));
    }

    #[test]
    fn solvable_multiple_join_types_ok() {
        let spec = {
            let mut s = make_spec();
            s.resolved_tables = vec!["orders".into(), "customers".into(), "products".into()];
            s.join_path = vec![
                ("orders".into(), "customers".into(), "customer_id".into()),
                ("orders".into(), "products".into(), "product_id".into()),
            ];
            s
        };
        let sql = "SELECT c.region, p.category, SUM(o.revenue) \
                   FROM orders AS o \
                   LEFT JOIN customers AS c ON o.customer_id = c.customer_id \
                   INNER JOIN products AS p ON o.product_id = p.product_id \
                   GROUP BY c.region, p.category";
        assert_eq!(validate_solvable(sql, &spec, &sample_catalog()), Ok(()));
    }

    // ── timeseries_order_by_check ──────────────────────────────────────────

    #[test]
    fn timeseries_order_by_present() {
        use super::{SolvableRule, TimeseriesOrderByCheckRule};
        use crate::ResultShape;
        use crate::validation::rule::SolvableCtx;

        let spec = {
            let mut s = make_spec();
            s.expected_result_shape = ResultShape::TimeSeries;
            s.resolved_tables = vec!["orders".into()];
            s.join_path = vec![];
            s
        };
        let sql = "SELECT date, SUM(revenue) FROM orders GROUP BY date ORDER BY date";
        let catalog = sample_catalog();
        let ctx = SolvableCtx {
            sql,
            spec: &spec,
            catalog: &catalog,
        };
        assert_eq!(TimeseriesOrderByCheckRule.check(&ctx), Ok(()));
    }

    #[test]
    fn timeseries_order_by_missing() {
        use super::{SolvableRule, TimeseriesOrderByCheckRule};
        use crate::ResultShape;
        use crate::validation::rule::SolvableCtx;

        let spec = {
            let mut s = make_spec();
            s.expected_result_shape = ResultShape::TimeSeries;
            s.resolved_tables = vec!["orders".into()];
            s.join_path = vec![];
            s
        };
        let sql = "SELECT date, SUM(revenue) FROM orders GROUP BY date";
        let catalog = sample_catalog();
        let ctx = SolvableCtx {
            sql,
            spec: &spec,
            catalog: &catalog,
        };
        assert!(matches!(
            TimeseriesOrderByCheckRule.check(&ctx),
            Err(AnalyticsError::SyntaxError { message, .. }) if message.contains("ORDER BY")
        ));
    }

    #[test]
    fn timeseries_order_by_in_cte_is_ok() {
        use super::{SolvableRule, TimeseriesOrderByCheckRule};
        use crate::ResultShape;
        use crate::validation::rule::SolvableCtx;

        let spec = {
            let mut s = make_spec();
            s.expected_result_shape = ResultShape::TimeSeries;
            s.resolved_tables = vec!["orders".into()];
            s.join_path = vec![];
            s
        };
        // ORDER BY is inside the CTE, not the outer query
        let sql = "WITH ordered AS (SELECT date, SUM(revenue) AS total \
                   FROM orders GROUP BY date ORDER BY date) \
                   SELECT * FROM ordered";
        let catalog = sample_catalog();
        let ctx = SolvableCtx {
            sql,
            spec: &spec,
            catalog: &catalog,
        };
        assert_eq!(TimeseriesOrderByCheckRule.check(&ctx), Ok(()));
    }

    #[test]
    fn timeseries_order_by_in_subquery_is_ok() {
        use super::{SolvableRule, TimeseriesOrderByCheckRule};
        use crate::ResultShape;
        use crate::validation::rule::SolvableCtx;

        let spec = {
            let mut s = make_spec();
            s.expected_result_shape = ResultShape::TimeSeries;
            s.resolved_tables = vec!["orders".into()];
            s.join_path = vec![];
            s
        };
        // ORDER BY is inside a derived table subquery
        let sql = "SELECT sub.date, sub.total \
                   FROM (SELECT date, SUM(revenue) AS total \
                         FROM orders GROUP BY date ORDER BY date) AS sub";
        let catalog = sample_catalog();
        let ctx = SolvableCtx {
            sql,
            spec: &spec,
            catalog: &catalog,
        };
        assert_eq!(TimeseriesOrderByCheckRule.check(&ctx), Ok(()));
    }

    #[test]
    fn timeseries_order_by_skipped_for_non_timeseries() {
        use super::{SolvableRule, TimeseriesOrderByCheckRule};
        use crate::validation::rule::SolvableCtx;

        // Table shape → rule doesn't fire even without ORDER BY
        let spec = {
            let mut s = make_spec();
            s.resolved_tables = vec!["orders".into()];
            s.join_path = vec![];
            s
        };
        let sql = "SELECT date, SUM(revenue) FROM orders GROUP BY date";
        let catalog = sample_catalog();
        let ctx = SolvableCtx {
            sql,
            spec: &spec,
            catalog: &catalog,
        };
        assert_eq!(TimeseriesOrderByCheckRule.check(&ctx), Ok(()));
    }

    #[test]
    fn solvable_union_query_ok() {
        let spec = {
            let mut s = make_spec();
            s.resolved_tables = vec!["orders".into()];
            s.join_path = vec![];
            s
        };
        let sql = "SELECT order_id FROM orders WHERE status = 'open' \
                   UNION ALL \
                   SELECT order_id FROM orders WHERE status = 'pending'";
        assert_eq!(validate_solvable(sql, &spec, &sample_catalog()), Ok(()));
    }
}
