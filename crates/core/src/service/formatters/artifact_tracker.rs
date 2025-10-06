use std::sync::Arc;

use sea_orm::ActiveValue::Set;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    config::constants::MARKDOWN_MAX_FENCES,
    errors::OxyError,
    execute::types::event::ArtifactKind,
    service::types::{
        ArtifactContent, Block, BlockValue, ContainerKind, Content, OmniArtifactContent,
        SemanticQueryParams,
    },
};

pub struct ArtifactTracker {
    artifacts: Arc<Mutex<Vec<entity::artifacts::ActiveModel>>>,
    artifact_queue: Vec<(String, ArtifactKind)>,
    // TODO: remove this, refactor the code similar to OmniQuery
    current_semantic_query: Option<SemanticQueryParams>,
}

impl ArtifactTracker {
    pub fn new() -> Self {
        Self {
            artifacts: Arc::new(Mutex::new(Vec::new())),
            artifact_queue: Vec::new(),
            current_semantic_query: None,
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

    pub fn set_semantic_query(&mut self, query: SemanticQueryParams) {
        self.current_semantic_query = Some(query);
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

            let (_, artifact) = self
                .artifact_queue
                .iter()
                .find(|(id, _)| id == artifact_id)
                .ok_or_else(|| {
                    OxyError::RuntimeError("No active artifact found with given id".to_string())
                })?;

            let content = match artifact {
                ArtifactKind::Workflow { r#ref } => Self::create_workflow_artifact(children)?,
                ArtifactKind::Agent { r#ref } => Self::create_agent_artifact(children)?,
                ArtifactKind::ExecuteSQL { sql, database } => Self::create_sql_artifact(children)?,
                ArtifactKind::SemanticQuery { .. } => {
                    Self::create_semantic_query_artifact_with_params(
                        children,
                        self.current_semantic_query.clone(),
                    )?
                }
                ArtifactKind::OmniQuery { topic, .. } => {
                    Self::create_omni_query_artifact(children, topic.clone())?
                }
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
        if let Some(Block { id: _, value }) = children.first()
            && let BlockValue::Children { kind, children } = &**value
            && let ContainerKind::Workflow { r#ref } = kind
        {
            return Ok(Some(ArtifactContent::Workflow {
                r#ref: r#ref.to_string(),
                output: children.iter().flat_map(|c| c.as_log_items()).collect(),
            }));
        }
        Ok(None)
    }

    fn create_agent_artifact(children: &[Block]) -> Result<Option<ArtifactContent>, OxyError> {
        if let Some(Block { id: _, value }) = children.first()
            && let BlockValue::Children { kind, children } = &**value
            && let ContainerKind::Agent { r#ref } = kind
        {
            return Ok(Some(ArtifactContent::Agent {
                r#ref: r#ref.to_string(),
                output: children.iter().fold(String::new(), |mut acc, c| {
                    acc.push_str(&c.clone().to_markdown(MARKDOWN_MAX_FENCES));
                    acc.push('\n');
                    acc
                }),
            }));
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

    fn create_semantic_query_artifact(
        children: &[Block],
    ) -> Result<Option<ArtifactContent>, OxyError> {
        Self::create_semantic_query_artifact_with_params(children, None)
    }

    fn create_semantic_query_artifact_with_params(
        children: &[Block],
        query_params: Option<SemanticQueryParams>,
    ) -> Result<Option<ArtifactContent>, OxyError> {
        let default_params = SemanticQueryParams {
            topic: "".to_string(),
            dimensions: vec![],
            measures: vec![],
            filters: vec![],
            orders: vec![],
            limit: None,
            offset: None,
        };
        let query = query_params.unwrap_or(default_params);

        if let Some(Block { id: _, value }) = children.last() {
            return match &**value {
                BlockValue::Content {
                    content: Content::Table(table),
                } => {
                    let (table_2d_array, is_truncated) = table.to_2d_array()?;
                    Ok(Some(ArtifactContent::SemanticQuery {
                        database: table.get_database_ref().unwrap_or_default(),
                        sql_query: table.get_sql_query().unwrap_or_default(),
                        result: table_2d_array,
                        is_result_truncated: is_truncated,
                        topic: query.topic,
                        dimensions: query.dimensions,
                        measures: query.measures,
                        filters: query.filters,
                        orders: query.orders,
                        limit: query.limit,
                        offset: query.offset,
                    }))
                }
                BlockValue::Content {
                    content: Content::SQL(sql),
                } => Ok(Some(ArtifactContent::SemanticQuery {
                    database: "".to_string(),
                    sql_query: sql.to_string(),
                    result: vec![],
                    is_result_truncated: false,
                    topic: query.topic,
                    dimensions: query.dimensions,
                    measures: query.measures,
                    filters: query.filters,
                    orders: query.orders,
                    limit: query.limit,
                    offset: query.offset,
                })),
                _ => Ok(None),
            };
        }
        Ok(None)
    }

    fn create_omni_query_artifact(
        children: &[Block],
        topic: String,
    ) -> Result<Option<ArtifactContent>, OxyError> {
        let default_params = crate::tools::types::OmniQueryParams {
            fields: vec![],
            limit: None,
            sorts: None,
        };
        let mut artifact_content = OmniArtifactContent {
            result: vec![],
            is_result_truncated: false,
            topic: "".to_string(),
            fields: vec![],
            limit: None,
            sorts: None,
            sql: "".to_string(),
        };

        let params_block = children
            .iter()
            .find(|c| {
                if let BlockValue::Content {
                    content: Content::OmniQuery(_),
                } = &*c.value
                {
                    true
                } else {
                    false
                }
            })
            .ok_or_else(|| {
                OxyError::RuntimeError("OmniQuery block not found in children".to_string())
            })?;

        let params_content = if let BlockValue::Content {
            content: Content::OmniQuery(params),
        } = &*params_block.value
        {
            params
        } else {
            return Err(OxyError::RuntimeError(
                "Failed to extract OmniQuery params".to_string(),
            ));
        };

        artifact_content.fields = params_content.fields.clone();
        artifact_content.topic = topic.clone();
        artifact_content.limit = params_content.limit;
        artifact_content.sorts = params_content.sorts.clone().map(|sorts| {
            sorts
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        match v {
                            crate::tools::types::OrderType::Ascending => "asc".to_string(),
                            crate::tools::types::OrderType::Descending => "desc".to_string(),
                        },
                    )
                })
                .collect()
        });

        if let Some(Block { id: _, value }) = children.last() {
            return match &**value {
                BlockValue::Content {
                    content: Content::Table(table),
                } => {
                    let (table_2d_array, is_truncated) = table.to_2d_array()?;

                    artifact_content.sql = table.get_sql_query().unwrap_or_default();
                    artifact_content.result = table_2d_array;

                    Ok(Some(ArtifactContent::OmniQuery(artifact_content)))
                }
                _ => Ok(None),
            };
        }
        Ok(None)
    }
}
