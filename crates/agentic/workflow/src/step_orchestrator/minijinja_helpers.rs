//! minijinja-compatible renderers and vote helpers for workflow steps.

use std::collections::HashMap;

use serde_json::{Value, json};

/// Convert a row-oriented step result `{columns: [...], rows: [[...]]}` to
/// a column-oriented JSON object `{col_name: [val, ...]}` for storage.
/// The actual minijinja-compatible wrapping happens in `build_minijinja_context`.
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

    // Store row count so we can use it for `| length` in templates.
    col_map.insert("__row_count__".to_string(), json!(rows.len()));

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
                    if k != "__row_count__" {
                        columns.insert(k.clone(), v.clone());
                    }
                }
            }
            ctx.insert(
                key.clone(),
                minijinja::Value::from_object(ColumnTable {
                    columns,
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
#[derive(Debug)]
struct ColumnTable {
    columns: serde_json::Map<String, Value>,
    row_count: usize,
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
        let col = self.columns.get(key_str)?;
        Some(minijinja::Value::from_serialize(col))
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
