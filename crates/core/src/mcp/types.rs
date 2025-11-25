// Standard library imports
use std::collections::HashMap;
use std::sync::Arc;

// External crate imports
use rmcp::model::Tool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// Internal crate imports
use crate::{adapters::project::manager::ProjectManager, service::types::SemanticQueryFilter};

use super::executor::ToolExecutor;

// =============================================================================
// Constants
// =============================================================================

pub const AGENT_TOOL_PREFIX: &str = "agent-";
pub const WORKFLOW_TOOL_PREFIX: &str = "workflow-";
pub const SEMANTIC_TOOL_PREFIX: &str = "semantic-";
pub const SQL_TOOL_PREFIX: &str = "sql-";

pub const EVENT_CHANNEL_SIZE: usize = 100;

// =============================================================================
// Type Definitions
// =============================================================================

#[derive(Debug, Clone)]
pub enum ToolType {
    Agent,
    Workflow,
    SemanticTopic,
    SqlFile,
}

impl ToolType {
    /// Returns the executor for this tool type
    pub fn executor(&self) -> Arc<dyn ToolExecutor> {
        match self {
            ToolType::Agent => Arc::new(super::executor::AgentExecutor),
            ToolType::Workflow => Arc::new(super::executor::WorkflowExecutor),
            ToolType::SemanticTopic => Arc::new(super::executor::SemanticExecutor),
            ToolType::SqlFile => Arc::new(super::executor::SqlExecutor),
        }
    }
}

#[derive(Debug, Clone)]
pub struct OxyTool {
    pub tool: Tool,
    pub tool_type: ToolType,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct OxyMcpServer {
    pub project_manager: ProjectManager,
    pub tools: HashMap<String, OxyTool>,
}

// =============================================================================
// Input Schemas
// =============================================================================

/// Input schema for agent tools
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentToolInput {
    /// Question to ask the agent
    pub question: String,
}

/// Input schema for SQL file tools
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SqlFileToolInput {
    /// Database connection to use (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
}

/// Input schema for semantic topic tools
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SemanticTopicToolInput {
    /// Dimensions to group by (e.g., column names from the views in this topic)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "List of dimensions to include in the query. Format: <view_name>.<dimension_name>"
    )]
    pub dimensions: Option<Vec<String>>,

    /// Measures to calculate (e.g., measures defined in the views)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "List of measures to include in the query. Format: <view_name>.<measure_name>"
    )]
    pub measures: Option<Vec<String>>,

    /// Filters to apply to the query
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filters: Option<Vec<SemanticQueryFilter>>,

    /// Maximum number of rows to return
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Maximum number of rows to return in the query results")]
    pub limit: Option<u64>,

    /// Sort order for the results
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_by: Option<Vec<crate::service::types::SemanticQueryOrder>>,

    /// Variables to substitute in the query
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Optional variables maybe required to render some semantic queries.")]
    pub variables: Option<HashMap<String, Value>>,
}
