//! The eight `SolvedRule` implementations.

use std::collections::HashSet;

use agentic_core::result::CellValue;
use serde_json::Value;
use statrs::statistics::Statistics;

use crate::{AnalyticsError, ResultShape};

use super::super::config::{DuplicateRowCheckParams, NullRatioCheckParams, OutlierDetectionParams};
use super::super::registry::RegistryError;
use super::super::rule::{SolvedCtx, SolvedRule};
use super::helpers::{infer_shape, looks_like_date};

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
