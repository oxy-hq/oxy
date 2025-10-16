use std::{collections::HashMap, path::PathBuf};

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{execute::types::Usage, service::types::SemanticQueryParams};

use super::{Chunk, ProgressType, ReferenceKind};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DataApp {
    pub file_path: PathBuf,
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

    SQLQueryGenerated {
        query: String,
        database: String,
        source: String,
        is_verified: bool,
    },
    SemanticQueryGenerated {
        query: SemanticQueryParams,
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
