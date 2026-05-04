//! JSON Schema definitions for structured LLM output.
//!
//! Each function returns a [`ResponseSchema`] that can be placed in
//! [`ToolLoopConfig::response_schema`] to enable provider-native constrained
//! decoding for the corresponding solver stage.

use serde_json::json;

use crate::llm::ResponseSchema;

/// Returns the [`ResponseSchema`] for the **Triage** sub-phase of Clarify.
///
/// The model must produce a JSON object matching
/// [`crate::types::DomainHypothesis`].
pub fn triage_response_schema() -> ResponseSchema {
    ResponseSchema {
        name: "triage_response".into(),
        schema: json!({
            "type": "object",
            "properties": {
                "summary": {
                    "type": "string",
                    "description": "One-sentence summary of what the user is asking about."
                },
                "question_type": {
                    "type": "string",
                    "description": "Broad question category. Trend: metric over time. Comparison: contrasting items/periods. Breakdown: metric split by category. SingleValue: one aggregate number. Distribution: spread or histogram. GeneralInquiry: question that does not need SQL — e.g. what tables are available, what metrics exist, or any conversational follow-up.",
                    "enum": ["Trend", "Comparison", "Breakdown", "SingleValue", "Distribution", "GeneralInquiry"]
                },
                "time_scope": {
                    "type": ["string", "null"],
                    "description": "Inferred time scope if any, e.g. 'last 30 days', 'this year'. Null when no time constraint is implied."
                },
                "confidence": {
                    "type": "number",
                    "description": "How confident you are in this interpretation, from 0.0 (pure guess) to 1.0 (unambiguous)."
                },
                "ambiguities": {
                    "type": "array",
                    "description": "Language-level ambiguities in the user's question that cannot be resolved without asking them — e.g. 'unclear which metric \"progress\" refers to' or 'time range is unspecified'. Empty array when the question is unambiguous. Do NOT list schema-level ambiguities (column mapping) here — those belong in Ground.",
                    "items": { "type": "string" }
                },
                "ambiguity_questions": {
                    "type": "array",
                    "description": "Structured version of ambiguities. For each ambiguity, provide a clear question to ask the user AND 2-4 concrete answer suggestions. Always populate this when ambiguities is non-empty. Each question must have specific, actionable suggestions the user can click.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "prompt": {
                                "type": "string",
                                "description": "The clarifying question to ask the user."
                            },
                            "suggestions": {
                                "type": "array",
                                "description": "2-4 concrete answer options for the user to choose from.",
                                "items": { "type": "string" }
                            }
                        },
                        "additionalProperties": false,
                        "required": ["prompt", "suggestions"]
                    }
                },
                "selected_procedure_path": {
                    "type": ["string", "null"],
                    "description": "If an available procedure/workflow/SQL file directly answers the question, set this to its exact path string (e.g. 'workflows/sales/report.procedure.yml' or 'example_sql/monthly_revenue.sql'). SQL files (.sql) are executed directly as verified queries and preferred over SQL generation. Set null when no match was found."
                },
                "missing_members": {
                    "type": "array",
                    "description": "Semantic members the question requires but that search_catalog could NOT find. Populate this when the catalog lacks a measure or dimension needed to fully answer the question. Empty array when all needed members exist in the catalog.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": {
                                "type": "string",
                                "description": "Suggested member name in snake_case (e.g. 'revenue_per_customer')."
                            },
                            "kind": {
                                "type": "string",
                                "description": "Whether this is a measure or dimension.",
                                "enum": ["measure", "dimension"]
                            },
                            "description": {
                                "type": "string",
                                "description": "Natural-language description of what this member should represent."
                            }
                        },
                        "additionalProperties": false,
                        "required": ["name", "kind", "description"]
                    }
                }
            },
            "additionalProperties": false,
            "required": [
                "summary",
                "question_type",
                "time_scope",
                "confidence",
                "ambiguities",
                "ambiguity_questions",
                "selected_procedure_path",
                "missing_members"
            ]
        }),
    }
}

/// Returns the [`ResponseSchema`] for the **Ground** sub-phase of Clarify
/// (formerly the full Clarify schema).
///
/// The model must produce a JSON object matching [`crate::types::AnalyticsIntent`]
/// (excluding `raw_question`, which is preserved from the input).
#[allow(dead_code)]
pub fn clarify_response_schema() -> ResponseSchema {
    ResponseSchema {
        name: "clarify_response".into(),
        schema: json!({
            "type": "object",
            "properties": {
                "question_type": {
                    "type": "string",
                    "description": "The type of analytical question. Trend: how a metric changes over time. Comparison: contrasting two or more items, groups, or periods. Breakdown: a metric split by a categorical dimension. SingleValue: one aggregate number with no grouping. Distribution: the spread, histogram, or frequency of a metric. GeneralInquiry: a question that does not need SQL — e.g. what data is available, what metrics exist, or any conversational follow-up.",
                    "enum": ["Trend", "Comparison", "Breakdown", "SingleValue", "Distribution", "GeneralInquiry"]
                },
                "metrics": {
                    "type": "array",
                    "description": "Exact metric names as returned by the search_catalog tool (the 'name' field). Do NOT use user-supplied terms or paraphrases — only names confirmed by the catalog.",
                    "items": { "type": "string" }
                },
                "dimensions": {
                    "type": "array",
                    "description": "Exact dimension names as returned by the search_catalog tool (the 'name' field in the dimensions array). Include time dimensions when the question implies a time axis. Do NOT invent dimension names.",
                    "items": { "type": "string" }
                },
                "filters": {
                    "type": "array",
                    "description": "Filter expressions using column names from the schema, e.g. \"date >= '2024-01-01'\", \"status = 'active'\". Extract explicit or implied constraints.",
                    "items": { "type": "string" }
                },
                "selected_procedure_path": {
                    "type": ["string", "null"],
                    "description": "MUST be set when search_procedures returned any matching procedure, workflow, or SQL file. Copy the exact 'path' string from the tool result (e.g. 'workflows/sales/report.procedure.yml' or 'example_sql/monthly_revenue.sql'). SQL files are executed directly as verified queries. Set null ONLY when search_procedures returned an empty list or was not called."
                }
            },
            "additionalProperties": false,
            "required": [
                "question_type",
                "metrics",
                "dimensions",
                "filters",
                "selected_procedure_path"
            ]
        }),
    }
}

/// Returns the [`ResponseSchema`] for the **Specify** stage (legacy).
///
/// Kept for backward compatibility; the new pipeline uses
/// [`specify_response_schema`] which produces airlayer-native QueryRequests.
pub fn specify_response_schema_legacy() -> ResponseSchema {
    let single_spec = json!({
        "type": "object",
        "properties": {
            "resolved_metrics": {
                "type": "array",
                "description": "SQL-level metric expressions, one per logical measure. E.g. [\"SUM(orders.amount)\", \"COUNT(*)\"].",
                "items": { "type": "string" }
            },
            "resolved_filters": {
                "type": "array",
                "description": "Filter expressions with fully-qualified table.column references, ready to embed in a WHERE clause. Resolve each raw filter from the intent to the exact column (e.g. \"orders.created_at >= '2024-01-01'\", \"orders.status = 'active'\"). Use sample_columns to verify filter values when needed. Return an empty array if there are no filters.",
                "items": { "type": "string" }
            },
            "resolved_tables": {
                "type": "array",
                "description": "All tables that must appear in the FROM clause.",
                "items": { "type": "string" }
            },
            "join_path": {
                "type": "array",
                "description": "Ordered list of [left_table, right_table, join_key] triples. Empty array if only one table is needed.",
                "items": {
                    "type": "array",
                    "items": { "type": "string" },
                    "minItems": 3,
                    "maxItems": 3
                }
            },
            "assumptions": {
                "type": "array",
                "description": "Any ambiguous resolutions or assumptions made during column/table resolution, so the user can review.",
                "items": { "type": "string" }
            }
        },
        "additionalProperties": false,
        "required": [
            "resolved_metrics",
            "resolved_filters",
            "resolved_tables",
            "join_path",
            "assumptions"
        ]
    });

    ResponseSchema {
        name: "specify_response".into(),
        schema: json!({
            "type": "object",
            "description": "Fan-out envelope. Usually one spec; multiple specs trigger independent solve/execute per spec followed by a merge.",
            "properties": {
                "specs": {
                    "type": "array",
                    "description": "One or more query specs. Return exactly one unless the intent requires completely independent queries.",
                    "items": single_spec,
                    "minItems": 1
                }
            },
            "additionalProperties": false,
            "required": ["specs"]
        }),
    }
}

/// Returns the [`ResponseSchema`] for the **Specify** stage (airlayer-native).
///
/// The model must produce a JSON object with a top-level `specs` array.
/// Each element mirrors an airlayer [`QueryRequest`]: measures, dimensions,
/// structured filters, time dimensions, ordering, and limit.  The orchestrator
/// compiles each spec via `engine.compile_query`; if compilation fails the
/// spec falls back to the LLM Solve stage with raw-schema context.
pub fn specify_response_schema() -> ResponseSchema {
    let filter_schema = json!({
        "type": "object",
        "description": "A single filter condition. Use view.member format for the member field.",
        "properties": {
            "member": {
                "type": "string",
                "description": "The member to filter on, in view.member format (e.g. 'orders.status', 'orders.order_date')."
            },
            "operator": {
                "type": "string",
                "description": "The filter operator to apply.",
                "enum": [
                    "equals", "notEquals",
                    "contains", "notContains",
                    "startsWith", "endsWith",
                    "gt", "gte", "lt", "lte",
                    "set", "notSet",
                    "inDateRange", "notInDateRange",
                    "beforeDate", "afterDate",
                    "beforeOrOnDate", "afterOrOnDate"
                ]
            },
            "values": {
                "type": "array",
                "description": "Filter values as strings. For 'set'/'notSet' use an empty array. For 'inDateRange' provide exactly 2 values [start, end]. For comparison operators provide 1 value.",
                "items": { "type": "string" }
            }
        },
        "additionalProperties": false,
        "required": ["member", "operator", "values"]
    });

    let time_dimension_schema = json!({
        "type": "object",
        "description": "A time dimension with optional granularity and date range. Use this for date-based grouping instead of putting date columns in 'dimensions'.",
        "properties": {
            "dimension": {
                "type": "string",
                "description": "The time dimension member in view.member format (e.g. 'orders.order_date')."
            },
            "granularity": {
                "description": "Time granularity for grouping. Use null to include the dimension without truncation.",
                "anyOf": [
                    {
                        "type": "string",
                        "enum": ["year", "quarter", "month", "week", "day", "hour", "minute", "second"]
                    },
                    { "type": "null" }
                ]
            },
            "date_range": {
                "type": ["array", "null"],
                "description": "Date range filter as [start, end] strings (e.g. ['2024-01-01', '2024-12-31']). Null if no date range constraint.",
                "items": { "type": "string" }
            }
        },
        "additionalProperties": false,
        "required": ["dimension", "granularity", "date_range"]
    });

    let order_schema = json!({
        "type": "object",
        "properties": {
            "id": {
                "type": "string",
                "description": "The member to order by, in view.member format."
            },
            "desc": {
                "type": "boolean",
                "description": "True for descending order, false for ascending."
            }
        },
        "additionalProperties": false,
        "required": ["id", "desc"]
    });

    let single_spec = json!({
        "type": "object",
        "properties": {
            "measures": {
                "type": "array",
                "description": "Measure members to aggregate, in view.measure format as returned by search_catalog (e.g. ['orders.total_revenue', 'orders.count']). Do NOT write SQL expressions — use the exact semantic names.",
                "items": { "type": "string" }
            },
            "dimensions": {
                "type": "array",
                "description": "Non-time dimension members to group by, in view.dimension format (e.g. ['orders.status', 'customers.region']). Do NOT include time/date dimensions here — use time_dimensions instead.",
                "items": { "type": "string" }
            },
            "filters": {
                "type": "array",
                "description": "Structured filter conditions. Each filter has a member (view.member), operator, and values. Use sample_columns to verify exact value formats before specifying filter values.",
                "items": filter_schema
            },
            "time_dimensions": {
                "type": "array",
                "description": "Time dimensions with granularity and optional date range. Use this for any date-based grouping or filtering. Use sample_columns on the date dimension to choose appropriate granularity (>365 distinct dates → month, >90 → week, otherwise day).",
                "items": time_dimension_schema
            },
            "order": {
                "type": "array",
                "description": "Sort order. Each entry specifies a member and direction. Empty array for default ordering.",
                "items": order_schema
            },
            "limit": {
                "type": ["integer", "null"],
                "description": "Maximum number of rows to return. Null for no limit. Use when the user asks for top-N results."
            },
            "assumptions": {
                "type": "array",
                "description": "Any ambiguous resolutions or assumptions made, so the user can review.",
                "items": { "type": "string" }
            }
        },
        "additionalProperties": false,
        "required": [
            "measures",
            "dimensions",
            "filters",
            "time_dimensions",
            "order",
            "limit",
            "assumptions"
        ]
    });

    ResponseSchema {
        name: "specify_response".into(),
        schema: json!({
            "type": "object",
            "description": "Fan-out envelope. Usually one spec; multiple specs trigger independent compile/execute per spec followed by a merge.",
            "properties": {
                "specs": {
                    "type": "array",
                    "description": "One or more query request specs. Return exactly one unless the intent requires completely independent queries (different views, incompatible shapes).",
                    "items": single_spec,
                    "minItems": 1
                }
            },
            "additionalProperties": false,
            "required": ["specs"]
        }),
    }
}

/// Returns the [`ResponseSchema`] for the **Solve** stage.
///
/// The model must produce a JSON object containing a single `sql` field with
/// the generated SQL query.
pub fn solve_response_schema() -> ResponseSchema {
    ResponseSchema {
        name: "solve_response".into(),
        schema: json!({
            "type": "object",
            "properties": {
                "sql": { "type": "string" }
            },
            "additionalProperties": false,
            "required": ["sql"]
        }),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use agentic_llm::validate_openai_strict_schema;

    // ── OpenAI strict-mode compliance ─────────────────────────────────────────

    /// Response schemas are sent as structured-output tools with `"strict": true`.
    /// Every property in every object must be in `required`; optional fields
    /// use nullable types.  This test catches regressions before they hit the
    /// API as a 400.
    #[test]
    fn all_response_schemas_are_openai_strict_compatible() {
        let schemas = [
            triage_response_schema(),
            clarify_response_schema(),
            specify_response_schema(),
            specify_response_schema_legacy(),
            solve_response_schema(),
        ];

        for rs in &schemas {
            let violations = validate_openai_strict_schema(&rs.schema, &rs.name);
            assert!(
                violations.is_empty(),
                "response schema '{}' violates OpenAI strict mode:\n  {}",
                rs.name,
                violations.join("\n  ")
            );
        }
    }

    #[test]
    fn clarify_schema_has_required_fields() {
        let schema = clarify_response_schema();
        assert_eq!(schema.name, "clarify_response");
        let required = schema.schema["required"].as_array().unwrap();
        let fields: Vec<&str> = required.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(fields.contains(&"question_type"));
        assert!(fields.contains(&"metrics"));
        assert!(fields.contains(&"dimensions"));
        assert!(fields.contains(&"filters"));
    }

    #[test]
    fn clarify_schema_question_type_has_all_variants() {
        let schema = clarify_response_schema();
        let enum_vals = schema.schema["properties"]["question_type"]["enum"]
            .as_array()
            .unwrap();
        assert_eq!(enum_vals.len(), 6);
        let variants: Vec<&str> = enum_vals.iter().map(|v| v.as_str().unwrap()).collect();
        for v in &[
            "Trend",
            "Comparison",
            "Breakdown",
            "SingleValue",
            "Distribution",
            "GeneralInquiry",
        ] {
            assert!(variants.contains(v), "missing variant: {v}");
        }
    }

    #[test]
    fn specify_schema_has_required_fields() {
        let schema = specify_response_schema();
        assert_eq!(schema.name, "specify_response");
        // Top-level envelope just requires "specs".
        let top_required = schema.schema["required"].as_array().unwrap();
        let top_fields: Vec<&str> = top_required.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(
            top_fields.contains(&"specs"),
            "envelope must require 'specs'"
        );
        // The spec item schema must require the airlayer-native fields.
        let item_required = schema.schema["properties"]["specs"]["items"]["required"]
            .as_array()
            .unwrap();
        let item_fields: Vec<&str> = item_required.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(item_fields.contains(&"measures"));
        assert!(item_fields.contains(&"dimensions"));
        assert!(item_fields.contains(&"filters"));
        assert!(item_fields.contains(&"time_dimensions"));
        assert!(item_fields.contains(&"order"));
        assert!(item_fields.contains(&"limit"));
        assert!(item_fields.contains(&"assumptions"));
    }

    #[test]
    fn specify_legacy_schema_has_required_fields() {
        let schema = specify_response_schema_legacy();
        assert_eq!(schema.name, "specify_response");
        let item_required = schema.schema["properties"]["specs"]["items"]["required"]
            .as_array()
            .unwrap();
        let item_fields: Vec<&str> = item_required.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(item_fields.contains(&"resolved_metrics"));
        assert!(item_fields.contains(&"resolved_tables"));
        assert!(item_fields.contains(&"join_path"));
        assert!(item_fields.contains(&"assumptions"));
    }

    #[test]
    fn solve_schema_has_sql_field() {
        let schema = solve_response_schema();
        assert_eq!(schema.name, "solve_response");
        let required = schema.schema["required"].as_array().unwrap();
        let fields: Vec<&str> = required.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(
            fields.contains(&"sql"),
            "solve_response schema must require 'sql'"
        );
        assert_eq!(schema.schema["properties"]["sql"]["type"], "string");
    }

    #[test]
    fn triage_schema_has_required_fields() {
        let schema = triage_response_schema();
        assert_eq!(schema.name, "triage_response");
        let required = schema.schema["required"].as_array().unwrap();
        let fields: Vec<&str> = required.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(fields.contains(&"summary"));
        assert!(fields.contains(&"question_type"));
        assert!(fields.contains(&"confidence"));
        assert!(fields.contains(&"ambiguities"));
    }

    #[test]
    fn triage_schema_question_type_has_all_variants() {
        let schema = triage_response_schema();
        let enum_vals = schema.schema["properties"]["question_type"]["enum"]
            .as_array()
            .unwrap();
        assert_eq!(enum_vals.len(), 6);
        let variants: Vec<&str> = enum_vals.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(
            variants.contains(&"GeneralInquiry"),
            "missing GeneralInquiry variant"
        );
    }

    #[test]
    fn triage_schema_includes_missing_members() {
        let schema = triage_response_schema();
        let required = schema.schema["required"].as_array().unwrap();
        let fields: Vec<&str> = required.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(
            fields.contains(&"missing_members"),
            "triage schema must require 'missing_members'"
        );

        let mm = &schema.schema["properties"]["missing_members"];
        assert_eq!(mm["type"], "array");
        let item_required = mm["items"]["required"].as_array().unwrap();
        let item_fields: Vec<&str> = item_required.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(item_fields.contains(&"name"));
        assert!(item_fields.contains(&"kind"));
        assert!(item_fields.contains(&"description"));
    }

    #[test]
    fn all_schemas_have_object_type() {
        for schema in [
            triage_response_schema(),
            clarify_response_schema(),
            specify_response_schema(),
            solve_response_schema(),
        ] {
            assert_eq!(
                schema.schema["type"], "object",
                "schema '{}' must be type:object",
                schema.name
            );
        }
    }
}
