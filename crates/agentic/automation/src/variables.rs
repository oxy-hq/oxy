//! Automation variable resolution.
//!
//! Combines an automation's *declared* variables (with optional `default:`
//! values) and any *runtime overrides* (from a parent's
//! `type: workflow` task's `variables:` block, or from a top-level run's
//! seed) into a single flat map that gets folded into the render
//! context. Templates then reference variables by name —
//! `{{ metric_label }}` — same as step results.
//!
//! ## Declaration shapes
//!
//! Two shapes are accepted, so automation authors can pick the one that
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

use crate::render::render_jinja_string;

/// Render a `type: workflow` task's `variables:` override map against the
/// parent's render context, before the map is handed to the child
/// automation as runtime overrides.
///
/// Each string value is treated as a Jinja template evaluated against
/// `render_context`, mirroring how `execute_sql` resolves its own
/// `variables:` block. Without this, a passthrough like
/// `variables: { month: "{{ month }}" }` reaches the child verbatim —
/// the child then "renders" `{{ month }}` against a context where
/// `month` is literally the string `"{{ month }}"`, so it never
/// substitutes and lands in SQL as `DATE '{{ month }}'`.
///
/// Non-string values (numbers, bools, arrays, objects) pass through
/// unchanged. `None` / non-object inputs are returned as-is.
pub fn render_override_variables(
    variables: Option<&Value>,
    render_context: &Value,
) -> Result<Option<Value>, String> {
    let Some(map) = variables.and_then(|v| v.as_object()) else {
        return Ok(variables.cloned());
    };
    let mut out: Map<String, Value> = Map::new();
    for (k, v) in map {
        let resolved = match v.as_str() {
            Some(s) => Value::String(
                render_jinja_string(s, render_context)
                    .map_err(|e| format!("render automation variable {k:?}: {e}"))?,
            ),
            None => v.clone(),
        };
        out.insert(k.clone(), resolved);
    }
    Ok(Some(Value::Object(out)))
}

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

/// Compute the effective variable map for an automation run.
///
/// `declared` is `AutomationConfig.variables` — the automation's own
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

    #[test]
    fn render_override_variables_resolves_string_templates() {
        // The monthly_report → portfolio_summary passthrough shape.
        let ctx = json!({ "month": "2026-04-01", "value": "Peppa Pig" });
        let vars = json!({ "month": "{{ month }}", "brand_rollup": "{{ value }}" });
        let out = render_override_variables(Some(&vars), &ctx).unwrap();
        assert_eq!(
            out,
            Some(json!({ "month": "2026-04-01", "brand_rollup": "Peppa Pig" }))
        );
    }

    #[test]
    fn render_override_variables_passes_through_non_strings_and_none() {
        let ctx = json!({});
        let vars = json!({ "limit": 4, "flag": true });
        assert_eq!(
            render_override_variables(Some(&vars), &ctx).unwrap(),
            Some(vars.clone())
        );
        assert_eq!(render_override_variables(None, &ctx).unwrap(), None);
    }
}
