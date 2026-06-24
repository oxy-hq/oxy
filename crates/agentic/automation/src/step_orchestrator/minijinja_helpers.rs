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
/// Each top-level entry is wrapped via [`wrap_context_value`] which
/// recursively decides between a [`ColumnTable`] (SQL results), a
/// [`TextShim`] (any other object — surfaces a string `.text` field as
/// the `{{ step }}` rendering, and recursively shims nested objects so
/// `{{ portfolio_summary.format_portfolio_section }}` likewise renders
/// just the text), and a passthrough `from_serialize` for primitives /
/// arrays.
pub(crate) fn build_minijinja_context(render_context: &Value) -> minijinja::Value {
    let Some(obj) = render_context.as_object() else {
        return minijinja::Value::from_serialize(render_context);
    };

    let mut ctx = std::collections::BTreeMap::new();
    for (key, value) in obj {
        ctx.insert(key.clone(), wrap_context_value(value));
    }

    minijinja::Value::from(ctx)
}

/// Wrap a single render-context value: column-table → [`ColumnTable`];
/// loop step output → [`LoopResult`]; any other object → [`TextShim`]
/// (shim activates for `.text` strings, otherwise the object falls
/// back to a JSON Display); primitives / arrays pass through unchanged
/// via `from_serialize`. Used both at the top level and recursively by
/// [`TextShim::get_value`] / [`LoopResult::get_value`], so nested
/// `{{ a.b.c }}` access surfaces the same shim semantics as top-level
/// access.
fn wrap_context_value(value: &Value) -> minijinja::Value {
    if let Some(table) = ColumnTable::from_value(value) {
        return minijinja::Value::from_object(table);
    }
    if let Some(loop_result) = LoopResult::from_value(value) {
        return minijinja::Value::from_object(loop_result);
    }
    if value.is_object() {
        return minijinja::Value::from_object(TextShim::from_value(value));
    }
    minijinja::Value::from_serialize(value)
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
    /// Build a `ColumnTable` from a render-context value that carries
    /// the `__row_count__` sidecar (the shape `to_column_oriented`
    /// emits for SQL step results). Returns `None` for any other
    /// shape so the caller can fall through to other wrappers.
    fn from_value(value: &Value) -> Option<Self> {
        let row_count = value.get("__row_count__").and_then(|v| v.as_u64())?;
        let inner = value.as_object()?;

        let mut columns = serde_json::Map::new();
        for (k, v) in inner {
            // Strip sidecars. `__row_count__` and `__columns__` are
            // internal metadata; everything else is a real data column.
            if k != "__row_count__" && k != "__columns__" {
                columns.insert(k.clone(), v.clone());
            }
        }
        // Recover declaration order from the sidecar, falling back to
        // the (alphabetical) map iter for legacy state rows written
        // before `__columns__` was persisted.
        let column_order: Vec<String> = value
            .get("__columns__")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|c| c.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_else(|| columns.keys().cloned().collect());

        Some(ColumnTable {
            columns,
            column_order,
            row_count: row_count as usize,
        })
    }

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

    /// Row-oriented view as `Vec<Map<col_name, cell>>` — the natural
    /// shape for templates that iterate over rows
    /// (`{% for row in table %}{{ row.col }}{% endfor %}`) and for the
    /// JSON list-of-objects rendering of `{{ table }}`.
    fn row_objects(&self) -> Vec<serde_json::Map<String, Value>> {
        let cols: Vec<(&String, Option<&Vec<Value>>)> = self
            .column_order
            .iter()
            .map(|name| (name, self.columns.get(name).and_then(|v| v.as_array())))
            .collect();
        (0..self.row_count)
            .map(|i| {
                let mut row = serde_json::Map::with_capacity(cols.len());
                for (name, col) in &cols {
                    let cell = col.and_then(|c| c.get(i).cloned()).unwrap_or(Value::Null);
                    row.insert((*name).clone(), cell);
                }
                row
            })
            .collect()
    }
}

impl std::fmt::Display for ColumnTable {
    /// Render as a compact JSON list-of-objects: the shape LLM-facing
    /// prompts expect when they do `Data: {{ step }}`, and the shape
    /// the user's `report_aggregator.agentic.yml` instructions already
    /// document as an example
    /// (`[{"yoy_views_perc": 14.3, "views": 1561132, ...}]`).
    ///
    /// Serialized by hand (vs `serde_json::to_string`) so columns
    /// appear in **declaration order** rather than the alphabetical
    /// order `serde_json::Map` falls back to without the
    /// `preserve_order` feature.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let cols: Vec<Option<&Vec<Value>>> = self
            .column_order
            .iter()
            .map(|name| self.columns.get(name).and_then(|v| v.as_array()))
            .collect();
        f.write_str("[")?;
        for row in 0..self.row_count {
            if row > 0 {
                f.write_str(",")?;
            }
            f.write_str("{")?;
            for (i, name) in self.column_order.iter().enumerate() {
                if i > 0 {
                    f.write_str(",")?;
                }
                let cell = cols[i]
                    .and_then(|c| c.get(row))
                    .cloned()
                    .unwrap_or(Value::Null);
                let key = serde_json::to_string(name).unwrap_or_else(|_| "\"\"".into());
                let val = serde_json::to_string(&cell).unwrap_or_else(|_| "null".into());
                write!(f, "{key}:{val}")?;
            }
            f.write_str("}")?;
        }
        f.write_str("]")
    }
}

impl minijinja::value::Object for ColumnTable {
    /// `Iterable` so the enumerator below yields row objects (not
    /// keys, as `Map` would); attribute access via `get_value` works
    /// independently of `repr` so `{{ table.col_name[i] }}` still
    /// resolves.
    fn repr(self: &std::sync::Arc<Self>) -> minijinja::value::ObjectRepr {
        minijinja::value::ObjectRepr::Iterable
    }

    /// Default `Iterable` render is `debug_list`; route through the
    /// JSON-list-of-objects `Display` instead.
    fn render(self: &std::sync::Arc<Self>, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self.as_ref(), f)
    }

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
        // Iterate row objects, not indices. Templates can then write
        // `{% for row in table %}{{ row.col_name }}{% endfor %}` and
        // get the cell value (previously `row` was an integer index
        // and `row.col_name` resolved to undefined — the "full of
        // undefined" symptom).
        let rows: Vec<minijinja::Value> = self
            .row_objects()
            .into_iter()
            .map(|row| minijinja::Value::from_serialize(Value::Object(row)))
            .collect();
        minijinja::value::Enumerator::Values(rows)
    }
}

/// Wraps a `loop_sequential` step's aggregated output so it iterates
/// as the per-iteration sub-workflow results.
///
/// The aggregator stores the loop output as an object of the shape
/// `{ "inline-0": {status, answer, index}, "inline-1": {...}, …,
///    "iterations": {<hash>: {value, index, status, answer}, …} }`,
/// where each `answer` is the JSON-serialized sub-workflow result.
/// Without this wrapper, `{% for it in loop_step %}` errored with
/// "plain object is not iterable" because `TextShim` (the
/// fall-through wrapper for objects) is `ObjectRepr::Plain`.
///
/// - `{% for it in loop_step %}{{ it.inner_task }}{% endfor %}`
///   yields the parsed sub-workflow results in iteration order, so
///   `it.inner_task` reaches the inner step's value (recursively
///   wrapped, so `.text` shimming still applies).
/// - `{{ loop_step }}` renders a compact JSON list of the parsed
///   iteration results — useful for `Data: {{ loop_step }}` prompts.
/// - `{{ loop_step | length }}` returns the iteration count.
/// - Legacy attribute access (`loop_step.iterations`,
///   `loop_step.inline-0`, …) is preserved unchanged.
#[derive(Debug)]
struct LoopResult {
    /// Parsed iteration results in inline-index order. Each entry is
    /// the JSON-deserialized `answer` of one iteration — i.e. the
    /// sub-workflow's result map (`{inner_task: …}`).
    iterations: Vec<Value>,
    /// Original aggregated object, kept so legacy keyed access
    /// (`loop_step.iterations`, `loop_step.inline-0`) still works.
    raw: serde_json::Map<String, Value>,
}

impl LoopResult {
    /// Detect a loop-aggregator object and parse iteration answers.
    /// Returns `None` for anything that doesn't look like a loop
    /// output so the caller can fall through to other wrappers.
    ///
    /// Two aggregator paths feed this shape and use different keys:
    /// the inline runner emits `inline-<n>` keys, while the queue
    /// coordinator emits child-task UUIDs. Detect by **entry shape**
    /// instead — every non-`iterations` value must look like a
    /// fan-out entry (`{status, index, answer|error}`). That marker
    /// fires for both paths and avoids over-triggering on plain user
    /// step results that happen to be named `iterations`.
    fn from_value(value: &Value) -> Option<Self> {
        let obj = value.as_object()?;
        let mut indexed: Vec<(usize, Value)> = Vec::new();
        for (k, v) in obj {
            if k == "iterations" {
                continue;
            }
            if !is_fanout_entry(v) {
                // Even one non-entry sibling means this isn't a
                // loop aggregator — fall through so user step results
                // keep their regular `TextShim` behaviour.
                return None;
            }
            // `is_fanout_entry` guarantees `index` exists as u64.
            let idx = v.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
            indexed.push((idx, v.clone()));
        }
        if indexed.is_empty() {
            return None;
        }
        // Order by the original loop iteration index (carried in
        // each entry) — not by the aggregator key, which is random
        // on the queue path and only happens to sort numerically on
        // the inline path.
        indexed.sort_by_key(|(n, _)| *n);

        // Each aggregated entry is `{status, answer, index}` where
        // `answer` is a JSON string of the iteration's sub-workflow
        // result. Parse it back into a Value so the template can
        // reach `it.inner_task`. If parsing fails (failed iteration
        // without a JSON body) keep the raw entry — the user can
        // still inspect `it.error` / `it.status`.
        let iterations: Vec<Value> = indexed
            .into_iter()
            .map(|(_, entry)| {
                entry
                    .get("answer")
                    .and_then(|a| a.as_str())
                    .and_then(|s| serde_json::from_str::<Value>(s).ok())
                    .unwrap_or(entry)
            })
            .collect();

        Some(LoopResult {
            iterations,
            raw: obj.clone(),
        })
    }
}

/// Does `value` look like a single fan-out aggregator entry — the
/// `{status, index, answer|error}` shape both the inline runner and
/// the queue coordinator emit?
fn is_fanout_entry(value: &Value) -> bool {
    let Some(obj) = value.as_object() else {
        return false;
    };
    let has_status = obj.get("status").and_then(|v| v.as_str()).is_some();
    let has_index = obj.get("index").and_then(|v| v.as_u64()).is_some();
    let has_payload = obj.get("answer").is_some() || obj.get("error").is_some();
    has_status && has_index && has_payload
}

impl std::fmt::Display for LoopResult {
    /// JSON list of parsed iteration results, matching the
    /// declaration-order/compact shape `ColumnTable` uses for
    /// LLM-facing `Data: {{ step }}` prompts. Falls back to the raw
    /// object representation if serialization somehow fails.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match serde_json::to_string(&self.iterations) {
            Ok(s) => f.write_str(&s),
            Err(_) => write!(f, "<loop: {} iterations>", self.iterations.len()),
        }
    }
}

impl minijinja::value::Object for LoopResult {
    fn repr(self: &std::sync::Arc<Self>) -> minijinja::value::ObjectRepr {
        minijinja::value::ObjectRepr::Iterable
    }

    fn render(self: &std::sync::Arc<Self>, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self.as_ref(), f)
    }

    fn get_value(self: &std::sync::Arc<Self>, key: &minijinja::Value) -> Option<minijinja::Value> {
        // Numeric index access (`loop_step[0]`) returns the parsed
        // iteration. String key access falls back to the raw
        // aggregated object so legacy `loop_step.iterations` /
        // `loop_step.inline-0` paths still resolve.
        if let Some(idx) = key.as_usize() {
            return self.iterations.get(idx).map(wrap_context_value);
        }
        let key_str = key.as_str()?;
        self.raw.get(key_str).map(wrap_context_value)
    }

    fn enumerate(self: &std::sync::Arc<Self>) -> minijinja::value::Enumerator {
        // One iteration per loop pass, recursively wrapped so
        // `{{ it.inner_task.text }}` and the like keep working.
        let values: Vec<minijinja::Value> =
            self.iterations.iter().map(wrap_context_value).collect();
        minijinja::value::Enumerator::Values(values)
    }
}

/// Wraps any object render-context value so:
///
/// - `{{ shim }}` writes the string `text` field directly when one is
///   present (agent / formatter / sub-workflow output shape:
///   `{ "text": "...", "metadata": {...}, "references": [...] }`),
///   or a compact JSON dump of the fields otherwise (so a generic
///   parent like `{{ portfolio_summary }}` still renders something
///   useful instead of `<object>`).
///
/// - `{{ shim.field }}` returns the named field, **recursively
///   wrapped** in another `TextShim` (or `ColumnTable`) so nested
///   access — `{{ portfolio_summary.format_portfolio_section }}` —
///   surfaces the same `.text` shimming as the top level.
///
/// Without the recursive wrap, accessing a nested object went through
/// `from_serialize` and dumped the full JSON `{"text": "..."}` into
/// the parent template.
#[derive(Debug)]
struct TextShim {
    fields: serde_json::Map<String, Value>,
}

impl TextShim {
    fn from_value(value: &Value) -> Self {
        TextShim {
            fields: value.as_object().cloned().unwrap_or_default(),
        }
    }
}

impl std::fmt::Display for TextShim {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(text) = self.fields.get("text").and_then(|v| v.as_str()) {
            f.write_str(text)
        } else {
            let s = serde_json::to_string(&self.fields).unwrap_or_default();
            f.write_str(&s)
        }
    }
}

impl minijinja::value::Object for TextShim {
    /// `Plain` so minijinja's default render path calls our [`render`]
    /// (instead of map-formatting the fields). Attribute access via
    /// [`get_value`] still works for `{{ shim.text }}` /
    /// `{{ shim.metadata }}`.
    fn repr(self: &std::sync::Arc<Self>) -> minijinja::value::ObjectRepr {
        minijinja::value::ObjectRepr::Plain
    }

    /// `{{ shim }}` writes the `text` payload (when present) or a JSON
    /// dump of the fields — see [`Display`].
    fn render(self: &std::sync::Arc<Self>, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self.as_ref(), f)
    }

    fn get_value(self: &std::sync::Arc<Self>, key: &minijinja::Value) -> Option<minijinja::Value> {
        let key_str = key.as_str()?;
        // Recurse through `wrap_context_value` so a nested object
        // surfaces the same `.text` shim semantics. A nested SQL
        // result (carries `__row_count__`) gets the ColumnTable
        // treatment; primitives / arrays fall through to
        // `from_serialize` unchanged.
        self.fields.get(key_str).map(wrap_context_value)
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
        let env = crate::render::automation_env();
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

    // ── TextShim ──────────────────────────────────────────────────────────────

    /// `{{ step }}` for an agent/formatter-shaped object (string `text`
    /// plus metadata) renders just the text, not the whole JSON.
    #[test]
    fn text_shim_renders_text_only() {
        let ctx = json!({
            "a": {
                "text": "Hello world",
                "metadata": { "agent": "demo" },
                "references": [1, 2, 3],
            }
        });
        assert_eq!(render("{{ a }}", &ctx), "Hello world");
    }

    /// Attribute access still works after the shim — all original
    /// fields are reachable.
    #[test]
    fn text_shim_preserves_attribute_access() {
        let ctx = json!({
            "a": { "text": "hi", "metadata": { "agent": "demo" } }
        });
        assert_eq!(render("{{ a.text }}", &ctx), "hi");
        assert_eq!(render("{{ a.metadata.agent }}", &ctx), "demo");
    }

    /// Objects that don't have a string `text` field are untouched.
    /// A SQL row with a column literally named `text` holding a list
    /// keeps its normal serialization rather than being mangled.
    #[test]
    fn text_shim_skips_non_string_text() {
        let ctx = json!({ "a": { "text": [1, 2], "n": 3 } });
        // No shim → `{{ a.n }}` still works; `{{ a }}` is the serialized object.
        assert_eq!(render("{{ a.n }}", &ctx), "3");
    }

    /// Single-field `{text: <string>}` (the shape every formatter
    /// step produces — `json!({ "text": rendered })`) must also be
    /// rendered as just the text by the shim.
    #[test]
    fn text_shim_renders_single_field_text_object() {
        let ctx = json!({ "a": { "text": "just text" } });
        assert_eq!(render("{{ a }}", &ctx), "just text");
    }

    /// A column-oriented SQL table whose payload happens to include a
    /// `text` column is wrapped as a table (column-table branch wins
    /// over the shim), preserving existing template access.
    #[test]
    fn text_shim_does_not_apply_to_column_tables() {
        let raw = json!({
            "columns": ["text", "n"],
            "rows": [["a", 1], ["b", 2]],
        });
        let ctx = json!({ "q": to_column_oriented(&raw) });
        assert_eq!(render("{{ q.text[0] }}", &ctx), "a");
        assert_eq!(render("{{ q | length }}", &ctx), "2");
    }

    // ── Recursive nested shim ─────────────────────────────────────────────────

    /// Nested access surfaces the same `.text` shim semantics as
    /// top-level access. This is the
    /// `{{ portfolio_summary.format_portfolio_section }}` case: the
    /// outer key is a generic sub-workflow result without a `text`
    /// field, but the inner step result IS `{ text: "..." }` and
    /// should render as just the text — not as `{"text": "..."}`.
    #[test]
    fn text_shim_wraps_nested_objects() {
        let ctx = json!({
            "portfolio_summary": {
                "format_portfolio_section": { "text": "## Hasbro K&F Portfolio\n\nbody", "metadata": {"a": 1} },
                "explain_portfolio_mom":    { "text": "MoM sentence." },
            }
        });
        assert_eq!(
            render("{{ portfolio_summary.format_portfolio_section }}", &ctx),
            "## Hasbro K&F Portfolio\n\nbody",
        );
        assert_eq!(
            render("{{ portfolio_summary.explain_portfolio_mom }}", &ctx),
            "MoM sentence.",
        );
        // Attribute access through nested shim still reaches deeper
        // fields untouched.
        assert_eq!(
            render(
                "{{ portfolio_summary.format_portfolio_section.metadata.a }}",
                &ctx
            ),
            "1",
        );
    }

    /// An object without a `text` field renders as a compact JSON dump
    /// — useful for `{{ obj }}` debugging without showing `<object>`.
    /// (The recursive `.field` access is the normal use; the JSON
    /// fallback is the safety net.)
    #[test]
    fn text_shim_no_text_falls_back_to_json() {
        let ctx = json!({ "a": { "k": 1, "v": "x" } });
        let out = render("{{ a }}", &ctx);
        assert!(out.contains("\"k\":1"), "got {out}");
        assert!(out.contains("\"v\":\"x\""), "got {out}");
    }

    // ── ColumnTable rendering & iteration ─────────────────────────────────────

    /// `{{ table }}` renders as a compact JSON list-of-objects — the
    /// shape LLM-facing prompts expect when dumping data. Matches the
    /// example the user's `report_aggregator.agentic.yml` documents.
    #[test]
    fn column_table_renders_as_json_list_of_objects() {
        let ctx = json!({ "q": to_column_oriented(&sample_result()) });
        let out = render("{{ q }}", &ctx);
        assert_eq!(
            out,
            r#"[{"month":"2010-02","total_sales":42000.0,"num_weeks":4},{"month":"2010-03","total_sales":31500.0,"num_weeks":5}]"#,
        );
    }

    /// `{% for row in table %}{{ row.col }}{% endfor %}` yields row
    /// objects, not indices, so `row.col` resolves to the cell value
    /// instead of "undefined". Previously the table enumerator was
    /// `Seq(N)` which made every iteration item an integer index,
    /// turning every `row.col` reference into an undefined chain.
    #[test]
    fn column_table_iteration_yields_row_objects() {
        let ctx = json!({ "q": to_column_oriented(&sample_result()) });
        assert_eq!(
            render(
                "{% for row in q %}{{ row.month }}={{ row.total_sales }};{% endfor %}",
                &ctx,
            ),
            "2010-02=42000.0;2010-03=31500.0;",
        );
    }

    // ── LoopResult ────────────────────────────────────────────────────────────

    /// The aggregated loop output shape (`{inline-0: {answer JSON}, ...,
    /// iterations: {...}}`) is now iterable as the per-iteration
    /// sub-workflow results. Mirrors the demo's
    /// `{% for brand_section in loop_brand_rollups %}{{ brand_section.format_brand_section }}{% endfor %}`
    /// pattern — without this, iteration errored with
    /// "plain object is not iterable" (TextShim is `Plain`).
    fn loop_step_value() -> Value {
        let it0 = json!({
            "brand_rollup_summary":  { "text": "Peppa summary." },
            "brand_top_channel":     { "text": "Peppa top channel." },
            "format_brand_section":  { "text": "### Peppa\n\nbody-0" },
        });
        let it1 = json!({
            "brand_rollup_summary":  { "text": "Bluey summary." },
            "brand_top_channel":     { "text": "Bluey top channel." },
            "format_brand_section":  { "text": "### Bluey\n\nbody-1" },
        });
        json!({
            "inline-0": {
                "status": "done",
                "index": 0,
                "answer": serde_json::to_string(&it0).unwrap(),
            },
            "inline-1": {
                "status": "done",
                "index": 1,
                "answer": serde_json::to_string(&it1).unwrap(),
            },
            "iterations": {
                "hash0": { "value": "Peppa", "index": 0, "status": "done", "answer": serde_json::to_string(&it0).unwrap() },
                "hash1": { "value": "Bluey", "index": 1, "status": "done", "answer": serde_json::to_string(&it1).unwrap() },
            }
        })
    }

    #[test]
    fn loop_result_iterates_as_per_iteration_results() {
        let ctx = json!({ "loop_brand_rollups": loop_step_value() });
        // `it.format_brand_section` reaches the inner step result and
        // the recursive TextShim renders just `.text`.
        let out = render(
            "{% for it in loop_brand_rollups %}{{ it.format_brand_section }}|{% endfor %}",
            &ctx,
        );
        assert_eq!(out, "### Peppa\n\nbody-0|### Bluey\n\nbody-1|");
    }

    #[test]
    fn loop_result_length_is_iteration_count() {
        let ctx = json!({ "loop_brand_rollups": loop_step_value() });
        assert_eq!(render("{{ loop_brand_rollups | length }}", &ctx), "2");
    }

    /// `{{ loop_step }}` renders a compact JSON list of the parsed
    /// per-iteration results (Display via render() override).
    #[test]
    fn loop_result_renders_as_json_list() {
        let ctx = json!({ "loop_brand_rollups": loop_step_value() });
        let out = render("{{ loop_brand_rollups }}", &ctx);
        assert!(out.starts_with("["), "got {out}");
        assert!(out.contains("\"format_brand_section\""), "got {out}");
    }

    /// Indexed access yields the same per-iteration shimmed value,
    /// and nested `.text` shimming still kicks in.
    #[test]
    fn loop_result_indexed_access_with_nested_shim() {
        let ctx = json!({ "loop_brand_rollups": loop_step_value() });
        assert_eq!(
            render("{{ loop_brand_rollups[0].format_brand_section }}", &ctx),
            "### Peppa\n\nbody-0",
        );
    }

    /// Legacy aggregator access (`loop_step.iterations`,
    /// `loop_step.inline-0`) keeps working through the raw map.
    #[test]
    fn loop_result_legacy_attribute_access_preserved() {
        let ctx = json!({ "loop_brand_rollups": loop_step_value() });
        assert_eq!(
            render("{{ loop_brand_rollups.iterations.hash0.value }}", &ctx),
            "Peppa",
        );
        assert_eq!(
            render("{{ loop_brand_rollups.iterations.hash1.status }}", &ctx),
            "done",
        );
    }

    /// Regression: the queue/coordinator fan-out keys aggregated
    /// entries by child-task UUID (not `inline-<n>`), so an earlier
    /// inline-only detection silently let the coordinator path fall
    /// through to TextShim and trigger
    /// "plain object is not iterable". Detect by entry **shape** so
    /// both fan-out paths work.
    #[test]
    fn loop_result_handles_coordinator_uuid_keys() {
        // Mirror `aggregate_child_results` / `serialize_completed` from
        // the runtime: random child IDs as keys, ordering carried by
        // each entry's `index` field (NOT the key order).
        let it0 = json!({ "format_brand_section": { "text": "iter-0" } });
        let it1 = json!({ "format_brand_section": { "text": "iter-1" } });
        let ctx = json!({
            "loop_brand_rollups": {
                // Intentionally inserted out of iteration order to
                // confirm we sort by `index`, not by key.
                "b1c2d3-uuid-second": {
                    "status": "done", "index": 1,
                    "answer": serde_json::to_string(&it1).unwrap(),
                },
                "a0b1c2-uuid-first": {
                    "status": "done", "index": 0,
                    "answer": serde_json::to_string(&it0).unwrap(),
                },
            }
        });
        let out = render(
            "{% for it in loop_brand_rollups %}{{ it.format_brand_section }}|{% endfor %}",
            &ctx,
        );
        assert_eq!(out, "iter-0|iter-1|");
        assert_eq!(render("{{ loop_brand_rollups | length }}", &ctx), "2");
    }

    /// A failed iteration entry (no `answer`, has `error`) is still
    /// a valid fan-out entry; the per-iteration value falls back to
    /// the raw entry so `it.status` / `it.error` remain accessible.
    #[test]
    fn loop_result_passes_failed_iterations_through() {
        let ctx = json!({
            "lr": {
                "child-a": {
                    "status": "done", "index": 0,
                    "answer": serde_json::to_string(&json!({"x": {"text": "ok"}})).unwrap(),
                },
                "child-b": {
                    "status": "failed", "index": 1, "error": "boom",
                },
            }
        });
        // Iteration 0 surfaces its parsed inner step via shim;
        // iteration 1 (failed, no answer) is empty for `it.x`.
        assert_eq!(
            render("{% for it in lr %}{{ it.x }};{% endfor %}", &ctx),
            "ok;;"
        );
        // Iteration 1 fell back to the raw entry, so `it.status` and
        // `it.error` reach the wrapper-level fields. Iteration 0 was
        // parsed into its sub-workflow result (no `status` field), so
        // `it.status` is undefined there — the failed-iteration fields
        // surface asymmetrically, which is fine for "is this iteration
        // OK?" checks since the iteration is keyed by data presence.
        assert_eq!(
            render(
                "{% for it in lr %}{{ it.status }}={{ it.error }};{% endfor %}",
                &ctx
            ),
            "=;failed=boom;",
        );
    }

    /// A regular object that happens NOT to be a fan-out aggregator
    /// is NOT incorrectly detected as a LoopResult and keeps its `TextShim`
    /// behaviour (so `{{ portfolio_summary.format_…}}` shimming from
    /// the earlier change still works).
    #[test]
    fn loop_result_does_not_match_plain_objects() {
        let ctx = json!({
            "portfolio_summary": {
                "format_portfolio_section": { "text": "hi" },
                "iterations": "not a loop",
            }
        });
        assert_eq!(
            render("{{ portfolio_summary.format_portfolio_section }}", &ctx),
            "hi",
        );
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
