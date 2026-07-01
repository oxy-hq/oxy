//! Strict-typed `reconcile.yml`. Stored as JSONB in `reconcile_configs`;
//! the runtime round-trips it back with `serde_json::from_value`.

use serde::{Deserialize, Serialize};

use super::Tolerance;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconcileConfig {
    #[serde(default)]
    pub checks: Vec<ReconcileCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconcileCheck {
    pub name: String,
    /// Adapter registry key — the external system *kind*, e.g. "toast".
    pub source: String,
    /// Name of the `config.yml` integration that backs this check (e.g. a
    /// specific `toast` account, when a workspace declares more than one).
    /// Optional: when omitted, the first integration of the matching `source`
    /// type is used. The integration supplies the source's secret var-names and
    /// API base URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integration: Option<String>,
    /// Semantic measure to compute the Oxy-side number, e.g. "orders.net_sales".
    pub measure: String,
    /// The view/topic time dimension the window filters on, e.g. "orders.created_date".
    pub time_dimension: String,
    /// Dimension filters narrowing the Oxy measure, e.g. scope a check to one
    /// restaurant so it lines up with a per-restaurant external figure. Applied
    /// to the measure query by the runner.
    #[serde(default)]
    pub filters: Vec<MeasureFilterSpec>,
    pub window: Window,
    pub external: ExternalSpec,
    pub tolerance: Tolerance,
}

/// One equality/comparison filter on a semantic dimension, e.g.
/// `{ field: sales_daily.restaurant_id, op: eq, value: "<guid>" }`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeasureFilterSpec {
    /// Fully-qualified dimension, e.g. "sales_daily.restaurant_id".
    pub field: String,
    #[serde(default)]
    pub op: FilterOp,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterOp {
    #[default]
    Eq,
    Neq,
    Gt,
    Gte,
    Lt,
    Lte,
    In,
    NotIn,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Window {
    pub last: u32,
    pub grain: Grain,
    /// Number of grains to shift back, so the current incomplete period is
    /// excluded (offset: 1, grain: day == "yesterday").
    #[serde(default)]
    pub offset: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Grain {
    Day,
    Week,
    Month,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalSpec {
    /// Toast Analytics metric field to read from each report row, one of
    /// `netSalesAmount`, `grossSalesAmount`, `ordersCount`.
    pub metric: String,
    /// Which restaurant GUIDs to sum on the external side. Absent/empty sums
    /// EVERY restaurant in the report (all-restaurants aggregate); present sums
    /// only the listed GUIDs (per-restaurant). Mirror with `filters` on the Oxy
    /// side so both sides scope to the same restaurants.
    #[serde(default)]
    pub restaurants: Vec<String>,
}

pub fn parse_reconcile_config(v: &serde_json::Value) -> Result<ReconcileConfig, serde_json::Error> {
    serde_json::from_value(v.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_full_check() {
        let yaml = r#"
checks:
  - name: revenue_vs_toast
    source: toast
    measure: orders.net_sales
    time_dimension: orders.created_date
    window: { last: 1, grain: day, offset: 1 }
    external:
      metric: netSalesAmount
      restaurants: ["abc123"]
    tolerance: { abs: 1.0, pct: 0.5, combinator: and }
"#;
        let v: serde_json::Value = serde_yaml::from_str(yaml).unwrap();
        let cfg = parse_reconcile_config(&v).unwrap();
        assert_eq!(cfg.checks.len(), 1);
        let c = &cfg.checks[0];
        assert_eq!(c.name, "revenue_vs_toast");
        assert_eq!(c.source, "toast");
        assert_eq!(c.measure, "orders.net_sales");
        assert_eq!(c.window.offset, 1);
        assert_eq!(c.external.metric, "netSalesAmount");
        assert_eq!(c.external.restaurants, vec!["abc123".to_string()]);
        assert_eq!(c.tolerance.abs, 1.0);
    }

    #[test]
    fn parses_per_restaurant_filter() {
        let yaml = r#"
checks:
  - name: net_sales_store_a
    source: toast
    measure: sales_daily.total_net_sales
    time_dimension: sales_daily.business_date
    filters:
      - field: sales_daily.restaurant_id
        value: "guid-a"
    window: { last: 1, grain: day, offset: 1 }
    external:
      metric: netSalesAmount
      restaurants: ["guid-a"]
    tolerance: { abs: 1.0, pct: 0.5 }
"#;
        let v: serde_json::Value = serde_yaml::from_str(yaml).unwrap();
        let cfg = parse_reconcile_config(&v).unwrap();
        let c = &cfg.checks[0];
        assert_eq!(c.filters.len(), 1);
        assert_eq!(c.filters[0].field, "sales_daily.restaurant_id");
        assert_eq!(c.filters[0].op, FilterOp::Eq);
        assert_eq!(c.filters[0].value, "guid-a");
        assert_eq!(c.external.restaurants, vec!["guid-a".to_string()]);
    }

    #[test]
    fn parses_named_integration_and_defaults_to_none() {
        let yaml = r#"
checks:
  - name: with_name
    source: toast
    integration: toast_main
    measure: m.x
    time_dimension: m.d
    window: { last: 1, grain: day, offset: 1 }
    external: { metric: x }
    tolerance: { abs: 1.0, pct: 0.5 }
  - name: no_name
    source: toast
    measure: m.x
    time_dimension: m.d
    window: { last: 1, grain: day, offset: 1 }
    external: { metric: x }
    tolerance: { abs: 1.0, pct: 0.5 }
"#;
        let v: serde_json::Value = serde_yaml::from_str(yaml).unwrap();
        let cfg = parse_reconcile_config(&v).unwrap();
        assert_eq!(cfg.checks[0].integration.as_deref(), Some("toast_main"));
        assert_eq!(cfg.checks[1].integration, None);
    }

    #[test]
    fn filters_default_to_empty() {
        let yaml = r#"
checks:
  - name: c
    source: toast
    measure: m.x
    time_dimension: m.d
    window: { last: 1, grain: day, offset: 1 }
    external: { metric: x }
    tolerance: { abs: 1.0, pct: 0.5 }
"#;
        let v: serde_json::Value = serde_yaml::from_str(yaml).unwrap();
        let cfg = parse_reconcile_config(&v).unwrap();
        assert!(cfg.checks[0].filters.is_empty());
    }

    #[test]
    fn combinator_defaults_to_and() {
        let yaml = r#"
checks:
  - name: c
    source: toast
    measure: m.x
    time_dimension: m.d
    window: { last: 1, grain: day, offset: 1 }
    external: { metric: x }
    tolerance: { abs: 1.0, pct: 0.5 }
"#;
        let v: serde_json::Value = serde_yaml::from_str(yaml).unwrap();
        let cfg = parse_reconcile_config(&v).unwrap();
        assert!(matches!(
            cfg.checks[0].tolerance.combinator,
            super::super::compare::Combinator::And
        ));
    }
}
