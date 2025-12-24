//! Oxy implementation of A2A TaskStorage trait using SeaORM.
//!
//! This module implements the `TaskStorage` trait from the `a2a` crate using
//! Oxy's database backend (SeaORM). The storage uses a four-table design for
//! optimal performance and scalability:
//!
//! - **Tasks Table** (`a2a_tasks`): Lightweight task metadata only
//! - **Task Status Table** (`a2a_task_status`): Status history with timestamps
//! - **Messages Table** (`a2a_messages`): Message history with sequence ordering
//! - **Artifacts Table** (`a2a_artifacts`): Artifact storage with parts and metadata
//!
//! # Architecture
//!
//! - **Agent Scoping**: Each `OxyTaskStorage` instance is scoped to a specific agent
//!   via the `agent_name` field. All database queries are automatically filtered by
//!   this agent name to ensure isolation between agents.
//! - **Factory Pattern**: Use `new_for_agent()` to create agent-scoped instances.
//! - **Transactional Updates**: Status updates, message appending, and artifact
//!   additions are done transactionally to ensure consistency.
//! - **Efficient Queries**: Task queries load only metadata by default. Messages,
//!   artifacts, and status history are loaded via separate efficient queries.
//!
//! # Example
//!
//! ```rust,ignore
//! use oxy_core::a2a::storage::OxyTaskStorage;
//!
//! let storage = OxyTaskStorage::new_for_agent(
//!     "sales-assistant".to_string(),
//!     db.clone()
//! );
//!
//! // All operations are automatically scoped to "sales-assistant"
//! let task = storage.create_task(task).await?;
//! ```

use a2a::{
    error::A2aError,
    storage::{TaskFilters, TaskStorage},
    types::{Artifact, Message, MessageKind, MessageRole, Part, Task, TaskState, TaskStatus},
};
use async_trait::async_trait;
use entity::{a2a_artifacts, a2a_messages, a2a_task_status, a2a_tasks};
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect, TransactionTrait,
};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

/// Oxy implementation of A2A task storage using SeaORM.
///
/// This storage implementation is scoped to a single agent. All operations
/// automatically filter by the agent name to ensure isolation.
///
/// # Example
///
/// ```rust,ignore
/// let storage = OxyTaskStorage::new_for_agent(
///     "sales-assistant".to_string(),
///     db.clone()
/// );
/// ```
#[derive(Clone)]
pub struct OxyTaskStorage {
    /// The name of the agent this storage is scoped to
    agent_name: String,
    /// Database connection for querying
    db: Arc<DatabaseConnection>,
}

impl OxyTaskStorage {
    /// Create a new task storage instance scoped to a specific agent.
    ///
    /// # Arguments
    ///
    /// * `agent_name` - The name of the agent this storage is for
    /// * `db` - Database connection
    ///
    /// # Returns
    ///
    /// A new storage instance that will automatically filter all queries by the agent name.
    pub fn new_for_agent(agent_name: String, db: Arc<DatabaseConnection>) -> Self {
        Self { agent_name, db }
    }

    /// Get all messages associated with a context ID.
    pub async fn get_messages_by_context_id(
        &self,
        context_id: &str,
    ) -> Result<Vec<Message>, A2aError> {
        let message_models = a2a_messages::Entity::find()
            .filter(a2a_messages::Column::ContextId.eq(context_id))
            .filter(a2a_messages::Column::AgentName.eq(&self.agent_name))
            .order_by_asc(a2a_messages::Column::CreatedAt)
            .all(self.db.as_ref())
            .await
            .map_err(|e| A2aError::ServerError(format!("Failed to fetch messages: {}", e)))?;

        let messages: Result<Vec<Message>, A2aError> = message_models
            .iter()
            .map(|m| {
                let role = string_to_message_role(&m.role);
                let parts = json_to_parts(&m.parts)?;
                Ok(Message {
                    kind: MessageKind::Message,
                    role,
                    parts,
                    metadata: m.metadata.as_ref().and_then(json_to_metadata),
                    extensions: None,
                    reference_task_ids: None,
                    message_id: m.id.to_string(),
                    task_id: m.task_id.map(|id| id.to_string()),
                    context_id: m
                        .context_id
                        .clone()
                        .or_else(|| Some(context_id.to_string())),
                })
            })
            .collect();

        messages
    }
}

/// Convert A2A TaskState to database string representation
fn task_state_to_string(state: &TaskState) -> String {
    match state {
        TaskState::Submitted => "submitted".to_string(),
        TaskState::Working => "working".to_string(),
        TaskState::InputRequired => "input-required".to_string(),
        TaskState::Completed => "completed".to_string(),
        TaskState::Canceled => "canceled".to_string(),
        TaskState::Failed => "failed".to_string(),
        TaskState::Rejected => "rejected".to_string(),
        TaskState::AuthRequired => "auth-required".to_string(),
        TaskState::Unknown => "unknown".to_string(),
    }
}

/// Convert database string to A2A TaskState
fn string_to_task_state(s: &str) -> TaskState {
    match s {
        "submitted" => TaskState::Submitted,
        "working" => TaskState::Working,
        "input-required" => TaskState::InputRequired,
        "completed" => TaskState::Completed,
        "canceled" => TaskState::Canceled,
        "failed" => TaskState::Failed,
        "rejected" => TaskState::Rejected,
        "auth-required" => TaskState::AuthRequired,
        _ => TaskState::Unknown,
    }
}

/// Convert A2A MessageRole to database string representation
fn message_role_to_string(role: &MessageRole) -> String {
    match role {
        MessageRole::User => "user".to_string(),
        MessageRole::Agent => "agent".to_string(),
    }
}

/// Convert database string to A2A MessageRole
fn string_to_message_role(s: &str) -> MessageRole {
    match s {
        "user" => MessageRole::User,
        "agent" => MessageRole::Agent,
        _ => MessageRole::User, // Default to user
    }
}

/// Convert A2A Parts to JSON for database storage
fn parts_to_json(parts: &[Part]) -> Result<serde_json::Value, A2aError> {
    serde_json::to_value(parts)
        .map_err(|e| A2aError::InvalidTask(format!("Failed to serialize parts: {}", e)))
}

/// Convert JSON from database to A2A Parts
fn json_to_parts(json: &serde_json::Value) -> Result<Vec<Part>, A2aError> {
    serde_json::from_value(json.clone())
        .map_err(|e| A2aError::InvalidTask(format!("Failed to deserialize parts: {}", e)))
}

/// Convert Task metadata to JSON for database storage
fn metadata_to_json(
    metadata: &Option<HashMap<String, serde_json::Value>>,
) -> Result<serde_json::Value, A2aError> {
    match metadata {
        Some(map) => serde_json::to_value(map)
            .map_err(|e| A2aError::InvalidTask(format!("Failed to serialize metadata: {}", e))),
        None => Ok(serde_json::json!({})),
    }
}

/// Convert JSON from database to Task metadata
fn json_to_metadata(json: &serde_json::Value) -> Option<HashMap<String, serde_json::Value>> {
    if json.is_null() || (json.is_object() && json.as_object().unwrap().is_empty()) {
        None
    } else {
        serde_json::from_value(json.clone()).ok()
    }
}

#[async_trait]
impl TaskStorage for OxyTaskStorage {
    async fn create_task(&self, task: Task) -> Result<Task, A2aError> {
        // Parse task ID as UUID
        let task_uuid = Uuid::parse_str(&task.id)
            .map_err(|e| A2aError::InvalidTask(format!("Invalid task ID: {}", e)))?;

        // Start a transaction to ensure atomicity
        let txn =
            self.db.begin().await.map_err(|e| {
                A2aError::StorageError(format!("Failed to start transaction: {}", e))
            })?;

        // 1. Insert task metadata into a2a_tasks table
        let task_model = a2a_tasks::ActiveModel {
            id: ActiveValue::Set(task_uuid),
            agent_name: ActiveValue::Set(self.agent_name.clone()),
            thread_id: ActiveValue::Set(None),
            run_id: ActiveValue::Set(None),
            context_id: ActiveValue::Set(Some(task.context_id.clone())),
            state: ActiveValue::Set(task_state_to_string(&task.status.state)),
            metadata: ActiveValue::Set(metadata_to_json(&task.metadata)?),
            created_at: ActiveValue::NotSet,
            updated_at: ActiveValue::NotSet,
        };

        task_model
            .insert(&txn)
            .await
            .map_err(|e| A2aError::StorageError(format!("Failed to insert task: {}", e)))?;

        // 2. Insert initial status into a2a_task_status table
        let status_model = a2a_task_status::ActiveModel {
            id: ActiveValue::Set(Uuid::new_v4()),
            task_id: ActiveValue::Set(task_uuid),
            agent_name: ActiveValue::Set(self.agent_name.clone()),
            state: ActiveValue::Set(task_state_to_string(&task.status.state)),
            message_id: ActiveValue::Set(None), // Will be set if status has a message
            metadata: ActiveValue::Set(task.status.timestamp.as_ref().map(|ts| {
                serde_json::json!({
                    "timestamp": ts
                })
            })),
            created_at: ActiveValue::NotSet,
        };

        status_model
            .insert(&txn)
            .await
            .map_err(|e| A2aError::StorageError(format!("Failed to insert status: {}", e)))?;

        // 3. Insert messages if any in history
        if let Some(ref history) = task.history {
            for (idx, message) in history.iter().enumerate() {
                let message_uuid =
                    Uuid::parse_str(&message.message_id).unwrap_or_else(|_| Uuid::new_v4());

                let message_task_id = message
                    .task_id
                    .as_ref()
                    .and_then(|id| Uuid::parse_str(id).ok())
                    .or(Some(task_uuid));

                let message_context_id = message
                    .context_id
                    .clone()
                    .or_else(|| Some(task.context_id.clone()));

                let message_model = a2a_messages::ActiveModel {
                    id: ActiveValue::Set(message_uuid),
                    task_id: ActiveValue::Set(message_task_id),
                    context_id: ActiveValue::Set(message_context_id),
                    agent_name: ActiveValue::Set(self.agent_name.clone()),
                    role: ActiveValue::Set(message_role_to_string(&message.role)),
                    sequence_number: ActiveValue::Set(idx as i32),
                    parts: ActiveValue::Set(parts_to_json(&message.parts)?),
                    metadata: ActiveValue::Set(
                        message
                            .metadata
                            .as_ref()
                            .map(|m| serde_json::to_value(m).unwrap_or(serde_json::json!({}))),
                    ),
                    created_at: ActiveValue::NotSet,
                };

                message_model.insert(&txn).await.map_err(|e| {
                    A2aError::StorageError(format!("Failed to insert message: {}", e))
                })?;
            }
        }

        // 4. Insert artifacts if any
        if let Some(ref artifacts) = task.artifacts {
            for (idx, artifact) in artifacts.iter().enumerate() {
                let artifact_uuid =
                    Uuid::parse_str(&artifact.artifact_id).unwrap_or_else(|_| Uuid::new_v4());

                let artifact_model = a2a_artifacts::ActiveModel {
                    id: ActiveValue::Set(artifact_uuid),
                    task_id: ActiveValue::Set(task_uuid),
                    agent_name: ActiveValue::Set(self.agent_name.clone()),
                    sequence_number: ActiveValue::Set(idx as i32),
                    description: ActiveValue::Set(artifact.description.clone()),
                    parts: ActiveValue::Set(parts_to_json(&artifact.parts)?),
                    storage_location: ActiveValue::Set(None),
                    size_bytes: ActiveValue::Set(None),
                    metadata: ActiveValue::Set(
                        artifact
                            .metadata
                            .as_ref()
                            .map(|m| serde_json::to_value(m).unwrap_or(serde_json::json!({}))),
                    ),
                    created_at: ActiveValue::NotSet,
                };

                artifact_model.insert(&txn).await.map_err(|e| {
                    A2aError::StorageError(format!("Failed to insert artifact: {}", e))
                })?;
            }
        }

        // Commit transaction
        txn.commit()
            .await
            .map_err(|e| A2aError::StorageError(format!("Failed to commit transaction: {}", e)))?;

        Ok(task)
    }

    async fn get_task(&self, task_id: String) -> Result<Option<Task>, A2aError> {
        // Parse task ID as UUID
        let task_uuid = Uuid::parse_str(&task_id)
            .map_err(|e| A2aError::InvalidTask(format!("Invalid task ID: {}", e)))?;

        // 1. Query task metadata
        let task_model = a2a_tasks::Entity::find_by_id(task_uuid)
            .filter(a2a_tasks::Column::AgentName.eq(&self.agent_name))
            .one(self.db.as_ref())
            .await
            .map_err(|e| A2aError::StorageError(format!("Failed to query task: {}", e)))?;

        let task_model = match task_model {
            Some(model) => model,
            None => return Ok(None),
        };

        // 2. Query latest status
        let latest_status = a2a_task_status::Entity::find()
            .filter(a2a_task_status::Column::TaskId.eq(task_uuid))
            .filter(a2a_task_status::Column::AgentName.eq(&self.agent_name))
            .order_by_desc(a2a_task_status::Column::CreatedAt)
            .one(self.db.as_ref())
            .await
            .map_err(|e| A2aError::StorageError(format!("Failed to query status: {}", e)))?;

        let status = match latest_status {
            Some(status_model) => {
                let state = string_to_task_state(&status_model.state);
                let timestamp = Some(status_model.created_at.to_rfc3339());

                // Load the message if message_id is set
                let message = if let Some(msg_id) = status_model.message_id {
                    let msg_model = a2a_messages::Entity::find_by_id(msg_id)
                        .filter(a2a_messages::Column::AgentName.eq(&self.agent_name))
                        .one(self.db.as_ref())
                        .await
                        .map_err(|e| {
                            A2aError::StorageError(format!("Failed to query status message: {}", e))
                        })?;

                    msg_model
                        .map(|m| {
                            let role = string_to_message_role(&m.role);
                            let parts = json_to_parts(&m.parts).ok()?;
                            Some(Message {
                                kind: MessageKind::Message,
                                role,
                                parts,
                                metadata: m.metadata.as_ref().and_then(json_to_metadata),
                                extensions: None,
                                reference_task_ids: None,
                                message_id: m.id.to_string(),
                                task_id: m.task_id.map(|id| id.to_string()),
                                context_id: m
                                    .context_id
                                    .clone()
                                    .or_else(|| task_model.context_id.clone()),
                            })
                        })
                        .flatten()
                } else {
                    None
                };

                TaskStatus {
                    state,
                    message,
                    timestamp,
                }
            }
            None => {
                // Fallback to task state if no status history
                TaskStatus::new(string_to_task_state(&task_model.state))
            }
        };

        // 3. Query messages
        let message_models = a2a_messages::Entity::find()
            .filter(a2a_messages::Column::TaskId.eq(task_uuid))
            .filter(a2a_messages::Column::AgentName.eq(&self.agent_name))
            .order_by_asc(a2a_messages::Column::SequenceNumber)
            .all(self.db.as_ref())
            .await
            .map_err(|e| A2aError::StorageError(format!("Failed to query messages: {}", e)))?;

        let history = if message_models.is_empty() {
            None
        } else {
            let messages: Result<Vec<Message>, A2aError> = message_models
                .iter()
                .map(|m| {
                    let role = string_to_message_role(&m.role);
                    let parts = json_to_parts(&m.parts)?;
                    Ok(Message {
                        kind: MessageKind::Message,
                        role,
                        parts,
                        metadata: m.metadata.as_ref().and_then(json_to_metadata),
                        extensions: None,
                        reference_task_ids: None,
                        message_id: m.id.to_string(),
                        task_id: m.task_id.map(|id| id.to_string()),
                        context_id: m
                            .context_id
                            .clone()
                            .or_else(|| task_model.context_id.clone()),
                    })
                })
                .collect();
            Some(messages?)
        };

        // 4. Query artifacts
        let artifact_models = a2a_artifacts::Entity::find()
            .filter(a2a_artifacts::Column::TaskId.eq(task_uuid))
            .filter(a2a_artifacts::Column::AgentName.eq(&self.agent_name))
            .order_by_asc(a2a_artifacts::Column::SequenceNumber)
            .all(self.db.as_ref())
            .await
            .map_err(|e| A2aError::StorageError(format!("Failed to query artifacts: {}", e)))?;

        let artifacts = if artifact_models.is_empty() {
            None
        } else {
            let artifacts: Result<Vec<Artifact>, A2aError> = artifact_models
                .iter()
                .map(|a| {
                    let parts = json_to_parts(&a.parts)?;
                    Ok(Artifact {
                        artifact_id: a.id.to_string(),
                        name: None,
                        description: a.description.clone(),
                        parts,
                        metadata: a.metadata.as_ref().and_then(json_to_metadata),
                        extensions: None,
                    })
                })
                .collect();
            Some(artifacts?)
        };

        // 5. Build Task
        let task = Task {
            id: task_id,
            context_id: task_model.context_id.unwrap_or_default(),
            status,
            history,
            artifacts,
            metadata: json_to_metadata(&task_model.metadata),
            kind: a2a::types::TaskKind::Task,
        };

        Ok(Some(task))
    }

    async fn update_task(&self, task: Task) -> Result<Task, A2aError> {
        // Parse task ID as UUID
        let task_uuid = Uuid::parse_str(&task.id)
            .map_err(|e| A2aError::InvalidTask(format!("Invalid task ID: {}", e)))?;

        // Start a transaction
        let txn =
            self.db.begin().await.map_err(|e| {
                A2aError::StorageError(format!("Failed to start transaction: {}", e))
            })?;

        // 1. Update task metadata
        let existing_task = a2a_tasks::Entity::find_by_id(task_uuid)
            .filter(a2a_tasks::Column::AgentName.eq(&self.agent_name))
            .one(&txn)
            .await
            .map_err(|e| A2aError::StorageError(format!("Failed to query task: {}", e)))?;

        let existing_task = existing_task
            .ok_or_else(|| A2aError::InvalidTask(format!("Task not found: {}", task.id)))?;

        let mut task_active: a2a_tasks::ActiveModel = existing_task.into();
        task_active.state = ActiveValue::Set(task_state_to_string(&task.status.state));
        task_active.context_id = ActiveValue::Set(Some(task.context_id.clone()));
        task_active.metadata = ActiveValue::Set(metadata_to_json(&task.metadata)?);
        task_active.updated_at = ActiveValue::NotSet; // Will be set by DB trigger

        task_active
            .update(&txn)
            .await
            .map_err(|e| A2aError::StorageError(format!("Failed to update task: {}", e)))?;

        // 2. Append new status update
        let status_model = a2a_task_status::ActiveModel {
            id: ActiveValue::Set(Uuid::new_v4()),
            task_id: ActiveValue::Set(task_uuid),
            agent_name: ActiveValue::Set(self.agent_name.clone()),
            state: ActiveValue::Set(task_state_to_string(&task.status.state)),
            message_id: ActiveValue::Set(None),
            metadata: ActiveValue::Set(task.status.timestamp.as_ref().map(|ts| {
                serde_json::json!({
                    "timestamp": ts
                })
            })),
            created_at: ActiveValue::NotSet,
        };

        status_model
            .insert(&txn)
            .await
            .map_err(|e| A2aError::StorageError(format!("Failed to insert status: {}", e)))?;

        // 3. Append new messages if any
        if let Some(ref history) = task.history {
            // Get current max sequence number
            let max_seq = a2a_messages::Entity::find()
                .filter(a2a_messages::Column::TaskId.eq(task_uuid))
                .filter(a2a_messages::Column::AgentName.eq(&self.agent_name))
                .order_by_desc(a2a_messages::Column::SequenceNumber)
                .one(&txn)
                .await
                .map_err(|e| A2aError::StorageError(format!("Failed to query messages: {}", e)))?
                .map(|m| m.sequence_number)
                .unwrap_or(-1);

            // Insert new messages starting from max_seq + 1
            for (idx, message) in history.iter().enumerate() {
                let message_uuid =
                    Uuid::parse_str(&message.message_id).unwrap_or_else(|_| Uuid::new_v4());

                let sequence_number = max_seq + 1 + (idx as i32);

                let message_task_id = message
                    .task_id
                    .as_ref()
                    .and_then(|id| Uuid::parse_str(id).ok())
                    .or(Some(task_uuid));

                let message_context_id = message
                    .context_id
                    .clone()
                    .or_else(|| Some(task.context_id.clone()));

                let message_model = a2a_messages::ActiveModel {
                    id: ActiveValue::Set(message_uuid),
                    task_id: ActiveValue::Set(message_task_id),
                    context_id: ActiveValue::Set(message_context_id),
                    agent_name: ActiveValue::Set(self.agent_name.clone()),
                    role: ActiveValue::Set(message_role_to_string(&message.role)),
                    sequence_number: ActiveValue::Set(sequence_number),
                    parts: ActiveValue::Set(parts_to_json(&message.parts)?),
                    metadata: ActiveValue::Set(
                        message
                            .metadata
                            .as_ref()
                            .map(|m| serde_json::to_value(m).unwrap_or(serde_json::json!({}))),
                    ),
                    created_at: ActiveValue::NotSet,
                };

                message_model.insert(&txn).await.map_err(|e| {
                    A2aError::StorageError(format!("Failed to insert message: {}", e))
                })?;
            }
        }

        // 4. Append new artifacts if any
        if let Some(ref artifacts) = task.artifacts {
            // Get current max sequence number
            let max_seq = a2a_artifacts::Entity::find()
                .filter(a2a_artifacts::Column::TaskId.eq(task_uuid))
                .filter(a2a_artifacts::Column::AgentName.eq(&self.agent_name))
                .order_by_desc(a2a_artifacts::Column::SequenceNumber)
                .one(&txn)
                .await
                .map_err(|e| A2aError::StorageError(format!("Failed to query artifacts: {}", e)))?
                .map(|a| a.sequence_number)
                .unwrap_or(-1);

            // Insert new artifacts starting from max_seq + 1
            for (idx, artifact) in artifacts.iter().enumerate() {
                let artifact_uuid =
                    Uuid::parse_str(&artifact.artifact_id).unwrap_or_else(|_| Uuid::new_v4());

                let sequence_number = max_seq + 1 + (idx as i32);

                let artifact_model = a2a_artifacts::ActiveModel {
                    id: ActiveValue::Set(artifact_uuid),
                    task_id: ActiveValue::Set(task_uuid),
                    agent_name: ActiveValue::Set(self.agent_name.clone()),
                    sequence_number: ActiveValue::Set(sequence_number),
                    description: ActiveValue::Set(artifact.description.clone()),
                    parts: ActiveValue::Set(parts_to_json(&artifact.parts)?),
                    storage_location: ActiveValue::Set(None),
                    size_bytes: ActiveValue::Set(None),
                    metadata: ActiveValue::Set(
                        artifact
                            .metadata
                            .as_ref()
                            .map(|m| serde_json::to_value(m).unwrap_or(serde_json::json!({}))),
                    ),
                    created_at: ActiveValue::NotSet,
                };

                artifact_model.insert(&txn).await.map_err(|e| {
                    A2aError::StorageError(format!("Failed to insert artifact: {}", e))
                })?;
            }
        }

        // Commit transaction
        txn.commit()
            .await
            .map_err(|e| A2aError::StorageError(format!("Failed to commit transaction: {}", e)))?;

        Ok(task)
    }

    async fn list_tasks(&self, filters: TaskFilters) -> Result<Vec<Task>, A2aError> {
        // Build query for task metadata only
        let mut query =
            a2a_tasks::Entity::find().filter(a2a_tasks::Column::AgentName.eq(&self.agent_name));

        // Apply filters
        if let Some(context_id) = &filters.context_id {
            query = query.filter(a2a_tasks::Column::ContextId.eq(context_id));
        }

        if let Some(state) = &filters.state {
            query = query.filter(a2a_tasks::Column::State.eq(task_state_to_string(state)));
        }

        // Apply pagination
        if let Some(offset) = filters.offset {
            query = query.offset(offset as u64);
        }

        if let Some(limit) = filters.limit {
            query = query.limit(limit as u64);
        }

        // Order by creation date descending
        query = query.order_by_desc(a2a_tasks::Column::CreatedAt);

        // Execute query
        let task_models = query
            .all(self.db.as_ref())
            .await
            .map_err(|e| A2aError::StorageError(format!("Failed to list tasks: {}", e)))?;

        // Convert to Task objects (load only metadata, not full history/artifacts for efficiency)
        let tasks: Result<Vec<Task>, A2aError> = task_models
            .iter()
            .map(|model| {
                let state = string_to_task_state(&model.state);
                let status = TaskStatus::new(state);

                Ok(Task {
                    id: model.id.to_string(),
                    context_id: model.context_id.clone().unwrap_or_default(),
                    status,
                    history: None,   // Not loaded for efficiency
                    artifacts: None, // Not loaded for efficiency
                    metadata: json_to_metadata(&model.metadata),
                    kind: a2a::types::TaskKind::Task,
                })
            })
            .collect();

        tasks
    }

    async fn delete_task(&self, task_id: String) -> Result<(), A2aError> {
        // Parse task ID as UUID
        let task_uuid = Uuid::parse_str(&task_id)
            .map_err(|e| A2aError::InvalidTask(format!("Invalid task ID: {}", e)))?;

        // Delete task (cascade will delete status, messages, and artifacts automatically)
        let result = a2a_tasks::Entity::delete_by_id(task_uuid)
            .filter(a2a_tasks::Column::AgentName.eq(&self.agent_name))
            .exec(self.db.as_ref())
            .await
            .map_err(|e| A2aError::StorageError(format!("Failed to delete task: {}", e)))?;

        if result.rows_affected == 0 {
            return Err(A2aError::InvalidTask(format!(
                "Task not found or not owned by agent: {}",
                task_id
            )));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_state_conversion() {
        assert_eq!(task_state_to_string(&TaskState::Working), "working");
        assert_eq!(task_state_to_string(&TaskState::Completed), "completed");
        assert_eq!(string_to_task_state("working"), TaskState::Working);
        assert_eq!(string_to_task_state("completed"), TaskState::Completed);
    }

    #[test]
    fn test_message_role_conversion() {
        assert_eq!(message_role_to_string(&MessageRole::User), "user");
        assert_eq!(message_role_to_string(&MessageRole::Agent), "agent");
        assert_eq!(string_to_message_role("user"), MessageRole::User);
        assert_eq!(string_to_message_role("agent"), MessageRole::Agent);
    }

    #[test]
    fn test_metadata_conversion() {
        let mut metadata = HashMap::new();
        metadata.insert("key".to_string(), serde_json::json!("value"));
        let json = metadata_to_json(&Some(metadata.clone())).unwrap();
        let converted = json_to_metadata(&json);
        assert_eq!(converted, Some(metadata));

        // Test empty metadata
        let json = metadata_to_json(&None).unwrap();
        assert!(json.is_object());
        assert!(json.as_object().unwrap().is_empty());
        assert_eq!(json_to_metadata(&json), None);
    }
}
