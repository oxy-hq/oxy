//! Pure helpers for reconcile operands: build the semantic `QueryRequest`
//! (window injected), render SQL templates, and coerce a SQL scalar cell to
//! `f64`. No I/O — all unit-testable.

use agentic_core::result::CellValue;
use airlayer::engine::query::{QueryRequest, TimeDimensionQuery};

/// Clone the user's semantic query and OWN the time window: overwrite
/// `time_dimensions` with the reconcile window bound to `time_dimension`. Any
/// user-supplied `time_dimensions` is intentionally replaced — reconcile always
/// controls the comparison period so both sides align.
#[allow(dead_code)]
pub(super) fn semantic_request(
    query: &QueryRequest,
    time_dimension: &str,
    period: &[String; 2],
) -> QueryRequest {
    let mut req = query.clone();
    req.time_dimensions = vec![TimeDimensionQuery {
        dimension: time_dimension.to_string(),
        granularity: None,
        date_range: Some(vec![period[0].clone(), period[1].clone()]),
    }];
    req
}

/// Render a reconcile SQL template, binding `start_date` / `end_date` from the
/// resolved window. Values are rendered BARE — the SQL author quotes them.
#[allow(dead_code)]
pub(super) fn render_sql(sql: &str, period: &[String; 2]) -> Result<String, String> {
    minijinja::Environment::new()
        .render_str(
            sql,
            minijinja::context! { start_date => period[0], end_date => period[1] },
        )
        .map_err(|e| format!("reconcile sql template render failed: {e}"))
}

/// Coerce the first cell of a SQL scalar result to `f64`.
#[allow(dead_code)]
pub(super) fn cell_to_f64(cell: &CellValue) -> Result<f64, String> {
    match cell {
        CellValue::Number(n) => Ok(*n),
        CellValue::Text(s) => s
            .trim()
            .parse::<f64>()
            .map_err(|_| format!("reconcile sql returned non-numeric value '{s}'")),
        CellValue::Null => Err("reconcile sql returned NULL".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn period() -> [String; 2] {
        ["2026-07-01".to_string(), "2026-07-01".to_string()]
    }

    #[test]
    fn semantic_request_overwrites_time_dimension_window() {
        let mut base = QueryRequest::new();
        base.measures = vec!["sales.net".to_string()];
        base.segments = vec!["sales.dine_in".to_string()];
        let req = semantic_request(&base, "sales.business_date", &period());
        // User params preserved.
        assert_eq!(req.measures, vec!["sales.net".to_string()]);
        assert_eq!(req.segments, vec!["sales.dine_in".to_string()]);
        // Window injected on the named dimension.
        assert_eq!(req.time_dimensions.len(), 1);
        assert_eq!(req.time_dimensions[0].dimension, "sales.business_date");
        assert_eq!(req.time_dimensions[0].granularity, None);
        assert_eq!(
            req.time_dimensions[0].date_range,
            Some(vec!["2026-07-01".to_string(), "2026-07-01".to_string()])
        );
    }

    #[test]
    fn render_sql_binds_bare_dates() {
        let out = render_sql(
            "select v where d between '{{ start_date }}' and '{{ end_date }}'",
            &period(),
        )
        .unwrap();
        assert_eq!(out, "select v where d between '2026-07-01' and '2026-07-01'");
    }

    #[test]
    fn render_sql_reports_bad_template() {
        assert!(render_sql("{{ unclosed", &period()).is_err());
    }

    #[test]
    fn cell_to_f64_handles_number_text_null() {
        assert_eq!(cell_to_f64(&CellValue::Number(12.5)).unwrap(), 12.5);
        assert_eq!(cell_to_f64(&CellValue::Text(" 7 ".to_string())).unwrap(), 7.0);
        assert!(cell_to_f64(&CellValue::Text("abc".to_string())).is_err());
        assert!(cell_to_f64(&CellValue::Null).is_err());
    }
}
