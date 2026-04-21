//! DuckDB schema introspection helpers and join-key detection.

use std::collections::HashMap;

use duckdb::Connection;

use agentic_core::result::CellValue;

use crate::connector::SchemaTableInfo;

pub(super) fn describe_table(
    conn: &Connection,
    table: &str,
) -> Result<Vec<(String, String)>, duckdb::Error> {
    let mut stmt = conn.prepare(&format!(r#"DESCRIBE "{table}""#))?;
    let cols = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(cols)
}

// ── DatabaseConnector impl ────────────────────────────────────────────────────

pub(super) fn parse_summarize_cell(s: &str, col_type: &str) -> Option<CellValue> {
    if s.is_empty() {
        return None;
    }
    let up = col_type.to_uppercase();
    let is_numeric = up.contains("INT")
        || up.contains("FLOAT")
        || up.contains("DOUBLE")
        || up.contains("DECIMAL")
        || up.contains("NUMERIC")
        || up.contains("REAL")
        || up.contains("HUGEINT");
    if is_numeric {
        s.parse::<f64>().ok().map(CellValue::Number)
    } else {
        Some(CellValue::Text(s.to_string()))
    }
}

/// Convert a `duckdb::types::Value` to an `Option<CellValue>`, returning

pub(super) fn detect_join_keys(tables: &[SchemaTableInfo]) -> Vec<(String, String, String)> {
    let mut col_to_tables: HashMap<&str, Vec<&str>> = HashMap::new();
    for t in tables {
        for c in &t.columns {
            if c.name.ends_with("_id") {
                col_to_tables
                    .entry(c.name.as_str())
                    .or_default()
                    .push(t.name.as_str());
            }
        }
    }
    let mut keys = Vec::new();
    for (col, tbs) in col_to_tables {
        for i in 0..tbs.len() {
            for j in (i + 1)..tbs.len() {
                keys.push((tbs[i].to_string(), tbs[j].to_string(), col.to_string()));
            }
        }
    }
    keys
}
