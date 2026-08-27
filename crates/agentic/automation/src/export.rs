//! File export wrapper for automation steps.
//!
//! When a step has `export: { path, format }` configured, [`write_export`]
//! is called after the step's inner execution succeeds. The path is rendered
//! through the same Jinja context the step saw, then the result is written
//! to disk in the requested format.
//!
//! Format support per task type follows the legacy `oxy-workflow` rules:
//!
//! - `csv` / `json` — any tabular result (`execute_sql`, `semantic_query`,
//!   `omni_query`, `looker_query`).
//! - `sql` — only `execute_sql` and `semantic_query`; writes the executed
//!   SQL string. Requires the step result to carry a `"sql"` key (added by
//!   the inner executor).
//!
//! Errors during export are returned to the caller; they do *not* fall back
//! to "best effort" — silent export failure is the kind of bug that takes
//! months to notice.

use serde_json::Value;
use tokio::fs;

use crate::config::{CacheConfig, ExportFormat, TaskExport};
use crate::render::{render_jinja_string, validate_workspace_relative_path};
use crate::workspace::WorkspaceContext;

/// Write a step's result to disk according to its export config.
pub async fn write_export(
    workspace: &dyn WorkspaceContext,
    task_name: &str,
    export: &TaskExport,
    result: &Value,
    render_context: &Value,
) -> Result<(), String> {
    let rendered_path = render_jinja_string(&export.path, render_context).map_err(|e| {
        format!(
            "export[{task_name}]: failed to render path '{}': {e}",
            export.path
        )
    })?;

    // Containment: `export.path` is a write primitive — a rendered
    // path with `..` or an absolute prefix would write outside the
    // workspace (and `create_dir_all` on the parent would even
    // create traversal directories along the way). Block both
    // shapes here, even when the path looks innocuous in the YAML
    // but the substituted Jinja value is hostile.
    let root = workspace.workspace_path().ok_or_else(|| {
        format!(
            "export[{task_name}]: this node holds no workspace files, so there is nowhere to write"
        )
    })?;
    let abs_path = validate_workspace_relative_path(root, &rendered_path)
        .map_err(|e| format!("export[{task_name}]: {e}"))?;
    if let Some(parent) = abs_path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("export[{task_name}]: create {}: {e}", parent.display()))?;
    }

    let bytes: Vec<u8> = match export.format {
        ExportFormat::Csv => render_csv(result)
            .map_err(|e| format!("export[{task_name}]: csv: {e}"))?
            .into_bytes(),
        ExportFormat::Json => render_json(result)
            .map_err(|e| format!("export[{task_name}]: json: {e}"))?
            .into_bytes(),
        ExportFormat::Sql => render_sql(result)
            .map_err(|e| format!("export[{task_name}]: sql: {e}"))?
            .into_bytes(),
        ExportFormat::Txt => render_txt(result).into_bytes(),
        // `Docx` is retired — see the `ExportFormat` enum docstring in
        // `config.rs`. The match becomes exhaustive without it; any
        // `format: docx` in YAML now fails at parse time with a clean
        // serde error.
    };

    fs::write(&abs_path, bytes)
        .await
        .map_err(|e| format!("export[{task_name}]: write {}: {e}", abs_path.display()))?;
    Ok(())
}

/// Write a step's result to its `cache.path`. Separate from
/// [`write_export`] because cache files store the raw result for
/// later round-tripping (the next run reads the file back as the
/// step's result), so they always go through a single text-shaped
/// serialization rather than the per-format renderers.
///
/// - Strings are written verbatim (the common case — SQL text, LLM
///   answer text). This is what lets the user later edit the file by
///   hand and have it round-trip.
/// - Anything else is written as compact JSON so the next run's
///   `serde_json::from_str` reproduces the original structure.
///
/// Caller is expected to skip already-cache-hit steps (avoid clobber).
pub async fn write_cache(
    workspace: &dyn WorkspaceContext,
    task_name: &str,
    cache: &CacheConfig,
    result: &Value,
    render_context: &Value,
) -> Result<(), String> {
    let rendered_path = render_jinja_string(&cache.path, render_context).map_err(|e| {
        format!(
            "cache[{task_name}]: failed to render path '{}': {e}",
            cache.path
        )
    })?;
    let root = workspace.workspace_path().ok_or_else(|| {
        format!(
            "cache[{task_name}]: this node holds no workspace files, so there is nowhere to write"
        )
    })?;
    let abs_path = validate_workspace_relative_path(root, &rendered_path)
        .map_err(|e| format!("cache[{task_name}]: {e}"))?;
    if let Some(parent) = abs_path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("cache[{task_name}]: create {}: {e}", parent.display()))?;
    }
    let bytes = cache_bytes(result).map_err(|e| format!("cache[{task_name}]: serialize: {e}"))?;
    fs::write(&abs_path, bytes)
        .await
        .map_err(|e| format!("cache[{task_name}]: write {}: {e}", abs_path.display()))?;
    Ok(())
}

/// Serialize a step result for the file-presence cache.
///
/// - `Value::String` → raw bytes verbatim. The SQL-gen mode terminates
///   the analytics FSM with the SQL string as the answer, and that
///   path arrives here as a `Value::String` when a step bypasses the
///   `{"text": ...}` wrap.
/// - `Value::Object({"text": <string>})` with no other keys → raw text
///   bytes. This is the common case: `step_decider::decide` wraps
///   every agent answer this way so existing render-context templates
///   (`{{ step.text }}`) keep working. Cache files want the raw text
///   so a user can edit `results/cache/foo.sql` by hand; a JSON-escaped
///   blob with `\n` escapes is hostile to that workflow.
/// - Anything else → compact JSON. The next run's `serde_json::from_str`
///   reproduces the original structure for round-trip.
pub(crate) fn cache_bytes(result: &Value) -> Result<Vec<u8>, serde_json::Error> {
    match result {
        Value::String(s) => Ok(s.clone().into_bytes()),
        Value::Object(map)
            if map.len() == 1 && map.get("text").and_then(|v| v.as_str()).is_some() =>
        {
            Ok(map["text"].as_str().unwrap().as_bytes().to_vec())
        }
        other => Ok(serde_json::to_string(other)?.into_bytes()),
    }
}

/// Tabular result: `{"columns": [...], "rows": [[...], ...]}`.
fn render_csv(result: &Value) -> Result<String, String> {
    let columns = result
        .get("columns")
        .and_then(|v| v.as_array())
        .ok_or("csv: result is missing 'columns' array")?;
    let rows = result
        .get("rows")
        .and_then(|v| v.as_array())
        .ok_or("csv: result is missing 'rows' array")?;

    let mut out = String::new();
    let header: Vec<String> = columns
        .iter()
        .map(|c| csv_field(&value_to_string(c)))
        .collect();
    out.push_str(&header.join(","));
    out.push('\n');

    for row in rows {
        let cells = row.as_array().ok_or("csv: row must be an array")?;
        let line: Vec<String> = cells
            .iter()
            .map(|c| csv_field(&value_to_string(c)))
            .collect();
        out.push_str(&line.join(","));
        out.push('\n');
    }
    Ok(out)
}

/// JSON: write the result Value verbatim (pretty-printed for human inspection).
fn render_json(result: &Value) -> Result<String, String> {
    serde_json::to_string_pretty(result).map_err(|e| format!("serialize: {e}"))
}

/// SQL: write the `"sql"` field from the result. Inner executors that don't
/// produce SQL (omni, looker) won't have this key — return a clear error.
fn render_sql(result: &Value) -> Result<String, String> {
    result
        .get("sql")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "sql format only supported for execute_sql / semantic_query steps".into())
}

/// TXT: project the result onto a flat string.
///
/// `txt` is paired with agent / formatter outputs in the legacy schema —
/// those produce `{"text": "..."}` shapes. We honour that shape directly
/// and fall back to JSON for unrecognized payloads so the file always
/// has *something* readable rather than an empty bytes write.
fn render_txt(result: &Value) -> String {
    if let Some(text) = result.get("text").and_then(|v| v.as_str()) {
        return text.to_string();
    }
    if let Some(text) = result.as_str() {
        return text.to_string();
    }
    serde_json::to_string_pretty(result).unwrap_or_else(|_| result.to_string())
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Quote a CSV field per RFC 4180 — wrap in double quotes if it contains
/// `,`, `"`, `\n`, or `\r`; double up internal `"`.
fn csv_field(s: &str) -> String {
    let needs_quote = s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r');
    if needs_quote {
        let escaped = s.replace('"', "\"\"");
        format!("\"{escaped}\"")
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn csv_header_and_rows() {
        let result = json!({
            "columns": ["a", "b"],
            "rows": [[1, "x"], [2, "y"]],
        });
        let csv = render_csv(&result).unwrap();
        assert_eq!(csv, "a,b\n1,x\n2,y\n");
    }

    #[test]
    fn csv_quotes_special_chars() {
        let result = json!({
            "columns": ["name"],
            "rows": [["a, b"], ["c\"d"], ["e\nf"]],
        });
        let csv = render_csv(&result).unwrap();
        assert!(csv.contains("\"a, b\""));
        assert!(csv.contains("\"c\"\"d\""));
        assert!(csv.contains("\"e\nf\""));
    }

    #[test]
    fn csv_empty_for_null() {
        let result = json!({
            "columns": ["x"],
            "rows": [[null]],
        });
        let csv = render_csv(&result).unwrap();
        assert_eq!(csv, "x\n\n");
    }

    #[test]
    fn json_preserves_shape() {
        let result = json!({"columns": ["a"], "rows": [[1]]});
        let s = render_json(&result).unwrap();
        let round_trip: Value = serde_json::from_str(&s).unwrap();
        assert_eq!(round_trip, result);
    }

    #[test]
    fn sql_extracts_field() {
        let result = json!({"sql": "select 1", "rows": []});
        assert_eq!(render_sql(&result).unwrap(), "select 1");
    }

    #[test]
    fn sql_errors_when_missing() {
        let result = json!({"rows": []});
        assert!(render_sql(&result).is_err());
    }

    /// Cache regression: a `{"text": <sql>}` payload (the standard
    /// fold-step shape for agent answers) must be written as raw text
    /// so users can edit `results/cache/*.sql` by hand. Previously the
    /// whole object was JSON-serialized, producing
    /// `{"text":"SELECT\n..."}` with escaped newlines.
    #[test]
    fn cache_unwraps_text_envelope() {
        let result = json!({"text": "SELECT\n  COUNT(*)\nFROM oxymart"});
        let bytes = cache_bytes(&result).unwrap();
        assert_eq!(
            String::from_utf8(bytes).unwrap(),
            "SELECT\n  COUNT(*)\nFROM oxymart"
        );
    }

    /// A bare string (SQL-gen mode without the fold-step wrap) is also
    /// written verbatim.
    #[test]
    fn cache_writes_bare_string_verbatim() {
        let result = json!("SELECT 1");
        assert_eq!(cache_bytes(&result).unwrap(), b"SELECT 1".to_vec());
    }

    /// Multi-key objects keep the JSON round-trip behaviour so the next
    /// run's `from_str` reproduces structure.
    #[test]
    fn cache_keeps_json_for_multi_key_objects() {
        let result = json!({"text": "foo", "metadata": "bar"});
        let bytes = cache_bytes(&result).unwrap();
        let round_trip: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(round_trip, result);
    }

    /// `{"text": ...}` with a non-string value (e.g. a number) still
    /// goes through JSON — the unwrap is shape-strict.
    #[test]
    fn cache_only_unwraps_text_when_value_is_string() {
        let result = json!({"text": 42});
        let bytes = cache_bytes(&result).unwrap();
        let round_trip: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(round_trip, result);
    }
}
