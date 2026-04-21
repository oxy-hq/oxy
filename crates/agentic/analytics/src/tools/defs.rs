//! [`ToolDef`] factories — one set per FSM state.

use agentic_core::tools::ToolDef;
use serde_json::json;

use crate::types::{ChartConfig, QuestionType};

use super::{SAMPLE_COLUMNS_DESC, SEARCH_CATALOG_DESC, SEARCH_PROCEDURES_DESC};

// ── Tool definitions per state ────────────────────────────────────────────────

/// Tools available during the **triage** sub-phase of Clarify.
///
/// Only `search_procedures` is exposed — triage must check for an existing
/// procedure before doing any schema discovery.
pub fn triage_tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "search_procedures",
            description: SEARCH_PROCEDURES_DESC,
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search term matched against procedure names and descriptions"
                    }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
            ..Default::default()
        },
        ToolDef {
            name: "search_catalog",
            description: SEARCH_CATALOG_DESC,
            parameters: json!({
                "type": "object",
                "properties": {
                    "queries": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Search terms matched against measure/dimension names and descriptions."
                    }
                },
                "required": ["queries"],
                "additionalProperties": false
            }),
            ..Default::default()
        },
        propose_semantic_query_tool(),
    ]
}

const PROPOSE_SEMANTIC_QUERY_DESC: &str = "Call this tool AFTER search_catalog confirms that ALL needed measures \
     and dimensions exist. Submits a structured semantic query for fast \
     compilation, skipping SQL generation. Only call when you are certain \
     about the view.member paths — do not guess.";

/// Tool definition for `propose_semantic_query`.
///
/// Extracted from the former `semantic_query` field of the triage response
/// schema so that the response schema stays small enough for strict-mode
/// grammar compilation.
pub fn propose_semantic_query_tool() -> ToolDef {
    ToolDef {
        name: "propose_semantic_query",
        description: PROPOSE_SEMANTIC_QUERY_DESC,
        parameters: json!({
            "type": "object",
            "properties": {
                "measures": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Exact measure member paths in view.member format (e.g. 'orders.revenue'). Must match names from search_catalog results exactly."
                },
                "dimensions": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Exact dimension member paths in view.member format. Must match names from search_catalog results exactly."
                },
                "filters": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "member": { "type": "string" },
                            "operator": { "type": "string" },
                            "values": { "type": "array", "items": { "type": "string" } }
                        },
                        "required": ["member", "operator", "values"],
                        "additionalProperties": false
                    },
                    "description": "Structured filter conditions using exact member paths."
                },
                "time_dimensions": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "dimension": { "type": "string" },
                            "granularity": { "type": ["string", "null"] },
                            "date_range": { "type": ["array", "null"], "items": { "type": "string" } }
                        },
                        "required": ["dimension", "granularity", "date_range"],
                        "additionalProperties": false
                    },
                    "description": "Time dimension entries with granularity and optional date range."
                },
                "order": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string" },
                            "desc": { "type": "boolean" }
                        },
                        "required": ["id", "desc"],
                        "additionalProperties": false
                    },
                    "description": "Sort order entries."
                },
                "limit": {
                    "type": ["integer", "null"],
                    "description": "Row limit, or null for no limit."
                },
                "confidence": {
                    "type": "number",
                    "description": "How confident you are that these members are correct (0.0–1.0). Only set >= 0.85 when ALL measures and dimensions were confirmed by search_catalog."
                }
            },
            "required": ["measures", "dimensions", "filters", "time_dimensions", "order", "limit", "confidence"],
            "additionalProperties": false
        }),
        strict: false,
    }
}

/// Tools available during the **clarifying** state.
///
/// When `has_semantic` is `true` the semantic layer covers the data model and
/// raw database introspection tools (`list_tables`, `describe_table`) are
/// excluded to avoid confusing the LLM with two competing schema views.
pub fn clarifying_tools(has_semantic: bool) -> Vec<ToolDef> {
    let mut tools = vec![
        ToolDef {
            name: "search_catalog",
            description: SEARCH_CATALOG_DESC,
            parameters: json!({
                "type": "object",
                "properties": {
                    "queries": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "One or more search terms. Each term is matched against metric names and descriptions. Use [\"\"] to list everything."
                    }
                },
                "required": ["queries"],
                "additionalProperties": false
            }),
            ..Default::default()
        },
        ToolDef {
            name: "search_procedures",
            description: SEARCH_PROCEDURES_DESC,
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search term matched against procedure names and descriptions"
                    }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
            ..Default::default()
        },
    ];
    if !has_semantic {
        tools.push(list_tables_tool_def());
        tools.push(describe_table_tool_def());
    }
    tools
}

/// Tools available during the **specifying** state.
///
/// Includes `search_catalog` so Specifying can discover metrics/dimensions
/// directly from the raw question without a prior Ground phase.
///
/// When `has_semantic` is `true`, raw database tools (`list_tables`,
/// `describe_table`) are excluded — same rationale as [`clarifying_tools`].
pub fn specifying_tools(has_semantic: bool) -> Vec<ToolDef> {
    let mut tools = vec![
        ToolDef {
            name: "search_catalog",
            description: SEARCH_CATALOG_DESC,
            parameters: json!({
                "type": "object",
                "properties": {
                    "queries": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "One or more search terms matched against metric names and descriptions. Use [\"\"] to list everything."
                    }
                },
                "required": ["queries"],
                "additionalProperties": false
            }),
            ..Default::default()
        },
        ToolDef {
            name: "sample_columns",
            description: SAMPLE_COLUMNS_DESC,
            parameters: json!({
                "type": "object",
                "properties": {
                    "columns": {
                        "type": "array",
                        "description": "One or more columns to sample.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "table": {
                                    "type": "string",
                                    "description": "Semantic view name or database table name"
                                },
                                "column": {
                                    "type": "string",
                                    "description": "Dimension/measure name or database column name"
                                },
                                "search_term": {
                                    "type": ["string", "null"],
                                    "description": "Optional substring filter (LIKE '%term%'). Pass null when not searching."
                                }
                            },
                            "required": ["table", "column", "search_term"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["columns"],
                "additionalProperties": false
            }),
            ..Default::default()
        },
    ];
    if !has_semantic {
        // Without a semantic layer, the LLM needs manual join discovery and
        // raw schema introspection tools.
        tools.push(ToolDef {
            name: "get_join_path",
            description:
                "Return the join path between two entities: path expression and join type.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "from_entity": {
                        "type": "string",
                        "description": "Source table or entity name"
                    },
                    "to_entity": {
                        "type": "string",
                        "description": "Target table or entity name"
                    }
                },
                "required": ["from_entity", "to_entity"],
                "additionalProperties": false
            }),
            ..Default::default()
        });
        tools.push(list_tables_tool_def());
        tools.push(describe_table_tool_def());
    }
    tools
}

/// Tools available during the **solving** state.
pub fn solving_tools() -> Vec<ToolDef> {
    vec![ToolDef {
        name: "execute_preview",
        description: "Run a SQL query with a hard LIMIT 5 and return real columns and rows. \
                      Use this to verify joins and filters produce actual results before \
                      finalizing the SQL. Returns {ok, columns, rows, row_count} on success \
                      or {ok: false, error} on failure.",
        parameters: json!({
            "type": "object",
            "properties": {
                "sql": {
                    "type": "string",
                    "description": "The SQL query to preview"
                }
            },
            "required": ["sql"],
            "additionalProperties": false
        }),
        ..Default::default()
    }]
}

/// Tools available during the **interpreting** state.
pub fn interpreting_tools() -> Vec<ToolDef> {
    vec![ToolDef {
        name: "render_chart",
        description: "Render a chart or table from the query result. \
                      The data is already available from the executed query — \
                      only specify the chart type and which columns to use. \
                      Column names must exactly match the columns in the result set. \
                      Returns {ok: true} on success or {ok: false, errors: [...]} when a \
                      column name is wrong — fix and retry immediately. \
                      The chart is streamed to the client immediately when this tool is called. \
                      You may call it multiple times to produce multiple charts. \
                      When multiple result sets are available, use `result_index` to select which \
                      one to visualise (0-based, default 0).",
        parameters: json!({
            "type": "object",
            "properties": {
                "chart_type": {
                    "type": "string",
                    "enum": ["line_chart", "bar_chart", "pie_chart", "table"],
                    "description": "Chart variant to render"
                },
                "x": {
                    "type": ["string", "null"],
                    "description": "Column name for the x-axis. Required for line_chart and bar_chart. Use null for pie_chart and table."
                },
                "y": {
                    "type": ["string", "null"],
                    "description": "Column name for the y-axis / metric. Required for line_chart and bar_chart. Use null for pie_chart and table."
                },
                "series": {
                    "type": ["string", "null"],
                    "description": "Optional grouping column name to split data into multiple series \
        (line_chart / bar_chart only). When set, the data is grouped by this column's \
        distinct values and each group becomes a separate line or bar series in the chart. \
        For example, if x='month', y='revenue', series='region', the chart renders one \
        line/bar per region. Use null when there is no grouping column or for pie_chart/table."
                },
                "name": {
                    "type": ["string", "null"],
                    "description": "Category column name. Required for pie_chart. Use null for other chart types."
                },
                "value": {
                    "type": ["string", "null"],
                    "description": "Value column name. Required for pie_chart. Use null for other chart types."
                },
                "x_axis_label": {
                    "type": ["string", "null"],
                    "description": "Human-readable x-axis label (include units, e.g. 'Date', 'Revenue (USD)'). Use null to omit."
                },
                "y_axis_label": {
                    "type": ["string", "null"],
                    "description": "Human-readable y-axis label (include units, e.g. 'Sales ($)', 'Count'). Use null to omit."
                },
                "result_index": {
                    "type": ["integer", "null"],
                    "description": "Which result set to visualise (0-based). Use null to default to the first result set."
                },
                "title": {
                    "type": ["string", "null"],
                    "description": "Optional chart title. Use null to omit."
                }
            },
            "required": ["chart_type", "x", "y", "series", "name", "value", "x_axis_label", "y_axis_label", "result_index", "title"],
            "additionalProperties": false
        }),
        ..Default::default()
    }]
}

/// Derive a deterministic [`ChartConfig`] suggestion from the question type and
/// result columns.
///
/// Returns `None` for question types that do not benefit from a chart (e.g.
/// `SingleValue`, `GeneralInquiry`) or when there are fewer than two columns.
pub fn suggest_chart_config(
    question_type: &QuestionType,
    columns: &[String],
) -> Option<ChartConfig> {
    if columns.len() < 2 {
        return None;
    }
    match question_type {
        QuestionType::Trend => Some(ChartConfig {
            chart_type: "line_chart".to_string(),
            x: Some(columns[0].clone()),
            y: Some(columns[1].clone()),
            series: columns.get(2).cloned(),
            name: None,
            value: None,
            title: None,
            x_axis_label: None,
            y_axis_label: None,
        }),
        QuestionType::Comparison | QuestionType::Breakdown => Some(ChartConfig {
            chart_type: "bar_chart".to_string(),
            x: Some(columns[0].clone()),
            y: Some(columns[1].clone()),
            series: columns.get(2).cloned(),
            name: None,
            value: None,
            title: None,
            x_axis_label: None,
            y_axis_label: None,
        }),
        QuestionType::Distribution => Some(ChartConfig {
            chart_type: "bar_chart".to_string(),
            x: Some(columns[0].clone()),
            y: Some(columns[1].clone()),
            series: None,
            name: None,
            value: None,
            title: None,
            x_axis_label: None,
            y_axis_label: None,
        }),
        QuestionType::SingleValue | QuestionType::GeneralInquiry => None,
    }
}

pub(super) fn list_tables_tool_def() -> ToolDef {
    ToolDef {
        name: "list_tables",
        description: "List all tables available in the connected database(s). \
                      Use this when the semantic layer doesn't cover the data \
                      the user is asking about. Returns {tables: [{name, database}]}.",
        parameters: json!({
            "type": "object",
            "properties": {
                "database": {
                    "type": ["string", "null"],
                    "description": "Specific database/connector name. Use null to list from all databases."
                }
            },
            "required": ["database"],
            "additionalProperties": false
        }),
        ..Default::default()
    }
}

pub(super) fn describe_table_tool_def() -> ToolDef {
    ToolDef {
        name: "describe_table",
        description: "Get column names, data types, and sample values for a database table. \
                      Use this to understand table structure when the semantic layer doesn't \
                      have the information needed. \
                      Returns {table, columns: [{name, data_type, sample_values}]}.",
        parameters: json!({
            "type": "object",
            "properties": {
                "table": {
                    "type": "string",
                    "description": "Table name to describe"
                },
                "database": {
                    "type": ["string", "null"],
                    "description": "Connector name if multiple databases are configured. Use null for the default database."
                }
            },
            "required": ["table", "database"],
            "additionalProperties": false
        }),
        ..Default::default()
    }
}
