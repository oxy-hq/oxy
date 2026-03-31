//! Validation rules for the **Execute** stage.
//!
//! | Rule | Name |
//! |---|---|
//! | [`NonEmptyRule`]            | `non_empty` |
//! | [`ShapeMatchRule`]          | `shape_match` |
//! | [`NoNanInfRule`]            | `no_nan_inf` |
//! | [`OutlierDetectionRule`]    | `outlier_detection` |
//! | [`TimeseriesDateCheckRule`] | `timeseries_date_check` |
//! | [`TruncationWarningRule`]   | `truncation_warning` |
//! | [`NullRatioCheckRule`]      | `null_ratio_check` |
//! | [`DuplicateRowCheckRule`]   | `duplicate_row_check` |

use std::collections::HashSet;

use agentic_core::result::CellValue;
use serde_json::Value;
use statrs::statistics::Statistics;

use crate::{AnalyticsError, AnalyticsResult, QuerySpec, ResultShape};

use super::config::{DuplicateRowCheckParams, NullRatioCheckParams, OutlierDetectionParams};
use super::registry::RegistryError;
use super::rule::{SolvedCtx, SolvedRule};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Return `true` when `s` looks like a date or timestamp string.
///
/// Recognises:
/// - ISO date: `YYYY-MM-DD` or partial `YYYY-MM`
/// - ISO week: `YYYY-W##` or `YYYY-W##-D`
/// - Weekday names: Monday … Sunday (full or 3-letter abbreviation)
///
/// Bug-fix #5: the original implementation also accepted any 8-digit pure
/// numeric string as a "Unix timestamp".  This was too loose — order IDs,
/// customer IDs, and SKUs are commonly 8-digit integers and would falsely
/// satisfy the TimeSeries date check.  Numeric timestamps stored as integers
/// appear as `CellValue::Number`, not `CellValue::Text`, so the string path
/// never needs to handle them.
fn looks_like_date(s: &str) -> bool {
    let b = s.as_bytes();
    // YYYY-MM or YYYY-MM-DD: 4 digits, dash, 2 digits
    if b.len() >= 7
        && b[0..4].iter().all(|x| x.is_ascii_digit())
        && b[4] == b'-'
        && b[5..7].iter().all(|x| x.is_ascii_digit())
    {
        return true;
    }
    // YYYY-W## ISO week or YYYY-W##-D ISO weekday format
    if b.len() >= 7
        && b[0..4].iter().all(|x| x.is_ascii_digit())
        && b[4] == b'-'
        && b[5] == b'W'
        && b.len() > 6
        && b[6..].iter().all(|x| x.is_ascii_digit() || *x == b'-')
    {
        return true;
    }
    // Weekday names (full or 3-letter abbreviation, case-insensitive)
    matches!(
        s.to_ascii_lowercase().as_str(),
        "monday"
            | "tuesday"
            | "wednesday"
            | "thursday"
            | "friday"
            | "saturday"
            | "sunday"
            | "mon"
            | "tue"
            | "wed"
            | "thu"
            | "fri"
            | "sat"
            | "sun"
    )
}

/// Infer the [`ResultShape`] from row/column counts for mismatch error reports.
///
/// Returns only `Scalar`, `Series`, or `Table` — never `TimeSeries`.
/// Time-series data is structurally a table (dimensions + metrics); treating
/// it as a separate shape caused false validation failures when the expected
/// shape was `Table` but the actual data contained date-like columns.
fn infer_shape(
    total_row_count: u64,
    columns: &[String],
    _rows: &[agentic_core::QueryRow],
) -> ResultShape {
    let n_rows = total_row_count;
    let n_cols = columns.len();

    if n_rows == 1 && n_cols == 1 {
        return ResultShape::Scalar;
    }
    if n_cols == 1 {
        return ResultShape::Series;
    }
    ResultShape::Table {
        columns: columns.to_vec(),
    }
}

// ---------------------------------------------------------------------------
// Rule: non_empty
// ---------------------------------------------------------------------------

/// Rule: `non_empty`
///
/// The query must return at least one row.
///
/// **Stage:** `solved`
/// **Errors:** [`AnalyticsError::EmptyResults`]
/// **Params:** none
pub struct NonEmptyRule;

impl NonEmptyRule {
    pub fn from_params(_params: &Value) -> Result<Box<dyn SolvedRule>, RegistryError> {
        Ok(Box::new(Self))
    }
}

impl SolvedRule for NonEmptyRule {
    fn name(&self) -> &'static str {
        "non_empty"
    }

    fn description(&self) -> &'static str {
        "The query must return at least one row."
    }

    fn check(&self, ctx: &SolvedCtx<'_>) -> Result<(), AnalyticsError> {
        if ctx.result.primary().data.total_row_count == 0 {
            return Err(AnalyticsError::EmptyResults {
                query: "executed query".to_string(),
            });
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Rule: shape_match
// ---------------------------------------------------------------------------

/// Rule: `shape_match`
///
/// The result shape must match `spec.expected_result_shape`:
/// - `Scalar` → exactly 1 row × 1 column.
/// - `Series` → exactly 1 column (any row count ≥ 1).
/// - `Table { columns }` → all expected column names present (case-insensitive).
/// - `TimeSeries` → ≥ 2 columns and ≥ 2 rows.
///
/// The `TimeSeries` date check is handled by [`TimeseriesDateCheckRule`].
///
/// **Stage:** `solved`
/// **Errors:** [`AnalyticsError::ShapeMismatch`]
/// **Params:** none
pub struct ShapeMatchRule;

impl ShapeMatchRule {
    pub fn from_params(_params: &Value) -> Result<Box<dyn SolvedRule>, RegistryError> {
        Ok(Box::new(Self))
    }
}

impl SolvedRule for ShapeMatchRule {
    fn name(&self) -> &'static str {
        "shape_match"
    }

    fn description(&self) -> &'static str {
        "The result shape must match spec.expected_result_shape."
    }

    fn check(&self, ctx: &SolvedCtx<'_>) -> Result<(), AnalyticsError> {
        let result = ctx.result.primary();
        let n_rows = result.data.total_row_count;
        let n_cols = result.data.columns.len();

        match &ctx.spec.expected_result_shape {
            ResultShape::Scalar => {
                if n_rows != 1 || n_cols != 1 {
                    return Err(AnalyticsError::ShapeMismatch {
                        expected: ResultShape::Scalar,
                        actual: infer_shape(n_rows, &result.data.columns, &result.data.rows),
                    });
                }
            }
            ResultShape::Series => {
                if n_cols != 1 {
                    return Err(AnalyticsError::ShapeMismatch {
                        expected: ResultShape::Series,
                        actual: infer_shape(n_rows, &result.data.columns, &result.data.rows),
                    });
                }
            }
            ResultShape::Table {
                columns: expected_cols,
            } => {
                for col in expected_cols {
                    if !result
                        .data
                        .columns
                        .iter()
                        .any(|c| c.eq_ignore_ascii_case(col))
                    {
                        return Err(AnalyticsError::ShapeMismatch {
                            expected: ctx.spec.expected_result_shape.clone(),
                            actual: infer_shape(n_rows, &result.data.columns, &result.data.rows),
                        });
                    }
                }
            }
            ResultShape::TimeSeries => {
                // Treat TimeSeries as a Table check (≥2 cols, ≥2 rows).
                // TimeSeries is no longer generated as an expected shape, but
                // handle it gracefully for backward compatibility.
                if n_cols < 2 || n_rows < 2 {
                    return Err(AnalyticsError::ShapeMismatch {
                        expected: ResultShape::Table {
                            columns: result.data.columns.clone(),
                        },
                        actual: infer_shape(n_rows, &result.data.columns, &result.data.rows),
                    });
                }
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Rule: no_nan_inf
// ---------------------------------------------------------------------------

/// Rule: `no_nan_inf`
///
/// No numeric cell in the result may contain `NaN` or ±∞.
///
/// **Stage:** `solved`
/// **Errors:** [`AnalyticsError::ValueAnomaly`]
/// **Params:** none
pub struct NoNanInfRule;

impl NoNanInfRule {
    pub fn from_params(_params: &Value) -> Result<Box<dyn SolvedRule>, RegistryError> {
        Ok(Box::new(Self))
    }
}

impl SolvedRule for NoNanInfRule {
    fn name(&self) -> &'static str {
        "no_nan_inf"
    }

    fn description(&self) -> &'static str {
        "No numeric column may contain NaN or infinity values."
    }

    fn check(&self, ctx: &SolvedCtx<'_>) -> Result<(), AnalyticsError> {
        let primary = ctx.result.primary();
        for (col_idx, col_name) in primary.data.columns.iter().enumerate() {
            for row in &primary.data.rows {
                if let Some(CellValue::Number(n)) = row.0.get(col_idx) {
                    if n.is_nan() {
                        return Err(AnalyticsError::ValueAnomaly {
                            column: col_name.clone(),
                            value: "NaN".to_string(),
                            reason: "NaN is not a valid numeric result".to_string(),
                        });
                    }
                    if n.is_infinite() {
                        let label = if *n > 0.0 { "Inf" } else { "-Inf" };
                        return Err(AnalyticsError::ValueAnomaly {
                            column: col_name.clone(),
                            value: label.to_string(),
                            reason: "infinite value detected".to_string(),
                        });
                    }
                }
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Rule: outlier_detection
// ---------------------------------------------------------------------------

/// Rule: `outlier_detection`
///
/// Detects values that are more than `threshold_sigma` standard deviations
/// from the column mean (Z-score outlier detection).
///
/// Requires at least `min_rows` numeric values in a column before the check
/// is applied.  Columns where all values are identical (std dev = 0) are
/// skipped.
///
/// Bug-fix #7: uses `statrs` sample standard deviation (N-1 denominator)
/// instead of population standard deviation (N denominator).
///
/// Bug-fix #8: the `min_rows` guard now applies to both the summary-stats
/// path and the fallback in-process path.  Previously the summary-stats path
/// ran even on 1–3 row result sets, producing false positives.
///
/// **Stage:** `solved`
/// **Errors:** [`AnalyticsError::ValueAnomaly`]
/// **Params:**
/// - `threshold_sigma` (`f64`, default `5.0`)
/// - `min_rows` (`usize`, default `4`)
pub struct OutlierDetectionRule {
    pub threshold_sigma: f64,
    pub min_rows: usize,
}

impl OutlierDetectionRule {
    pub fn from_params(params: &Value) -> Result<Box<dyn SolvedRule>, RegistryError> {
        let p: OutlierDetectionParams = if params.is_null() {
            OutlierDetectionParams::default()
        } else {
            serde_json::from_value(params.clone()).map_err(|e| RegistryError::InvalidParams {
                name: "outlier_detection".into(),
                reason: e.to_string(),
            })?
        };
        Ok(Box::new(Self {
            threshold_sigma: p.threshold_sigma,
            min_rows: p.min_rows,
        }))
    }
}

impl SolvedRule for OutlierDetectionRule {
    fn name(&self) -> &'static str {
        "outlier_detection"
    }

    fn description(&self) -> &'static str {
        "No numeric value may exceed threshold_sigma standard deviations from the column mean."
    }

    fn check(&self, ctx: &SolvedCtx<'_>) -> Result<(), AnalyticsError> {
        let result = ctx.result.primary();

        for (col_idx, col_name) in result.data.columns.iter().enumerate() {
            let numbers: Vec<f64> = result
                .data
                .rows
                .iter()
                .filter_map(|row| row.0.get(col_idx))
                .filter_map(|cell| {
                    if let CellValue::Number(n) = cell {
                        Some(*n)
                    } else {
                        None
                    }
                })
                .collect();

            if numbers.is_empty() {
                continue;
            }

            // Bug-fix #8: gate min_rows check on BOTH paths (summary and fallback).
            if numbers.len() < self.min_rows {
                continue;
            }

            // Prefer database-computed statistics when available.
            let stats_from_summary = result.summary.as_ref().and_then(|s| {
                s.columns
                    .iter()
                    .find(|c| c.name.eq_ignore_ascii_case(col_name))
                    .and_then(|c| match (c.mean, c.std_dev) {
                        (Some(mean), Some(std_dev)) => Some((mean, std_dev)),
                        _ => None,
                    })
            });

            // Bug-fix #7: use statrs sample std_dev (N-1 denominator).
            let (mean, std_dev) = if let Some(stats) = stats_from_summary {
                stats
            } else {
                let mean = numbers.clone().mean();
                let std_dev = numbers.clone().std_dev(); // sample std dev from statrs
                (mean, std_dev)
            };

            if std_dev > 0.0 {
                for &n in &numbers {
                    let z = ((n - mean) / std_dev).abs();
                    if z > self.threshold_sigma {
                        return Err(AnalyticsError::ValueAnomaly {
                            column: col_name.clone(),
                            value: n.to_string(),
                            reason: format!("value is {z:.1}σ from the column mean"),
                        });
                    }
                }
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Rule: timeseries_date_check
// ---------------------------------------------------------------------------

/// Rule: `timeseries_date_check`
///
/// When the expected shape is `TimeSeries`, verifies that the first column
/// of the result contains date-like text values (ISO dates, ISO weeks, or
/// weekday names).
///
/// This check is intentionally separate from [`ShapeMatchRule`] so it can be
/// disabled independently when working with numeric time axes (e.g. fiscal
/// periods stored as integers).
///
/// Bug-fix #5: pure-numeric strings are no longer accepted as "Unix
/// timestamps" — see [`looks_like_date`] for the updated heuristic.
///
/// **Stage:** `solved`
/// **Errors:** [`AnalyticsError::ShapeMismatch`]
/// **Params:** none
pub struct TimeseriesDateCheckRule;

impl TimeseriesDateCheckRule {
    pub fn from_params(_params: &Value) -> Result<Box<dyn SolvedRule>, RegistryError> {
        Ok(Box::new(Self))
    }
}

impl SolvedRule for TimeseriesDateCheckRule {
    fn name(&self) -> &'static str {
        "timeseries_date_check"
    }

    fn description(&self) -> &'static str {
        "For TimeSeries results, the first column must contain date-like values."
    }

    fn check(&self, ctx: &SolvedCtx<'_>) -> Result<(), AnalyticsError> {
        if ctx.spec.expected_result_shape != ResultShape::TimeSeries {
            return Ok(());
        }

        let result = ctx.result.primary();
        let n_rows = result.data.total_row_count;
        let n_cols = result.data.columns.len();

        // If shape is already invalid (< 2 cols / rows), ShapeMatchRule handles it.
        if n_cols < 2 || n_rows < 2 {
            return Ok(());
        }

        // Sample up to the first 3 text values in the first column.
        let first_col_text_values: Vec<&str> = result
            .data
            .rows
            .iter()
            .take(3)
            .filter_map(|row| row.0.first())
            .filter_map(|cell| {
                if let CellValue::Text(s) = cell {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .collect();

        // Only validate when there are text values to check — if the first
        // column contains only Numbers (numeric timestamps), we skip this rule.
        // Use `timeseries_date_check: enabled: false` to disable entirely.
        if !first_col_text_values.is_empty()
            && !first_col_text_values.iter().any(|v| looks_like_date(v))
        {
            return Err(AnalyticsError::ShapeMismatch {
                expected: ResultShape::TimeSeries,
                actual: infer_shape(n_rows, &result.data.columns, &result.data.rows),
            });
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Rule: truncation_warning
// ---------------------------------------------------------------------------

/// Rule: `truncation_warning`
///
/// When the result was truncated (`truncated == true`) and the expected shape
/// is `Scalar` or `Series`, the query is structurally wrong — aggregates
/// should never produce enough rows to hit the sample limit.
///
/// **Stage:** `solved`
/// **Errors:** [`AnalyticsError::ShapeMismatch`]
/// **Params:** none
pub struct TruncationWarningRule;

impl TruncationWarningRule {
    pub fn from_params(_params: &Value) -> Result<Box<dyn SolvedRule>, RegistryError> {
        Ok(Box::new(Self))
    }
}

impl SolvedRule for TruncationWarningRule {
    fn name(&self) -> &'static str {
        "truncation_warning"
    }

    fn description(&self) -> &'static str {
        "Aggregates (Scalar/Series) must not produce enough rows to trigger truncation."
    }

    fn check(&self, ctx: &SolvedCtx<'_>) -> Result<(), AnalyticsError> {
        let result = ctx.result.primary();
        if !result.data.truncated {
            return Ok(());
        }
        match ctx.spec.expected_result_shape {
            ResultShape::Scalar | ResultShape::Series => Err(AnalyticsError::ShapeMismatch {
                expected: ctx.spec.expected_result_shape.clone(),
                actual: infer_shape(
                    result.data.total_row_count,
                    &result.data.columns,
                    &result.data.rows,
                ),
            }),
            _ => Ok(()),
        }
    }
}

// ---------------------------------------------------------------------------
// Rule: null_ratio_check
// ---------------------------------------------------------------------------

/// Rule: `null_ratio_check`
///
/// Flags any metric (numeric) column where the proportion of NULL values
/// exceeds `threshold` (default 0.5 = 50%).  A high NULL ratio in a metric
/// column almost always indicates a bad JOIN that fails to match rows.
///
/// At `threshold: 1.0` this becomes an "all nulls column check".
///
/// **Stage:** `solved`
/// **Errors:** [`AnalyticsError::SyntaxError`] (routes to Solve, not Interpret)
/// **Params:**
/// - `threshold` (`f64`, default `0.5`) — NULL ratio above which the check fails
pub struct NullRatioCheckRule {
    pub threshold: f64,
}

impl NullRatioCheckRule {
    pub fn from_params(params: &Value) -> Result<Box<dyn SolvedRule>, RegistryError> {
        let p: NullRatioCheckParams = if params.is_null() {
            NullRatioCheckParams::default()
        } else {
            serde_json::from_value(params.clone()).map_err(|e| RegistryError::InvalidParams {
                name: "null_ratio_check".into(),
                reason: e.to_string(),
            })?
        };
        Ok(Box::new(Self {
            threshold: p.threshold,
        }))
    }
}

impl SolvedRule for NullRatioCheckRule {
    fn name(&self) -> &'static str {
        "null_ratio_check"
    }

    fn description(&self) -> &'static str {
        "Metric columns must not have a NULL ratio above the configured threshold."
    }

    fn check(&self, ctx: &SolvedCtx<'_>) -> Result<(), AnalyticsError> {
        let result = ctx.result.primary();
        let total_rows = result.data.rows.len();
        if total_rows == 0 {
            return Ok(()); // non_empty handles this
        }

        // When the spec includes JOINs, LEFT/OUTER JOINs legitimately produce
        // NULLs in the joined table's columns.  Since the spec doesn't carry
        // JOIN type info, we conservatively raise the threshold by 0.25 when
        // any joins are present (e.g. 0.5 → 0.75).
        let effective_threshold = if ctx.spec.join_path.is_empty() {
            self.threshold
        } else {
            (self.threshold + 0.25).min(1.0)
        };

        for (col_idx, col_name) in result.data.columns.iter().enumerate() {
            // Only check columns that contain at least one numeric value
            // (i.e. metric columns). Pure text/dimension columns are skipped.
            let has_any_number = result
                .data
                .rows
                .iter()
                .any(|row| matches!(row.0.get(col_idx), Some(CellValue::Number(_))));
            if !has_any_number {
                continue;
            }

            let null_count = result
                .data
                .rows
                .iter()
                .filter(|row| matches!(row.0.get(col_idx), None | Some(CellValue::Null)))
                .count();

            let ratio = null_count as f64 / total_rows as f64;
            if ratio > effective_threshold {
                // Emit SyntaxError (not ValueAnomaly) because a high NULL ratio
                // indicates a SQL-generation bug (bad JOIN), not a result
                // interpretation issue.  This routes to BackTarget::Solve so the
                // LLM regenerates the SQL instead of re-interpreting bad data.
                return Err(AnalyticsError::SyntaxError {
                    query: String::new(),
                    message: format!(
                        "metric column `{col_name}` has {null_count}/{total_rows} NULL values \
                         ({:.0}%), which exceeds the {:.0}% threshold — likely a bad JOIN. \
                         Regenerate the query with correct JOIN conditions.",
                        ratio * 100.0,
                        effective_threshold * 100.0
                    ),
                });
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Rule: duplicate_row_check
// ---------------------------------------------------------------------------

/// Rule: `duplicate_row_check`
///
/// Detects fully-duplicate rows in the result.  For analytic queries this
/// almost always indicates a bad JOIN or missing `DISTINCT`.
///
/// To avoid false positives on queries that legitimately produce identical
/// aggregate rows (e.g. two categories with the same count), the rule only
/// fires when the *ratio* of duplicate rows to total rows exceeds
/// `max_duplicate_ratio` (default 10%).
///
/// The check runs only on the bounded sample (`rows`), so it is O(rows)
/// with a hash set — safe even on the maximum sample size.
///
/// **Stage:** `solved`
/// **Errors:** [`AnalyticsError::SyntaxError`] (routes to Solve, not Interpret)
/// **Params:**
/// - `max_duplicate_ratio` (`f64`, default `0.1`) — duplicate fraction above which the check fails
pub struct DuplicateRowCheckRule {
    pub max_duplicate_ratio: f64,
}

impl DuplicateRowCheckRule {
    pub fn from_params(params: &Value) -> Result<Box<dyn SolvedRule>, RegistryError> {
        let p: DuplicateRowCheckParams = if params.is_null() {
            DuplicateRowCheckParams::default()
        } else {
            serde_json::from_value(params.clone()).map_err(|e| RegistryError::InvalidParams {
                name: "duplicate_row_check".into(),
                reason: e.to_string(),
            })?
        };
        Ok(Box::new(Self {
            max_duplicate_ratio: p.max_duplicate_ratio,
        }))
    }
}

/// Format a row's cells as a comparable string for dedup hashing.
fn row_key(row: &agentic_core::QueryRow) -> Vec<String> {
    row.0
        .iter()
        .map(|cell| match cell {
            CellValue::Text(s) => format!("T:{s}"),
            CellValue::Number(n) => format!("N:{n}"),
            CellValue::Null => "NULL".to_string(),
        })
        .collect()
}

impl SolvedRule for DuplicateRowCheckRule {
    fn name(&self) -> &'static str {
        "duplicate_row_check"
    }

    fn description(&self) -> &'static str {
        "Result must not have more than max_duplicate_ratio fraction of duplicate rows."
    }

    fn check(&self, ctx: &SolvedCtx<'_>) -> Result<(), AnalyticsError> {
        let result = ctx.result.primary();
        let rows = &result.data.rows;
        let total = rows.len();
        if total < 2 {
            return Ok(());
        }

        let mut seen = HashSet::with_capacity(total);
        let mut duplicate_count: usize = 0;
        for row in rows {
            let key = row_key(row);
            if !seen.insert(key) {
                duplicate_count += 1;
            }
        }

        let ratio = duplicate_count as f64 / total as f64;
        if ratio > self.max_duplicate_ratio {
            // Emit SyntaxError (not ValueAnomaly) because duplicate rows
            // indicate a SQL-generation bug (bad JOIN or missing DISTINCT),
            // not a result interpretation issue.  This routes to
            // BackTarget::Solve so the LLM regenerates the SQL.
            return Err(AnalyticsError::SyntaxError {
                query: String::new(),
                message: format!(
                    "{:.0}% of rows are duplicates ({duplicate_count}/{total}, \
                     threshold {:.0}%) — likely a bad JOIN or missing DISTINCT. \
                     Regenerate the query with correct JOIN conditions or add DISTINCT.",
                    ratio * 100.0,
                    self.max_duplicate_ratio * 100.0
                ),
            });
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Backward-compatible free function
// ---------------------------------------------------------------------------

/// Validate that the results of an executed query are non-empty, match the
/// expected shape, and contain plausible numeric values.
pub fn validate_solved(result: &AnalyticsResult, spec: &QuerySpec) -> Result<(), AnalyticsError> {
    let ctx = SolvedCtx { result, spec };
    // Order matters: structural checks first, then value-level checks.
    // Structural errors route to Solve; ValueAnomaly routes to Interpret.
    NonEmptyRule.check(&ctx)?;
    TruncationWarningRule.check(&ctx)?;
    NoNanInfRule.check(&ctx)?;
    OutlierDetectionRule {
        threshold_sigma: 5.0,
        min_rows: 4,
    }
    .check(&ctx)?;
    NullRatioCheckRule { threshold: 0.5 }.check(&ctx)?;
    DuplicateRowCheckRule {
        max_duplicate_ratio: 0.1,
    }
    .check(&ctx)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::validate_solved;
    use crate::validation::test_fixtures::*;
    use crate::{AnalyticsError, AnalyticsResult, QuerySpec, ResultShape};
    use agentic_core::QueryResult;
    use agentic_core::result::CellValue;

    // ── validate_solved: happy paths ──────────────────────────────────────────

    #[test]
    fn solved_happy_scalar() {
        let spec = QuerySpec {
            expected_result_shape: ResultShape::Scalar,
            ..make_spec()
        };
        assert_eq!(validate_solved(&scalar_result(42.0), &spec), Ok(()));
    }

    #[test]
    fn solved_happy_series() {
        let spec = QuerySpec {
            expected_result_shape: ResultShape::Series,
            ..make_spec()
        };
        assert_eq!(
            validate_solved(&series_result(&[1.0, 2.0, 3.0]), &spec),
            Ok(())
        );
    }

    #[test]
    fn solved_happy_table() {
        let result = table_result(
            vec!["region".into(), "revenue".into()],
            vec![
                vec![CellValue::Text("East".into()), CellValue::Number(500.0)],
                vec![CellValue::Text("West".into()), CellValue::Number(400.0)],
            ],
        );
        assert_eq!(validate_solved(&result, &make_spec()), Ok(()));
    }

    #[test]
    fn solved_happy_timeseries() {
        let spec = QuerySpec {
            expected_result_shape: ResultShape::TimeSeries,
            ..make_spec()
        };
        assert_eq!(validate_solved(&timeseries_result(), &spec), Ok(()));
    }

    #[test]
    fn solved_extra_columns_in_table_are_ok() {
        let result = table_result(
            vec!["region".into(), "revenue".into(), "order_count".into()],
            vec![vec![
                CellValue::Text("East".into()),
                CellValue::Number(500.0),
                CellValue::Number(10.0),
            ]],
        );
        assert_eq!(validate_solved(&result, &make_spec()), Ok(()));
    }

    // ── validate_solved: empty results ────────────────────────────────────────

    #[test]
    fn solved_empty_rows() {
        let result = AnalyticsResult::single(
            QueryResult {
                columns: vec!["a".into()],
                rows: vec![],
                total_row_count: 0,
                truncated: false,
            },
            None,
        );
        assert_eq!(
            validate_solved(&result, &make_spec()),
            Err(AnalyticsError::EmptyResults {
                query: "executed query".into()
            })
        );
    }

    // ── validate_solved: shape_match removed from default chain ────────────
    // ShapeMatchRule and TimeseriesDateCheckRule are no longer in the default
    // validation chain.  Tests for those rules live on their own structs
    // (ShapeMatchRule.check, TimeseriesDateCheckRule.check) — the free
    // function `validate_solved` only runs non_empty + no_nan_inf + outlier.

    // ── validate_solved: value anomalies ──────────────────────────────────────

    #[test]
    fn solved_nan_value() {
        let spec = QuerySpec {
            expected_result_shape: ResultShape::Scalar,
            ..make_spec()
        };
        assert!(matches!(
            validate_solved(&scalar_result(f64::NAN), &spec),
            Err(AnalyticsError::ValueAnomaly { value, .. }) if value == "NaN"
        ));
    }

    #[test]
    fn solved_positive_infinity() {
        let spec = QuerySpec {
            expected_result_shape: ResultShape::Scalar,
            ..make_spec()
        };
        assert!(matches!(
            validate_solved(&scalar_result(f64::INFINITY), &spec),
            Err(AnalyticsError::ValueAnomaly { value, .. }) if value == "Inf"
        ));
    }

    #[test]
    fn solved_negative_infinity() {
        let spec = QuerySpec {
            expected_result_shape: ResultShape::Scalar,
            ..make_spec()
        };
        assert!(matches!(
            validate_solved(&scalar_result(f64::NEG_INFINITY), &spec),
            Err(AnalyticsError::ValueAnomaly { value, .. }) if value == "-Inf"
        ));
    }

    #[test]
    fn solved_statistical_outlier_z_above_5() {
        let spec = QuerySpec {
            expected_result_shape: ResultShape::Series,
            ..make_spec()
        };
        let mut values = vec![100.0_f64; 50];
        values.push(100_000.0);
        assert!(matches!(
            validate_solved(&series_result(&values), &spec),
            Err(AnalyticsError::ValueAnomaly { column, .. }) if column == "value"
        ));
    }

    #[test]
    fn solved_borderline_z_below_5_is_ok() {
        let spec = QuerySpec {
            expected_result_shape: ResultShape::Series,
            ..make_spec()
        };
        assert_eq!(
            validate_solved(&series_result(&[10.0, 11.0, 9.0, 10.5, 10.2]), &spec),
            Ok(())
        );
    }

    #[test]
    fn solved_outlier_check_requires_at_least_4_rows() {
        // Only 3 rows �� min_rows check skips outlier detection entirely.
        // Use unique rows to avoid triggering duplicate_row_check.
        let result = table_result(
            vec!["id".into(), "value".into()],
            vec![
                vec![CellValue::Text("a".into()), CellValue::Number(1.0)],
                vec![CellValue::Text("b".into()), CellValue::Number(1.0)],
                vec![CellValue::Text("c".into()), CellValue::Number(999.0)],
            ],
        );
        assert_eq!(validate_solved(&result, &make_spec()), Ok(()));
    }

    #[test]
    fn solved_all_same_value_no_std_dev_ok() {
        // Use a table with unique dimension values but identical metric values
        // to test that outlier detection skips when std dev = 0, without
        // triggering the duplicate_row_check.
        let result = table_result(
            vec!["region".into(), "revenue".into()],
            vec![
                vec![CellValue::Text("A".into()), CellValue::Number(5.0)],
                vec![CellValue::Text("B".into()), CellValue::Number(5.0)],
                vec![CellValue::Text("C".into()), CellValue::Number(5.0)],
                vec![CellValue::Text("D".into()), CellValue::Number(5.0)],
                vec![CellValue::Text("E".into()), CellValue::Number(5.0)],
            ],
        );
        assert_eq!(validate_solved(&result, &make_spec()), Ok(()));
    }

    #[test]
    fn solved_text_columns_skip_numeric_checks() {
        let result = table_result(
            vec!["region".into(), "revenue".into()],
            vec![
                vec![
                    CellValue::Text("East".into()),
                    CellValue::Text("not_a_number".into()),
                ],
                vec![
                    CellValue::Text("West".into()),
                    CellValue::Text("also_text".into()),
                ],
            ],
        );
        assert_eq!(validate_solved(&result, &make_spec()), Ok(()));
    }

    // ── truncation_warning ─────────────────────────────────────────────────────

    #[test]
    fn truncation_warning_fires_for_truncated_scalar() {
        use super::{SolvedRule, TruncationWarningRule};
        use crate::validation::rule::SolvedCtx;

        let spec = QuerySpec {
            expected_result_shape: ResultShape::Scalar,
            ..make_spec()
        };
        let result = AnalyticsResult::single(
            QueryResult {
                columns: vec!["total".into()],
                rows: vec![agentic_core::QueryRow(vec![CellValue::Number(1.0)])],
                total_row_count: 1000,
                truncated: true,
            },
            None,
        );
        let ctx = SolvedCtx {
            result: &result,
            spec: &spec,
        };
        assert!(matches!(
            TruncationWarningRule.check(&ctx),
            Err(AnalyticsError::ShapeMismatch { .. })
        ));
    }

    #[test]
    fn truncation_warning_fires_for_truncated_series() {
        use super::{SolvedRule, TruncationWarningRule};
        use crate::validation::rule::SolvedCtx;

        let spec = QuerySpec {
            expected_result_shape: ResultShape::Series,
            ..make_spec()
        };
        let result = AnalyticsResult::single(
            QueryResult {
                columns: vec!["value".into()],
                rows: vec![agentic_core::QueryRow(vec![CellValue::Number(1.0)])],
                total_row_count: 5000,
                truncated: true,
            },
            None,
        );
        let ctx = SolvedCtx {
            result: &result,
            spec: &spec,
        };
        assert!(matches!(
            TruncationWarningRule.check(&ctx),
            Err(AnalyticsError::ShapeMismatch { .. })
        ));
    }

    #[test]
    fn truncation_warning_ok_for_truncated_table() {
        use super::{SolvedRule, TruncationWarningRule};
        use crate::validation::rule::SolvedCtx;

        // Table shape being truncated is normal — not an error.
        let spec = make_spec(); // default is Table shape
        let result = AnalyticsResult::single(
            QueryResult {
                columns: vec!["region".into(), "revenue".into()],
                rows: vec![agentic_core::QueryRow(vec![
                    CellValue::Text("East".into()),
                    CellValue::Number(500.0),
                ])],
                total_row_count: 5000,
                truncated: true,
            },
            None,
        );
        let ctx = SolvedCtx {
            result: &result,
            spec: &spec,
        };
        assert_eq!(TruncationWarningRule.check(&ctx), Ok(()));
    }

    #[test]
    fn truncation_warning_ok_when_not_truncated() {
        use super::{SolvedRule, TruncationWarningRule};
        use crate::validation::rule::SolvedCtx;

        let spec = QuerySpec {
            expected_result_shape: ResultShape::Scalar,
            ..make_spec()
        };
        let result = scalar_result(42.0);
        let ctx = SolvedCtx {
            result: &result,
            spec: &spec,
        };
        assert_eq!(TruncationWarningRule.check(&ctx), Ok(()));
    }

    // ── null_ratio_check ─────────────────────────────────────────────────────

    #[test]
    fn null_ratio_check_fires_when_over_threshold() {
        use super::{NullRatioCheckRule, SolvedRule};
        use crate::validation::rule::SolvedCtx;

        // 3 out of 4 metric values are NULL → 75% > 50% threshold
        // Use a spec with no joins so the base threshold applies directly.
        let result = table_result(
            vec!["region".into(), "revenue".into()],
            vec![
                vec![CellValue::Text("A".into()), CellValue::Number(100.0)],
                vec![CellValue::Text("B".into()), CellValue::Null],
                vec![CellValue::Text("C".into()), CellValue::Null],
                vec![CellValue::Text("D".into()), CellValue::Null],
            ],
        );
        let mut spec = make_spec();
        spec.join_path = vec![]; // no joins → base threshold 0.5 applies
        let ctx = SolvedCtx {
            result: &result,
            spec: &spec,
        };
        assert!(matches!(
            NullRatioCheckRule { threshold: 0.5 }.check(&ctx),
            Err(AnalyticsError::SyntaxError { message, .. }) if message.contains("revenue")
        ));
    }

    #[test]
    fn null_ratio_check_ok_when_under_threshold() {
        use super::{NullRatioCheckRule, SolvedRule};
        use crate::validation::rule::SolvedCtx;

        // 1 out of 4 NULL → 25% < 50% threshold
        let result = table_result(
            vec!["region".into(), "revenue".into()],
            vec![
                vec![CellValue::Text("A".into()), CellValue::Number(100.0)],
                vec![CellValue::Text("B".into()), CellValue::Number(200.0)],
                vec![CellValue::Text("C".into()), CellValue::Number(300.0)],
                vec![CellValue::Text("D".into()), CellValue::Null],
            ],
        );
        let spec = make_spec();
        let ctx = SolvedCtx {
            result: &result,
            spec: &spec,
        };
        assert_eq!(NullRatioCheckRule { threshold: 0.5 }.check(&ctx), Ok(()));
    }

    #[test]
    fn null_ratio_check_skips_text_only_columns() {
        use super::{NullRatioCheckRule, SolvedRule};
        use crate::validation::rule::SolvedCtx;

        // "region" column is all NULL but has no numeric values → skip
        let result = table_result(
            vec!["region".into(), "revenue".into()],
            vec![
                vec![CellValue::Null, CellValue::Number(100.0)],
                vec![CellValue::Null, CellValue::Number(200.0)],
            ],
        );
        let spec = make_spec();
        let ctx = SolvedCtx {
            result: &result,
            spec: &spec,
        };
        assert_eq!(NullRatioCheckRule { threshold: 0.5 }.check(&ctx), Ok(()));
    }

    #[test]
    fn null_ratio_check_join_boost_raises_threshold() {
        use super::{NullRatioCheckRule, SolvedRule};
        use crate::validation::rule::SolvedCtx;

        // 3 out of 4 NULL → 75%.  Base threshold 0.5 would fail,
        // but spec has join_path → effective threshold = 0.75 → exactly at
        // boundary (uses >, not >=) → OK.
        let result = table_result(
            vec!["region".into(), "revenue".into()],
            vec![
                vec![CellValue::Text("A".into()), CellValue::Number(100.0)],
                vec![CellValue::Text("B".into()), CellValue::Null],
                vec![CellValue::Text("C".into()), CellValue::Null],
                vec![CellValue::Text("D".into()), CellValue::Null],
            ],
        );
        // make_spec() has join_path = [("orders", "customers", "customer_id")]
        let spec = make_spec();
        let ctx = SolvedCtx {
            result: &result,
            spec: &spec,
        };
        assert_eq!(NullRatioCheckRule { threshold: 0.5 }.check(&ctx), Ok(()));
    }

    #[test]
    fn null_ratio_check_no_join_uses_base_threshold() {
        use super::{NullRatioCheckRule, SolvedRule};
        use crate::validation::rule::SolvedCtx;

        // Same 75% NULL, but no joins → base threshold 0.5 applies → fails
        let result = table_result(
            vec!["region".into(), "revenue".into()],
            vec![
                vec![CellValue::Text("A".into()), CellValue::Number(100.0)],
                vec![CellValue::Text("B".into()), CellValue::Null],
                vec![CellValue::Text("C".into()), CellValue::Null],
                vec![CellValue::Text("D".into()), CellValue::Null],
            ],
        );
        let mut spec = make_spec();
        spec.join_path = vec![]; // no joins
        let ctx = SolvedCtx {
            result: &result,
            spec: &spec,
        };
        assert!(matches!(
            NullRatioCheckRule { threshold: 0.5 }.check(&ctx),
            Err(AnalyticsError::SyntaxError { message, .. }) if message.contains("revenue")
        ));
    }

    #[test]
    fn null_ratio_check_custom_threshold_1_0_catches_all_nulls() {
        use super::{NullRatioCheckRule, SolvedRule};
        use crate::validation::rule::SolvedCtx;

        // 2 out of 3 NULL → 67%, but threshold is 1.0 → OK
        let result = table_result(
            vec!["revenue".into()],
            vec![
                vec![CellValue::Number(100.0)],
                vec![CellValue::Null],
                vec![CellValue::Null],
            ],
        );
        let spec = make_spec();
        let ctx = SolvedCtx {
            result: &result,
            spec: &spec,
        };
        assert_eq!(NullRatioCheckRule { threshold: 1.0 }.check(&ctx), Ok(()));
    }

    // ── duplicate_row_check ──────────────────────────────────────────────────

    #[test]
    fn duplicate_row_check_fires_when_ratio_exceeds_threshold() {
        use super::{DuplicateRowCheckRule, SolvedRule};
        use crate::validation::rule::SolvedCtx;

        // 2 identical rows out of 2 → 50% duplicate ratio > 10% default
        let result = table_result(
            vec!["region".into(), "revenue".into()],
            vec![
                vec![CellValue::Text("East".into()), CellValue::Number(500.0)],
                vec![CellValue::Text("East".into()), CellValue::Number(500.0)],
            ],
        );
        let spec = make_spec();
        let ctx = SolvedCtx {
            result: &result,
            spec: &spec,
        };
        let rule = DuplicateRowCheckRule {
            max_duplicate_ratio: 0.1,
        };
        assert!(matches!(
            rule.check(&ctx),
            Err(AnalyticsError::SyntaxError { message, .. })
                if message.contains("duplicate")
        ));
    }

    #[test]
    fn duplicate_row_check_ok_when_ratio_below_threshold() {
        use super::{DuplicateRowCheckRule, SolvedRule};
        use crate::validation::rule::SolvedCtx;

        // 1 duplicate out of 10 → 10%, threshold is 0.1 → exactly at boundary
        // (uses >, not >=, so 10% == 10% does not fire)
        let mut rows = Vec::new();
        for i in 0..9 {
            rows.push(vec![
                CellValue::Text(format!("region_{i}")),
                CellValue::Number(i as f64 * 100.0),
            ]);
        }
        // Add one duplicate of the first row
        rows.push(vec![
            CellValue::Text("region_0".into()),
            CellValue::Number(0.0),
        ]);
        let result = table_result(vec!["region".into(), "revenue".into()], rows);
        let spec = make_spec();
        let ctx = SolvedCtx {
            result: &result,
            spec: &spec,
        };
        let rule = DuplicateRowCheckRule {
            max_duplicate_ratio: 0.1,
        };
        assert_eq!(rule.check(&ctx), Ok(()));
    }

    #[test]
    fn duplicate_row_check_ok_with_unique_rows() {
        use super::{DuplicateRowCheckRule, SolvedRule};
        use crate::validation::rule::SolvedCtx;

        let result = table_result(
            vec!["region".into(), "revenue".into()],
            vec![
                vec![CellValue::Text("East".into()), CellValue::Number(500.0)],
                vec![CellValue::Text("West".into()), CellValue::Number(400.0)],
            ],
        );
        let spec = make_spec();
        let ctx = SolvedCtx {
            result: &result,
            spec: &spec,
        };
        let rule = DuplicateRowCheckRule {
            max_duplicate_ratio: 0.1,
        };
        assert_eq!(rule.check(&ctx), Ok(()));
    }

    #[test]
    fn duplicate_row_check_ok_with_single_row() {
        use super::{DuplicateRowCheckRule, SolvedRule};
        use crate::validation::rule::SolvedCtx;

        let result = scalar_result(42.0);
        let spec = make_spec();
        let ctx = SolvedCtx {
            result: &result,
            spec: &spec,
        };
        let rule = DuplicateRowCheckRule {
            max_duplicate_ratio: 0.1,
        };
        assert_eq!(rule.check(&ctx), Ok(()));
    }

    // ── Bug-fix #5: 8-digit numbers no longer accepted as dates ──────────────

    #[test]
    fn timeseries_rejects_8_digit_number_as_date() {
        use super::looks_like_date;
        assert!(
            !looks_like_date("12345678"),
            "8-digit number should NOT be a date"
        );
        assert!(
            !looks_like_date("20240101"),
            "8-digit YYYYMMDD should NOT match without dash"
        );
    }

    #[test]
    fn timeseries_accepts_iso_date_formats() {
        use super::looks_like_date;
        assert!(looks_like_date("2024-01-15"));
        assert!(looks_like_date("2024-01"));
        assert!(looks_like_date("2024-W05"));
        assert!(looks_like_date("2024-W05-3"));
        assert!(looks_like_date("Monday"));
        assert!(looks_like_date("mon"));
    }

    // ── Bug-fix #6: infer_shape now returns TimeSeries ────────────────────────

    #[test]
    fn infer_shape_returns_table_for_date_first_col() {
        use super::infer_shape;
        use agentic_core::QueryRow;
        let columns = vec!["date".into(), "revenue".into()];
        let rows = vec![
            QueryRow(vec![
                CellValue::Text("2024-01".into()),
                CellValue::Number(100.0),
            ]),
            QueryRow(vec![
                CellValue::Text("2024-02".into()),
                CellValue::Number(200.0),
            ]),
        ];
        // infer_shape no longer returns TimeSeries — date columns are just Table columns.
        assert_eq!(
            infer_shape(2, &columns, &rows),
            ResultShape::Table {
                columns: vec!["date".into(), "revenue".into()]
            }
        );
    }

    // ── Bug-fix #8: min_rows guard applies on summary-stats path ─────────────

    #[test]
    fn outlier_skips_when_fewer_than_min_rows_even_with_summary_stats() {
        use super::OutlierDetectionRule;
        use super::SolvedRule;
        use crate::validation::rule::SolvedCtx;
        use agentic_connector::{ColumnStats, ResultSummary};
        use agentic_core::result::CellValue as CV;

        // Build a 3-row result with summary stats attached.
        let mut result = series_result(&[1.0, 1.0, 100_000.0]);
        result.results[0].summary = Some(ResultSummary {
            row_count: 3,
            columns: vec![ColumnStats {
                name: "value".into(),
                data_type: None,
                null_count: 0,
                distinct_count: None,
                min: Some(CV::Number(1.0)),
                max: Some(CV::Number(100_000.0)),
                mean: Some(33334.0),
                std_dev: Some(1.0), // tiny std dev → extreme z-score if not gated
            }],
        });

        let spec = QuerySpec {
            expected_result_shape: ResultShape::Series,
            ..make_spec()
        };

        let rule = OutlierDetectionRule {
            threshold_sigma: 5.0,
            min_rows: 4, // 3 rows < min_rows → should skip
        };
        let ctx = SolvedCtx {
            result: &result,
            spec: &spec,
        };
        // Should pass because min_rows gate fires before summary-stats check.
        assert_eq!(rule.check(&ctx), Ok(()));
    }
}
