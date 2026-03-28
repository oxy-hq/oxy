//! JSON schema helpers for LLM structured output.

use agentic_llm::ResponseSchema;
use serde_json::json;

pub fn triage_response_schema() -> ResponseSchema {
    ResponseSchema {
        name: "triage_response".to_string(),
        schema: json!({
            "type": "object",
            "properties": {
                "app_name": { "type": "string" },
                "description": { "type": "string" },
                "desired_metrics": { "type": "array", "items": { "type": "string" } },
                "desired_controls": { "type": "array", "items": { "type": "string" } },
                "mentioned_tables": { "type": "array", "items": { "type": "string" } },
                "ambiguities": { "type": "array", "items": { "type": "string" } },
                "key_findings": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Brief bullet-points of schema findings from tool exploration (column structures, value samples, join paths). Empty during triage."
                }
            },
            "required": ["app_name", "description", "desired_metrics", "desired_controls", "mentioned_tables", "ambiguities", "key_findings"]
        }),
    }
}

pub fn specify_response_schema() -> ResponseSchema {
    ResponseSchema {
        name: "specify_response".to_string(),
        schema: json!({
            "type": "object",
            "properties": {
                "app_name": { "type": "string" },
                "tasks": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string" },
                            "description": { "type": "string" },
                            "control_deps": {
                                "anyOf": [
                                    { "type": "array", "items": { "type": "string" } },
                                    { "type": "null" }
                                ],
                                "description": "Control names this task depends on. Use null if none."
                            },
                            "is_control_source": { "type": "boolean" }
                        },
                        "required": ["name", "description", "control_deps", "is_control_source"]
                    }
                },
                "controls": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string" },
                            "label": { "type": "string" },
                            "control_type": { "type": "string", "enum": ["select", "date", "toggle"] },
                            "source_task": {
                                "anyOf": [{ "type": "string" }, { "type": "null" }],
                                "description": "Task that populates this control's options. Use null if not applicable."
                            },
                            "options": {
                                "type": "array",
                                "items": { "type": "string" },
                                "description": "Static option values for select controls without a source_task. Must include the default value. Use empty array if source_task is set."
                            },
                            "default": { "type": "string" }
                        },
                        "required": ["name", "label", "control_type", "source_task", "options", "default"]
                    }
                },
                "layout": {
                    "type": "array",
                    "description": "Ordered list of layout nodes that define the app's visual structure.",
                    "items": {
                        "anyOf": [
                            {
                                "type": "object",
                                "description": "Chart node — renders a chart for the named task",
                                "properties": {
                                    "type": { "type": "string", "enum": ["chart"] },
                                    "task": { "type": "string", "description": "Task name to visualize" },
                                    "preferred": { "type": "string", "enum": ["bar", "line", "pie", "table", "auto"] }
                                },
                                "required": ["type", "task", "preferred"]
                            },
                            {
                                "type": "object",
                                "description": "Table node — renders a data table for the named task",
                                "properties": {
                                    "type": { "type": "string", "enum": ["table"] },
                                    "task": { "type": "string" },
                                    "title": {
                                        "anyOf": [{ "type": "string" }, { "type": "null" }],
                                        "description": "Optional table title. Use null to omit."
                                    }
                                },
                                "required": ["type", "task", "title"]
                            },
                            {
                                "type": "object",
                                "description": "Row node — arranges children side-by-side in a multi-column row",
                                "properties": {
                                    "type": { "type": "string", "enum": ["row"] },
                                    "columns": { "type": "integer", "description": "Number of columns, e.g. 2 for side-by-side" },
                                    "children": {
                                        "type": "array",
                                        "description": "Leaf layout nodes inside this row (chart, table, or markdown — no nested rows)",
                                        "items": {
                                            "anyOf": [
                                                {
                                                    "type": "object",
                                                    "properties": {
                                                        "type": { "type": "string", "enum": ["chart"] },
                                                        "task": { "type": "string" },
                                                        "preferred": { "type": "string", "enum": ["bar", "line", "pie", "table", "auto"] }
                                                    },
                                                    "required": ["type", "task", "preferred"]
                                                },
                                                {
                                                    "type": "object",
                                                    "properties": {
                                                        "type": { "type": "string", "enum": ["table"] },
                                                        "task": { "type": "string" },
                                                        "title": {
                                                            "anyOf": [{ "type": "string" }, { "type": "null" }]
                                                        }
                                                    },
                                                    "required": ["type", "task", "title"]
                                                },
                                                {
                                                    "type": "object",
                                                    "properties": {
                                                        "type": { "type": "string", "enum": ["markdown"] },
                                                        "content": { "type": "string" }
                                                    },
                                                    "required": ["type", "content"]
                                                },
                                                {
                                                    "type": "object",
                                                    "properties": {
                                                        "type": { "type": "string", "enum": ["insight"] },
                                                        "tasks": { "type": "array", "items": { "type": "string" } },
                                                        "focus": {
                                                            "anyOf": [{ "type": "string" }, { "type": "null" }]
                                                        }
                                                    },
                                                    "required": ["type", "tasks", "focus"]
                                                }
                                            ]
                                        }
                                    }
                                },
                                "required": ["type", "columns", "children"]
                            },
                            {
                                "type": "object",
                                "description": "Markdown node — renders static markdown text",
                                "properties": {
                                    "type": { "type": "string", "enum": ["markdown"] },
                                    "content": { "type": "string" }
                                },
                                "required": ["type", "content"]
                            },
                            {
                                "type": "object",
                                "description": "Insight node — generates data-driven markdown insights from task results during interpreting",
                                "properties": {
                                    "type": { "type": "string", "enum": ["insight"] },
                                    "tasks": {
                                        "type": "array",
                                        "items": { "type": "string" },
                                        "description": "Task names whose results will be analyzed for insights"
                                    },
                                    "focus": {
                                        "anyOf": [{ "type": "string" }, { "type": "null" }],
                                        "description": "Optional focus hint: trends, comparison, summary, outliers, highlights. Null for general insight."
                                    }
                                },
                                "required": ["type", "tasks", "focus"]
                            }
                        ]
                    }
                }
            },
            "required": ["app_name", "tasks", "controls", "layout"]
        }),
    }
}

pub fn solve_response_schema() -> ResponseSchema {
    ResponseSchema {
        name: "solve_response".to_string(),
        schema: json!({
            "type": "object",
            "properties": {
                "sql": { "type": "string" }
            },
            "required": ["sql"]
        }),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use agentic_llm::validate_openai_strict_schema;

    /// All response schemas are sent as structured-output tools with `"strict": true`.
    /// Every property in every object must be in `required`; optional fields
    /// must use `anyOf` with null.  This test catches regressions before they
    /// reach the API as a 400.
    #[test]
    fn all_response_schemas_are_openai_strict_compatible() {
        let schemas = [
            triage_response_schema(),
            specify_response_schema(),
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
    fn specify_schema_layout_items_use_anyof() {
        let schema = specify_response_schema();
        let items = &schema.schema["properties"]["layout"]["items"];
        assert!(
            items.get("anyOf").is_some(),
            "layout items must use anyOf to enumerate LayoutNode variants"
        );
        let variants: Vec<&str> = items["anyOf"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v["properties"]["type"]["enum"][0].as_str())
            .collect();
        for expected in &["chart", "table", "row", "markdown", "insight"] {
            assert!(
                variants.contains(expected),
                "layout anyOf is missing the '{expected}' variant"
            );
        }
    }

    #[test]
    fn specify_schema_tasks_and_controls_are_fully_required() {
        let schema = specify_response_schema();

        let task_required: Vec<&str> = schema.schema["properties"]["tasks"]["items"]["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        for field in &["name", "description", "control_deps", "is_control_source"] {
            assert!(
                task_required.contains(field),
                "tasks.items missing required field '{field}'"
            );
        }

        let ctrl_required: Vec<&str> = schema.schema["properties"]["controls"]["items"]["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        for field in &[
            "name",
            "label",
            "control_type",
            "source_task",
            "options",
            "default",
        ] {
            assert!(
                ctrl_required.contains(field),
                "controls.items missing required field '{field}'"
            );
        }
    }
}
