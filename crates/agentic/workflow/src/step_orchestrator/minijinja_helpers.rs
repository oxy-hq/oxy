//! minijinja-compatible renderers and vote helpers for workflow steps.

use std::collections::HashMap;

use serde_json::{Value, json};

/// Convert a row-oriented step result `{columns: [...], rows: [[...]]}` to
/// a column-oriented JSON object `{col_name: [val, ...]}` for storage.
/// The actual minijinja-compatible wrapping happens in `build_minijinja_context`.
///
/// Also writes `__columns__: [...]` carrying the original column-declaration
/// order, so the minijinja wrapper can serve the legacy `.rows` / `.columns`
/// accessors with stable ordering — `serde_json::Map` is alphabetical without
/// the `preserve_order` feature, so reading map keys directly would shuffle
/// rows on every persist/reload.
pub(crate) fn to_column_oriented(value: &Value) -> Value {
    let Some(columns) = value.get("columns").and_then(|v| v.as_array()) else {
        return value.clone();
    };
    let Some(rows) = value.get("rows").and_then(|v| v.as_array()) else {
        return value.clone();
    };

    let col_names: Vec<String> = columns
        .iter()
        .filter_map(|c| c.as_str().map(String::from))
        .collect();

    let mut col_map = serde_json::Map::new();
    for (col_idx, col_name) in col_names.iter().enumerate() {
        let col_values: Vec<Value> = rows
            .iter()
            .filter_map(|row| row.as_array().and_then(|cells| cells.get(col_idx).cloned()))
            .collect();
        col_map.insert(col_name.clone(), Value::Array(col_values));
    }

    // Sidecar metadata. Both keys are `__`-prefixed sentinels that
    // `build_minijinja_context` strips before exposing the columns to
    // user templates.
    col_map.insert("__row_count__".to_string(), json!(rows.len()));
    col_map.insert("__columns__".to_string(), Value::Array(columns.clone()));

    Value::Object(col_map)
}

/// Build a minijinja context value from the render_context JSON.
///
/// Step results that have column arrays get wrapped in a `ColumnTable`
/// minijinja Object so that `{{ step | length }}` returns the row count
/// and `{{ step.col_name[i] }}` accesses column values.
pub(crate) fn build_minijinja_context(render_context: &Value) -> minijinja::Value {
    let Some(obj) = render_context.as_object() else {
        return minijinja::Value::from_serialize(render_context);
    };

    let mut ctx = std::collections::BTreeMap::new();
    for (key, value) in obj {
        if let Some(row_count) = value.get("__row_count__").and_then(|v| v.as_u64()) {
            // This is a column-oriented table result — wrap it.
            let mut columns = serde_json::Map::new();
            if let Some(inner) = value.as_object() {
                for (k, v) in inner {
                    // Strip sidecars. `__row_count__` and `__columns__`
                    // are internal metadata; everything else is a real
                    // data column.
                    if k != "__row_count__" && k != "__columns__" {
                        columns.insert(k.clone(), v.clone());
                    }
                }
            }
            // Recover declaration order from the sidecar, falling back
            // to the (alphabetical) map iter for legacy state rows
            // written before `__columns__` was persisted.
            let column_order: Vec<String> = value
                .get("__columns__")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|c| c.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_else(|| columns.keys().cloned().collect());
            ctx.insert(
                key.clone(),
                minijinja::Value::from_object(ColumnTable {
                    columns,
                    column_order,
                    row_count: row_count as usize,
                }),
            );
        } else {
            ctx.insert(key.clone(), minijinja::Value::from_serialize(value));
        }
    }

    minijinja::Value::from(ctx)
}

/// Column-oriented table wrapper for minijinja.
///
/// Provides column access via attribute lookup (`table.col_name[i]`) and
/// responds to `| length` with the row count (not column count).
///
/// Also exposes two **virtual** accessors for parity with the legacy
/// `oxy-workflow` row-oriented shape:
///
/// - `{{ step.rows }}` → `Vec<Vec<Value>>` reconstructed from the
///   columns in declaration order. Lets templates ported from the
///   legacy engine keep using `{{ step.rows[i][0] }}`.
/// - `{{ step.columns }}` → `Vec<String>` of column names in
///   declaration order.
///
/// A SQL result that genuinely has a column literally named `rows` or
/// `columns` keeps its real value — the real column always wins over
/// the virtual accessor, so backward compatibility is exact.
#[derive(Debug)]
struct ColumnTable {
    columns: serde_json::Map<String, Value>,
    column_order: Vec<String>,
    row_count: usize,
}

impl ColumnTable {
    /// Reconstruct the row-oriented view on demand. Each row is an
    /// array of cells in `column_order`, padded with `null` for any
    /// column shorter than `row_count` (defensive — connector results
    /// should be rectangular).
    fn rows_view(&self) -> Vec<Vec<Value>> {
        let cols: Vec<Option<&Vec<Value>>> = self
            .column_order
            .iter()
            .map(|name| self.columns.get(name).and_then(|v| v.as_array()))
            .collect();
        (0..self.row_count)
            .map(|i| {
                cols.iter()
                    .map(|col| col.and_then(|c| c.get(i).cloned()).unwrap_or(Value::Null))
                    .collect()
            })
            .collect()
    }
}

impl std::fmt::Display for ColumnTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "<table: {} cols, {} rows>",
            self.columns.len(),
            self.row_count
        )
    }
}

impl minijinja::value::Object for ColumnTable {
    fn get_value(self: &std::sync::Arc<Self>, key: &minijinja::Value) -> Option<minijinja::Value> {
        let key_str = key.as_str()?;
        // Real columns always win. A SQL result with an actual column
        // named `rows` / `columns` keeps that as its visible value,
        // preserving backward compatibility for any in-the-wild data.
        if let Some(col) = self.columns.get(key_str) {
            return Some(minijinja::Value::from_serialize(col));
        }
        // Virtual row-oriented accessors for legacy template parity.
        match key_str {
            "rows" => Some(minijinja::Value::from_serialize(self.rows_view())),
            "columns" => Some(minijinja::Value::from_serialize(&self.column_order)),
            _ => None,
        }
    }

    fn enumerate(self: &std::sync::Arc<Self>) -> minijinja::value::Enumerator {
        // Expose row indices as the enumeration so `| length` returns row count
        // and `{% for i in step %}` iterates row indices.
        minijinja::value::Enumerator::Seq(self.row_count)
    }
}

/// Majority-vote: pick the most frequently occurring answer by exact string equality.
/// Returns `(winning_answer, score)` where score = `count / total`.
pub(crate) fn majority_vote(answers: &[String]) -> (String, f64) {
    let mut vote_counts: HashMap<&str, usize> = HashMap::new();
    for a in answers {
        *vote_counts.entry(a.as_str()).or_insert(0) += 1;
    }
    let (best, best_count) = vote_counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .unwrap(); // safe: caller checks answers is non-empty
    let score = best_count as f64 / answers.len() as f64;
    (best.to_string(), score)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_result() -> Value {
        json!({
            "columns": ["month", "total_sales", "num_weeks"],
            "rows": [
                ["2010-02", 42000.0, 4],
                ["2010-03", 31500.0, 5],
            ],
        })
    }

    fn render(tmpl: &str, ctx: &Value) -> String {
        let env = crate::render::workflow_env();
        let parsed = env.template_from_str(tmpl).unwrap();
        let mjctx = build_minijinja_context(ctx);
        parsed.render(&mjctx).unwrap()
    }

    /// `to_column_oriented` now also stamps `__columns__` so reload-via-DB
    /// retains the original declaration order without needing
    /// `serde_json/preserve_order`.
    #[test]
    fn column_oriented_carries_order_sidecar() {
        let col = to_column_oriented(&sample_result());
        let order = col.get("__columns__").unwrap().as_array().unwrap();
        let names: Vec<&str> = order.iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(names, vec!["month", "total_sales", "num_weeks"]);
    }

    /// Existing `step | length` / `step.col[i]` access pattern still
    /// works — the sidecar additions are invisible to templates.
    #[test]
    fn column_access_still_works() {
        let ctx = json!({ "q": to_column_oriented(&sample_result()) });
        assert_eq!(render("{{ q | length }}", &ctx), "2");
        assert_eq!(render("{{ q.month[0] }}", &ctx), "2010-02");
        assert_eq!(render("{{ q.total_sales[1] }}", &ctx), "31500.0");
    }

    /// `.rows` virtual accessor reconstructs the row-oriented form in
    /// declaration order, so legacy `{{ step.rows[i][0] }}` templates
    /// keep working after the column-oriented switch.
    #[test]
    fn rows_virtual_accessor_returns_row_oriented_view() {
        let ctx = json!({ "q": to_column_oriented(&sample_result()) });
        assert_eq!(render("{{ q.rows[0][0] }}", &ctx), "2010-02");
        assert_eq!(render("{{ q.rows[0][1] }}", &ctx), "42000.0");
        assert_eq!(render("{{ q.rows[1][2] }}", &ctx), "5");
    }

    /// `.columns` virtual accessor returns the declaration-ordered
    /// column names.
    #[test]
    fn columns_virtual_accessor_returns_column_names() {
        let ctx = json!({ "q": to_column_oriented(&sample_result()) });
        assert_eq!(
            render("{{ q.columns | join(',') }}", &ctx),
            "month,total_sales,num_weeks"
        );
    }

    /// A SQL result with an actual column literally named `rows` keeps
    /// its real value visible — the real column shadows the virtual
    /// accessor so we don't break any in-the-wild data shape.
    #[test]
    fn real_columns_named_rows_or_columns_win() {
        let raw = json!({
            "columns": ["rows", "columns"],
            "rows": [["a", "b"], ["c", "d"]],
        });
        let ctx = json!({ "q": to_column_oriented(&raw) });
        // `q.rows` is the real column array, not the row-oriented view.
        assert_eq!(render("{{ q.rows[0] }}", &ctx), "a");
        assert_eq!(render("{{ q.rows[1] }}", &ctx), "c");
        // `q.columns` is the real column array, not the column names list.
        assert_eq!(render("{{ q.columns[0] }}", &ctx), "b");
    }

    /// Legacy state rows persisted before `__columns__` existed still
    /// render — column order falls back to the (alphabetical) map iter.
    #[test]
    fn legacy_without_columns_sidecar_falls_back_to_map_order() {
        let ctx = json!({
            "q": {
                "month": ["2010-02"],
                "total_sales": [42000.0],
                "__row_count__": 1,
            }
        });
        // Column access still works (lookup is by key, not order).
        assert_eq!(render("{{ q.month[0] }}", &ctx), "2010-02");
        // `.rows` produces something — order is alphabetical fallback.
        // Just check that it's not undefined.
        assert!(!render("{{ q.rows[0][0] }}", &ctx).is_empty());
    }
}
