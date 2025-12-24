//! Task storage abstraction for A2A protocol.
//!
//! This module provides a storage abstraction for managing A2A tasks. The storage
//! is designed to be scoped to a single agent - multi-agent routing and isolation
//! is handled by the consumer (e.g., the core crate).
//!
//! # Architecture Notes
//!
//! - **Single-Agent Scope**: The `TaskStorage` trait operates on a single agent's
//!   task space. There are no `agent_name` parameters in the trait methods.
//! - **Multi-Agent Handling**: The consumer (core crate) creates separate storage
//!   instances per agent and routes requests appropriately.
//! - **Handler Scoping**: Handler instances are scoped to specific agents with
//!   `agent_name` as a field, not in the context.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::error::A2aError;
use crate::types::{Task, TaskState};

/// Filters for querying tasks.
///
/// All filters are optional. When multiple filters are specified, they are
/// combined with AND logic.
#[derive(Debug, Clone, Default)]
pub struct TaskFilters {
    /// Filter by context ID - only return tasks with this context
    pub context_id: Option<String>,

    /// Filter by task state - only return tasks in this state
    pub state: Option<TaskState>,

    /// Limit the number of results returned
    pub limit: Option<usize>,

    /// Offset for pagination (skip this many results)
    pub offset: Option<usize>,
}

impl TaskFilters {
    /// Create a new empty filter set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by context ID.
    pub fn with_context_id(mut self, context_id: impl Into<String>) -> Self {
        self.context_id = Some(context_id.into());
        self
    }

    /// Filter by task state.
    pub fn with_state(mut self, state: TaskState) -> Self {
        self.state = Some(state);
        self
    }

    /// Set result limit.
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set result offset for pagination.
    pub fn with_offset(mut self, offset: usize) -> Self {
        self.offset = Some(offset);
        self
    }
}

/// Trait for task storage implementations.
///
/// This trait defines the interface for storing and retrieving A2A tasks.
/// Implementations should be scoped to a single agent - the consumer is
/// responsible for creating separate storage instances per agent.
///
/// # Single-Agent Scope
///
/// This trait operates on a single agent's task space. There are no `agent_name`
/// parameters. The consumer (core crate) handles multi-agent routing by:
/// - Creating separate storage instances per agent
/// - Routing requests to the correct storage instance
/// - Managing agent identity and isolation
///
/// # Thread Safety
///
/// Implementations must be thread-safe (`Send + Sync`) as they will be shared
/// across async tasks.
#[async_trait]
pub trait TaskStorage: Send + Sync {
    /// Create a new task.
    ///
    /// # Arguments
    ///
    /// * `task` - The task to create. The task ID should already be set.
    ///
    /// # Returns
    ///
    /// The created task, potentially with additional fields populated by the storage.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - A task with the same ID already exists
    /// - The storage backend fails
    async fn create_task(&self, task: Task) -> Result<Task, A2aError>;

    /// Get a task by ID.
    ///
    /// # Arguments
    ///
    /// * `task_id` - The unique identifier of the task
    ///
    /// # Returns
    ///
    /// `Some(task)` if found, `None` if not found
    ///
    /// # Errors
    ///
    /// Returns an error if the storage backend fails.
    async fn get_task(&self, task_id: String) -> Result<Option<Task>, A2aError>;

    /// Update an existing task.
    ///
    /// # Arguments
    ///
    /// * `task` - The task with updated fields. The task ID is used to identify
    ///   which task to update.
    ///
    /// # Returns
    ///
    /// The updated task
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The task does not exist
    /// - The storage backend fails
    async fn update_task(&self, task: Task) -> Result<Task, A2aError>;

    /// List tasks matching the given filters.
    ///
    /// # Arguments
    ///
    /// * `filters` - Filter criteria for tasks. All filters are optional and
    ///   combined with AND logic.
    ///
    /// # Returns
    ///
    /// A vector of tasks matching the filters. The results respect the `limit`
    /// and `offset` parameters for pagination.
    ///
    /// # Errors
    ///
    /// Returns an error if the storage backend fails.
    async fn list_tasks(&self, filters: TaskFilters) -> Result<Vec<Task>, A2aError>;

    /// Delete a task by ID.
    ///
    /// # Arguments
    ///
    /// * `task_id` - The unique identifier of the task to delete
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The task does not exist
    /// - The storage backend fails
    async fn delete_task(&self, task_id: String) -> Result<(), A2aError>;
}

/// In-memory task storage implementation for testing and development.
///
/// This implementation uses a `HashMap` protected by a `RwLock` for thread-safe
/// access. It's suitable for:
/// - Unit tests
/// - Integration tests
/// - Development environments
/// - Single-instance deployments where persistence is not required
///
/// For production use with multiple instances or persistence requirements,
/// implement `TaskStorage` with a database backend.
#[derive(Debug, Clone)]
pub struct InMemoryTaskStorage {
    tasks: Arc<RwLock<HashMap<String, Task>>>,
}

impl InMemoryTaskStorage {
    /// Create a new in-memory task storage.
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get the number of tasks currently stored.
    ///
    /// This is a helper method for testing.
    pub fn len(&self) -> usize {
        self.tasks.read().unwrap().len()
    }

    /// Check if the storage is empty.
    ///
    /// This is a helper method for testing.
    pub fn is_empty(&self) -> bool {
        self.tasks.read().unwrap().is_empty()
    }

    /// Clear all tasks from storage.
    ///
    /// This is a helper method for testing.
    pub fn clear(&self) {
        self.tasks.write().unwrap().clear();
    }
}

impl Default for InMemoryTaskStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TaskStorage for InMemoryTaskStorage {
    async fn create_task(&self, task: Task) -> Result<Task, A2aError> {
        let mut tasks = self.tasks.write().unwrap();

        // Check if task already exists
        if tasks.contains_key(&task.id) {
            return Err(A2aError::InvalidTask(format!(
                "Task with ID '{}' already exists",
                task.id
            )));
        }

        tasks.insert(task.id.clone(), task.clone());
        Ok(task)
    }

    async fn get_task(&self, task_id: String) -> Result<Option<Task>, A2aError> {
        let tasks = self.tasks.read().unwrap();
        Ok(tasks.get(&task_id).cloned())
    }

    async fn update_task(&self, task: Task) -> Result<Task, A2aError> {
        let mut tasks = self.tasks.write().unwrap();

        // Check if task exists
        if !tasks.contains_key(&task.id) {
            return Err(A2aError::InvalidTask(format!(
                "Task with ID '{}' not found",
                task.id
            )));
        }

        tasks.insert(task.id.clone(), task.clone());
        Ok(task)
    }

    async fn list_tasks(&self, filters: TaskFilters) -> Result<Vec<Task>, A2aError> {
        let tasks = self.tasks.read().unwrap();

        // Start with all tasks
        let mut filtered: Vec<Task> = tasks.values().cloned().collect();

        // Apply context filter
        if let Some(ref context_id) = filters.context_id {
            filtered.retain(|task| &task.context_id == context_id);
        }

        // Apply state filter
        if let Some(ref state) = filters.state {
            filtered.retain(|task| &task.status.state == state);
        }

        // Sort by task ID for consistent ordering (newest first if IDs are sequential)
        filtered.sort_by(|a, b| b.id.cmp(&a.id));

        // Apply offset
        if let Some(offset) = filters.offset {
            if offset < filtered.len() {
                filtered = filtered.into_iter().skip(offset).collect();
            } else {
                filtered.clear();
            }
        }

        // Apply limit
        if let Some(limit) = filters.limit {
            filtered.truncate(limit);
        }

        Ok(filtered)
    }

    async fn delete_task(&self, task_id: String) -> Result<(), A2aError> {
        let mut tasks = self.tasks.write().unwrap();

        // Check if task exists
        if !tasks.contains_key(&task_id) {
            return Err(A2aError::InvalidTask(format!(
                "Task with ID '{}' not found",
                task_id
            )));
        }

        tasks.remove(&task_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Message, Part, TaskStatus, TextPart};
    use uuid::Uuid;

    #[tokio::test]
    async fn test_create_and_get_task() {
        let storage = InMemoryTaskStorage::new();

        let task = Task {
            id: "task-1".to_string(),
            context_id: "context-1".to_string(),
            status: TaskStatus {
                state: TaskState::Working,
                message: Some(Message::new_agent(vec![Part::Text(TextPart::new(
                    "Processing",
                ))])),
                timestamp: Some(chrono::Utc::now().to_rfc3339()),
            },
            artifacts: None,
            history: None,
            metadata: None,
            kind: crate::types::TaskKind::Task,
        };

        // Create task
        let created = storage.create_task(task.clone()).await.unwrap();
        assert_eq!(created.id, task.id);

        // Get task
        let retrieved = storage.get_task("task-1".to_string()).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, "task-1");
    }

    #[tokio::test]
    async fn test_create_duplicate_task() {
        let storage = InMemoryTaskStorage::new();

        let task = Task {
            id: "task-1".to_string(),
            context_id: Uuid::new_v4().to_string(),
            status: TaskStatus {
                state: TaskState::Working,
                message: None,
                timestamp: Some(chrono::Utc::now().to_rfc3339()),
            },
            artifacts: None,
            history: None,
            metadata: None,
            kind: crate::types::TaskKind::Task,
        };

        // Create task
        storage.create_task(task.clone()).await.unwrap();

        // Try to create duplicate
        let result = storage.create_task(task).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_update_task() {
        let storage = InMemoryTaskStorage::new();

        let mut task = Task {
            id: "task-1".to_string(),
            context_id: Uuid::new_v4().to_string(),
            status: TaskStatus {
                state: TaskState::Working,
                message: None,
                timestamp: Some(chrono::Utc::now().to_rfc3339()),
            },
            artifacts: None,
            history: None,
            metadata: None,
            kind: crate::types::TaskKind::Task,
        };

        // Create task
        storage.create_task(task.clone()).await.unwrap();

        // Update task
        task.status.state = TaskState::Completed;
        let updated = storage.update_task(task.clone()).await.unwrap();
        assert_eq!(updated.status.state, TaskState::Completed);

        // Verify update
        let retrieved = storage.get_task("task-1".to_string()).await.unwrap();
        assert_eq!(retrieved.unwrap().status.state, TaskState::Completed);
    }

    #[tokio::test]
    async fn test_list_tasks_with_filters() {
        let storage = InMemoryTaskStorage::new();

        // Create multiple tasks
        for i in 1..=5 {
            let task = Task {
                id: format!("task-{}", i),
                context_id: format!("context-{}", i % 2),
                status: TaskStatus {
                    state: if i % 2 == 0 {
                        TaskState::Completed
                    } else {
                        TaskState::Working
                    },
                    message: None,
                    timestamp: Some(chrono::Utc::now().to_rfc3339()),
                },
                artifacts: None,
                history: None,
                metadata: None,
                kind: crate::types::TaskKind::Task,
            };
            storage.create_task(task).await.unwrap();
        }

        // List all tasks
        let all_tasks = storage.list_tasks(TaskFilters::new()).await.unwrap();
        assert_eq!(all_tasks.len(), 5);

        // Filter by state
        let working_tasks = storage
            .list_tasks(TaskFilters::new().with_state(TaskState::Working))
            .await
            .unwrap();
        assert_eq!(working_tasks.len(), 3);

        // Filter by context
        let context_0_tasks = storage
            .list_tasks(TaskFilters::new().with_context_id("context-0"))
            .await
            .unwrap();
        assert_eq!(context_0_tasks.len(), 2);

        // Test limit
        let limited_tasks = storage
            .list_tasks(TaskFilters::new().with_limit(3))
            .await
            .unwrap();
        assert_eq!(limited_tasks.len(), 3);

        // Test offset
        let offset_tasks = storage
            .list_tasks(TaskFilters::new().with_offset(2))
            .await
            .unwrap();
        assert_eq!(offset_tasks.len(), 3);
    }

    #[tokio::test]
    async fn test_delete_task() {
        let storage = InMemoryTaskStorage::new();

        let task = Task {
            id: "task-1".to_string(),
            context_id: Uuid::new_v4().to_string(),
            status: TaskStatus {
                state: TaskState::Working,
                message: None,
                timestamp: Some(chrono::Utc::now().to_rfc3339()),
            },
            artifacts: None,
            history: None,
            metadata: None,
            kind: crate::types::TaskKind::Task,
        };

        // Create task
        storage.create_task(task).await.unwrap();
        assert_eq!(storage.len(), 1);

        // Delete task
        storage.delete_task("task-1".to_string()).await.unwrap();
        assert_eq!(storage.len(), 0);

        // Try to get deleted task
        let retrieved = storage.get_task("task-1".to_string()).await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_task() {
        let storage = InMemoryTaskStorage::new();

        let result = storage.delete_task("nonexistent".to_string()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_helper_methods() {
        let storage = InMemoryTaskStorage::new();

        assert!(storage.is_empty());
        assert_eq!(storage.len(), 0);

        // Add a task
        let task = Task {
            id: "task-1".to_string(),
            context_id: Uuid::new_v4().to_string(),
            status: TaskStatus {
                state: TaskState::Working,
                message: None,
                timestamp: Some(chrono::Utc::now().to_rfc3339()),
            },
            artifacts: None,
            history: None,
            metadata: None,
            kind: crate::types::TaskKind::Task,
        };
        storage.create_task(task).await.unwrap();

        assert!(!storage.is_empty());
        assert_eq!(storage.len(), 1);

        // Clear storage
        storage.clear();
        assert!(storage.is_empty());
        assert_eq!(storage.len(), 0);
    }
}
