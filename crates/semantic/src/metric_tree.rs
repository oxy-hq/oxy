//! Metric tree builder and pure analysis ops, wrapping airlayer.
//!
//! This module exposes the *pure* (database-free) metric tree operations:
//! building the tree, extracting subtrees, and the `sensitivity` / `predict`
//! graph ops. The query-executing ops (`explain`, `opportunity`) require a
//! query executor and are orchestrated by the HTTP layer in `oxy-app`.

use airlayer::SemanticLayer;
use airlayer::engine::EngineError;
use airlayer::engine::metric_tree::MetricTree;
use airlayer::engine::metric_tree_ops::{
    PredictResult, SensitivityResult, predict as al_predict, sensitivity as al_sensitivity,
};

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

#[cfg(test)]
mod tests {
    use super::*;
    use airlayer::SemanticLayer;

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
}
