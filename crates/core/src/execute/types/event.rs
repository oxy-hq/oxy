use std::{collections::HashMap, path::PathBuf};

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

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
}

impl std::fmt::Display for ArtifactKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArtifactKind::Workflow { r#ref } => write!(f, "workflow"),
            ArtifactKind::Agent { r#ref } => write!(f, "agent"),
            ArtifactKind::ExecuteSQL { sql, database } => {
                write!(f, "execute_sql")
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
    Updated {
        chunk: Chunk,
    },
    DataAppCreated {
        data_app: DataApp,
    },
    Finished {
        message: String,
    },
    ArtifactStarted {
        kind: ArtifactKind,
        title: String,
        is_verified: bool,
    },
    ArtifactFinished,
    // UI events
    Progress {
        progress: ProgressType,
    },
    Message {
        message: String,
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

pub struct EventFormat {
    pub content: String,
    pub reference: Option<ReferenceKind>,
    pub is_error: bool,
    pub kind: String,
}
