use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    execute::types::{ReferenceKind, Table, event::ArtifactKind},
    utils::get_file_stem,
    workflow::loggers::types::LogItem,
};

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
                write!(f, "⏳Execute SQL on Database: {}", database)
            }
            ContainerKind::Task { name } => write!(f, "⏳Starting {}", name),
            ContainerKind::Artifact {
                kind,
                title,
                is_verified,
                artifact_id,
            } => write!(
                f,
                ":::artifact{{id={} kind={} title={} verified={}}}\n:::\n",
                artifact_id, kind, title, is_verified
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

#[derive(Serialize, ToSchema)]
#[serde(tag = "type", content = "value")]
pub enum ArtifactValue {
    #[serde(rename = "log_item")]
    LogItem(LogItem),
    #[serde(rename = "content")]
    Content(String),
    #[serde(rename = "execute_sql")]
    ExecuteSQL(ExecuteSQL),
}

#[derive(Serialize, Deserialize, ToSchema)]
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
    },
    Error {
        message: String,
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
}

impl Content {
    fn to_markdown(&self) -> String {
        match self {
            Content::Text(text) => text.clone(),
            Content::SQL(sql) => format!("\n```sql\n{}\n```\n", sql),
            Content::Table(table) => table.to_markdown(),
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
        format!("<details>\n<summary>{}</summary>\n", summary)
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
                    log_items.push(LogItem::info(format!("Query:\n```sql\n{}\n```\n", sql)))
                }
                Content::Table(table) => {
                    log_items.push(LogItem::info(
                        format!("Result:\n{}\n", table.to_markdown(),),
                    ));
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
                        "⏳Execute SQL on Database: {}",
                        database
                    )));
                    log_items.extend(children.iter().flat_map(|child| child.as_log_items()));
                }
                ContainerKind::Task { name } => {
                    log_items.push(LogItem::info(format!("⏳Starting {}", name)));
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
