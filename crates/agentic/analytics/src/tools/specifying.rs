//! Executor for **specifying**-state tools (`get_join_path`, `sample_columns`).

use agentic_connector::DatabaseConnector;
use agentic_core::result::CellValue;
use agentic_core::tools::ToolError;
use serde_json::{Value, json};

use crate::catalog::Catalog;

use super::{cell_to_json, emit_tool_error, emit_tool_input, emit_tool_output};

/// Execute a **specifying** tool.
///
/// `catalog` is used for `get_join_path` lookups.
/// `connector` is used by `sample_columns` to run live queries.
#[tracing::instrument(
    skip(catalog, connector),
    fields(oxy.name = "analytics.tool", oxy.span_type = "analytics", tool = %name)
)]
pub async fn execute_specifying_tool(
    name: &str,
    params: Value,
    catalog: &dyn Catalog,
    connector: &dyn DatabaseConnector,
) -> Result<Value, ToolError> {
    emit_tool_input(name, &params);
    let result = execute_specifying_tool_inner(name, params, catalog, connector).await;
    match &result {
        Ok(v) => emit_tool_output(v),
        Err(e) => emit_tool_error(e),
    }
    result
}

async fn execute_specifying_tool_inner(
    name: &str,
    params: Value,
    catalog: &dyn Catalog,
    connector: &dyn DatabaseConnector,
) -> Result<Value, ToolError> {
    match name {
        "get_join_path" => {
            let from = params["from_entity"]
                .as_str()
                .ok_or_else(|| ToolError::BadParams("missing 'from_entity'".into()))?;
            let to = params["to_entity"]
                .as_str()
                .ok_or_else(|| ToolError::BadParams("missing 'to_entity'".into()))?;
            match catalog.get_join_path(from, to) {
                Some(jp) => Ok(json!({ "path": jp.path, "join_type": jp.join_type })),
                None => {
                    let available = catalog.table_names().join(", ");
                    Err(ToolError::Execution(format!(
                        "no registered join path between '{from}' and '{to}'. \
                         You can still join these tables: use a bare column name if both tables \
                         share the same name (e.g. `Date`), or a \
                         `left_table.left_col = right_table.right_col` expression when the \
                         column names differ (e.g. `macro.Date = strength.workout_date`). \
                         Inspect both tables' columns to find the right key. \
                         Available tables: [{available}]."
                    )))
                }
            }
        }

        "sample_columns" => {
            let columns = params["columns"]
                .as_array()
                .ok_or_else(|| ToolError::BadParams("missing 'columns' array".into()))?;
            if columns.is_empty() {
                return Err(ToolError::BadParams(
                    "'columns' array must not be empty".into(),
                ));
            }

            // Collect validated column specs before spawning futures.
            let specs: Vec<(&str, &str, Option<&str>)> = columns
                .iter()
                .map(|entry| {
                    let table = entry["table"].as_str().ok_or_else(|| {
                        ToolError::BadParams("each column entry requires 'table'".into())
                    })?;
                    let column = entry["column"].as_str().ok_or_else(|| {
                        ToolError::BadParams("each column entry requires 'column'".into())
                    })?;
                    let search_term = entry
                        .get("search_term")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty());
                    Ok((table, column, search_term))
                })
                .collect::<Result<Vec<_>, ToolError>>()?;

            // Run all column samples concurrently.
            let futures: Vec<_> = specs
                .iter()
                .map(|(table, column, search_term)| {
                    sample_single_column(table, column, *search_term, catalog, connector)
                })
                .collect();
            let results = futures::future::join_all(futures).await;

            // Collect results — individual column errors become inline error objects
            // rather than failing the whole batch.
            let results_json: Vec<Value> = results
                .into_iter()
                .enumerate()
                .map(|(i, r)| match r {
                    Ok(v) => v,
                    Err(e) => json!({
                        "table": specs[i].0,
                        "column": specs[i].1,
                        "error": e.to_string()
                    }),
                })
                .collect();

            Ok(json!({ "results": results_json }))
        }

        _ => Err(ToolError::UnknownTool(name.into())),
    }
}

/// Sample a single column: resolve via catalog, query the database for distinct
/// values and statistics.  Used by `sample_columns` (batch) above.
async fn sample_single_column(
    table_param: &str,
    column_param: &str,
    search_term: Option<&str>,
    catalog: &dyn Catalog,
    connector: &dyn DatabaseConnector,
) -> Result<Value, ToolError> {
    // Resolve semantic view/dimension names to physical table/column.
    let resolved = catalog.resolve_sample_target(table_param, column_param);

    // If the semantic layer has static samples, return them directly.
    // When a search_term is provided, filter the static samples.
    if let Some(ref target) = resolved
        && !target.static_samples.is_empty()
    {
        let values: Vec<Value> = target
            .static_samples
            .iter()
            .filter(|s| {
                search_term
                    .map(|term| s.to_lowercase().contains(&term.to_lowercase()))
                    .unwrap_or(true)
            })
            .map(|s| Value::String(s.clone()))
            .collect();
        return Ok(json!({
            "column": column_param,
            "table": table_param,
            "data_type": target.data_type.as_deref().unwrap_or("string"),
            "sample_values": values,
            "source": "semantic_layer",
            "hint": "These are pre-defined sample values from the semantic layer definition."
        }));
    }

    // When the semantic layer resolves the target, `column_expr` is
    // already a SQL expression (e.g. `"Datetime (Local)"` with its own
    // quoting).  Use it verbatim instead of wrapping in extra quotes.
    let (table_sql, col_sql) = match &resolved {
        Some(target) => {
            let t = format!("\"{}\"", target.table.replace('"', "\"\""));
            // column_expr from the semantic layer is a raw SQL
            // expression — use it as-is.
            (t, target.column_expr.clone())
        }
        None => (
            format!("\"{}\"", table_param.replace('"', "\"\"")),
            format!("\"{}\"", column_param.replace('"', "\"\"")),
        ),
    };

    let sample_sql = if let Some(term) = search_term {
        // Escape single quotes in the search term to prevent SQL injection.
        let escaped = term.replace('\'', "''");
        format!(
            "SELECT DISTINCT {col_sql} FROM {table_sql} WHERE {col_sql} IS NOT NULL AND CAST({col_sql} AS TEXT) LIKE '%{escaped}%' LIMIT 20"
        )
    } else {
        format!("SELECT DISTINCT {col_sql} FROM {table_sql} WHERE {col_sql} IS NOT NULL LIMIT 20")
    };
    let count_sql = format!("SELECT COUNT(*) FROM {table_sql}");

    let sample_res = connector
        .execute_query(&sample_sql, 20)
        .await
        .map_err(|e| ToolError::Execution(e.to_string()))?;

    let values: Vec<Value> = sample_res
        .result
        .rows
        .iter()
        .filter_map(|row| row.0.first())
        .map(cell_to_json)
        .collect();

    let data_type = sample_res
        .summary
        .columns
        .first()
        .map(|c| c.name.clone())
        .unwrap_or_default();

    // Detect column kind from sample values.
    // A value looks like a date if it starts with four digits followed by '-'.
    let is_date_col = values.iter().any(|v| {
        v.as_str()
            .map(|s| {
                s.len() >= 5
                    && s[..4].chars().all(|c| c.is_ascii_digit())
                    && s.as_bytes()[4] == b'-'
            })
            .unwrap_or(false)
    });
    // Numeric: all sampled values are JSON numbers (not dates).
    let is_numeric_col = !is_date_col && !values.is_empty() && values.iter().all(|v| v.is_number());

    // Helper to extract a u64 from a cell.
    let cell_u64 = |cell: &CellValue| -> Option<u64> {
        match cell {
            CellValue::Number(n) => Some(*n as u64),
            CellValue::Text(s) => s.parse().ok(),
            CellValue::Null => None,
        }
    };
    // Helper to extract a f64 from a cell.
    let cell_f64 = |cell: &CellValue| -> Option<f64> {
        match cell {
            CellValue::Number(n) => Some(*n),
            CellValue::Text(s) => s.parse().ok(),
            CellValue::Null => None,
        }
    };
    // Helper to extract a string from a cell.
    let cell_str = |cell: &CellValue| -> Option<String> {
        match cell {
            CellValue::Text(s) => Some(s.clone()),
            CellValue::Number(n) => Some(n.to_string()),
            CellValue::Null => None,
        }
    };

    let base = json!({ "column": column_param, "table": table_param });

    if is_numeric_col {
        // Numeric columns: fetch count, distinct count, min, max, avg, stdev in one query.
        let stats_sql = format!(
            "SELECT COUNT(*), COUNT(DISTINCT {col_sql}), MIN({col_sql}), MAX({col_sql}), \
             AVG({col_sql}), STDDEV_POP({col_sql}) FROM {table_sql}"
        );
        if let Ok(stats_res) = connector.execute_query(&stats_sql, 1).await
            && let Some(row) = stats_res.result.rows.first()
        {
            let row_count = row.0.first().and_then(&cell_u64).unwrap_or(0);
            let distinct_count = row.0.get(1).and_then(&cell_u64);
            let min_val = row.0.get(2).and_then(&cell_f64);
            let max_val = row.0.get(3).and_then(&cell_f64);
            let avg_val = row.0.get(4).and_then(&cell_f64);
            let stdev_val = row.0.get(5).and_then(cell_f64);
            let mut r = base;
            r["data_type"] = json!(data_type);
            r["sample_values"] = json!(values);
            r["row_count"] = json!(row_count);
            r["distinct_count"] = json!(distinct_count);
            r["min"] = json!(min_val);
            r["max"] = json!(max_val);
            r["avg"] = json!(avg_val);
            r["stdev"] = json!(stdev_val);
            return Ok(r);
        }
    } else {
        // Date and text columns: fetch count, distinct count, min, max.
        let stats_sql = format!(
            "SELECT COUNT(*), COUNT(DISTINCT {col_sql}), MIN({col_sql}), MAX({col_sql}) \
             FROM {table_sql}"
        );
        if let Ok(stats_res) = connector.execute_query(&stats_sql, 1).await
            && let Some(row) = stats_res.result.rows.first()
        {
            let row_count = row.0.first().and_then(&cell_u64).unwrap_or(0);
            let distinct_count = row.0.get(1).and_then(cell_u64);
            let min_val = row.0.get(2).and_then(&cell_str);
            let max_val = row.0.get(3).and_then(cell_str);
            let mut r = base;
            r["data_type"] = json!(data_type);
            r["sample_values"] = json!(values);
            r["row_count"] = json!(row_count);
            r["distinct_count"] = json!(distinct_count);
            r["min"] = json!(min_val);
            r["max"] = json!(max_val);
            return Ok(r);
        }
    }

    // Fallback: just return sample values with basic count.
    let count_res = connector
        .execute_query(&count_sql, 1)
        .await
        .map_err(|e| ToolError::Execution(e.to_string()))?;
    let row_count: u64 = count_res
        .result
        .rows
        .first()
        .and_then(|row| row.0.first())
        .and_then(|cell| match cell {
            CellValue::Number(n) => Some(*n as u64),
            CellValue::Text(s) => s.parse().ok(),
            CellValue::Null => None,
        })
        .unwrap_or(0);
    let mut r = base;
    r["data_type"] = json!(data_type);
    r["sample_values"] = json!(values);
    r["row_count"] = json!(row_count);
    Ok(r)
}
