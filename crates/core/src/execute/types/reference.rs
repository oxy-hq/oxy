use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::errors::OxyError;

use super::{Document, Output};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ReferenceKind {
    SqlQuery(QueryReference),
    Retrieval(RetrievalReference),
    DataApp(DataAppReference),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryReference {
    pub sql_query: String,
    pub database: String,
    pub result: Vec<Vec<String>>,
    pub is_result_truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalReference {
    pub documents: Vec<Document>,
}

impl TryFrom<Output> for ReferenceKind {
    type Error = OxyError;

    fn try_from(output: Output) -> Result<Self, self::OxyError> {
        match output {
            Output::Table(table) => table.into_reference().ok_or(OxyError::RuntimeError(
                "Failed to convert table to reference".to_string(),
            )),
            Output::Documents(documents) => {
                Ok(ReferenceKind::Retrieval(RetrievalReference { documents }))
            }
            _ => Err(OxyError::RuntimeError(
                "Cannot convert Output into Reference".to_string(),
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataAppReference {
    pub file_path: PathBuf,
}
