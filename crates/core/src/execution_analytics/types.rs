use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Execution type enum matching frontend ExecutionType
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionType {
    SemanticQuery,
    OmniQuery,
    SqlGenerated,
    Workflow,
    AgentTool,
}

impl ExecutionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExecutionType::SemanticQuery => "semantic_query",
            ExecutionType::OmniQuery => "omni_query",
            ExecutionType::SqlGenerated => "sql_generated",
            ExecutionType::Workflow => "workflow",
            ExecutionType::AgentTool => "agent_tool",
        }
    }

    pub fn is_verified(&self) -> bool {
        matches!(
            self,
            ExecutionType::SemanticQuery | ExecutionType::OmniQuery | ExecutionType::Workflow
        )
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "semantic_query" => Some(ExecutionType::SemanticQuery),
            "omni_query" => Some(ExecutionType::OmniQuery),
            "sql_generated" => Some(ExecutionType::SqlGenerated),
            "workflow" => Some(ExecutionType::Workflow),
            "agent_tool" => Some(ExecutionType::AgentTool),
            _ => None,
        }
    }
}

/// Source type (agent or workflow)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    Agent,
    Workflow,
}

impl SourceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            SourceType::Agent => "agent",
            SourceType::Workflow => "workflow",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "agent" => Some(SourceType::Agent),
            "workflow" => Some(SourceType::Workflow),
            _ => None,
        }
    }
}

/// Summary statistics for execution analytics
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionSummary {
    pub total_executions: u64,
    pub verified_count: u64,
    pub generated_count: u64,
    pub verified_percent: f64,
    pub generated_percent: f64,
    pub success_rate_verified: f64,
    pub success_rate_generated: f64,
    pub most_executed_type: String,
    // Breakdown by type
    pub semantic_query_count: u64,
    pub omni_query_count: u64,
    pub sql_generated_count: u64,
    pub workflow_count: u64,
    pub agent_tool_count: u64,
}

/// Time bucket for time series data
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionTimeBucket {
    pub timestamp: String,
    pub verified_count: u64,
    pub generated_count: u64,
    // Optional detailed breakdown
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_query_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub omni_query_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sql_generated_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_tool_count: Option<u64>,
}

/// Per-agent execution statistics
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentExecutionStats {
    pub agent_ref: String,
    pub total_executions: u64,
    pub verified_count: u64,
    pub generated_count: u64,
    pub verified_percent: f64,
    pub most_executed_type: String,
    pub success_rate: f64,
}

/// Detailed execution record
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionDetail {
    pub trace_id: String,
    pub span_id: String,
    pub timestamp: String,
    pub execution_type: String,
    pub is_verified: bool,
    // Source information
    pub source_type: String,
    pub source_ref: String,
    // Common fields
    pub status: String,
    pub duration_ms: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    // Type-specific fields
    // For semantic queries
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_query_params: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_sql: Option<String>,
    // For omni queries
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integration: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    // For SQL queries (verified)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sql: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sql_ref: Option<String>,
    // For SQL queries (generated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_question: Option<String>,
    // For workflows
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_ref: Option<String>,
    // For agent tools
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_input: Option<String>,
}

/// Paginated response for execution details
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionListResponse {
    pub executions: Vec<ExecutionDetail>,
    pub total: u64,
    pub limit: usize,
    pub offset: usize,
}
