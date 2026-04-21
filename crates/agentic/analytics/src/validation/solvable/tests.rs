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
