//! Validation rules for the **Specify** stage.
//!
//! | Rule | Name |
//! |---|---|
//! | [`MetricResolvesRule`] | `metric_resolves` |
//! | [`JoinKeyExistsRule`]  | `join_key_exists` |
//! | [`FilterUnambiguousRule`] | `filter_unambiguous` |

use serde_json::Value;
use sqlparser::ast::{Expr, SetExpr, Statement};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

use crate::AnalyticsError;
use crate::QuerySpec;
use crate::semantic::SemanticCatalog;

use super::registry::RegistryError;
use super::rule::{SpecifiedCtx, SpecifiedRule};

// ---------------------------------------------------------------------------
// Helpers (shared across rules in this module)
// ---------------------------------------------------------------------------

/// Parse `"table.column"` into `(table, column)` (both lowercased).
pub(super) fn parse_dotted(s: &str) -> Result<(String, String), AnalyticsError> {
    let mut parts = s.splitn(2, '.');
    match (parts.next(), parts.next()) {
        (Some(t), Some(c)) if !t.is_empty() && !c.is_empty() => {
            Ok((t.to_lowercase(), c.to_lowercase()))
        }
        _ => Err(AnalyticsError::UnresolvedMetric {
            metric: s.to_string(),
        }),
    }
}

/// Return `true` when `s` is a SQL expression rather than a bare
/// `table.column` reference.
///
/// Only `(` (function call) and `*` (wildcard) are unambiguous expression
/// markers.  A string containing spaces is **not** treated as an expression —
/// it may still be a valid dotted pair with whitespace noise, and the
/// downstream SQL parser will reject genuine syntax errors.
///
/// Bug-fix #3: the original implementation matched `s.contains(' ')` which
/// caused a spaced metric like `"orders. revenue"` to silently bypass
/// column-existence checks.
pub(super) fn is_sql_expression(s: &str) -> bool {
    s.contains('(') || s == "*"
}

/// Parse a filter expression string and return the column reference on its
/// left-hand side (e.g. `"date >= '2024'"` → `Some("date")`).
fn extract_filter_lhs(filter: &str) -> Option<String> {
    let sql = format!("SELECT 1 WHERE {filter}");
    let dialect = GenericDialect {};
    let stmts = Parser::parse_sql(&dialect, &sql).ok()?;
    let stmt = stmts.into_iter().next()?;
    let selection = match stmt {
        Statement::Query(q) => match *q.body {
            SetExpr::Select(sel) => sel.selection,
            _ => return None,
        },
        _ => return None,
    };
    lhs_column_name(selection.as_ref()?)
}

/// Recursively find the left-most column identifier of a WHERE expression.
fn lhs_column_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Identifier(i) => Some(i.value.clone()),
        Expr::CompoundIdentifier(parts) => Some(
            parts
                .iter()
                .map(|p| p.value.as_str())
                .collect::<Vec<_>>()
                .join("."),
        ),
        Expr::BinaryOp { left, .. } => lhs_column_name(left),
        Expr::InList { expr, .. } => lhs_column_name(expr),
        Expr::Between { expr, .. } => lhs_column_name(expr),
        Expr::IsNull(e) | Expr::IsNotNull(e) => lhs_column_name(e),
        Expr::Like { expr, .. } | Expr::ILike { expr, .. } => lhs_column_name(expr),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Rule: metric_resolves
// ---------------------------------------------------------------------------

/// Rule: `metric_resolves`
///
/// Checks that every `resolved_metric` in the [`QuerySpec`] is in
/// `"table.column"` form, that the table exists in the catalog, and that
/// the column exists in that table.
///
/// SQL expressions (containing `(`) are scanned for `table.column`
/// sub-references instead.
///
/// **Stage:** `specified`
/// **Errors:** [`AnalyticsError::UnresolvedMetric`], [`AnalyticsError::AmbiguousColumn`]
/// **Params:** none
pub struct MetricResolvesRule;

impl MetricResolvesRule {
    pub fn from_params(_params: &Value) -> Result<Box<dyn SpecifiedRule>, RegistryError> {
        Ok(Box::new(Self))
    }
}

impl SpecifiedRule for MetricResolvesRule {
    fn name(&self) -> &'static str {
        "metric_resolves"
    }

    fn description(&self) -> &'static str {
        "Every resolved_metric must resolve to a known table.column in the schema catalog."
    }

    fn check(&self, ctx: &SpecifiedCtx<'_>) -> Result<(), AnalyticsError> {
        validate_metrics(ctx.spec, ctx.catalog)
    }
}

fn validate_metrics(spec: &QuerySpec, catalog: &SemanticCatalog) -> Result<(), AnalyticsError> {
    for metric in &spec.resolved_metrics {
        // If the semantic layer recognizes this metric, it's valid — skip
        // the schema catalog check entirely.
        if catalog.metric_resolves_in_semantic(metric) {
            continue;
        }

        if is_sql_expression(metric) {
            // SQL expression: scan for qualified table.column references.
            let refs = super::extract_table_column_refs(metric);
            for (table, column) in refs {
                if !catalog.table_exists(&table) {
                    return Err(AnalyticsError::UnresolvedMetric {
                        metric: metric.clone(),
                    });
                }
                if !catalog.column_exists(&table, &column) {
                    return Err(AnalyticsError::UnresolvedMetric {
                        metric: metric.clone(),
                    });
                }
            }
        } else {
            let (table, column) = parse_dotted(metric)?;
            if !catalog.table_exists(&table) {
                return Err(AnalyticsError::UnresolvedMetric {
                    metric: metric.clone(),
                });
            }
            if !catalog.column_exists(&table, &column) {
                // Bug-fix #2: only return AmbiguousColumn when the bare
                // column name (without the table prefix the user wrote)
                // exists in multiple catalog tables.  This is the
                // *ambiguity* path: the user wrote `products.customer_id`
                // but the column resolves to multiple tables, suggesting
                // the table qualifier may be wrong.
                // If the column doesn't exist anywhere, return UnresolvedMetric.
                let mut matching: Vec<String> = catalog.column_tables(&column);
                matching.sort();
                if matching.len() > 1 {
                    return Err(AnalyticsError::AmbiguousColumn {
                        column,
                        tables: matching,
                    });
                }
                return Err(AnalyticsError::UnresolvedMetric {
                    metric: metric.clone(),
                });
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Rule: join_key_exists
// ---------------------------------------------------------------------------

/// Rule: `join_key_exists`
///
/// Checks that every `(left, right, key)` triple in the spec's `join_path`:
/// - both tables exist in the catalog, and
/// - the join key exists in at least one of those tables.
///
/// **Stage:** `specified`
/// **Errors:** [`AnalyticsError::UnresolvedJoin`]
/// **Params:** none
pub struct JoinKeyExistsRule;

impl JoinKeyExistsRule {
    pub fn from_params(_params: &Value) -> Result<Box<dyn SpecifiedRule>, RegistryError> {
        Ok(Box::new(Self))
    }
}

impl SpecifiedRule for JoinKeyExistsRule {
    fn name(&self) -> &'static str {
        "join_key_exists"
    }

    fn description(&self) -> &'static str {
        "Every join_path entry must reference tables and a key that exist in the schema catalog."
    }

    fn check(&self, ctx: &SpecifiedCtx<'_>) -> Result<(), AnalyticsError> {
        validate_joins(ctx.spec, ctx.catalog)
    }
}

/// Validate a single join key string against the catalog.
///
/// Accepted formats:
/// - Bare column name: `Date`  — must exist in `left` or `right`.
/// - Equality expression: `macro.Date = strength.workout_date`
///   Each side may be qualified (`table.col`) or unqualified (`col`).
///   A qualified side is checked against the named table; an unqualified side
///   is accepted if the column exists in either `left` or `right`.
fn is_valid_join_key(catalog: &SemanticCatalog, left: &str, right: &str, key: &str) -> bool {
    if let Some((lhs, rhs)) = key.split_once('=') {
        let lhs = lhs.trim();
        let rhs = rhs.trim();
        col_ref_exists(catalog, left, right, lhs) && col_ref_exists(catalog, left, right, rhs)
    } else {
        // Bare column name.
        catalog.column_exists(left, key) || catalog.column_exists(right, key)
    }
}

/// Return true if `col_ref` (qualified or bare) resolves against the catalog.
fn col_ref_exists(catalog: &SemanticCatalog, left: &str, right: &str, col_ref: &str) -> bool {
    if let Some((table, col)) = col_ref.split_once('.') {
        catalog.column_exists(table, col)
    } else {
        catalog.column_exists(left, col_ref) || catalog.column_exists(right, col_ref)
    }
}

fn validate_joins(spec: &QuerySpec, catalog: &SemanticCatalog) -> Result<(), AnalyticsError> {
    // When the semantic layer (airlayer) compiled the query successfully,
    // joins have already been resolved internally.  Skip validation here
    // because `extract_join_paths` can misparse join keys when underlying
    // table names contain dots (e.g. `body_composition.csv`).
    if spec.solution_source == crate::SolutionSource::SemanticLayer && spec.precomputed.is_some() {
        return Ok(());
    }

    for (left, right, key) in &spec.join_path {
        if !catalog.table_exists(left) {
            // If both tables are semantic views with a known join, accept.
            if catalog.join_exists_in_semantic(left, right) {
                continue;
            }
            return Err(AnalyticsError::UnresolvedJoin {
                left: left.clone(),
                right: right.clone(),
                key: key.clone(),
                reason: format!("table `{left}` does not exist in the schema"),
            });
        }
        if !catalog.table_exists(right) {
            if catalog.join_exists_in_semantic(left, right) {
                continue;
            }
            return Err(AnalyticsError::UnresolvedJoin {
                left: left.clone(),
                right: right.clone(),
                key: key.clone(),
                reason: format!("table `{right}` does not exist in the schema"),
            });
        }
        if !is_valid_join_key(catalog, left, right, key) {
            let left_cols = catalog.columns_of(left).join(", ");
            let right_cols = catalog.columns_of(right).join(", ");
            let suggestion = catalog
                .join_key(left, right)
                .map(|k| format!(" Use the registered join key `{k}` instead."))
                .unwrap_or_default();
            return Err(AnalyticsError::UnresolvedJoin {
                left: left.clone(),
                right: right.clone(),
                key: key.clone(),
                reason: format!(
                    "`{key}` is not a valid join key.{suggestion} \
                     Use either a bare column name that exists in both tables (e.g. `Date`), \
                     or a `left_table.left_col = right_table.right_col` expression when column \
                     names differ (e.g. `macro.Date = strength.workout_date`). \
                     Columns in `{left}`: [{left_cols}]. Columns in `{right}`: [{right_cols}]."
                ),
            });
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Rule: filter_unambiguous
// ---------------------------------------------------------------------------

/// Rule: `filter_unambiguous`
///
/// Validates filter expressions in `spec.intent.filters`:
/// 1. Fully-qualified `table.column` references must resolve in the catalog.
/// 2. Unqualified LHS column names that match multiple *resolved* tables
///    return [`AnalyticsError::AmbiguousColumn`].
///
/// Unqualified column names that don't appear in any catalog table are assumed
/// to be aliases or computed references and are silently accepted — the SQL
/// stage will surface genuine errors.
///
/// **Stage:** `specified`
/// **Errors:** [`AnalyticsError::AmbiguousColumn`], [`AnalyticsError::UnresolvedJoin`]
/// **Params:** none
///
/// Bug-fix #1: filter column errors previously used `UnresolvedMetric`
/// (wrong variant).  Qualified filter references to unknown tables/columns now
/// use `UnresolvedJoin` (closest semantic match), while ambiguous unqualified
/// columns use `AmbiguousColumn`.
pub struct FilterUnambiguousRule;

impl FilterUnambiguousRule {
    pub fn from_params(_params: &Value) -> Result<Box<dyn SpecifiedRule>, RegistryError> {
        Ok(Box::new(Self))
    }
}

impl SpecifiedRule for FilterUnambiguousRule {
    fn name(&self) -> &'static str {
        "filter_unambiguous"
    }

    fn description(&self) -> &'static str {
        "Filter column references must resolve unambiguously to the schema catalog."
    }

    fn check(&self, ctx: &SpecifiedCtx<'_>) -> Result<(), AnalyticsError> {
        validate_filters(ctx.spec, ctx.catalog)
    }
}

fn validate_filters(spec: &QuerySpec, catalog: &SemanticCatalog) -> Result<(), AnalyticsError> {
    // When the semantic layer compiled the query, filters were handled by
    // airlayer's structured filter API.  The raw `intent.filters` strings
    // are no longer appended as SQL, so schema-level validation is
    // unnecessary (and can false-positive on dotted table names).
    if spec.solution_source == crate::SolutionSource::SemanticLayer && spec.precomputed.is_some() {
        return Ok(());
    }

    for filter in &spec.intent.filters {
        // 1. Validate explicit table.column references in the filter.
        let refs = super::extract_table_column_refs(filter);
        for (table, column) in &refs {
            if !catalog.table_exists(table) {
                // Bug-fix #1: use UnresolvedJoin rather than UnresolvedMetric.
                return Err(AnalyticsError::UnresolvedJoin {
                    left: table.clone(),
                    right: String::new(),
                    key: column.clone(),
                    reason: format!("filter references unknown table `{table}` in: {filter}"),
                });
            }
            if !catalog.column_exists(table, column) {
                // Check if a column exists whose name *starts with* the extracted
                // token — this happens when the column contains spaces and the LLM
                // wrote it unquoted (e.g. `body_composition.Datetime (Local)` gets
                // truncated to `Datetime` by the unquoted-ident parser).
                let col_lc = column.to_lowercase();
                let space_col = catalog
                    .columns_of(table)
                    .into_iter()
                    .find(|c| c.len() > column.len() && c.to_lowercase().starts_with(&col_lc));
                let hint = if let Some(ref actual) = space_col {
                    format!(
                        " Column `{actual}` contains spaces and must be quoted in filters — \
                         use backtick syntax: `{table}`.`{actual}`"
                    )
                } else {
                    String::new()
                };
                return Err(AnalyticsError::UnresolvedJoin {
                    left: table.clone(),
                    right: String::new(),
                    key: column.clone(),
                    reason: format!(
                        "filter references unknown column `{table}.{column}` in: {filter}{hint}"
                    ),
                });
            }
        }

        // 2. Check the bare LHS identifier for ambiguity across resolved tables.
        let lhs = match extract_filter_lhs(filter) {
            Some(c) => c,
            None => continue,
        };

        if lhs.contains('.') {
            // Already covered by the table.column scan above.
            continue;
        }

        let matching: Vec<String> = catalog.column_tables(&lhs);

        if matching.is_empty() {
            // Not a known column — likely an alias; skip.
            continue;
        }

        let in_spec: Vec<String> = matching
            .iter()
            .filter(|t| {
                spec.resolved_tables
                    .iter()
                    .any(|rt| rt.eq_ignore_ascii_case(t))
            })
            .cloned()
            .collect();

        if in_spec.len() > 1 {
            return Err(AnalyticsError::AmbiguousColumn {
                column: lhs,
                tables: in_spec,
            });
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Backward-compatible free function (used by old call sites via mod.rs)
// ---------------------------------------------------------------------------

/// Validate that a [`QuerySpec`] is fully resolved against a [`SchemaCatalog`].
///
/// Runs [`MetricResolvesRule`], [`JoinKeyExistsRule`], and
/// [`FilterUnambiguousRule`] in order, returning the first error encountered.
#[allow(dead_code)]
pub fn validate_specified(
    spec: &QuerySpec,
    catalog: &SemanticCatalog,
) -> Result<(), AnalyticsError> {
    validate_metrics(spec, catalog)?;
    validate_joins(spec, catalog)?;
    validate_filters(spec, catalog)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
