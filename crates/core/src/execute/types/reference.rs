use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use oxy_shared::errors::OxyError;

use super::{Document, Output};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ReferenceKind {
    SqlQuery(QueryReference),
    Retrieval(RetrievalReference),
    DataApp(DataAppReference),
    SemanticQuery(SemanticQueryReference),
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct QueryReference {
    pub sql_query: String,
    pub database: String,
    pub result: Vec<Vec<String>>,
    pub is_result_truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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
            Output::SemanticQuery(sq) => Ok(ReferenceKind::SemanticQuery(SemanticQueryReference {
                database: sq.database,
                topic: sq.topic,
                sql_query: if sq.sql_query.is_empty() {
                    None
                } else {
                    Some(sq.sql_query)
                },
                result: sq.result,
                is_result_truncated: sq.is_result_truncated,
            })),
            _ => Err(OxyError::RuntimeError(
                "Cannot convert Output into Reference".to_string(),
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DataAppReference {
    #[schema(value_type = String)]
    pub file_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SemanticQueryReference {
    pub database: String,
    pub topic: Option<String>,
    pub sql_query: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub result: Vec<Vec<String>>,
    #[serde(default)]
    pub is_result_truncated: bool,
}
