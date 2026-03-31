//! Pure translation helpers: `AnalyticsIntent` → vendor-native query format.
//!
//! Both functions are **pure** — no I/O, no side effects. [`CubeEngine`] and
//! [`LookerEngine`] delegate their `translate()` implementations here.
//!
//! # Filter DSL
//!
//! `AnalyticsIntent::filters` uses a simple DSL: `"col op val"` where `op` is
//! one of `=`, `!=`, `>`, `>=`, `<`, `<=`, or `IS NULL`.
//!
//! ```text
//! "orders.status = 'completed'"  →  { "member": "orders.status", "operator": "equals", "values": ["completed"] }
//! "orders.amount > 100"          →  { "member": "orders.amount", "operator": "gt", "values": ["100"] }
//! "orders.deleted_at IS NULL"    →  { "member": "orders.deleted_at", "operator": "notSet", "values": [] }
//! ```

use serde_json::{Value, json};

use super::{EngineError, TranslationContext, VendorQuery};
use crate::types::AnalyticsIntent;

// ── Cube translation ──────────────────────────────────────────────────────────

/// Translate an intent into a Cube REST API `/v1/load` query payload.
///
/// Returns [`EngineError::TranslationFailed`] when:
/// - A metric in the intent is not found in `ctx.metrics`
/// - A filter expression cannot be parsed
pub fn cube_translate(
    ctx: &TranslationContext,
    intent: &AnalyticsIntent,
) -> Result<VendorQuery, EngineError> {
    // Build measures: "<view_name>.<metric_name>"
    let mut measures = Vec::new();
    for metric_name in &intent.metrics {
        let metric = ctx
            .metrics
            .iter()
            .find(|m| &m.name == metric_name)
            .ok_or_else(|| {
                EngineError::TranslationFailed(format!(
                    "metric '{metric_name}' not found in translation context"
                ))
            })?;
        // table field holds the view name in the semantic catalog
        let member = format!("{}.{}", metric.table, metric.name);
        measures.push(json!(member));
    }

    // Build dimensions: "<view_name>.<dim_name>"
    let mut dimensions = Vec::new();
    let mut time_dimensions = Vec::new();
    for dim_name in &intent.dimensions {
        // Find in ctx or fall back to treating as a bare column reference
        let (view_name, field_name, is_date) =
            if let Some(dim) = ctx.dimensions.iter().find(|d| &d.name == dim_name) {
                // Try to extract view prefix from name (e.g. "orders_view.status")
                if let Some((v, f)) = dim.name.split_once('.') {
                    (v.to_string(), f.to_string(), dim.data_type == "date")
                } else {
                    // Use first metric's table as default view
                    let view = ctx
                        .metrics
                        .first()
                        .map(|m| m.table.as_str())
                        .unwrap_or("unknown");
                    (view.to_string(), dim.name.clone(), dim.data_type == "date")
                }
            } else {
                // Bare dimension name — use first metric table
                let view = ctx
                    .metrics
                    .first()
                    .map(|m| m.table.as_str())
                    .unwrap_or("unknown");
                (view.to_string(), dim_name.clone(), false)
            };

        let member = format!("{view_name}.{field_name}");
        if is_date {
            time_dimensions.push(json!({
                "dimension": member,
                "granularity": "month"
            }));
        } else {
            dimensions.push(json!(member));
        }
    }

    // Build filters
    let filters = parse_cube_filters(&intent.filters)?;

    let mut payload = json!({
        "measures": measures,
        "dimensions": dimensions,
        "filters": filters,
        "limit": 10000
    });

    if !time_dimensions.is_empty() {
        payload["timeDimensions"] = json!(time_dimensions);
    }

    Ok(VendorQuery { payload })
}

/// Parse filter DSL strings into Cube filter objects.
fn parse_cube_filters(filters: &[String]) -> Result<Vec<Value>, EngineError> {
    let mut result = Vec::new();
    for filter in filters {
        let filter = filter.trim();

        // IS NULL check
        if let Some(col) = filter.strip_suffix(" IS NULL") {
            result.push(json!({
                "member": col.trim(),
                "operator": "notSet",
                "values": []
            }));
            continue;
        }
        if let Some(col) = filter.strip_suffix(" is null") {
            result.push(json!({
                "member": col.trim(),
                "operator": "notSet",
                "values": []
            }));
            continue;
        }

        // Try two-char operators first, then one-char
        let ops = [
            ("!=", "notEquals"),
            (">=", "gte"),
            ("<=", "lte"),
            ("=", "equals"),
            (">", "gt"),
            ("<", "lt"),
        ];
        let mut matched = false;
        for (op_str, cube_op) in ops {
            if let Some(pos) = filter.find(op_str) {
                let col = filter[..pos].trim();
                let val = filter[pos + op_str.len()..].trim().trim_matches('\'');
                result.push(json!({
                    "member": col,
                    "operator": cube_op,
                    "values": [val]
                }));
                matched = true;
                break;
            }
        }
        if !matched {
            return Err(EngineError::TranslationFailed(format!(
                "cannot parse filter expression: '{filter}'"
            )));
        }
    }
    Ok(result)
}

// ── Looker translation ────────────────────────────────────────────────────────

/// Translate an intent into a Looker inline query payload (`/api/4.0/queries/run/json`).
///
/// Returns [`EngineError::TranslationFailed`] when a metric is not in `ctx`.
pub fn looker_translate(
    ctx: &TranslationContext,
    intent: &AnalyticsIntent,
) -> Result<VendorQuery, EngineError> {
    // Determine model and primary view from the first metric
    let first_metric = ctx.metrics.first().ok_or_else(|| {
        EngineError::TranslationFailed("no metrics in intent for Looker translation".to_string())
    })?;

    // Validate all intent metrics exist in context
    for metric_name in &intent.metrics {
        if !ctx.metrics.iter().any(|m| &m.name == metric_name) {
            return Err(EngineError::TranslationFailed(format!(
                "metric '{metric_name}' not found in translation context"
            )));
        }
    }

    let model = first_metric.table.clone();
    let view = first_metric.table.clone();

    // Build fields: "<view>.<field>"
    let mut fields: Vec<String> = intent
        .metrics
        .iter()
        .map(|m| format!("{view}.{m}"))
        .collect();
    for dim_name in &intent.dimensions {
        fields.push(format!("{view}.{dim_name}"));
    }

    // Build filters: {"view.col": "value"}
    let filters = parse_looker_filters(&view, &intent.filters)?;

    // Build sorts
    let sorts: Vec<String> = intent
        .metrics
        .iter()
        .map(|m| format!("{view}.{m} desc"))
        .collect();

    let payload = json!({
        "model": model,
        "view": view,
        "fields": fields,
        "filters": filters,
        "sorts": sorts,
        "limit": "10000"
    });

    Ok(VendorQuery { payload })
}

/// Parse filter DSL strings into Looker filter map `{"view.col": "value"}`.
fn parse_looker_filters(view: &str, filters: &[String]) -> Result<Value, EngineError> {
    let mut map = serde_json::Map::new();
    for filter in filters {
        let filter = filter.trim();

        // IS NULL
        if let Some(col) = filter
            .strip_suffix(" IS NULL")
            .or_else(|| filter.strip_suffix(" is null"))
        {
            let col = col.trim();
            let key = if col.contains('.') {
                col.to_string()
            } else {
                format!("{view}.{col}")
            };
            map.insert(key, json!("NULL"));
            continue;
        }

        // = operator for Looker (simple equality)
        let ops = ["!=", ">=", "<=", "=", ">", "<"];
        let mut matched = false;
        for op_str in ops {
            if let Some(pos) = filter.find(op_str) {
                let col = filter[..pos].trim();
                let val = filter[pos + op_str.len()..].trim().trim_matches('\'');
                let key = if col.contains('.') {
                    col.to_string()
                } else {
                    format!("{view}.{col}")
                };
                // Looker uses the value directly for "=" and Looker filter expressions for others
                let looker_val = match op_str {
                    "=" => val.to_string(),
                    "!=" => format!("-{val}"),
                    ">" => format!(">{val}"),
                    ">=" => format!(">={val}"),
                    "<" => format!("<{val}"),
                    "<=" => format!("<={val}"),
                    _ => val.to_string(),
                };
                map.insert(key, json!(looker_val));
                matched = true;
                break;
            }
        }
        if !matched {
            return Err(EngineError::TranslationFailed(format!(
                "cannot parse filter expression for Looker: '{filter}'"
            )));
        }
    }
    Ok(Value::Object(map))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{DimensionSummary, JoinPath, MetricDef};
    use crate::engine::TranslationContext;
    use crate::types::{AnalyticsIntent, QuestionType};

    fn make_intent(metrics: &[&str], dimensions: &[&str], filters: &[&str]) -> AnalyticsIntent {
        AnalyticsIntent {
            raw_question: "test".into(),
            question_type: QuestionType::Breakdown,
            metrics: metrics.iter().map(|s| s.to_string()).collect(),
            dimensions: dimensions.iter().map(|s| s.to_string()).collect(),
            filters: filters.iter().map(|s| s.to_string()).collect(),
            history: vec![],
            spec_hint: None,
            selected_procedure: None,
        }
    }

    fn make_metric(name: &str, table: &str) -> MetricDef {
        MetricDef {
            name: name.to_string(),
            expr: name.to_string(),
            metric_type: "sum".to_string(),
            table: table.to_string(),
            description: None,
            data_source: None,
        }
    }

    fn make_dim(name: &str, data_type: &str) -> DimensionSummary {
        DimensionSummary {
            name: name.to_string(),
            description: String::new(),
            data_type: data_type.to_string(),
        }
    }

    fn make_ctx(metrics: Vec<MetricDef>, dimensions: Vec<DimensionSummary>) -> TranslationContext {
        TranslationContext {
            metrics,
            dimensions,
            join_paths: vec![],
        }
    }

    // A1: Cube simple breakdown
    #[test]
    fn cube_simple_breakdown() {
        let ctx = make_ctx(
            vec![make_metric("revenue", "orders_view")],
            vec![make_dim("region", "string")],
        );
        let intent = make_intent(&["revenue"], &["region"], &[]);
        let vq = cube_translate(&ctx, &intent).unwrap();
        let measures = vq.payload["measures"].as_array().unwrap();
        assert_eq!(measures[0], "orders_view.revenue");
        let dims = vq.payload["dimensions"].as_array().unwrap();
        assert_eq!(dims[0], "orders_view.region");
        assert!(
            vq.payload["timeDimensions"].is_null() || vq.payload.get("timeDimensions").is_none()
        );
    }

    // A2: Cube date dimension goes to timeDimensions
    #[test]
    fn cube_trend_date_dimension() {
        let ctx = make_ctx(
            vec![make_metric("revenue", "orders_view")],
            vec![make_dim("orders_view.order_date", "date")],
        );
        let intent = make_intent(&["revenue"], &["orders_view.order_date"], &[]);
        let vq = cube_translate(&ctx, &intent).unwrap();
        let time_dims = vq.payload["timeDimensions"].as_array().unwrap();
        assert_eq!(time_dims.len(), 1);
        assert_eq!(time_dims[0]["granularity"], "month");
        // Not in regular dimensions
        let dims = vq.payload["dimensions"].as_array().unwrap();
        assert!(dims.is_empty());
    }

    // A3: Cube filter operators
    #[test]
    fn cube_filter_operators() {
        let cases = [
            ("col = 'val'", "equals", "val"),
            ("col != 'val'", "notEquals", "val"),
            ("col > 100", "gt", "100"),
            ("col >= 100", "gte", "100"),
            ("col < 100", "lt", "100"),
            ("col <= 100", "lte", "100"),
        ];
        for (filter_str, expected_op, expected_val) in cases {
            let ctx = make_ctx(vec![make_metric("revenue", "orders_view")], vec![]);
            let intent = make_intent(&["revenue"], &[], &[filter_str]);
            let vq = cube_translate(&ctx, &intent).unwrap();
            let filters = vq.payload["filters"].as_array().unwrap();
            assert_eq!(filters[0]["operator"], expected_op, "filter: {filter_str}");
            assert_eq!(
                filters[0]["values"][0], expected_val,
                "filter: {filter_str}"
            );
        }
    }

    // A4: Cube IS NULL
    #[test]
    fn cube_filter_is_null() {
        let ctx = make_ctx(vec![make_metric("revenue", "orders_view")], vec![]);
        let intent = make_intent(&["revenue"], &[], &["orders.deleted_at IS NULL"]);
        let vq = cube_translate(&ctx, &intent).unwrap();
        let filters = vq.payload["filters"].as_array().unwrap();
        assert_eq!(filters[0]["operator"], "notSet");
        assert_eq!(filters[0]["values"].as_array().unwrap().len(), 0);
    }

    // A4 (unknown metric): Cube unknown metric → TranslationFailed
    #[test]
    fn cube_unknown_metric_returns_translation_failed() {
        let ctx = make_ctx(vec![make_metric("revenue", "orders_view")], vec![]);
        let intent = make_intent(&["unknown_metric"], &[], &[]);
        let err = cube_translate(&ctx, &intent).unwrap_err();
        assert!(matches!(err, EngineError::TranslationFailed(_)));
    }

    // A5: Looker basic fields
    #[test]
    fn looker_basic_fields() {
        let ctx = make_ctx(
            vec![make_metric("revenue", "orders")],
            vec![make_dim("status", "string")],
        );
        let intent = make_intent(&["revenue"], &["status"], &[]);
        let vq = looker_translate(&ctx, &intent).unwrap();
        assert_eq!(vq.payload["model"], "orders");
        assert_eq!(vq.payload["view"], "orders");
        let fields = vq.payload["fields"].as_array().unwrap();
        assert!(fields.contains(&json!("orders.revenue")));
        assert!(fields.contains(&json!("orders.status")));
    }

    // A6: Looker filter mapping
    #[test]
    fn looker_filter_mapping() {
        let ctx = make_ctx(vec![make_metric("revenue", "orders")], vec![]);
        let intent = make_intent(&["revenue"], &[], &["status = 'completed'"]);
        let vq = looker_translate(&ctx, &intent).unwrap();
        let filters = &vq.payload["filters"];
        assert_eq!(filters["orders.status"], "completed");
    }

    // A7: Looker unknown metric → TranslationFailed
    #[test]
    fn looker_unknown_metric_returns_translation_failed() {
        let ctx = make_ctx(vec![make_metric("revenue", "orders")], vec![]);
        let intent = make_intent(&["no_such_metric"], &[], &[]);
        let err = looker_translate(&ctx, &intent).unwrap_err();
        assert!(matches!(err, EngineError::TranslationFailed(_)));
    }
}
