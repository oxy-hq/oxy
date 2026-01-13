use std::{collections::HashMap, path::PathBuf};

use serde::{Deserialize, Serialize};
use utoipa::{
    ToSchema,
    openapi::{RefOr, Schema},
};

use crate::execute::types::{Usage, VizParams};

use super::{Chunk, ProgressType, ReferenceKind};

/// Represents the type of sandbox application
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema, PartialEq, Eq)]
#[serde(tag = "type", content = "metadata")]
pub enum SandboxAppKind {
    V0 { chat_id: String },
}

impl std::fmt::Display for SandboxAppKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SandboxAppKind::V0 { .. } => write!(f, "v0"),
        }
    }
}

/// Contains sandbox reference information
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema, PartialEq, Eq)]
pub struct SandboxInfo {
    #[serde(flatten)]
    pub kind: SandboxAppKind,
    pub preview_url: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct DataApp {
    #[schema(schema_with = any_schema)]
    pub file_path: PathBuf,
}
fn any_schema() -> impl Into<RefOr<Schema>> {
    RefOr::T(Schema::default())
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(tag = "type", content = "value")]
pub enum ArtifactKind {
    #[serde(rename = "workflow")]
    Workflow { r#ref: String },
    #[serde(rename = "agent")]
    Agent { r#ref: String },
    #[serde(rename = "execute_sql")]
    ExecuteSQL { sql: String, database: String },
    #[serde(rename = "semantic_query")]
    SemanticQuery {},
    #[serde(rename = "omni_query")]
    OmniQuery { topic: String, integration: String },
    #[serde(rename = "sandbox_app")]
    SandboxApp {
        #[serde(flatten)]
        kind: SandboxAppKind,
    },
}

impl std::fmt::Display for ArtifactKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArtifactKind::Workflow { r#ref: _ } => write!(f, "workflow"),
            ArtifactKind::Agent { r#ref: _ } => write!(f, "agent"),
            ArtifactKind::ExecuteSQL {
                sql: _,
                database: _,
            } => {
                write!(f, "execute_sql")
            }
            ArtifactKind::SemanticQuery {} => {
                write!(f, "semantic_query")
            }
            ArtifactKind::OmniQuery { .. } => {
                write!(f, "omni_query")
            }
            ArtifactKind::SandboxApp { .. } => {
                write!(f, "sandbox_app")
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "snake_case", tag = "step_type")]
pub enum StepKind {
    Idle,
    Plan,
    Query,
    Visualize,
    Insight,
    Subflow,
    BuildApp,
    Troubleshoot,
    End,
}

impl std::fmt::Display for StepKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StepKind::Idle => write!(f, "idle"),
            StepKind::Plan => write!(f, "plan"),
            StepKind::Query => write!(f, "query"),
            StepKind::Visualize => write!(f, "visualize"),
            StepKind::Insight => write!(f, "insight"),
            StepKind::Subflow => write!(f, "subflow"),
            StepKind::BuildApp => write!(f, "build_app"),
            StepKind::Troubleshoot => write!(f, "troubleshoot"),
            StepKind::End => write!(f, "end"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct Step {
    pub id: String,
    #[serde(flatten)]
    pub kind: StepKind,
    pub objective: Option<String>,
}

impl Step {
    pub fn new(id: String, kind: StepKind, objective: Option<String>) -> Self {
        Self {
            id,
            kind,
            objective,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum EventKind {
    // Output events
    Started {
        name: String,
        attributes: HashMap<String, String>,
    },
    SetMetadata {
        attributes: HashMap<String, String>,
    },
    Updated {
        chunk: Chunk,
    },
    DataAppCreated {
        data_app: DataApp,
    },
    SandboxAppCreated {
        #[serde(flatten)]
        kind: SandboxAppKind,
        preview_url: String,
    },
    Finished {
        attributes: HashMap<String, String>,
        message: String,
        error: Option<String>,
    },
    ArtifactStarted {
        kind: ArtifactKind,
        title: String,
        is_verified: bool,
    },
    VizGenerated {
        viz: VizParams,
    },
    SQLQueryGenerated {
        query: String,
        database: String,
        source: String,
        is_verified: bool,
    },
    OmniQueryGenerated {
        query: crate::tools::types::OmniQueryParams,
        is_verified: bool,
    },
    ArtifactFinished {
        error: Option<String>,
    },
    // UI events
    Progress {
        progress: ProgressType,
    },
    Message {
        message: String,
    },
    Usage {
        usage: Usage,
    },
    Error {
        message: String,
    },
    // Agentic workflow events
    AgenticStarted {
        agent_id: String,
        run_id: String,
        agent_config: serde_json::Value,
    },
    AgenticFinished {
        agent_id: String,
        run_id: String,
        error: Option<String>,
    },
    StepStarted {
        step: Step,
    },
    StepFinished {
        step_id: String,
        error: Option<String>,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Event {
    pub source: Source,
    pub kind: EventKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub id: String,
    pub kind: String,
    pub parent_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EventFormat {
    pub content: String,
    pub reference: Option<ReferenceKind>,
    pub is_error: bool,
    pub kind: String,
}
