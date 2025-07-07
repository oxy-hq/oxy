use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Clone)]
pub struct RunningTask {
    pub task_id: Uuid,
    pub join_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
    pub cancellation_token: CancellationToken,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct TaskManager {
    tasks: Arc<RwLock<HashMap<Uuid, RunningTask>>>,
}

impl TaskManager {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register_task(
        &self,
        task_id: Uuid,
        join_handle: JoinHandle<()>,
        cancellation_token: CancellationToken,
    ) {
        let task = RunningTask {
            task_id,
            join_handle: Arc::new(Mutex::new(Some(join_handle))),
            cancellation_token,
            created_at: chrono::Utc::now(),
        };

        self.tasks.write().await.insert(task_id, task);
        tracing::info!("Registered task for thread: {}", task_id);
    }

    pub async fn cancel_task(&self, task_id: Uuid) -> Result<bool, String> {
        let tasks = self.tasks.read().await;

        if let Some(task) = tasks.get(&task_id) {
            task.cancellation_token.cancel();
            tracing::info!("Sent cancellation signal for thread: {}", task_id);

            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

            if let Some(handle) = task.join_handle.lock().await.take() {
                if !handle.is_finished() {
                    tracing::warn!(
                        "Task did not respond to cancellation signal, aborting forcefully for thread: {}",
                        task_id
                    );
                    handle.abort();
                    tracing::info!("Aborted task for thread: {}", task_id);
                }
            }

            tracing::info!("Cancelled task for thread: {}", task_id);
            Ok(true)
        } else {
            tracing::warn!("Task not found for thread: {}", task_id);
            Ok(false)
        }
    }

    pub async fn remove_task(&self, task_id: Uuid) {
        self.tasks.write().await.remove(&task_id);
        tracing::info!("Removed task for thread: {}", task_id);
    }
}

impl Default for TaskManager {
    fn default() -> Self {
        Self::new()
    }
}

lazy_static::lazy_static! {
    pub static ref TASK_MANAGER: TaskManager = TaskManager::new();
}
