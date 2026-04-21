//! `BuilderSchemaProvider` implementation using oxy config types + embedded semantic schemas.
//!
//! Semantic type schemas (Dimension, View, Topic, etc.) are pre-generated from
//! `oxy-semantic` and embedded as static JSON. Non-semantic schemas (AgentConfig,
//! Workflow, etc.) are still generated at runtime from `oxy::config::model`.

use agentic_builder::BuilderSchemaProvider;
use schemars::schema_for;

const SUPPORTED_TYPES: &[&str] = &[
    // Semantic (embedded)
    "Dimension",
    "DimensionType",
    "Measure",
    "MeasureType",
    "MeasureFilter",
    "View",
    "Topic",
    "Entity",
    "SemanticLayer",
    // Agent
    "AgentConfig",
    "AgentType",
    "AgentToolsConfig",
    "AgentContext",
    "AgentContextType",
    "RouteRetrievalConfig",
    "ReasoningConfig",
    "ToolType",
    // Agentic/FSM
    "AgenticConfig",
    // Workflow
    "Workflow",
    "Task",
    "TaskType",
    "AgentTask",
    "ExecuteSQLTask",
    "SemanticQueryTask",
    "FormatterTask",
    "WorkflowTask",
    "VisualizeTask",
    "LoopSequentialTask",
    "ConditionalTask",
    "EvalConfig",
    // App
    "AppConfig",
    "Display",
    "MarkdownDisplay",
    "LineChartDisplay",
    "BarChartDisplay",
    "PieChartDisplay",
    "TableDisplay",
    // Config
    "Config",
    "Database",
    "DatabaseType",
    // Test
    "TestFileConfig",
    "TestSettings",
    "TestCase",
];

/// Schema provider that uses embedded JSON for semantic types and runtime
/// `schema_for!()` on oxy config types.
pub struct OxyBuilderSchemaProvider;

impl OxyBuilderSchemaProvider {
    pub fn new() -> Self {
        Self
    }
}

impl BuilderSchemaProvider for OxyBuilderSchemaProvider {
    fn get_schema(&self, object_name: &str) -> Option<serde_json::Value> {
        // Semantic types — pre-generated, embedded as static JSON.
        let embedded = match object_name {
            "Dimension" => Some(include_str!("schemas/Dimension.json")),
            "DimensionType" => Some(include_str!("schemas/DimensionType.json")),
            "Measure" => Some(include_str!("schemas/Measure.json")),
            "MeasureType" => Some(include_str!("schemas/MeasureType.json")),
            "MeasureFilter" => Some(include_str!("schemas/MeasureFilter.json")),
            "View" => Some(include_str!("schemas/View.json")),
            "Topic" => Some(include_str!("schemas/Topic.json")),
            "Entity" | "SemanticEntity" => Some(include_str!("schemas/Entity.json")),
            "SemanticLayer" => Some(include_str!("schemas/SemanticLayer.json")),
            _ => None,
        };
        if let Some(json_str) = embedded {
            return serde_json::from_str(json_str).ok();
        }

        // Non-semantic types — generated at runtime from oxy config types.
        use oxy::config::agent_config as aw;
        use oxy::config::model as cfg;

        let schema = match object_name {
            // Agent
            "AgentConfig" => serde_json::to_value(schema_for!(cfg::AgentConfig)),
            "AgentType" => serde_json::to_value(schema_for!(cfg::AgentType)),
            "AgentToolsConfig" => serde_json::to_value(schema_for!(cfg::AgentToolsConfig)),
            "AgentContext" => serde_json::to_value(schema_for!(cfg::AgentContext)),
            "AgentContextType" => serde_json::to_value(schema_for!(cfg::AgentContextType)),
            "RouteRetrievalConfig" => serde_json::to_value(schema_for!(cfg::RouteRetrievalConfig)),
            "ReasoningConfig" => serde_json::to_value(schema_for!(cfg::ReasoningConfig)),
            "ToolType" => serde_json::to_value(schema_for!(cfg::ToolType)),

            // Agentic/FSM
            "AgenticConfig" => serde_json::to_value(schema_for!(aw::AgenticConfig)),

            // Workflow
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

            // App
            "AppConfig" => serde_json::to_value(schema_for!(cfg::AppConfig)),
            "Display" => serde_json::to_value(schema_for!(cfg::Display)),
            "MarkdownDisplay" => serde_json::to_value(schema_for!(cfg::MarkdownDisplay)),
            "LineChartDisplay" => serde_json::to_value(schema_for!(cfg::LineChartDisplay)),
            "BarChartDisplay" => serde_json::to_value(schema_for!(cfg::BarChartDisplay)),
            "PieChartDisplay" => serde_json::to_value(schema_for!(cfg::PieChartDisplay)),
            "TableDisplay" => serde_json::to_value(schema_for!(cfg::TableDisplay)),

            // Config
            "Config" => serde_json::to_value(schema_for!(cfg::Config)),
            "Database" => serde_json::to_value(schema_for!(cfg::Database)),
            "DatabaseType" => serde_json::to_value(schema_for!(cfg::DatabaseType)),

            // Test
            "TestFileConfig" => {
                serde_json::to_value(schema_for!(oxy::config::test_config::TestFileConfig))
            }
            "TestSettings" => {
                serde_json::to_value(schema_for!(oxy::config::test_config::TestSettings))
            }
            "TestCase" => serde_json::to_value(schema_for!(oxy::config::test_config::TestCase)),

            _ => return None,
        };

        schema.ok()
    }

    fn supported_types(&self) -> &[&str] {
        SUPPORTED_TYPES
    }
}
