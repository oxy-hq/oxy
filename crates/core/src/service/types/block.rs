use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use utoipa::{
    ToSchema,
    openapi::{RefOr, Schema},
};

use crate::{
    agent::builders::fsm::config::AgenticConfig,
    config::model::Workflow,
    execute::types::{
        VizParams,
        event::{ArtifactKind, DataApp, Step},
    },
    service::types::task::TaskMetadata,
};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum BlockKind {
    Task {
        task_name: String,
        task_metadata: Option<TaskMetadata>,
    },
    Step(Step),
    Text {
        content: String,
    },
    #[serde(rename = "sql")]
    SQL {
        sql_query: String,
        database: String,
        result: Vec<Vec<String>>,
        is_result_truncated: bool,
    },
    DataApp(DataApp),
    Viz(VizParams),
    Group {
        group_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Block {
    pub id: String,
    pub children: Vec<String>,
    pub error: Option<String>,
    #[serde(flatten)]
    pub block_kind: BlockKind,
}

impl Block {
    pub fn new(id: String, block_kind: BlockKind) -> Self {
        Self {
            id,
            children: Vec::new(),
            error: None,
            block_kind,
        }
    }

    pub fn add_child(&mut self, child_id: String) {
        if self.children.iter().any(|id| id == &child_id) {
            return;
        }
        self.children.push(child_id);
    }

    pub fn set_error(&mut self, error: String) {
        self.error = Some(error);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub error: Option<String>,
    #[serde(flatten)]
    pub group_kind: GroupKind,
    pub blocks: HashMap<String, Block>,
    pub children: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GroupId {
    Workflow { workflow_id: String, run_id: String },
    Artifact { artifact_id: String },
    Agentic { agent_id: String, run_id: String },
}

impl Group {
    pub fn new(group_kind: GroupKind) -> Self {
        Self {
            error: None,
            group_kind,
            blocks: HashMap::new(),
            children: Vec::new(),
        }
    }

    pub fn with_blocks(self, blocks: HashMap<String, Block>, children: Vec<String>) -> Self {
        Self {
            blocks,
            children,
            ..self
        }
    }

    pub fn set_error(&mut self, error: String) {
        self.error = Some(error);
    }

    pub fn id(&self) -> String {
        self.group_kind.id()
    }

    pub fn group_id(&self) -> GroupId {
        self.group_kind.group_id()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum GroupKind {
    Workflow {
        workflow_id: String,
        run_id: String,
        #[schema(schema_with = any_schema)]
        workflow_config: Workflow,
    },
    Agentic {
        agent_id: String,
        run_id: String,
        #[schema(schema_with = any_schema)]
        agent_config: AgenticConfig,
    },
    Artifact {
        artifact_id: String,
        artifact_name: String,
        artifact_metadata: ArtifactKind,
        is_verified: bool,
    },
}

fn any_schema() -> impl Into<RefOr<Schema>> {
    RefOr::T(Schema::default())
}

impl GroupKind {
    pub fn id(&self) -> String {
        match &self {
            GroupKind::Workflow {
                workflow_id,
                run_id,
                ..
            } => format!("{workflow_id}::{run_id}"),
            GroupKind::Agentic {
                agent_id, run_id, ..
            } => {
                format!("{agent_id}::{run_id}")
            }
            GroupKind::Artifact { artifact_id, .. } => artifact_id.to_string(),
        }
    }
    pub fn group_id(&self) -> GroupId {
        match &self {
            GroupKind::Workflow {
                workflow_id,
                run_id,
                ..
            } => GroupId::Workflow {
                workflow_id: workflow_id.clone(),
                run_id: run_id.clone(),
            },
            GroupKind::Agentic {
                agent_id, run_id, ..
            } => GroupId::Agentic {
                agent_id: agent_id.clone(),
                run_id: run_id.clone(),
            },
            GroupKind::Artifact { artifact_id, .. } => GroupId::Artifact {
                artifact_id: artifact_id.clone(),
            },
        }
    }
}
