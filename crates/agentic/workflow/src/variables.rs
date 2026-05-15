//! Workflow variable resolution.
//!
//! Combines a workflow's *declared* variables (with optional `default:`
//! values) and any *runtime overrides* (from a parent's
//! `type: workflow` task's `variables:` block, or from a top-level run's
//! seed) into a single flat map that gets folded into the render
//! context. Templates then reference variables by name —
//! `{{ metric_label }}` — same as step results.
//!
//! ## Declaration shapes
//!
//! Two shapes are accepted, so workflow authors can pick the one that
//! reads best for their case:
//!
//! ```yaml
//! # Plain key→value (terse).
//! variables:
//!   metric_label: Total weekly sales
//!
//! # Declared with metadata (mirrors Argo/dbt-style).
//! variables:
//!   metric_label:
//!     default: Total weekly sales
//! ```
//!
//! For the declared-with-metadata shape, we extract the `default` key
//! as the effective value. Other metadata keys (`description`, `type`)
//! are accepted but ignored at render time.
//!
//! ## Override precedence
//!
//! Runtime overrides win unconditionally: if both are present for the
//! same key, the override replaces the declared default.

use serde_json::{Map, Value};

/// Resolve the effective value for a single variable declaration.
///
/// Returns the `default` field for the `{default: X, ...}` shape;
/// otherwise returns the declaration as-is.
fn declared_value(declaration: &Value) -> Value {
    if let Some(obj) = declaration.as_object()
        && let Some(d) = obj.get("default")
    {
        return d.clone();
    }
    declaration.clone()
}

/// Compute the effective variable map for a workflow run.
///
/// `declared` is `WorkflowConfig.variables` — the workflow's own
/// declarations (possibly `{default: X}` shape). `overrides` is the
/// runtime override map (parent's `type: workflow.variables`, or a
/// top-level run's seed variables). Both are `Option<Value>` because
/// either side may be absent.
///
/// Returns a flat `Value::Object` ready to merge into a render context.
/// Returns `Value::Object({})` when both sides are empty.
pub fn effective_variables(declared: Option<&Value>, overrides: Option<&Value>) -> Value {
    let mut out: Map<String, Value> = Map::new();
    if let Some(decl) = declared.and_then(|v| v.as_object()) {
        for (k, v) in decl {
            out.insert(k.clone(), declared_value(v));
        }
    }
    if let Some(over) = overrides.and_then(|v| v.as_object()) {
        for (k, v) in over {
            out.insert(k.clone(), v.clone());
        }
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn plain_values_pass_through() {
        let decl = json!({"a": "x", "n": 42});
        let v = effective_variables(Some(&decl), None);
        assert_eq!(v, json!({"a": "x", "n": 42}));
    }

    #[test]
    fn default_shape_is_unwrapped() {
        let decl = json!({
            "metric_label": { "default": "Total weekly sales" },
            "n": { "default": 7, "description": "ignored" },
        });
        let v = effective_variables(Some(&decl), None);
        assert_eq!(v, json!({ "metric_label": "Total weekly sales", "n": 7 }));
    }

    #[test]
    fn overrides_replace_defaults() {
        let decl = json!({ "metric_label": { "default": "Sales" } });
        let over = json!({ "metric_label": "Profit" });
        let v = effective_variables(Some(&decl), Some(&over));
        assert_eq!(v, json!({ "metric_label": "Profit" }));
    }

    #[test]
    fn overrides_without_declaration() {
        let over = json!({ "x": 1 });
        let v = effective_variables(None, Some(&over));
        assert_eq!(v, json!({ "x": 1 }));
    }

    #[test]
    fn empty_inputs_yield_empty_object() {
        assert_eq!(effective_variables(None, None), json!({}));
    }

    /// `{default: ...}` shape with non-default metadata keys still
    /// uses `default` as the value (other keys are ignored).
    #[test]
    fn metadata_keys_ignored() {
        let decl = json!({
            "x": { "default": 1, "type": "integer", "description": "count" }
        });
        let v = effective_variables(Some(&decl), None);
        assert_eq!(v, json!({ "x": 1 }));
    }

    /// An object without a `default` key is treated as the literal
    /// value of the variable.
    #[test]
    fn object_without_default_is_literal() {
        let decl = json!({ "config": { "host": "localhost", "port": 5432 } });
        let v = effective_variables(Some(&decl), None);
        assert_eq!(v, decl);
    }
}
