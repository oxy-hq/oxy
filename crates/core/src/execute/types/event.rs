use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{Chunk, ProgressType};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DataApp {
    pub file_path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum EventKind {
    // Output events
    Started { name: String },
    Updated { chunk: Chunk },
    DataAppCreated { data_app: DataApp },
    Finished { message: String },
    // UI events
    Progress { progress: ProgressType },
    Message { message: String },
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
