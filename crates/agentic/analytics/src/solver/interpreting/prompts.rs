//! Prompt builders and result-set formatters for the Interpreting stage.

use agentic_core::{back_target::RetryContext, orchestrator::CompletedTurn, result::CellValue};

use crate::types::{AnalyticsDomain, ConversationTurn};

use super::super::prompts::{format_history_section, format_retry_section};

pub(super) fn cell_to_json(cell: &CellValue) -> serde_json::Value {
    match cell {
        CellValue::Text(s) => serde_json::Value::String(s.clone()),
        CellValue::Number(n) => serde_json::json!(n),
        CellValue::Null => serde_json::Value::Null,
    }
}

// ---------------------------------------------------------------------------
// User-prompt builder
// ---------------------------------------------------------------------------

/// Maximum number of rows included in the LLM prompt for text interpretation.
///
/// The full result set (up to `DEFAULT_SAMPLE_LIMIT` rows in executing.rs) is
/// still passed to the `render_chart` tool via `result_sets_for_tool`, so
/// chart rendering is unaffected by this limit.
const INTERPRET_DISPLAY_LIMIT: usize = 50;

/// Format a single `QueryResultSet` as a markdown block for the LLM prompt.
pub(super) fn format_result_set(rs: &crate::types::QueryResultSet, label: Option<&str>) -> String {
    let columns = &rs.data.columns;
    let total_row_count = rs.data.total_row_count;
    let display_rows = &rs.data.rows[..rs.data.rows.len().min(INTERPRET_DISPLAY_LIMIT)];
    let sample_size = display_rows.len();
    let is_tabular = columns.len() >= 2 && sample_size >= 2;

    let rows: Vec<Vec<String>> = display_rows
        .iter()
        .map(|row| {
            row.0
                .iter()
                .map(|cell| match cell {
                    CellValue::Text(s) => s.clone(),
                    CellValue::Number(n) => n.to_string(),
                    CellValue::Null => "NULL".to_string(),
                })
                .collect()
        })
        .collect();

    let fetched_size = rs.data.rows.len();
    let row_context = if sample_size < fetched_size {
        format!(
            "{total_row_count} rows total, showing first {sample_size} of {fetched_size} fetched."
        )
    } else if (fetched_size as u64) < total_row_count {
        format!("{total_row_count} rows total, showing {sample_size}.")
    } else {
        format!("{total_row_count} rows total.")
    };

    let summary_context = if let Some(summary) = &rs.summary {
        let stats: Vec<String> = summary
            .columns
            .iter()
            .map(|c| {
                let mut parts: Vec<String> = Vec::new();
                let cell_str = |v: &CellValue| match v {
                    CellValue::Text(s) => s.clone(),
                    CellValue::Number(n) => n.to_string(),
                    CellValue::Null => "NULL".to_string(),
                };
                if let Some(min) = &c.min {
                    parts.push(format!("min={}", cell_str(min)));
                }
                if let Some(max) = &c.max {
                    parts.push(format!("max={}", cell_str(max)));
                }
                if let Some(mean) = c.mean {
                    parts.push(format!("mean={mean:.2}"));
                }
                if let Some(std_dev) = c.std_dev {
                    parts.push(format!("std_dev={std_dev:.2}"));
                }
                if let Some(distinct) = c.distinct_count {
                    parts.push(format!("distinct={distinct}"));
                }
                if c.null_count > 0 {
                    parts.push(format!("nulls={}", c.null_count));
                }
                format!("  {}: {}", c.name, parts.join(" "))
            })
            .collect();
        if !stats.is_empty() {
            format!("\nColumn stats:\n{}", stats.join("\n"))
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let data_section = if is_tabular {
        let header = format!("| {} |", columns.join(" | "));
        let separator = format!(
            "| {} |",
            columns
                .iter()
                .map(|_| "---")
                .collect::<Vec<_>>()
                .join(" | ")
        );
        let body: Vec<String> = rows
            .iter()
            .map(|r| format!("| {} |", r.join(" | ")))
            .collect();
        format!("{}\n{}\n{}", header, separator, body.join("\n"))
    } else {
        let flat: Vec<String> = rows.iter().map(|r| r.join(" | ")).collect();
        format!(
            "Columns: {}\nRows:\n{}",
            columns.join(", "),
            flat.join("\n")
        )
    };

    match label {
        Some(lbl) => format!("**{lbl}** ({row_context}){summary_context}\n{data_section}"),
        None => format!("{row_context}{summary_context}\n{data_section}"),
    }
}

/// Build the user-turn message for the Interpret LLM call.
///
/// `pub(super)` so the unit tests in `mod.rs` can access it directly.
pub fn build_interpret_user_prompt(
    raw_question: &str,
    history: &[ConversationTurn],
    result: &crate::types::AnalyticsResult,
    retry_ctx: Option<&RetryContext>,
    session_turns: &[CompletedTurn<AnalyticsDomain>],
    suggested_config: Option<&crate::types::ChartConfig>,
) -> String {
    // Format result data — one block per result set for fan-out queries.
    let data_section = if result.is_multi() {
        result
            .results
            .iter()
            .enumerate()
            .map(|(i, rs)| {
                format_result_set(
                    rs,
                    Some(&format!("Result set {} (result_index: {})", i + 1, i)),
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    } else {
        format_result_set(result.primary(), None)
    };

    // Prior turn context for comparative framing (most recent turn only).
    let prior_turn_section = if let Some(last) = session_turns.last() {
        format!(
            "\nPrevious question: {}\nPrevious answer: {}\n\n\
             If the current question is a follow-up, frame the answer \
             comparatively (e.g. \"Unlike the previous result…\" or \
             \"Breaking down the same data differently…\").",
            last.intent.raw_question, last.answer.text,
        )
    } else {
        String::new()
    };

    // All chart configs from every prior session turn — the LLM needs the full
    // history to handle chart-edit requests like "change that to a bar chart".
    let prior_charts_section = {
        let charts: Vec<String> = session_turns
            .iter()
            .enumerate()
            .flat_map(|(turn_idx, t)| {
                t.answer
                    .display_blocks
                    .iter()
                    .enumerate()
                    .map(move |(chart_idx, db)| {
                        let json = serde_json::to_string(&db.config).unwrap_or_default();
                        format!("  Turn {}, chart {}: {json}", turn_idx + 1, chart_idx + 1)
                    })
            })
            .collect();
        if charts.is_empty() {
            String::new()
        } else {
            format!(
                "\n\nPrevious chart configs (reference when the user asks to edit a chart):\n{}",
                charts.join("\n")
            )
        }
    };

    let chart_suggestion_section = if let Some(cfg) = suggested_config {
        let json = serde_json::to_string(cfg).unwrap_or_default();
        format!(
            "\n\nSuggested chart config (auto-computed from result shape — use as a starting point):\n{json}"
        )
    } else {
        String::new()
    };

    let history_section = format_history_section(history);
    let retry_section = format_retry_section(retry_ctx);

    format!(
        "{history_section}Original question: {raw_question}\n\n\
         Query results:\n{data_section}\
         {prior_charts_section}\
         {chart_suggestion_section}\n\n\
         Answer the original question based on these results.\
         {prior_turn_section}{retry_section}",
    )
}

// ---------------------------------------------------------------------------
// interpret_impl
// ---------------------------------------------------------------------------

pub(super) fn format_delegation_data(
    result_sets: &[(Vec<String>, Vec<Vec<serde_json::Value>>)],
) -> String {
    result_sets
        .iter()
        .enumerate()
        .map(|(i, (cols, rows))| {
            let header = cols.join(" | ");
            let data_rows: Vec<String> = rows
                .iter()
                .take(50) // cap for prompt size
                .map(|row| {
                    row.iter()
                        .map(|v| match v {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        })
                        .collect::<Vec<_>>()
                        .join(" | ")
                })
                .collect();
            format!(
                "### Result set {}\n{header}\n{}\n({} rows total)",
                i + 1,
                data_rows.join("\n"),
                rows.len()
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Parse the delegation answer (JSON array of step results) into result sets
/// that the interpreting prompt builder can use.
pub(super) fn parse_delegation_result_sets(
    answer: &str,
) -> Option<Vec<(Vec<String>, Vec<Vec<serde_json::Value>>)>> {
    let steps: Vec<serde_json::Value> = serde_json::from_str(answer).ok()?;
    let mut result_sets = Vec::new();
    for step in &steps {
        if let Some(columns) = step["columns"].as_array() {
            let cols: Vec<String> = columns
                .iter()
                .filter_map(|c| c.as_str().map(str::to_string))
                .collect();
            let rows: Vec<Vec<serde_json::Value>> = step["rows"]
                .as_array()
                .map(|arr| arr.iter().filter_map(|r| r.as_array().cloned()).collect())
                .unwrap_or_default();
            result_sets.push((cols, rows));
        } else if let Some(text) = step["text"].as_str() {
            // Text-only step — wrap as a single-cell result.
            result_sets.push((
                vec!["result".to_string()],
                vec![vec![serde_json::Value::String(text.to_string())]],
            ));
        }
    }
    if result_sets.is_empty() {
        None
    } else {
        Some(result_sets)
    }
}
