use serde::{Deserialize, Serialize};

use crate::{
    adapters::runs::Mergeable,
    agent::builders::fsm::config::AgenticConfig,
    config::{
        constants::{
            ARTIFACT_SOURCE, CONCURRENCY_SOURCE, CONSISTENCY_SOURCE, TASK_SOURCE, WORKFLOW_SOURCE,
        },
        model::Workflow,
    },
    errors::OxyError,
    execute::types::{
        Event, EventKind as ExecuteEventKind, Output, Usage,
        event::{ArtifactKind, Step},
    },
    service::types::{content::ContentType, task::TaskMetadata},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum EventKind {
    WorkflowStarted {
        workflow_id: String,
        run_id: String,
        workflow_config: Workflow,
    },
    WorkflowFinished {
        workflow_id: String,
        run_id: String,
        error: Option<String>,
    },
    TaskStarted {
        task_id: String,
        task_name: String,
        task_metadata: Option<TaskMetadata>,
    },
    TaskMetadata {
        task_id: String,
        metadata: TaskMetadata,
    },
    TaskFinished {
        task_id: String,
        error: Option<String>,
    },
    ArtifactStarted {
        artifact_id: String,
        artifact_name: String,
        artifact_metadata: ArtifactKind,
        is_verified: bool,
    },
    ArtifactFinished {
        artifact_id: String,
        error: Option<String>,
    },
    Usage {
        usage: Usage,
    },
    AgenticStarted {
        agent_id: String,
        run_id: String,
        agent_config: AgenticConfig,
    },
    AgenticFinished {
        agent_id: String,
        run_id: String,
        error: Option<String>,
    },
    StepStarted {
        #[serde(flatten)]
        step: Step,
    },
    StepFinished {
        step_id: String,
        error: Option<String>,
    },
    ContentAdded {
        content_id: String,
        item: ContentType,
    },
    ContentDone {
        content_id: String,
        item: ContentType,
    },
}

impl Mergeable for EventKind {
    fn merge(&mut self, other: Self) -> bool {
        match (self, other) {
            (
                EventKind::ContentAdded { content_id, item },
                EventKind::ContentAdded {
                    content_id: other_content_id,
                    item: other_item,
                },
            ) if content_id == &other_content_id => {
                if let ContentType::Text { content } = item {
                    if let ContentType::Text {
                        content: other_content,
                    } = other_item
                    {
                        *item = ContentType::Text {
                            content: format!("{content}{other_content}"),
                        };
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            _ => false,
        }
    }
}

impl TryFrom<Event> for EventKind {
    type Error = OxyError;

    fn try_from(event: Event) -> Result<Self, OxyError> {
        match event.source.kind.as_str() {
            WORKFLOW_SOURCE => match event.kind {
                ExecuteEventKind::Started {
                    name: _,
                    attributes,
                } => Ok(EventKind::WorkflowStarted {
                    workflow_id: event.source.id.to_string(),
                    run_id: attributes.get("run_id").cloned().unwrap_or_default(),
                    workflow_config: attributes
                        .get("workflow_config")
                        .and_then(|s| serde_json::from_str(s).ok())
                        .ok_or(OxyError::RuntimeError(
                            "Cannot find workflow config".to_string(),
                        ))?,
                }),
                ExecuteEventKind::Finished {
                    message: _,
                    attributes,
                    error,
                } => Ok(EventKind::WorkflowFinished {
                    workflow_id: event.source.id.to_string(),
                    run_id: attributes.get("run_id").cloned().unwrap_or_default(),
                    error,
                }),
                _ => Err(OxyError::ArgumentError(
                    "Unsupported event kind".to_string(),
                )),
            },
            ARTIFACT_SOURCE => match event.kind {
                ExecuteEventKind::ArtifactStarted {
                    title,
                    kind,
                    is_verified,
                } => Ok(EventKind::ArtifactStarted {
                    artifact_id: event.source.id.to_string(),
                    artifact_name: title,
                    artifact_metadata: kind,
                    is_verified,
                }),
                ExecuteEventKind::ArtifactFinished { error } => Ok(EventKind::ArtifactFinished {
                    artifact_id: event.source.id.to_string(),
                    error,
                }),
                _ => Err(OxyError::ArgumentError(
                    "Unsupported event kind".to_string(),
                )),
            },
            CONSISTENCY_SOURCE | CONCURRENCY_SOURCE => Err(OxyError::ArgumentError(
                "Unsupported event kind".to_string(),
            )),
            TASK_SOURCE => match event.kind {
                ExecuteEventKind::Started { name, .. } => Ok(EventKind::TaskStarted {
                    task_id: event.source.id.to_string(),
                    task_name: name.to_string(),
                    task_metadata: None,
                }),
                ExecuteEventKind::SetMetadata { attributes } => {
                    if let Some(task_metadata) = attributes.get("metadata") {
                        let metadata: serde_json::Value = serde_json::from_str(task_metadata)
                            .map_err(|e| {
                                OxyError::RuntimeError(format!(
                                    "Failed to parse task metadata: {e}"
                                ))
                            })?;
                        return Ok(EventKind::TaskMetadata {
                            task_id: event.source.id.to_string(),
                            metadata: serde_json::from_value(metadata).map_err(|e| {
                                OxyError::RuntimeError(format!(
                                    "Failed to deserialize task metadata: {e}"
                                ))
                            })?,
                        });
                    }
                    Err(OxyError::ArgumentError(
                        "Unsupported event kind".to_string(),
                    ))
                }
                ExecuteEventKind::Finished {
                    message: _,
                    error,
                    attributes: _,
                } => Ok(EventKind::TaskFinished {
                    task_id: event.source.id.to_string(),
                    error,
                }),
                ExecuteEventKind::Updated { chunk } => match chunk.delta.clone() {
                    Output::Table(table) => {
                        let reference = table.reference.clone().unwrap_or_default();
                        let (result, is_result_truncated) = table.to_2d_array()?;
                        Ok(EventKind::ContentAdded {
                            content_id: table.file_path.to_string(),
                            item: ContentType::SQL {
                                sql_query: reference.sql.clone(),
                                database: reference.database_ref.to_string(),
                                result,
                                is_result_truncated,
                            },
                        })
                    }
                    Output::Text(text) => {
                        let event = match chunk.finished {
                            true => EventKind::ContentDone {
                                content_id: event.source.id.to_string(),
                                item: ContentType::Text {
                                    content: text.clone(),
                                },
                            },
                            false => EventKind::ContentAdded {
                                content_id: event.source.id.to_string(),
                                item: ContentType::Text {
                                    content: text.clone(),
                                },
                            },
                        };
                        Ok(event)
                    }
                    _ => Err(OxyError::ArgumentError(
                        "Unsupported event kind".to_string(),
                    )),
                },
                _ => Err(OxyError::ArgumentError(
                    "Unsupported event kind".to_string(),
                )),
            },
            _ => match event.kind {
                ExecuteEventKind::StepStarted { step } => Ok(EventKind::StepStarted { step }),
                ExecuteEventKind::StepFinished { step_id, error } => {
                    Ok(EventKind::StepFinished { step_id, error })
                }
                ExecuteEventKind::AgenticStarted {
                    agent_id,
                    run_id,
                    agent_config,
                } => Ok(EventKind::AgenticStarted {
                    agent_id,
                    run_id,
                    agent_config: serde_json::from_value(agent_config).map_err(|e| {
                        OxyError::RuntimeError(format!("Failed to deserialize agentic config: {e}"))
                    })?,
                }),
                ExecuteEventKind::AgenticFinished {
                    agent_id,
                    run_id,
                    error,
                } => Ok(EventKind::AgenticFinished {
                    agent_id,
                    run_id,
                    error,
                }),
                ExecuteEventKind::Usage { usage } => Ok(EventKind::Usage { usage }),
                ExecuteEventKind::Updated { chunk } => match chunk.delta.clone() {
                    Output::SQL(sql) => Ok(EventKind::ContentDone {
                        content_id: event.source.id.to_string(),
                        item: ContentType::Text {
                            content: format!("```sql\n{}\n```", sql.0),
                        },
                    }),
                    Output::Table(table) => {
                        let reference = table.reference.clone().unwrap_or_default();
                        let (result, is_result_truncated) = table.to_2d_array()?;
                        Ok(EventKind::ContentDone {
                            content_id: table.file_path.to_string(),
                            item: ContentType::SQL {
                                sql_query: reference.sql,
                                database: reference.database_ref,
                                result,
                                is_result_truncated,
                            },
                        })
                    }
                    Output::Text(text) => {
                        let event = match chunk.finished {
                            true => EventKind::ContentDone {
                                content_id: event.source.id.to_string(),
                                item: ContentType::Text {
                                    content: text.clone(),
                                },
                            },
                            false => EventKind::ContentAdded {
                                content_id: event.source.id.to_string(),
                                item: ContentType::Text {
                                    content: text.clone(),
                                },
                            },
                        };
                        Ok(event)
                    }
                    _ => Err(OxyError::ArgumentError(
                        "Unsupported event kind".to_string(),
                    )),
                },
                ExecuteEventKind::DataAppCreated { data_app } => Ok(EventKind::ContentDone {
                    content_id: event.source.id.to_string(),
                    item: ContentType::DataApp(data_app),
                }),
                ExecuteEventKind::SandboxAppCreated { kind, preview_url } => {
                    Ok(EventKind::ContentDone {
                        content_id: event.source.id.to_string(),
                        item: ContentType::SandboxApp { kind, preview_url },
                    })
                }
                ExecuteEventKind::VizGenerated { viz } => Ok(EventKind::ContentDone {
                    content_id: event.source.id.to_string(),
                    item: ContentType::Viz(viz),
                }),
                _ => Err(OxyError::ArgumentError(
                    "Unsupported event kind".to_string(),
                )),
            },
        }
    }
}
