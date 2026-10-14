//! Metric tree builder and pure analysis ops, wrapping airlayer.
//!
//! This module exposes the *pure* (database-free) metric tree operations:
//! building the tree, extracting subtrees, and the `sensitivity` / `predict`
//! graph ops. The query-executing ops (`explain`, `opportunity`) require a
//! query executor and are orchestrated by the HTTP layer in `oxy-app`.

use oxy_airlayer_compat::engine::EngineError;
use oxy_airlayer_compat::engine::metric_tree::MetricTree;
use oxy_airlayer_compat::engine::metric_tree_ops::{
    MeasureValues, PredictResult, SensitivityResult, predict as al_predict,
    predict_with_values as al_predict_with_values, sensitivity as al_sensitivity,
};
use oxy_airlayer_compat::{Dialect, SemanticLayer};

pub fn build(layer: &SemanticLayer) -> MetricTree {
    MetricTree::build(layer)
}

/// The subtree rooted at `root_id`, or `None` if the measure is absent.
pub fn subtree(tree: &MetricTree, root_id: &str) -> Option<MetricTree> {
    tree.subtree(root_id)
}

/// Rank the drivers of `target` by influence (pure graph op).
pub fn sensitivity(tree: &MetricTree, target: &str) -> Result<SensitivityResult, EngineError> {
    al_sensitivity(tree, target)
}

/// Propagate hypothetical `(measure, delta)` changes upward (pure graph op).
pub fn predict(tree: &MetricTree, changes: &[(String, f64)]) -> Result<PredictResult, EngineError> {
    al_predict(tree, changes)
}

/// Propagate hypothetical `(measure, delta)` changes upward, using current
/// values so multiplicative edges can be sized instead of reported
/// `unquantifiable` (pure graph op).
///
/// `dialect` is the warehouse the values came from — resolve it with
/// [`oxy_airlayer_compat::predict_dialect`]. `None` means unknown and is the
/// conservative path: a ratio move integer division could change is refused
/// even on a warehouse that never integer-divides.
pub fn predict_with_values(
    tree: &MetricTree,
    changes: &[(String, f64)],
    values: &MeasureValues,
    dialect: Option<Dialect>,
) -> Result<PredictResult, EngineError> {
    al_predict_with_values(tree, changes, values, dialect)
}

/// Lever-conflict detection and the refusal built on it.
///
/// Re-exported rather than defined here: `agentic-analytics` needs the same
/// rule and must not depend on this crate (an agentic → platform edge —
/// `internal-docs/backend-architecture.md`), so the one definition lives in
/// `oxy-airlayer-compat`, which every caller already depends on. See that
/// module's doc for the history.
pub use oxy_airlayer_compat::lever_conflicts::{
    LeverConflict, lever_conflicts, reject_lever_conflicts,
};

#[cfg(test)]
mod tests {
    use super::*;
    use oxy_airlayer_compat::SemanticLayer;

    /// Build a SemanticLayer from inline view YAML via oxy-airlayer-compat.
    fn layer_from_views(yamls: &[&str]) -> SemanticLayer {
        let views: Vec<_> = yamls
            .iter()
            .map(|y| oxy_airlayer_compat::parse_view_yaml(y).unwrap())
            .collect();
        SemanticLayer::new(views, None)
    }

    #[test]
    fn build_produces_component_edges_from_expr_refs() {
        let layer = layer_from_views(&[r#"
name: orders
table: public.orders
dialect: postgres
measures:
  - name: revenue
    type: sum
    expr: amount
  - name: cost
    type: sum
    expr: cost
  - name: profit
    type: number
    expr: "{{orders.revenue}} - {{orders.cost}}"
"#]);
        let tree = build(&layer);
        assert!(tree.nodes.iter().any(|n| n.id == "orders.profit"));
        assert!(tree.edges.iter().any(|e| e.to == "orders.profit"));
    }

    #[test]
    fn sensitivity_errors_on_unknown_measure() {
        let layer = layer_from_views(&[r#"
name: orders
table: public.orders
dialect: postgres
measures:
  - name: revenue
    type: sum
    expr: amount
"#]);
        let tree = build(&layer);
        assert!(sensitivity(&tree, "orders.nonexistent").is_err());
    }

    #[test]
    fn predict_propagates_component_delta() {
        let layer = layer_from_views(&[r#"
name: orders
table: public.orders
dialect: postgres
measures:
  - name: revenue
    type: sum
    expr: amount
  - name: cost
    type: sum
    expr: cost
  - name: profit
    type: number
    expr: "{{orders.revenue}} - {{orders.cost}}"
"#]);
        let tree = build(&layer);
        let result = predict(&tree, &[("orders.revenue".to_string(), 100.0)]).unwrap();
        assert!(result.impacts.iter().any(|i| i.measure == "orders.profit"));
    }

    #[test]
    fn predict_with_values_sizes_a_multiplicative_edge() {
        let layer = layer_from_views(&[r#"
name: orders
table: public.orders
dialect: postgres
measures:
  - name: units
    type: sum
    expr: qty
  - name: unit_price
    type: number
    expr: price
  - name: revenue
    type: number
    expr: "{{orders.units}} * {{orders.unit_price}}"
"#]);
        let tree = build(&layer);
        let values = oxy_airlayer_compat::engine::metric_tree_ops::MeasureValues::from([
            ("orders.units".to_string(), 1000.0),
            ("orders.unit_price".to_string(), 4.0),
            ("orders.revenue".to_string(), 4000.0),
        ]);

        let with = predict_with_values(
            &tree,
            &[("orders.units".to_string(), 100.0)],
            &values,
            Some(Dialect::Postgres),
        )
        .unwrap();
        let revenue = with
            .impacts
            .iter()
            .find(|i| i.measure == "orders.revenue")
            .expect("revenue is impacted");
        assert_ne!(
            revenue.confidence, "unquantifiable",
            "supplying values must size the multiplicative edge"
        );
        assert!(revenue.estimated_delta.abs() > 0.0);

        // And without values it stays honestly unsized — the contract today.
        let without = predict(&tree, &[("orders.units".to_string(), 100.0)]).unwrap();
        let revenue_unsized = without
            .impacts
            .iter()
            .find(|i| i.measure == "orders.revenue")
            .expect("revenue is still reported, just unsized");
        assert_eq!(revenue_unsized.confidence, "unquantifiable");
        assert_eq!(revenue_unsized.estimated_delta, 0.0);
    }

    /// A `0/50` conversion rate, moved by ten conversions, read through the
    /// dialect the workspace's databases resolve to.
    fn conversion_move(
        db_type: &str,
    ) -> oxy_airlayer_compat::engine::metric_tree_ops::PredictImpact {
        let layer = layer_from_views(&[r#"
name: sales
table: main.sales
datasource: warehouse
measures:
  - name: converted
    type: count
    expr: order_id
  - name: visits
    type: count
    expr: visit_id
  - name: conversion
    type: number
    expr: "{{sales.converted}} / {{sales.visits}}"
"#]);
        let tree = build(&layer);
        let databases = [oxy_airlayer_compat::database_config("warehouse", db_type)];
        let changes = [("sales.converted".to_string(), 10.0)];
        let dialect =
            oxy_airlayer_compat::predict_dialect(&layer, &databases, &tree, &["sales.converted"]);
        let values = oxy_airlayer_compat::engine::metric_tree_ops::MeasureValues::from([
            ("sales.converted".to_string(), 0.0),
            ("sales.visits".to_string(), 50.0),
            ("sales.conversion".to_string(), 0.0),
        ]);
        predict_with_values(&tree, &changes, &values, dialect)
            .unwrap()
            .impacts
            .into_iter()
            .find(|i| i.measure == "sales.conversion")
            .expect("conversion is impacted")
    }

    #[test]
    fn a_ratio_move_on_duckdb_is_sized() {
        // DuckDB divides integers fractionally, so 0/50 → 10/50 is a clean
        // +0.2 and nothing about integer division can change it.
        let conversion = conversion_move("duckdb");
        assert_ne!(conversion.confidence, "unquantifiable", "{conversion:?}");
        assert!(
            (conversion.estimated_delta - 0.2).abs() < 1e-9,
            "{conversion:?}"
        );
    }

    #[test]
    fn a_ratio_move_on_postgres_is_refused() {
        // Postgres truncates `count / count`, and a level of 0 cannot rule
        // that out: 10/50 reads 0 there, not 0.2.
        let conversion = conversion_move("postgres");
        assert_eq!(conversion.confidence, "unquantifiable", "{conversion:?}");
    }
}
