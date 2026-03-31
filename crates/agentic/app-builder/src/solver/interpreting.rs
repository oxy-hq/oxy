//! **Interpreting** pipeline stage for the app builder domain.
//!
//! Resolves chart types from column shapes, generates data-driven insights
//! via LLM for `Insight` layout nodes, and assembles the final `.app.yml`
//! YAML string.

use std::collections::HashMap;
use std::sync::Arc;

use agentic_core::{
    back_target::BackTarget,
    orchestrator::{RunContext, SessionMemory, StateHandler, TransitionResult},
    state::ProblemState,
};

use crate::events::AppBuilderEvent;
use crate::types::{
    AppAnswer, AppBuilderDomain, AppBuilderError, AppResult, ChartPreference, ChartType,
    ControlType, LayoutNode, ResultShape, TaskResult,
};

use agentic_core::result::{CellValue, QueryRow};
use agentic_llm::LlmClient;

use super::prompts::{
    CHART_CONFIG_SYSTEM_PROMPT, INSIGHT_GENERATION_PROMPT, INTERPRETING_SYSTEM_PROMPT,
    PATCH_APP_RESULT_PROMPT,
};
use super::solver::AppBuilderSolver;

// ── Chart type inference ──────────────────────────────────────────────────────

const TIME_KEYWORDS: &[&str] = &[
    "date",
    "time",
    "month",
    "year",
    "week",
    "day",
    "hour",
    "minute",
    "created",
    "updated",
    "timestamp",
    "ts",
    "dt",
    "period",
    "quarter",
];

/// Detect whether a column name looks like a time/date column.
///
/// Uses word-boundary tokenization (split on `_`, `-`, whitespace) to avoid
/// false positives from substring matches like `"monday"` containing `"day"`.
fn looks_like_time_column_by_name(name: &str) -> bool {
    name.split(|c: char| c == '_' || c == '-' || c.is_whitespace())
        .any(|token| {
            let lower = token.to_lowercase();
            TIME_KEYWORDS.iter().any(|&kw| kw == lower)
        })
}

/// Return `true` if a text value looks like a common date/datetime string.
fn text_looks_like_date(s: &str) -> bool {
    let s = s.trim();
    if s.len() < 4 {
        return false;
    }
    let bytes = s.as_bytes();
    // YYYY-MM or YYYY-MM-DD (ISO 8601)
    if bytes.len() >= 7
        && bytes[..4].iter().all(|b| b.is_ascii_digit())
        && bytes[4] == b'-'
        && bytes[5..7].iter().all(|b| b.is_ascii_digit())
    {
        return true;
    }
    // Q1 / Q2 / Q3 / Q4 (optionally followed by a year, e.g. "Q1 2024")
    if bytes.len() >= 2 && bytes[0] == b'Q' && matches!(bytes[1], b'1'..=b'4') {
        return true;
    }
    // Month abbreviation prefix: "Jan 2024", "Feb", etc.
    let lower = s.to_lowercase();
    [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ]
    .iter()
    .any(|m| lower.starts_with(m))
}

/// Return `true` if a single cell value looks temporal.
fn cell_looks_temporal(v: &CellValue) -> bool {
    match v {
        CellValue::Text(s) => text_looks_like_date(s),
        // A Number that is a plausible calendar year (integer in 1800-2200).
        CellValue::Number(n) => {
            let i = *n as i64;
            (1800..=2200).contains(&i) && (*n - i as f64).abs() < 1e-9
        }
        CellValue::Null => false,
    }
}

/// Return `true` if a majority of non-null sample values in column `col_idx`
/// look temporal. Falls back to `false` when there are no non-null samples.
fn column_values_look_temporal(col_idx: usize, rows: &[QueryRow]) -> bool {
    let samples: Vec<&CellValue> = rows
        .iter()
        .filter_map(|r| r.0.get(col_idx))
        .filter(|v| !matches!(v, CellValue::Null))
        .take(5)
        .collect();
    if samples.is_empty() {
        return false;
    }
    let temporal_count = samples.iter().filter(|v| cell_looks_temporal(v)).count();
    temporal_count * 2 >= samples.len() // majority vote
}

/// Check whether column at position `col_idx` (with name `name`) looks
/// temporal, using both the column name and optional sample row data.
fn is_temporal_column(col_idx: usize, name: &str, rows: Option<&[QueryRow]>) -> bool {
    looks_like_time_column_by_name(name)
        || rows.is_some_and(|r| column_values_look_temporal(col_idx, r))
}

/// Infer [`ResultShape`] from actual execution columns and sample rows.
///
/// Used by solving/fanout to populate `expected_shape` after SQL is generated
/// and executed, so specifying does not need to predict it upfront.
pub(crate) fn infer_result_shape(columns: &[String], rows: &[QueryRow]) -> ResultShape {
    match columns.len() {
        0 | 1 => ResultShape::Scalar,
        _ => {
            if is_temporal_column(0, &columns[0], Some(rows)) {
                ResultShape::TimeSeries
            } else {
                ResultShape::Table { columns: vec![] }
            }
        }
    }
}

fn infer_from_columns(columns: &[String], rows: Option<&[QueryRow]>) -> ChartType {
    let has_time = columns
        .iter()
        .enumerate()
        .any(|(i, c)| is_temporal_column(i, c, rows));
    match (columns.len(), has_time) {
        (2, true) | (3, true) => ChartType::Line,
        (2, false) | (3, false) => ChartType::Bar,
        (1, _) => ChartType::Bar,
        _ => ChartType::Table,
    }
}

/// Determine the x, y, and optional series columns for a line/bar chart.
///
/// Strategy:
/// 1. If `expected` has exactly 2 entries that both appear (case-insensitive) in
///    `actual`, use those as the chart axes and treat any remaining actual column
///    as the series dimension.
///    - x = the time-like entry, else `expected[0]`
///    - y = the other entry
///    - series = actual column not in expected (if present)
/// 2. Otherwise fall back to position + time-column heuristics:
///    - x = first time-like column, else `actual[0]`
///    - y = last column
///    - series = middle column when there are exactly 3 columns
fn resolve_chart_columns(
    actual: &[String],
    expected: &[String],
    rows: Option<&[QueryRow]>,
) -> (String, String, Option<String>) {
    let actual_lower: Vec<String> = actual.iter().map(|c| c.to_lowercase()).collect();

    // Strategy 1: use expected_columns when they give us a clear 2-column signal.
    if expected.len() == 2 {
        let e0_lower = expected[0].to_lowercase();
        let e1_lower = expected[1].to_lowercase();
        let e0_in_actual = actual_lower.iter().any(|c| c == &e0_lower);
        let e1_in_actual = actual_lower.iter().any(|c| c == &e1_lower);

        if e0_in_actual && e1_in_actual {
            // Resolve to the canonical casing from actual.
            let e0_idx = actual
                .iter()
                .position(|c| c.to_lowercase() == e0_lower)
                .unwrap_or(0);
            let e1_idx = actual
                .iter()
                .position(|c| c.to_lowercase() == e1_lower)
                .unwrap_or(1);
            let e0 = actual[e0_idx].clone();
            let e1 = actual[e1_idx].clone();

            let (x, y) = if is_temporal_column(e0_idx, &e0, rows) {
                (e0, e1)
            } else if is_temporal_column(e1_idx, &e1, rows) {
                (e1, e0)
            } else {
                (e0, e1) // neither is time-like; preserve spec order
            };

            // Series = any actual column not matched by expected.
            let series = actual
                .iter()
                .find(|c| {
                    let cl = c.to_lowercase();
                    cl != e0_lower && cl != e1_lower
                })
                .cloned();

            return (x, y, series);
        }
    }

    // Strategy 2: positional fallback with time-column heuristic.
    if actual.is_empty() {
        return ("x".to_string(), "y".to_string(), None);
    }
    if actual.len() == 1 {
        return (actual[0].clone(), actual[0].clone(), None);
    }

    let x_idx = actual
        .iter()
        .enumerate()
        .position(|(i, c)| is_temporal_column(i, c, rows))
        .unwrap_or(0);
    let x = actual[x_idx].clone();
    let y = actual[actual.len() - 1].clone();
    let series = if actual.len() == 3 {
        // The column that is neither x nor y.
        actual.iter().find(|c| *c != &x && *c != &y).cloned()
    } else {
        None
    };

    (x, y, series)
}

/// Resolve a `ChartPreference` to a concrete `ChartType` given actual columns
/// and the optional expected shape from the spec.
pub(crate) fn resolve_chart_type(
    pref: &ChartPreference,
    columns: &[String],
    expected_shape: Option<&ResultShape>,
    rows: Option<&[QueryRow]>,
) -> ChartType {
    match pref {
        ChartPreference::Auto => {
            // Use expected_shape as a hint when available.
            if let Some(shape) = expected_shape {
                match shape {
                    ResultShape::TimeSeries => return ChartType::Line,
                    ResultShape::Scalar | ResultShape::Series => return ChartType::Bar,
                    ResultShape::Table { .. } => {} // fall through to column inference
                }
            }
            infer_from_columns(columns, rows)
        }
        ChartPreference::Bar => ChartType::Bar,
        ChartPreference::Line => ChartType::Line,
        ChartPreference::Pie => ChartType::Pie,
        ChartPreference::Table => ChartType::Table,
    }
}

// ── Insight resolution ───────────────────────────────────────────────────────

fn cell_to_string(cell: &CellValue) -> String {
    match cell {
        CellValue::Text(s) => s.clone(),
        CellValue::Number(n) => {
            if *n == (*n as i64) as f64 {
                format!("{}", *n as i64)
            } else {
                format!("{n:.2}")
            }
        }
        CellValue::Null => "NULL".into(),
    }
}

/// Format a `TaskResult`'s sample data as a readable text block for the LLM.
fn format_task_data_for_insight(task: &TaskResult) -> String {
    let columns = &task.sample.columns;
    let rows = &task.sample.rows;
    let total = task.row_count;
    let sample_count = rows.len();

    let row_context = if sample_count < total {
        format!("{total} rows total, showing {sample_count}")
    } else {
        format!("{total} rows")
    };

    if columns.len() >= 2 && sample_count >= 1 {
        let header = format!("| {} |", columns.join(" | "));
        let sep = format!(
            "| {} |",
            columns
                .iter()
                .map(|_| "---")
                .collect::<Vec<_>>()
                .join(" | ")
        );
        let body: Vec<String> = rows
            .iter()
            .take(20) // Cap at 20 rows to keep prompt concise
            .map(|row| {
                let cells: Vec<String> = row.0.iter().map(cell_to_string).collect();
                format!("| {} |", cells.join(" | "))
            })
            .collect();
        let truncation_note = if sample_count > 20 {
            format!("\n... ({} more rows omitted)", sample_count - 20)
        } else {
            String::new()
        };
        format!(
            "**{}** ({row_context}):\n{header}\n{sep}\n{}{}",
            task.name,
            body.join("\n"),
            truncation_note
        )
    } else {
        let flat: Vec<String> = rows
            .iter()
            .take(20)
            .map(|r| {
                r.0.iter()
                    .map(cell_to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .collect();
        format!(
            "**{}** ({row_context}): columns=[{}], values: {}",
            task.name,
            columns.join(", "),
            flat.join("; ")
        )
    }
}

/// Resolve a single `Insight` node into markdown content via LLM.
async fn resolve_insight_node(
    client: &LlmClient,
    system_prompt: &str,
    tasks: &[String],
    focus: &Option<String>,
    task_results: &[TaskResult],
) -> String {
    let data_blocks: Vec<String> = tasks
        .iter()
        .filter_map(|name| task_results.iter().find(|r| &r.name == name))
        .map(format_task_data_for_insight)
        .collect();

    if data_blocks.is_empty() {
        return "No data available for insight.".to_string();
    }

    let focus_hint = focus
        .as_deref()
        .map(|f| format!("\nFocus: {f}"))
        .unwrap_or_default();

    let user_prompt = format!(
        "Generate a data-driven insight from the following query results:{focus_hint}\n\n{}",
        data_blocks.join("\n\n")
    );

    client
        .complete(system_prompt, &user_prompt)
        .await
        .unwrap_or_else(|_| "Key metrics are shown in the charts below.".to_string())
}

/// Walk the layout tree, replacing `Insight` nodes with `Markdown` nodes
/// whose content is generated by the LLM from actual task data.
fn resolve_layout_insights<'a>(
    nodes: Vec<LayoutNode>,
    client: &'a LlmClient,
    system_prompt: &'a str,
    task_results: &'a [TaskResult],
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Vec<LayoutNode>> + Send + 'a>> {
    Box::pin(async move {
        let mut resolved = Vec::with_capacity(nodes.len());
        for node in nodes {
            match node {
                LayoutNode::Insight { tasks, focus } => {
                    let content =
                        resolve_insight_node(client, system_prompt, &tasks, &focus, task_results)
                            .await;
                    resolved.push(LayoutNode::Markdown { content });
                }
                LayoutNode::Row { columns, children } => {
                    let resolved_children =
                        resolve_layout_insights(children, client, system_prompt, task_results)
                            .await;
                    resolved.push(LayoutNode::Row {
                        columns,
                        children: resolved_children,
                    });
                }
                other => resolved.push(other),
            }
        }
        resolved
    })
}

// ── LLM-based chart configuration ────────────────────────────────────────────

/// Resolved chart configuration from LLM.
#[derive(Debug, Clone, serde::Deserialize)]
struct ResolvedChartConfig {
    #[allow(dead_code)]
    task: String,
    chart_type: String,
    x: Option<String>,
    y: Option<String>,
    series: Option<String>,
    name: Option<String>,
    value: Option<String>,
}

/// Collect all `LayoutNode::Chart` entries from a layout tree (including nested rows).
fn collect_chart_tasks(nodes: &[LayoutNode]) -> Vec<(&str, &ChartPreference)> {
    let mut charts = Vec::new();
    for node in nodes {
        match node {
            LayoutNode::Chart { task, preferred } => {
                charts.push((task.as_str(), preferred));
            }
            LayoutNode::Row { children, .. } => {
                charts.extend(collect_chart_tasks(children));
            }
            _ => {}
        }
    }
    charts
}

/// Format a single chart task's data block for the LLM prompt.
fn format_chart_task_block(task: &TaskResult, preferred: &ChartPreference) -> String {
    let mut block = format!("### Task: {}\n", task.name);

    // Column names with types.
    block.push_str("Columns:\n");
    for (i, col) in task.columns.iter().enumerate() {
        let type_str = task
            .column_types
            .get(i)
            .and_then(|t| t.as_deref())
            .unwrap_or("unknown");
        block.push_str(&format!("- {col} ({type_str})\n"));
    }

    // Chart preference.
    let pref_str = match preferred {
        ChartPreference::Auto => "Auto",
        ChartPreference::Bar => "bar_chart",
        ChartPreference::Line => "line_chart",
        ChartPreference::Pie => "pie_chart",
        ChartPreference::Table => "table",
    };
    block.push_str(&format!("Preference: {pref_str}\n"));

    // Sample data (up to 10 rows).
    if !task.sample.rows.is_empty() && task.columns.len() >= 2 {
        block.push_str(&format!("Sample data ({} rows total):\n", task.row_count));
        block.push_str(&format!("| {} |\n", task.columns.join(" | ")));
        block.push_str(&format!(
            "| {} |\n",
            task.columns
                .iter()
                .map(|_| "---")
                .collect::<Vec<_>>()
                .join(" | ")
        ));
        for row in task.sample.rows.iter().take(10) {
            let cells: Vec<String> = row.0.iter().map(cell_to_string).collect();
            block.push_str(&format!("| {} |\n", cells.join(" | ")));
        }
    }

    block
}

/// Resolve chart configurations for all chart nodes via a single batch LLM call.
///
/// Returns a map from task name to resolved config. Falls back to heuristic-based
/// resolution if the LLM call fails or returns unparsable JSON.
async fn resolve_all_chart_configs(
    client: &LlmClient,
    system_prompt: &str,
    layout: &[LayoutNode],
    task_results: &[TaskResult],
) -> HashMap<String, ResolvedChartConfig> {
    let chart_tasks = collect_chart_tasks(layout);
    if chart_tasks.is_empty() {
        return HashMap::new();
    }

    // Build a single user prompt with all chart tasks.
    let mut blocks = Vec::new();
    for (task_name, preferred) in &chart_tasks {
        if let Some(task) = task_results.iter().find(|r| r.name == *task_name) {
            blocks.push(format_chart_task_block(task, preferred));
        }
    }

    if blocks.is_empty() {
        return HashMap::new();
    }

    let user_prompt = format!(
        "Configure the chart visualization for the following {} task(s):\n\n{}",
        blocks.len(),
        blocks.join("\n")
    );

    // Call LLM and parse response.
    match client.complete(system_prompt, &user_prompt).await {
        Ok(raw) => {
            let json_str = crate::solver::strip_json_fences(&raw);
            match serde_json::from_str::<Vec<ResolvedChartConfig>>(json_str) {
                Ok(configs) => {
                    let mut map = HashMap::new();
                    for cfg in configs {
                        map.insert(cfg.task.clone(), cfg);
                    }
                    map
                }
                Err(e) => {
                    tracing::warn!("LLM chart config returned unparsable JSON: {e}");
                    HashMap::new() // fall back to heuristic in render_layout_node
                }
            }
        }
        Err(e) => {
            tracing::warn!("LLM chart config call failed: {e}");
            HashMap::new() // fall back to heuristic in render_layout_node
        }
    }
}

// ── YAML helpers ─────────────────────────────────────────────────────────────

/// Returns true when a YAML scalar value needs double-quoting.
/// Covers common YAML-special patterns: colons, leading/trailing spaces,
/// reserved words (true/false/null/yes/no), etc.
fn needs_yaml_quoting(value: &str) -> bool {
    if value.is_empty() {
        return true;
    }
    // Reserved YAML literals that would be misparsed as booleans/null.
    let reserved = ["true", "false", "null", "yes", "no", "on", "off", "~"];
    if reserved.iter().any(|r| r.eq_ignore_ascii_case(value)) {
        return true;
    }
    // Contains characters that require quoting in YAML.
    value.contains(':')
        || value.contains('#')
        || value.contains('{')
        || value.contains('}')
        || value.contains('[')
        || value.contains(']')
        || value.contains(',')
        || value.contains('&')
        || value.contains('*')
        || value.contains('?')
        || value.contains('|')
        || value.contains('>')
        || value.contains('!')
        || value.contains('%')
        || value.contains('`')
        || value.contains('"')
        || value.contains('\'')
        || value.starts_with(' ')
        || value.ends_with(' ')
        || value.starts_with('@')
}

/// Format a YAML scalar value, quoting only when necessary.
fn yaml_scalar(value: &str) -> String {
    if needs_yaml_quoting(value) {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        value.to_string()
    }
}

// ── YAML assembly ─────────────────────────────────────────────────────────────

/// Render a single `LayoutNode` recursively into YAML lines at `indent` level.
fn render_layout_node(
    node: &LayoutNode,
    task_results: &[TaskResult],
    chart_configs: &HashMap<String, ResolvedChartConfig>,
    indent: usize,
) -> Result<Vec<String>, AppBuilderError> {
    let pad = "  ".repeat(indent);
    let pad1 = "  ".repeat(indent + 1);
    let pad2 = "  ".repeat(indent + 2);

    match node {
        LayoutNode::Chart { task, preferred } => {
            let result = task_results.iter().find(|r| &r.name == task);
            let columns = result.map(|r| r.columns.as_slice()).unwrap_or(&[]);

            // Try LLM-resolved config first, fall back to heuristic.
            if let Some(cfg) = chart_configs.get(task.as_str()) {
                let type_str = cfg.chart_type.as_str();
                let mut lines = vec![
                    format!("{pad}- type: {type_str}"),
                    format!("{pad1}data: {task}"),
                ];
                match type_str {
                    "line_chart" | "bar_chart" => {
                        if let Some(x) = &cfg.x {
                            lines.push(format!("{pad1}x: {}", yaml_scalar(x)));
                        }
                        if let Some(y) = &cfg.y {
                            lines.push(format!("{pad1}y: {}", yaml_scalar(y)));
                        }
                        if let Some(s) = &cfg.series {
                            lines.push(format!("{pad1}series: {}", yaml_scalar(s)));
                        }
                    }
                    "pie_chart" => {
                        if let Some(n) = &cfg.name {
                            lines.push(format!("{pad1}name: {}", yaml_scalar(n)));
                        }
                        if let Some(v) = &cfg.value {
                            lines.push(format!("{pad1}value: {}", yaml_scalar(v)));
                        }
                    }
                    _ => {} // "table" — no axis config needed
                }
                return Ok(lines);
            }

            // Heuristic fallback.
            let expected_columns = result.map(|r| r.expected_columns.as_slice()).unwrap_or(&[]);
            let shape = result.map(|r| &r.expected_shape);
            let rows = result.map(|r| r.sample.rows.as_slice());
            let mut chart_type = resolve_chart_type(preferred, columns, shape, rows);

            // Bar/Line/Pie charts require at least 2 columns (x+y / name+value).
            // Fall back to table when the result set is too narrow.
            if matches!(
                chart_type,
                ChartType::Bar | ChartType::Line | ChartType::Pie
            ) && columns.len() < 2
            {
                chart_type = ChartType::Table;
            }

            // Use the chart type directly as the YAML `type:` discriminant,
            // with `data:` referencing the task (oxy app format).
            let type_str = match &chart_type {
                ChartType::Line => "line_chart",
                ChartType::Bar => "bar_chart",
                ChartType::Pie => "pie_chart",
                ChartType::Table => "table",
            };

            let mut lines = vec![
                format!("{pad}- type: {type_str}"),
                format!("{pad1}data: {task}"),
            ];

            // Add x/y/series/name/value based on chart type and available columns.
            match &chart_type {
                ChartType::Line | ChartType::Bar => {
                    // columns.len() >= 2 guaranteed by the guard above.
                    let (x, y, series) = resolve_chart_columns(columns, expected_columns, rows);
                    lines.push(format!("{pad1}x: {}", yaml_scalar(&x)));
                    lines.push(format!("{pad1}y: {}", yaml_scalar(&y)));
                    if let Some(s) = series {
                        lines.push(format!("{pad1}series: {}", yaml_scalar(&s)));
                    }
                }
                ChartType::Pie => {
                    lines.push(format!("{pad1}name: {}", yaml_scalar(&columns[0])));
                    lines.push(format!("{pad1}value: {}", yaml_scalar(&columns[1])));
                }
                ChartType::Table => {}
            }
            Ok(lines)
        }

        LayoutNode::Table { task, title } => {
            let mut lines = vec![format!("{pad}- type: table"), format!("{pad1}data: {task}")];
            if let Some(t) = title {
                lines.push(format!("{pad1}title: {t}"));
            }
            Ok(lines)
        }

        LayoutNode::Row { columns, children } => {
            let mut lines = vec![
                format!("{pad}- type: row"),
                format!("{pad1}columns: {columns}"),
                format!("{pad1}children:"),
            ];
            for child in children {
                let child_lines =
                    render_layout_node(child, task_results, chart_configs, indent + 2)?;
                lines.extend(child_lines);
            }
            Ok(lines)
        }

        LayoutNode::Markdown { content } => {
            // Use block scalar for multi-line markdown.
            let mut lines = vec![
                format!("{pad}- type: markdown"),
                format!("{pad1}content: |"),
            ];
            for text_line in content.lines() {
                lines.push(format!("{pad2}{text_line}"));
            }
            Ok(lines)
        }

        LayoutNode::Insight { .. } => {
            // Should have been resolved to Markdown before YAML assembly.
            // Emit a placeholder if somehow reached.
            Ok(vec![
                format!("{pad}- type: markdown"),
                format!("{pad1}content: \"(insight unavailable)\""),
            ])
        }
    }
}

/// Assemble the complete `.app.yml` from the result and layout.
fn assemble_yaml(
    result: &AppResult,
    chart_configs: &HashMap<String, ResolvedChartConfig>,
) -> Result<String, AppBuilderError> {
    let mut lines: Vec<String> = Vec::new();

    // Controls section
    if !result.controls.is_empty() {
        lines.push("controls:".to_string());
        for ctrl in &result.controls {
            lines.push(format!("  - name: {}", ctrl.name));
            lines.push(format!("    label: {}", ctrl.label));
            lines.push(format!("    type: {}", ctrl.control_type));
            lines.push(format!("    default: {}", yaml_scalar(&ctrl.default)));
            if let (ControlType::Select, Some(src)) = (&ctrl.control_type, &ctrl.source_task) {
                lines.push(format!("    source: {src}"));
            } else if matches!(ctrl.control_type, ControlType::Select) && !ctrl.options.is_empty() {
                let opts: Vec<String> = ctrl.options.iter().map(|o| yaml_scalar(o)).collect();
                lines.push(format!("    options: [{}]", opts.join(", ")));
            }
        }
        lines.push(String::new());
    }

    // Tasks section — only display tasks (control-source tasks are system-internal)
    let display_tasks: Vec<&TaskResult> = result
        .task_results
        .iter()
        .filter(|t| !t.is_control_source)
        .collect();
    let control_source_tasks: Vec<&TaskResult> = result
        .task_results
        .iter()
        .filter(|t| t.is_control_source)
        .collect();

    if !result.task_results.is_empty() {
        lines.push("tasks:".to_string());
        // Control-source tasks first, then display tasks.
        for task in control_source_tasks.iter().chain(display_tasks.iter()) {
            lines.push(format!("  - name: {}", task.name));
            lines.push("    type: execute_sql".to_string());
            lines.push(format!("    database: {}", result.connector_name));
            lines.push("    sql_query: >".to_string());
            for sql_line in task.sql.lines() {
                lines.push(format!("      {}", sql_line.trim()));
            }
            lines.push("    mode: server".to_string());
        }
        lines.push(String::new());
    }

    // Display section
    if !result.layout.is_empty() {
        lines.push("display:".to_string());
        for node in &result.layout {
            let node_lines = render_layout_node(node, &result.task_results, chart_configs, 1)?;
            lines.extend(node_lines);
        }
    }

    Ok(lines.join("\n"))
}

// ---------------------------------------------------------------------------
// interpret_impl
// ---------------------------------------------------------------------------

impl AppBuilderSolver {
    /// Assemble the final `AppAnswer` (YAML) from the task results.
    pub(crate) async fn interpret_impl(
        &mut self,
        result: AppResult,
    ) -> Result<AppAnswer, (AppBuilderError, BackTarget<AppBuilderDomain>)> {
        let task_count = result.task_results.len();
        let control_count = result.controls.len();

        // Resolve Insight nodes into Markdown via LLM before YAML assembly.
        let insight_prompt = self.build_system_prompt("interpreting", INSIGHT_GENERATION_PROMPT);
        let AppResult {
            task_results,
            controls,
            layout,
            connector_name,
        } = result;
        let resolved_layout =
            resolve_layout_insights(layout, &self.client, &insight_prompt, &task_results).await;
        let result = AppResult {
            task_results,
            controls,
            layout: resolved_layout,
            connector_name,
        };

        // Resolve chart configurations via LLM before YAML assembly.
        let chart_prompt = self.build_system_prompt("interpreting", CHART_CONFIG_SYSTEM_PROMPT);
        let chart_configs = resolve_all_chart_configs(
            &self.client,
            &chart_prompt,
            &result.layout,
            &result.task_results,
        )
        .await;

        let mut result = result;
        let mut yaml = assemble_yaml(&result, &chart_configs).map_err(|e| {
            (
                e.clone(),
                BackTarget::Interpret(result.clone(), Default::default()),
            )
        })?;

        // ── Validate + LLM patch loop ───────────────────────────────────────
        const MAX_PATCH_ATTEMPTS: usize = 2;

        if let Some(validator) = &self.validator {
            for attempt in 0..MAX_PATCH_ATTEMPTS {
                match validator.validate(&yaml).await {
                    Ok(()) => break,
                    Err(errors) => {
                        if attempt == MAX_PATCH_ATTEMPTS - 1 {
                            return Err((
                                AppBuilderError::InvalidSpec { errors },
                                BackTarget::Interpret(result.clone(), Default::default()),
                            ));
                        }
                        // Ask LLM to fix the controls that caused validation errors.
                        let controls_json =
                            serde_json::to_string_pretty(&result.controls).unwrap_or_default();
                        let patch_user = format!(
                            "Validation errors:\n{}\n\nCurrent controls JSON:\n{controls_json}\n\n\
                             Return ONLY the corrected controls JSON array.",
                            errors.join("\n"),
                        );
                        let patch_system =
                            self.build_system_prompt("interpreting", PATCH_APP_RESULT_PROMPT);
                        match self.client.complete(&patch_system, &patch_user).await {
                            Ok(raw) => {
                                let json_str = crate::solver::strip_json_fences(&raw).to_owned();
                                match serde_json::from_str(&json_str) {
                                    Ok(patched_controls) => {
                                        result.controls = patched_controls;
                                    }
                                    Err(e) => {
                                        tracing::warn!(
                                            "LLM patch produced unparsable JSON (attempt {attempt}): {e}"
                                        );
                                        continue;
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!("LLM patch call failed (attempt {attempt}): {e}");
                                continue;
                            }
                        }
                        // Re-assemble with patched result.
                        yaml = assemble_yaml(&result, &chart_configs).map_err(|e| {
                            (
                                e.clone(),
                                BackTarget::Interpret(result.clone(), Default::default()),
                            )
                        })?;
                    }
                }
            }
        }

        let char_count = yaml.len();

        // Summarize the generated app via LLM.
        let system_prompt = self.build_system_prompt("interpreting", INTERPRETING_SYSTEM_PROMPT);
        let summary = self
            .client
            .complete(&system_prompt, &yaml)
            .await
            .unwrap_or_else(|_| {
                format!("Data app with {task_count} tasks and {control_count} controls.",)
            });

        // Emit the summary as LlmToken events so the frontend sees it as
        // text_delta in the interpreting step's SSE stream.
        if let Some(tx) = &self.event_tx {
            if !summary.is_empty() {
                let _ = tx
                    .send(agentic_core::events::Event::Core(
                        agentic_core::events::CoreEvent::LlmToken {
                            token: summary.clone(),
                            sub_spec_index: None,
                        },
                    ))
                    .await;
            }
            let _ = tx
                .send(agentic_core::events::Event::Domain(
                    AppBuilderEvent::AppYamlReady { char_count },
                ))
                .await;
        }

        Ok(AppAnswer {
            yaml,
            summary,
            task_count,
            control_count,
        })
    }
}

// ---------------------------------------------------------------------------
// State handler
// ---------------------------------------------------------------------------

/// Build the `StateHandler` for the **interpreting** state.
pub(super) fn build_interpreting_handler()
-> StateHandler<AppBuilderDomain, AppBuilderSolver, AppBuilderEvent> {
    StateHandler {
        next: "done",
        execute: Arc::new(
            |solver: &mut AppBuilderSolver,
             state,
             _events,
             _run_ctx: &RunContext<AppBuilderDomain>,
             _memory: &SessionMemory<AppBuilderDomain>| {
                Box::pin(async move {
                    let result = match state {
                        ProblemState::Interpreting(r) => r,
                        _ => unreachable!("interpreting handler called with wrong state"),
                    };
                    match solver.interpret_impl(result).await {
                        Ok(answer) => TransitionResult::ok(ProblemState::Done(answer)),
                        Err((err, back)) => {
                            TransitionResult::diagnosing(ProblemState::Diagnosing {
                                error: err,
                                back,
                            })
                        }
                    }
                })
            },
        ),
        diagnose: None,
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ControlPlan;

    #[test]
    fn test_resolve_chart_type_auto_time_series() {
        let cols = vec!["month".to_string(), "revenue".to_string()];
        assert_eq!(
            resolve_chart_type(&ChartPreference::Auto, &cols, None, None),
            ChartType::Line
        );
    }

    #[test]
    fn test_resolve_chart_type_auto_two_col() {
        let cols = vec!["store".to_string(), "sales".to_string()];
        assert_eq!(
            resolve_chart_type(&ChartPreference::Auto, &cols, None, None),
            ChartType::Bar
        );
    }

    #[test]
    fn test_resolve_chart_type_auto_wide() {
        let cols = vec!["a".into(), "b".into(), "c".into(), "d".into()];
        assert_eq!(
            resolve_chart_type(&ChartPreference::Auto, &cols, None, None),
            ChartType::Table
        );
    }

    #[test]
    fn test_resolve_chart_type_explicit_pie() {
        let cols = vec!["store".into(), "revenue".into()];
        assert_eq!(
            resolve_chart_type(&ChartPreference::Pie, &cols, None, None),
            ChartType::Pie
        );
    }

    #[test]
    fn test_resolve_chart_type_auto_single_col() {
        let cols = vec!["count".into()];
        assert_eq!(
            resolve_chart_type(&ChartPreference::Auto, &cols, None, None),
            ChartType::Bar
        );
    }

    #[test]
    fn test_resolve_chart_type_auto_with_timeseries_shape() {
        // Even without time-like column names, TimeSeries shape should resolve to Line.
        let cols = vec!["period".to_string(), "amount".to_string()];
        assert_eq!(
            resolve_chart_type(
                &ChartPreference::Auto,
                &cols,
                Some(&ResultShape::TimeSeries),
                None,
            ),
            ChartType::Line
        );
    }

    #[test]
    fn test_resolve_chart_type_auto_with_scalar_shape() {
        let cols = vec!["total".into()];
        assert_eq!(
            resolve_chart_type(
                &ChartPreference::Auto,
                &cols,
                Some(&ResultShape::Scalar),
                None
            ),
            ChartType::Bar
        );
    }

    #[test]
    fn test_resolve_chart_type_explicit_overrides_shape() {
        // Explicit preference takes priority over expected_shape.
        let cols = vec!["month".into(), "revenue".into()];
        assert_eq!(
            resolve_chart_type(
                &ChartPreference::Table,
                &cols,
                Some(&ResultShape::TimeSeries),
                None,
            ),
            ChartType::Table
        );
    }

    #[test]
    fn test_resolve_chart_type_value_based_iso_dates() {
        // Column name "period" doesn't match any keyword, but values are ISO dates.
        let cols = vec!["period".to_string(), "revenue".to_string()];
        let rows = vec![
            QueryRow(vec![
                CellValue::Text("2024-01".into()),
                CellValue::Number(100.0),
            ]),
            QueryRow(vec![
                CellValue::Text("2024-02".into()),
                CellValue::Number(120.0),
            ]),
            QueryRow(vec![
                CellValue::Text("2024-03".into()),
                CellValue::Number(95.0),
            ]),
        ];
        assert_eq!(
            resolve_chart_type(&ChartPreference::Auto, &cols, None, Some(&rows)),
            ChartType::Line
        );
    }

    #[test]
    fn test_resolve_chart_type_no_false_positive_monday() {
        // "monday" used to match "day" via substring; token split prevents this.
        let cols = vec!["monday".to_string(), "sales".to_string()];
        assert_eq!(
            resolve_chart_type(&ChartPreference::Auto, &cols, None, None),
            ChartType::Bar
        );
    }

    #[test]
    fn test_resolve_chart_type_value_based_quarters() {
        let cols = vec!["fiscal_period".to_string(), "profit".to_string()];
        let rows = vec![
            QueryRow(vec![
                CellValue::Text("Q1 2024".into()),
                CellValue::Number(500.0),
            ]),
            QueryRow(vec![
                CellValue::Text("Q2 2024".into()),
                CellValue::Number(620.0),
            ]),
        ];
        assert_eq!(
            resolve_chart_type(&ChartPreference::Auto, &cols, None, Some(&rows)),
            ChartType::Line
        );
    }

    // ── Helper to build minimal AppResult for YAML assembly tests ────────

    fn make_task_result(name: &str, columns: Vec<&str>, is_control_source: bool) -> TaskResult {
        make_task_result_with_expected(name, columns, vec![], is_control_source)
    }

    fn make_task_result_with_expected(
        name: &str,
        columns: Vec<&str>,
        expected_columns: Vec<&str>,
        is_control_source: bool,
    ) -> TaskResult {
        TaskResult {
            name: name.to_string(),
            sql: format!("SELECT {} FROM t", columns.join(", ")),
            columns: columns.iter().map(|c| c.to_string()).collect(),
            column_types: vec![None; columns.len()],
            row_count: 5,
            is_control_source,
            expected_shape: ResultShape::Series,
            expected_columns: expected_columns.iter().map(|c| c.to_string()).collect(),
            sample: agentic_core::result::QueryResult {
                columns: columns.iter().map(|c| c.to_string()).collect(),
                rows: vec![],
                total_row_count: 5,
                truncated: false,
            },
        }
    }

    fn make_app_result(
        task_results: Vec<TaskResult>,
        controls: Vec<ControlPlan>,
        layout: Vec<LayoutNode>,
    ) -> AppResult {
        AppResult {
            task_results,
            controls,
            layout,
            connector_name: "local".to_string(),
        }
    }

    // ── Issue 2: bar_chart must always have a `y` field ──────────────────

    // ── 3-column GROUP BY tests ──────────────────────────────────────────

    #[test]
    fn test_line_chart_three_col_group_by_with_expected_columns() {
        // Simulates: SELECT Date, Exercise, SUM(Weight*Reps) AS "Volume (lbs x reps)"
        // GROUP BY Date, Exercise — where expected_columns tells us x/y.
        let task = make_task_result_with_expected(
            "task_strength_volume",
            vec!["Date", "Exercise", "Volume (lbs x reps)"],
            vec!["Date", "Volume (lbs x reps)"],
            false,
        );
        let layout = vec![LayoutNode::Chart {
            task: "task_strength_volume".to_string(),
            preferred: ChartPreference::Line,
        }];
        let result = make_app_result(vec![task], vec![], layout);
        let yaml = assemble_yaml(&result, &HashMap::new()).unwrap();
        assert!(
            yaml.contains("type: line_chart"),
            "should be line_chart, got:\n{yaml}"
        );
        assert!(yaml.contains("x: Date"), "x should be Date, got:\n{yaml}");
        assert!(
            yaml.contains("y: Volume (lbs x reps)"),
            "y should be Volume column, got:\n{yaml}"
        );
        assert!(
            yaml.contains("series: Exercise"),
            "series should be Exercise, got:\n{yaml}"
        );
    }

    #[test]
    fn test_line_chart_three_col_group_by_fallback_heuristic() {
        // No expected_columns set — falls back to: time→x, last→y, middle→series.
        let task = make_task_result(
            "task_strength_volume",
            vec!["Date", "Exercise", "Volume (lbs x reps)"],
            false,
        );
        let layout = vec![LayoutNode::Chart {
            task: "task_strength_volume".to_string(),
            preferred: ChartPreference::Line,
        }];
        let result = make_app_result(vec![task], vec![], layout);
        let yaml = assemble_yaml(&result, &HashMap::new()).unwrap();
        assert!(
            yaml.contains("type: line_chart"),
            "should be line_chart, got:\n{yaml}"
        );
        assert!(
            yaml.contains("x: Date"),
            "x should be Date (time heuristic), got:\n{yaml}"
        );
        assert!(
            yaml.contains("y: Volume (lbs x reps)"),
            "y should be last column, got:\n{yaml}"
        );
        assert!(
            yaml.contains("series: Exercise"),
            "series should be middle column, got:\n{yaml}"
        );
    }

    #[test]
    fn test_auto_three_col_with_time_infers_line_chart() {
        // Auto + 3 columns + TimeSeries shape → line_chart with series, not Table.
        // (TimeSeries is the expected_shape the LLM sets for time-grouped queries;
        //  a Series shape would short-circuit to bar_chart before column inspection.)
        let mut task = make_task_result(
            "grouped_by_exercise",
            vec!["Date", "Exercise", "Volume"],
            false,
        );
        task.expected_shape = ResultShape::TimeSeries;
        let layout = vec![LayoutNode::Chart {
            task: "grouped_by_exercise".to_string(),
            preferred: ChartPreference::Auto,
        }];
        let result = make_app_result(vec![task], vec![], layout);
        let yaml = assemble_yaml(&result, &HashMap::new()).unwrap();
        assert!(
            yaml.contains("type: line_chart"),
            "3-col TimeSeries should be line_chart, got:\n{yaml}"
        );
        assert!(
            yaml.contains("series: Exercise"),
            "should have series field, got:\n{yaml}"
        );
    }

    #[test]
    fn test_bar_chart_with_two_columns_has_y_field() {
        let task = make_task_result("revenue_by_store", vec!["Store", "Revenue"], false);
        let layout = vec![LayoutNode::Chart {
            task: "revenue_by_store".to_string(),
            preferred: ChartPreference::Bar,
        }];
        let result = make_app_result(vec![task], vec![], layout);
        let yaml = assemble_yaml(&result, &HashMap::new()).unwrap();
        assert!(yaml.contains("type: bar_chart"), "should be bar_chart");
        assert!(yaml.contains("x: Store"), "bar_chart must have x field");
        assert!(
            yaml.contains("y: Revenue"),
            "bar_chart with 2 columns must have y field, got:\n{yaml}"
        );
    }

    #[test]
    fn test_bar_chart_single_column_must_not_omit_y() {
        // A single-column task used as bar_chart is problematic — the chart
        // needs both x and y. The assembler should still produce a valid y
        // (or avoid bar_chart altogether for single-column results).
        let task = make_task_result("kpi_total", vec!["Total_Revenue"], false);
        let layout = vec![LayoutNode::Chart {
            task: "kpi_total".to_string(),
            preferred: ChartPreference::Bar,
        }];
        let result = make_app_result(vec![task], vec![], layout);
        let yaml = assemble_yaml(&result, &HashMap::new()).unwrap();
        // bar_chart output must always have both x and y
        if yaml.contains("type: bar_chart") {
            assert!(
                yaml.contains("y:"),
                "bar_chart must always have a y field, got:\n{yaml}"
            );
        }
        // Alternatively, a single-column result could become a table —
        // either way, y must not be missing on a bar_chart.
    }

    // ── Issue 3: control defaults must not be double-quoted ──────────────

    #[test]
    fn test_control_default_no_extra_quotes() {
        let controls = vec![ControlPlan {
            name: "region".to_string(),
            label: "Region".to_string(),
            control_type: ControlType::Select,
            source_task: Some("region_options".to_string()),
            options: vec![],
            default: "All".to_string(),
        }];
        let result = make_app_result(vec![], controls, vec![]);
        let yaml = assemble_yaml(&result, &HashMap::new()).unwrap();
        // Should be `default: All` not `default: "All"` or `default: \"All\"`
        assert!(
            yaml.contains("default: All"),
            "simple default should not be wrapped in quotes, got:\n{yaml}"
        );
        assert!(
            !yaml.contains("default: \"All\""),
            "default should not have unnecessary double-quotes, got:\n{yaml}"
        );
    }

    #[test]
    fn test_control_default_with_special_chars_is_quoted() {
        let controls = vec![ControlPlan {
            name: "filter".to_string(),
            label: "Filter".to_string(),
            control_type: ControlType::Select,
            source_task: None,
            options: vec!["hello: world".to_string()],
            default: "hello: world".to_string(),
        }];
        let result = make_app_result(vec![], controls, vec![]);
        let yaml = assemble_yaml(&result, &HashMap::new()).unwrap();
        // Values with YAML-special chars (colon) need quoting
        assert!(
            yaml.contains("default: \"hello: world\""),
            "defaults with special YAML chars should be quoted, got:\n{yaml}"
        );
    }

    #[test]
    fn test_control_default_numeric_not_quoted() {
        let controls = vec![ControlPlan {
            name: "limit".to_string(),
            label: "Limit".to_string(),
            control_type: ControlType::Select,
            source_task: None,
            options: vec!["100".to_string()],
            default: "100".to_string(),
        }];
        let result = make_app_result(vec![], controls, vec![]);
        let yaml = assemble_yaml(&result, &HashMap::new()).unwrap();
        assert!(
            yaml.contains("default: 100"),
            "numeric defaults should not be quoted, got:\n{yaml}"
        );
    }

    #[test]
    fn test_select_with_static_options_emits_options_in_yaml() {
        let controls = vec![ControlPlan {
            name: "status".to_string(),
            label: "Status".to_string(),
            control_type: ControlType::Select,
            source_task: None,
            options: vec![
                "All".to_string(),
                "Active".to_string(),
                "Inactive".to_string(),
            ],
            default: "All".to_string(),
        }];
        let result = make_app_result(vec![], controls, vec![]);
        let yaml = assemble_yaml(&result, &HashMap::new()).unwrap();
        assert!(
            yaml.contains("options: [All, Active, Inactive]"),
            "select with static options should emit options list, got:\n{yaml}"
        );
        assert!(
            !yaml.contains("source:"),
            "select with static options should not emit source, got:\n{yaml}"
        );
    }

    #[test]
    fn test_select_with_source_does_not_emit_options() {
        let controls = vec![ControlPlan {
            name: "store".to_string(),
            label: "Store".to_string(),
            control_type: ControlType::Select,
            source_task: Some("store_options".to_string()),
            options: vec![],
            default: "All".to_string(),
        }];
        let result = make_app_result(vec![], controls, vec![]);
        let yaml = assemble_yaml(&result, &HashMap::new()).unwrap();
        assert!(
            yaml.contains("source: store_options"),
            "select with source_task should emit source, got:\n{yaml}"
        );
        assert!(
            !yaml.contains("options:"),
            "select with source_task should not emit options, got:\n{yaml}"
        );
    }

    // ── Insight node tests ──────────────────────────────────────────────

    #[test]
    fn test_insight_fallback_in_yaml() {
        // An unresolved Insight node should render as a markdown placeholder.
        let layout = vec![LayoutNode::Insight {
            tasks: vec!["some_task".into()],
            focus: Some("trends".into()),
        }];
        let result = make_app_result(vec![], vec![], layout);
        let yaml = assemble_yaml(&result, &HashMap::new()).unwrap();
        assert!(
            yaml.contains("type: markdown"),
            "unresolved insight should fall back to markdown, got:\n{yaml}"
        );
        assert!(
            yaml.contains("insight unavailable"),
            "unresolved insight should show placeholder, got:\n{yaml}"
        );
    }

    #[test]
    fn test_format_task_data_for_insight_table() {
        use agentic_core::result::{CellValue, QueryResult, QueryRow};
        let task = TaskResult {
            name: "revenue_by_store".to_string(),
            sql: "SELECT store, revenue FROM t".to_string(),
            columns: vec!["store".into(), "revenue".into()],
            column_types: vec![None, None],
            row_count: 3,
            is_control_source: false,
            expected_shape: ResultShape::Series,
            expected_columns: vec![],
            sample: QueryResult {
                columns: vec!["store".into(), "revenue".into()],
                rows: vec![
                    QueryRow(vec![
                        CellValue::Text("Store A".into()),
                        CellValue::Number(1200.0),
                    ]),
                    QueryRow(vec![
                        CellValue::Text("Store B".into()),
                        CellValue::Number(800.5),
                    ]),
                    QueryRow(vec![
                        CellValue::Text("Store C".into()),
                        CellValue::Number(950.0),
                    ]),
                ],
                total_row_count: 3,
                truncated: false,
            },
        };
        let output = format_task_data_for_insight(&task);
        assert!(
            output.contains("**revenue_by_store**"),
            "should contain task name"
        );
        assert!(output.contains("3 rows"), "should mention row count");
        assert!(output.contains("| store | revenue |"), "should have header");
        assert!(output.contains("Store A"), "should contain data values");
        assert!(output.contains("1200"), "should contain numeric values");
    }

    #[test]
    fn test_format_task_data_for_insight_single_col() {
        use agentic_core::result::{CellValue, QueryResult, QueryRow};
        let task = TaskResult {
            name: "total_revenue".to_string(),
            sql: "SELECT sum(rev) AS total FROM t".to_string(),
            columns: vec!["total".into()],
            column_types: vec![None],
            row_count: 1,
            is_control_source: false,
            expected_shape: ResultShape::Scalar,
            expected_columns: vec![],
            sample: QueryResult {
                columns: vec!["total".into()],
                rows: vec![QueryRow(vec![CellValue::Number(42000.0)])],
                total_row_count: 1,
                truncated: false,
            },
        };
        let output = format_task_data_for_insight(&task);
        assert!(
            output.contains("**total_revenue**"),
            "should contain task name"
        );
        assert!(output.contains("42000"), "should contain the value");
    }

    #[test]
    fn test_cell_to_string_formatting() {
        assert_eq!(cell_to_string(&CellValue::Text("hello".into())), "hello");
        assert_eq!(cell_to_string(&CellValue::Number(42.0)), "42");
        assert_eq!(cell_to_string(&CellValue::Number(3.14)), "3.14");
        assert_eq!(cell_to_string(&CellValue::Null), "NULL");
    }
}
