use agentic_core::tools::{ToolDef, ToolError};
use serde_json::{Value, json};

pub fn lookup_schema_def() -> ToolDef {
    ToolDef {
        name: "lookup_schema",
        description: "Look up the JSON schema for a named Oxy object type. Returns the full JSON Schema describing all fields, types, and constraints. \
Semantic: Dimension, DimensionType, Measure, MeasureType, MeasureFilter, View, Topic, Entity, SemanticLayer. \
Agent (.agent.yml): AgentConfig, AgentType, AgentToolsConfig, AgentContext, AgentContextType, RouteRetrievalConfig, ReasoningConfig, ToolType. \
Agentic/FSM (.aw.yml): AgenticConfig. \
Workflow (.workflow.yml): Workflow, Task, TaskType, AgentTask, ExecuteSQLTask, SemanticQueryTask, FormatterTask, WorkflowTask, VisualizeTask, LoopSequentialTask, ConditionalTask, EvalConfig. \
App (.app.yml): AppConfig, Display, MarkdownDisplay, LineChartDisplay, BarChartDisplay, PieChartDisplay, TableDisplay. \
Test (.test.yml): TestFileConfig, TestSettings, TestCase. \
Config (config.yml): Config, Database, DatabaseType.",
        parameters: json!({
            "type": "object",
            "properties": {
                "object_name": {
                    "type": "string",
                    "description": "Name of the Oxy object type to look up (e.g. 'AgentConfig', 'Workflow', 'Task', 'View', 'Dimension')"
                }
            },
            "required": ["object_name"],
            "additionalProperties": false
        }),
    }
}

pub fn execute_lookup_schema(params: &Value) -> Result<Value, ToolError> {
    use oxy::config::agent_config as aw;
    use oxy::config::model as cfg;
    use schemars::schema_for;

    let object_name = params["object_name"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'object_name'".into()))?;

    let schema = match object_name {
        // ── Semantic layer ────────────────────────────────────────────────
        "Dimension" => serde_json::to_value(schema_for!(oxy_semantic::models::Dimension)),
        "DimensionType" => serde_json::to_value(schema_for!(oxy_semantic::models::DimensionType)),
        "Measure" => serde_json::to_value(schema_for!(oxy_semantic::models::Measure)),
        "MeasureType" => serde_json::to_value(schema_for!(oxy_semantic::models::MeasureType)),
        "MeasureFilter" => serde_json::to_value(schema_for!(oxy_semantic::models::MeasureFilter)),
        "View" => serde_json::to_value(schema_for!(oxy_semantic::models::View)),
        "Topic" => serde_json::to_value(schema_for!(oxy_semantic::models::Topic)),
        "Entity" | "SemanticEntity" => {
            serde_json::to_value(schema_for!(oxy_semantic::models::Entity))
        }
        "SemanticLayer" => serde_json::to_value(schema_for!(oxy_semantic::models::SemanticLayer)),

        // ── Agent (.agent.yml) ────────────────────────────────────────────
        "AgentConfig" => serde_json::to_value(schema_for!(cfg::AgentConfig)),
        "AgentType" => serde_json::to_value(schema_for!(cfg::AgentType)),
        "AgentToolsConfig" => serde_json::to_value(schema_for!(cfg::AgentToolsConfig)),
        "AgentContext" => serde_json::to_value(schema_for!(cfg::AgentContext)),
        "AgentContextType" => serde_json::to_value(schema_for!(cfg::AgentContextType)),
        "RouteRetrievalConfig" => serde_json::to_value(schema_for!(cfg::RouteRetrievalConfig)),
        "ReasoningConfig" => serde_json::to_value(schema_for!(cfg::ReasoningConfig)),
        "ToolType" => serde_json::to_value(schema_for!(cfg::ToolType)),

        // ── Agentic workflow / FSM (.aw.yml) ──────────────────────────────
        "AgenticConfig" => serde_json::to_value(schema_for!(aw::AgenticConfig)),

        // ── Workflow (.workflow.yml / .procedure.yml) ─────────────────────
        "Workflow" => serde_json::to_value(schema_for!(cfg::Workflow)),
        "Task" => serde_json::to_value(schema_for!(cfg::Task)),
        "TaskType" => serde_json::to_value(schema_for!(cfg::TaskType)),
        "AgentTask" => serde_json::to_value(schema_for!(cfg::AgentTask)),
        "ExecuteSQLTask" => serde_json::to_value(schema_for!(cfg::ExecuteSQLTask)),
        "SemanticQueryTask" => serde_json::to_value(schema_for!(cfg::SemanticQueryTask)),
        "FormatterTask" => serde_json::to_value(schema_for!(cfg::FormatterTask)),
        "WorkflowTask" => serde_json::to_value(schema_for!(cfg::WorkflowTask)),
        "VisualizeTask" => serde_json::to_value(schema_for!(cfg::VisualizeTask)),
        "LoopSequentialTask" => serde_json::to_value(schema_for!(cfg::LoopSequentialTask)),
        "ConditionalTask" => serde_json::to_value(schema_for!(cfg::ConditionalTask)),
        "EvalConfig" => serde_json::to_value(schema_for!(cfg::EvalConfig)),

        // ── App (.app.yml) ────────────────────────────────────────────────
        "AppConfig" => serde_json::to_value(schema_for!(cfg::AppConfig)),
        "Display" => serde_json::to_value(schema_for!(cfg::Display)),
        "MarkdownDisplay" => serde_json::to_value(schema_for!(cfg::MarkdownDisplay)),
        "LineChartDisplay" => serde_json::to_value(schema_for!(cfg::LineChartDisplay)),
        "BarChartDisplay" => serde_json::to_value(schema_for!(cfg::BarChartDisplay)),
        "PieChartDisplay" => serde_json::to_value(schema_for!(cfg::PieChartDisplay)),
        "TableDisplay" => serde_json::to_value(schema_for!(cfg::TableDisplay)),

        // ── Config (config.yml) ───────────────────────────────────────────
        "Config" => serde_json::to_value(schema_for!(cfg::Config)),
        "Database" => serde_json::to_value(schema_for!(cfg::Database)),
        "DatabaseType" => serde_json::to_value(schema_for!(cfg::DatabaseType)),

        // ── Test (.test.yml) ──────────────────────────────────────────────
        "TestFileConfig" => {
            serde_json::to_value(schema_for!(oxy::config::test_config::TestFileConfig))
        }
        "TestSettings" => {
            serde_json::to_value(schema_for!(oxy::config::test_config::TestSettings))
        }
        "TestCase" => serde_json::to_value(schema_for!(oxy::config::test_config::TestCase)),

        other => {
            return Err(ToolError::BadParams(format!(
                "unknown object type '{other}'. Supported types — \
                Semantic: Dimension, DimensionType, Measure, MeasureType, MeasureFilter, View, Topic, Entity, SemanticLayer; \
                Agent: AgentConfig, AgentType, AgentToolsConfig, AgentContext, AgentContextType, RouteRetrievalConfig, ReasoningConfig, ToolType; \
                Agentic/FSM: AgenticConfig; \
                Workflow: Workflow, Task, TaskType, AgentTask, ExecuteSQLTask, SemanticQueryTask, FormatterTask, WorkflowTask, VisualizeTask, LoopSequentialTask, ConditionalTask, EvalConfig; \
                App: AppConfig, Display, MarkdownDisplay, LineChartDisplay, BarChartDisplay, PieChartDisplay, TableDisplay; \
                Test: TestFileConfig, TestSettings, TestCase; \
                Config: Config, Database, DatabaseType"
            )));
        }
    }
    .map_err(|e| ToolError::Execution(format!("failed to serialize schema: {e}")))?;

    Ok(json!({ "object_name": object_name, "schema": schema }))
}
