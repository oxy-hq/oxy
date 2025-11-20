use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use utoipa::ToSchema;

use crate::{
    execute::types::{
        ReferenceKind, Table, Usage,
        event::{ArtifactKind, Step},
    },
    tools::types::OmniQueryParams,
    utils::get_file_stem,
    workflow::loggers::types::LogItem,
};
use serde_json::Value as JsonValue;

pub mod block;
pub mod content;
pub mod event;
pub mod pagination;
pub mod run;
pub mod task;

#[derive(Serialize, Debug, Clone, ToSchema)]
#[serde(tag = "type")]
pub enum ContainerKind {
    #[serde(rename = "workflow")]
    Workflow { r#ref: String },
    #[serde(rename = "agent")]
    Agent { r#ref: String },
    #[serde(rename = "execute_sql")]
    ExecuteSQL { database: String },
    #[serde(rename = "task")]
    Task { name: String },
    #[serde(rename = "artifact")]
    Artifact {
        artifact_id: String,
        kind: String,
        title: String,
        is_verified: bool,
    },
}

impl std::fmt::Display for ContainerKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContainerKind::Workflow { r#ref } => {
                write!(f, "⏳Running workflow: {}", get_file_stem(r#ref))
            }
            ContainerKind::Agent { r#ref } => write!(f, "⏳Starting {}", get_file_stem(r#ref)),
            ContainerKind::ExecuteSQL { database } => {
                write!(f, "⏳Execute SQL on Database: {database}")
            }
            ContainerKind::Task { name } => write!(f, "⏳Starting {name}"),
            ContainerKind::Artifact {
                kind,
                title,
                is_verified,
                artifact_id,
            } => write!(
                f,
                ":::artifact{{id={artifact_id} kind={kind} title={title} verified={is_verified}}}\n:::\n"
            ),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct ExecuteSQL {
    pub database: String,
    pub sql_query: String,
    pub result: Vec<Vec<String>>,
    pub is_result_truncated: bool,
}

// SemanticQuery result mirrors ExecuteSQL but carries semantic layer context
// (topic, measures, dimensions) alongside the generated SQL and tabular data.
// This enables downstream consumers to understand both the logical and
// physical lineage of the result.
#[derive(Serialize, Deserialize, ToSchema, JsonSchema, Clone, Debug)]
pub struct SemanticQueryFilter {
    pub field: String,
    pub op: String,
    pub value: JsonValue,
}

impl std::hash::Hash for SemanticQueryFilter {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.field.hash(state);
        self.op.hash(state);
        // Hash the JSON value as a string for consistency
        if let Ok(s) = serde_json::to_string(&self.value) {
            s.hash(state);
        }
    }
}

#[derive(Serialize, Deserialize, ToSchema, JsonSchema, Clone, Debug, Hash)]
pub struct SemanticQueryOrder {
    pub field: String,
    pub direction: String,
}

#[derive(Serialize, Deserialize, ToSchema, JsonSchema, Clone, Debug)]
pub struct SemanticQueryExport {
    pub path: String,
    pub format: String,
}

// Reusable set of semantic query parameters (mirrors task definition inputs)
#[derive(Serialize, Deserialize, ToSchema, JsonSchema, Clone, Debug)]
pub struct SemanticQueryParams {
    pub topic: String,
    #[schemars(
        description = "List of measures to include in the query. Format: <view_name>.<measure_name>"
    )]
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub measures: Vec<String>,
    #[schemars(
        description = "List of dimensions to include in the query. Format: <view_name>.<dimension_name>"
    )]
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub dimensions: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub filters: Vec<SemanticQueryFilter>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub orders: Vec<SemanticQueryOrder>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
    /// Variables for semantic layer expressions (e.g. table names, column names, filters)
    #[schemars(
        description = "Variables to resolve in semantic layer expressions. Use {{variables.variable_name}} syntax in semantic definitions."
    )]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variables: Option<HashMap<String, Value>>,
}

impl std::hash::Hash for SemanticQueryParams {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.topic.hash(state);
        self.measures.hash(state);
        self.dimensions.hash(state);
        for filter in &self.filters {
            filter.hash(state);
        }
        for order in &self.orders {
            order.hash(state);
        }
        self.limit.hash(state);
        self.offset.hash(state);
        // Variables affect query results, so include them in hash
        if let Some(variables) = &self.variables {
            for (key, value) in variables {
                key.hash(state);
                value.to_string().hash(state); // Hash the JSON string representation
            }
        }
    }
}

#[derive(Serialize, Deserialize, Clone, ToSchema, Debug)]
pub struct SemanticQuery {
    pub database: String,
    pub sql_query: String,
    pub result: Vec<Vec<String>>,
    pub is_result_truncated: bool,
    pub topic: String,
    pub dimensions: Vec<String>,
    pub measures: Vec<String>,
    pub filters: Vec<SemanticQueryFilter>,
    pub orders: Vec<SemanticQueryOrder>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, ToSchema, Debug)]
pub struct OmniQuery {
    pub result: Vec<Vec<String>>,
    pub is_result_truncated: bool,
    pub topic: String,
    pub fields: Vec<String>,
    pub limit: Option<u64>,
    pub sorts: Option<std::collections::HashMap<String, String>>,
}

#[derive(Serialize, ToSchema)]
#[serde(tag = "type", content = "value")]
pub enum ArtifactValue {
    #[serde(rename = "log_item")]
    LogItem(LogItem),
    #[serde(rename = "content")]
    Content(String),
    #[serde(rename = "execute_sql")]
    ExecuteSQL(ExecuteSQL),
    #[serde(rename = "semantic_query")]
    SemanticQuery(SemanticQuery),
    #[serde(rename = "omni_query")]
    OmniQuery(OmniQuery),
}

#[derive(Serialize, Deserialize, ToSchema, Debug)]
#[serde(tag = "type", content = "value")]
pub enum ArtifactContent {
    #[serde(rename = "workflow")]
    Workflow { r#ref: String, output: Vec<LogItem> },
    #[serde(rename = "agent")]
    Agent { r#ref: String, output: String },
    #[serde(rename = "execute_sql")]
    ExecuteSQL {
        database: String,
        sql_query: String,
        result: Vec<Vec<String>>,
        is_result_truncated: bool,
    },
    #[serde(rename = "semantic_query")]
    SemanticQuery {
        database: String,
        sql_query: String,
        result: Vec<Vec<String>>,
        is_result_truncated: bool,
        topic: String,
        dimensions: Vec<String>,
        measures: Vec<String>,
        filters: Vec<SemanticQueryFilter>,
        orders: Vec<SemanticQueryOrder>,
        limit: Option<u64>,
        offset: Option<u64>,
    },
    #[serde(rename = "omni_query")]
    OmniQuery(OmniArtifactContent),
}

#[derive(Serialize, Deserialize, ToSchema, Debug)]
pub struct OmniArtifactContent {
    pub result: Vec<Vec<String>>,
    pub is_result_truncated: bool,
    pub topic: String,
    pub sql: String,
    pub fields: Vec<String>,
    pub limit: Option<u64>,
    pub sorts: Option<std::collections::HashMap<String, String>>,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum AnswerContent {
    Text {
        content: String,
    },
    ArtifactStarted {
        id: String,
        title: String,
        is_verified: bool,
        kind: ArtifactKind,
    },
    ArtifactValue {
        id: String,
        value: ArtifactValue,
    },
    ArtifactDone {
        id: String,
        error: Option<String>,
    },
    Error {
        message: String,
    },
    Usage {
        usage: Usage,
    },
    DataApp {
        file_path: String,
    },
    StepStarted {
        step: Step,
    },
    StepFinished {
        step_id: String,
        error: Option<String>,
    },
}

#[derive(Serialize, ToSchema)]
pub struct AnswerStream {
    pub content: AnswerContent,
    pub references: Vec<ReferenceKind>,
    pub is_error: bool,
    pub step: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(tag = "type", content = "value")]
pub enum Content {
    Text(String),
    SQL(String),
    Table(Table),
    OmniQuery(OmniQueryParams),
}

impl Content {
    fn to_markdown(&self) -> String {
        match self {
            Content::Text(text) => text.clone(),
            Content::SQL(sql) => format!("\n```sql\n{sql}\n```\n"),
            Content::Table(table) => table.to_markdown(),
            Content::OmniQuery(omni_query_params) => {
                let json = serde_json::to_string_pretty(omni_query_params)
                    .unwrap_or_else(|_| "Failed to serialize OmniQueryParams".to_string());
                format!("\n```json\n{json}\n```\n")
            }
        }
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(untagged)]
pub enum BlockValue {
    Content {
        content: Content,
    },
    Children {
        #[serde(flatten)]
        kind: ContainerKind,
        children: Vec<Block>,
    },
}

#[derive(Serialize, Clone, Debug)]
pub struct Block {
    pub id: String,
    #[serde(flatten)]
    pub value: Box<BlockValue>,
}

impl Block {
    pub fn is_artifact(&self) -> bool {
        matches!(self.value.as_ref(), BlockValue::Children { kind, .. } if matches!(kind, ContainerKind::Artifact { .. }))
    }
    pub fn container(id: String, kind: ContainerKind) -> Self {
        Block {
            id,
            value: Box::new(BlockValue::Children {
                kind,
                children: vec![],
            }),
        }
    }
    pub fn content(id: String, content: Content) -> Self {
        Block {
            id,
            value: Box::new(BlockValue::Content { content }),
        }
    }
    fn details_opener(summary: &str) -> String {
        format!("<details>\n<summary>{summary}</summary>\n")
    }
    fn details_closer() -> String {
        "</details>".to_string()
    }
    fn artifacts_opener(
        id: &str,
        kind: &str,
        title: &str,
        is_verified: bool,
        fences_count: usize,
    ) -> String {
        format!(
            "{}artifact{{id={} kind={} title={} is_verified={}}}",
            ":".repeat(fences_count),
            id,
            kind,
            title,
            is_verified
        )
    }
    fn artifacts_closer(fences_count: usize) -> String {
        ":".repeat(fences_count)
    }
    pub fn container_opener_closer(
        kind: &ContainerKind,
        max_artifact_fences: &mut usize,
    ) -> (String, String) {
        match kind {
            ContainerKind::Workflow { .. } => (
                Block::details_opener(&kind.to_string()),
                Block::details_closer(),
            ),
            ContainerKind::Agent { .. } => (
                Block::details_opener(&kind.to_string()),
                Block::details_closer(),
            ),
            ContainerKind::ExecuteSQL { .. } => (
                Block::details_opener(&kind.to_string()),
                Block::details_closer(),
            ),
            ContainerKind::Task { .. } => (
                Block::details_opener(&kind.to_string()),
                Block::details_closer(),
            ),
            ContainerKind::Artifact {
                artifact_id,
                kind,
                title,
                is_verified,
            } => {
                let result = (
                    Block::artifacts_opener(
                        artifact_id,
                        kind,
                        title,
                        *is_verified,
                        *max_artifact_fences,
                    ),
                    Block::artifacts_closer(*max_artifact_fences),
                );
                *max_artifact_fences = max_artifact_fences.saturating_sub(1);
                if *max_artifact_fences < 3 {
                    *max_artifact_fences = 3;
                }
                result
            }
        }
    }
    pub fn to_markdown(&self, max_artifact_fences: usize) -> String {
        let mut next_fences = max_artifact_fences;
        match self.value.as_ref() {
            BlockValue::Content { content } => content.to_markdown(),
            BlockValue::Children { kind, children } => {
                let mut markdown = String::new();
                let (block_opener, block_closer) =
                    Block::container_opener_closer(kind, &mut next_fences);
                markdown.push('\n');
                markdown.push_str(&block_opener);
                markdown.push('\n');
                for child in children {
                    markdown.push_str(&child.to_markdown(next_fences));
                }
                markdown.push_str("\n\n");
                markdown.push_str(&block_closer);
                markdown.push('\n');
                markdown
            }
        }
    }
    pub fn as_log_items(&self) -> Vec<LogItem> {
        let mut log_items = vec![];
        match self.value.as_ref() {
            BlockValue::Content { content } => match content {
                Content::Text(text) => log_items.push(LogItem::info(text.clone())),
                Content::SQL(sql) => {
                    log_items.push(LogItem::info(format!("Query:\n```sql\n{sql}\n```\n")))
                }
                Content::Table(table) => {
                    log_items.push(LogItem::info(
                        format!("Result:\n{}\n", table.to_markdown(),),
                    ));
                }
                Content::OmniQuery(omni_query_params) => {
                    let json = serde_json::to_string_pretty(omni_query_params)
                        .unwrap_or_else(|_| "Failed to serialize OmniQueryParams".to_string());
                    log_items.push(LogItem::info(format!(
                        "Omni Query:\n```json\n{json}\n```\n"
                    )));
                }
            },
            BlockValue::Children { kind, children } => match kind {
                ContainerKind::Workflow { r#ref } => {
                    log_items.push(LogItem::info(format!(
                        "⏳Running Workflow: {}",
                        get_file_stem(r#ref)
                    )));
                    log_items.extend(children.iter().flat_map(|child| child.as_log_items()));
                }
                ContainerKind::Agent { r#ref } => {
                    log_items.push(LogItem::info(format!(
                        "⏳Starting {}",
                        get_file_stem(r#ref)
                    )));
                    log_items.extend(children.iter().flat_map(|child| child.as_log_items()));
                }
                ContainerKind::ExecuteSQL { database } => {
                    log_items.push(LogItem::info(format!(
                        "⏳Execute SQL on Database: {database}"
                    )));
                    log_items.extend(children.iter().flat_map(|child| child.as_log_items()));
                }
                ContainerKind::Task { name } => {
                    log_items.push(LogItem::info(format!("⏳Starting {name}")));
                    log_items.extend(children.iter().flat_map(|child| child.as_log_items()));
                }
                ContainerKind::Artifact { .. } => {
                    log_items.push(LogItem::info(kind.to_string()));
                }
            },
        }
        log_items
    }
}
