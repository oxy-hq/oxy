use std::sync::Arc;

use sea_orm::ActiveValue::Set;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    config::constants::MARKDOWN_MAX_FENCES,
    errors::OxyError,
    execute::types::event::ArtifactKind,
    service::types::{ArtifactContent, Block, BlockValue, ContainerKind, Content},
};

pub struct ArtifactTracker {
    artifacts: Arc<Mutex<Vec<entity::artifacts::ActiveModel>>>,
    artifact_queue: Vec<(String, ArtifactKind)>,
}

impl ArtifactTracker {
    pub fn new() -> Self {
        Self {
            artifacts: Arc::new(Mutex::new(vec![])),
            artifact_queue: vec![],
        }
    }

    pub fn get_artifacts_clone(&self) -> Arc<Mutex<Vec<entity::artifacts::ActiveModel>>> {
        Arc::clone(&self.artifacts)
    }

    pub fn start_artifact(&mut self, id: String, kind: ArtifactKind) {
        self.artifact_queue.push((id, kind));
    }

    pub fn finish_artifact(&mut self) -> Option<String> {
        self.artifact_queue.pop().map(|(id, _)| id)
    }

    pub fn has_active_artifact(&self) -> bool {
        !self.artifact_queue.is_empty()
    }

    pub fn get_active_artifact(&self) -> Option<&(String, ArtifactKind)> {
        self.artifact_queue.last()
    }

    pub async fn store_artifact(&mut self, block: &Block) -> Result<(), OxyError> {
        if let BlockValue::Children {
            kind: ContainerKind::Artifact {
                artifact_id, kind, ..
            },
            children,
        } = &*block.value
        {
            let artifact_uuid = Uuid::parse_str(artifact_id).map_err(|_| {
                OxyError::RuntimeError("Failed to generate artifact_id".to_string())
            })?;

            let content = match kind.as_str() {
                "workflow" => Self::create_workflow_artifact(children)?,
                "agent" => Self::create_agent_artifact(children)?,
                "execute_sql" => Self::create_sql_artifact(children)?,
                _ => None,
            };

            if let Some(content) = content {
                let content = serde_json::to_value(content)?;
                self.artifacts
                    .lock()
                    .await
                    .push(entity::artifacts::ActiveModel {
                        id: Set(artifact_uuid),
                        content: Set(content),
                        kind: Set(kind.to_string()),
                        ..Default::default()
                    });
            }
        }
        Ok(())
    }

    fn create_workflow_artifact(children: &[Block]) -> Result<Option<ArtifactContent>, OxyError> {
        if let Some(Block { id: _, value }) = children.first() {
            if let BlockValue::Children { kind, children } = &**value {
                if let ContainerKind::Workflow { r#ref } = kind {
                    return Ok(Some(ArtifactContent::Workflow {
                        r#ref: r#ref.to_string(),
                        output: children.iter().flat_map(|c| c.as_log_items()).collect(),
                    }));
                }
            }
        }
        Ok(None)
    }

    fn create_agent_artifact(children: &[Block]) -> Result<Option<ArtifactContent>, OxyError> {
        if let Some(Block { id: _, value }) = children.first() {
            if let BlockValue::Children { kind, children } = &**value {
                if let ContainerKind::Agent { r#ref } = kind {
                    return Ok(Some(ArtifactContent::Agent {
                        r#ref: r#ref.to_string(),
                        output: children.iter().fold(String::new(), |mut acc, c| {
                            acc.push_str(&c.clone().to_markdown(MARKDOWN_MAX_FENCES));
                            acc.push_str("\n");
                            acc
                        }),
                    }));
                }
            }
        }
        Ok(None)
    }

    fn create_sql_artifact(children: &[Block]) -> Result<Option<ArtifactContent>, OxyError> {
        if let Some(Block { id: _, value }) = children.last() {
            return match &**value {
                BlockValue::Content {
                    content: Content::Table(table),
                } => {
                    let (table_2d_array, is_truncated) = table.to_2d_array()?;
                    Ok(Some(ArtifactContent::ExecuteSQL {
                        database: table.get_database_ref().unwrap_or_default(),
                        is_result_truncated: is_truncated,
                        sql_query: table.get_sql_query().unwrap_or_default(),
                        result: table_2d_array,
                    }))
                }
                BlockValue::Content {
                    content: Content::SQL(sql),
                } => Ok(Some(ArtifactContent::ExecuteSQL {
                    database: "".to_string(),
                    is_result_truncated: false,
                    sql_query: sql.to_string(),
                    result: vec![],
                })),
                _ => Ok(None),
            };
        }
        Ok(None)
    }
}
