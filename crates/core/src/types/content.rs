use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::execute::types::{
    VizParams,
    event::{DataApp, SandboxAppKind},
};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ContentType {
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
    SandboxApp {
        #[serde(flatten)]
        kind: SandboxAppKind,
        preview_url: String,
    },
    Viz(VizParams),
    SemanticQuery {
        semantic_query: String,
        sql_query: String,
        results: Vec<Vec<String>>,
    },
}
