use super::{extract_filter_lhs, validate_specified};
use crate::AnalyticsError;
use crate::validation::test_fixtures::*;

// ── validate_specified: happy path ────────────────────────────────────────

#[test]
fn specified_happy_path() {
    assert_eq!(validate_specified(&make_spec(), &sample_catalog()), Ok(()));
}

#[test]
fn specified_happy_path_with_filter() {
    let mut spec = make_spec();
    spec.intent.filters = vec!["date >= '2024-01-01'".into()];
    assert_eq!(validate_specified(&spec, &sample_catalog()), Ok(()));
}

#[test]
fn specified_happy_path_qualified_filter() {
    let mut spec = make_spec();
    spec.intent.filters = vec!["orders.status = 'completed'".into()];
    assert_eq!(validate_specified(&spec, &sample_catalog()), Ok(()));
}

// ── validate_specified: unresolved metric ─────────────────────────────────

#[test]
fn specified_unknown_table_in_metric() {
    let mut spec = make_spec();
    spec.resolved_metrics = vec!["ghost_table.revenue".into()];
    assert_eq!(
        validate_specified(&spec, &sample_catalog()),
        Err(AnalyticsError::UnresolvedMetric {
            metric: "ghost_table.revenue".into()
        })
    );
}

#[test]
fn specified_unknown_column_in_metric() {
    let mut spec = make_spec();
    spec.resolved_metrics = vec!["orders.nonexistent".into()];
    assert_eq!(
        validate_specified(&spec, &sample_catalog()),
        Err(AnalyticsError::UnresolvedMetric {
            metric: "orders.nonexistent".into()
        })
    );
}

#[test]
fn specified_metric_not_dotted() {
    let mut spec = make_spec();
    spec.resolved_metrics = vec!["bare_column".into()];
    assert!(matches!(
        validate_specified(&spec, &sample_catalog()),
        Err(AnalyticsError::UnresolvedMetric { .. })
    ));
}

// ── Bug-fix #3: is_sql_expression no longer triggers on spaces ────────────

#[test]
fn specified_metric_with_space_not_treated_as_expression() {
    // A metric with a space is NOT treated as a SQL expression — it falls
    // through to parse_dotted, which rejects it as a non-dotted string.
    let mut spec = make_spec();
    spec.resolved_metrics = vec!["orders revenue".into()]; // space, not dot
    assert!(matches!(
        validate_specified(&spec, &sample_catalog()),
        Err(AnalyticsError::UnresolvedMetric { .. })
    ));
}

// ── validate_specified: ambiguous column ──────────────────────────────────

#[test]
fn specified_ambiguous_column_in_metric() {
    // products.customer_id: table exists, but column doesn't.
    // customer_id is in orders + customers → ambiguous.
    let mut spec = make_spec();
    spec.resolved_metrics = vec!["products.customer_id".into()];
    assert!(matches!(
        validate_specified(&spec, &sample_catalog()),
        Err(AnalyticsError::AmbiguousColumn { column, .. }) if column == "customer_id"
    ));
}

// ── validate_specified: join path errors ──────────────────────────────────

#[test]
fn specified_join_unknown_left_table() {
    let mut spec = make_spec();
    spec.join_path = vec![("ghost".into(), "customers".into(), "customer_id".into())];
    assert!(matches!(
        validate_specified(&spec, &sample_catalog()),
        Err(AnalyticsError::UnresolvedJoin { left, .. }) if left == "ghost"
    ));
}

#[test]
fn specified_join_unknown_right_table() {
    let mut spec = make_spec();
    spec.join_path = vec![("orders".into(), "ghost".into(), "customer_id".into())];
    assert!(matches!(
        validate_specified(&spec, &sample_catalog()),
        Err(AnalyticsError::UnresolvedJoin { right, .. }) if right == "ghost"
    ));
}

#[test]
fn specified_join_key_not_in_either_table() {
    let mut spec = make_spec();
    spec.join_path = vec![("orders".into(), "products".into(), "nonexistent_key".into())];
    assert!(matches!(
        validate_specified(&spec, &sample_catalog()),
        Err(AnalyticsError::UnresolvedJoin { key, .. }) if key == "nonexistent_key"
    ));
}

#[test]
fn specified_join_key_in_one_table_is_ok() {
    let mut spec = make_spec();
    spec.resolved_tables = vec!["orders".into(), "products".into()];
    spec.join_path = vec![("orders".into(), "products".into(), "order_id".into())];
    assert_eq!(validate_specified(&spec, &sample_catalog()), Ok(()));
}

// ── validate_specified: filter column errors ──────────────────────────────

#[test]
fn specified_filter_unqualified_unknown_column_is_treated_as_alias() {
    let mut spec = make_spec();
    spec.intent.filters = vec!["ghost_col = 'x'".into()];
    assert_eq!(validate_specified(&spec, &sample_catalog()), Ok(()));
}

#[test]
fn specified_filter_qualified_ref_to_unknown_column_fails() {
    // Bug-fix #1: now returns UnresolvedJoin (not UnresolvedMetric).
    let mut spec = make_spec();
    spec.intent.filters = vec!["ref_date = max(orders.ghost_col)".into()];
    assert!(matches!(
        validate_specified(&spec, &sample_catalog()),
        Err(AnalyticsError::UnresolvedJoin { .. })
    ));
}

#[test]
fn specified_filter_alias_with_valid_table_refs_passes() {
    let mut spec = make_spec();
    spec.resolved_tables = vec!["orders".into()];
    spec.intent.filters = vec!["reference_date = max(orders.date)".into()];
    assert_eq!(validate_specified(&spec, &sample_catalog()), Ok(()));
}

#[test]
fn specified_filter_qualified_unknown_table() {
    let mut spec = make_spec();
    spec.intent.filters = vec!["ghost.date >= '2024-01-01'".into()];
    // Bug-fix #1: now UnresolvedJoin, not UnresolvedMetric.
    assert!(matches!(
        validate_specified(&spec, &sample_catalog()),
        Err(AnalyticsError::UnresolvedJoin { .. })
    ));
}

#[test]
fn specified_filter_ambiguous_in_both_resolved_tables() {
    let mut spec = make_spec();
    spec.intent.filters = vec!["customer_id = 42".into()];
    assert!(matches!(
        validate_specified(&spec, &sample_catalog()),
        Err(AnalyticsError::AmbiguousColumn { column, .. }) if column == "customer_id"
    ));
}

// ── extract_filter_lhs (sqlparser-based) ──────────────────────────────────

#[test]
fn filter_lhs_simple_eq() {
    assert_eq!(
        extract_filter_lhs("status = 'active'"),
        Some("status".into())
    );
}

#[test]
fn filter_lhs_gte() {
    assert_eq!(
        extract_filter_lhs("date >= '2024-01-01'"),
        Some("date".into())
    );
}

#[test]
fn filter_lhs_qualified() {
    assert_eq!(
        extract_filter_lhs("orders.status IN ('open','closed')"),
        Some("orders.status".into())
    );
}

#[test]
fn filter_lhs_between() {
    assert_eq!(
        extract_filter_lhs("revenue BETWEEN 100 AND 500"),
        Some("revenue".into())
    );
}

#[test]
fn filter_lhs_is_null() {
    assert_eq!(
        extract_filter_lhs("deleted_at IS NULL"),
        Some("deleted_at".into())
    );
}

#[test]
fn filter_lhs_like() {
    assert_eq!(extract_filter_lhs("name LIKE 'A%'"), Some("name".into()));
}

#[test]
fn filter_lhs_not_between() {
    assert_eq!(
        extract_filter_lhs("revenue NOT BETWEEN 0 AND 10"),
        Some("revenue".into())
    );
}
