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
