use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub struct RunningTask {
    pub task_id: String,
    pub join_handle: JoinHandle<()>,
    pub cancellation_token: CancellationToken,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct TaskManager {
    stop_timeout: tokio::time::Duration,
    tasks: Arc<RwLock<HashMap<String, RunningTask>>>,
}

impl TaskManager {
    pub fn new() -> Self {
        Self {
            stop_timeout: tokio::time::Duration::from_secs(10),
            tasks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn spawn<F, Fut>(&self, name: impl Into<String>, task_fn: F)
    where
        F: FnOnce(CancellationToken) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let task_id = name.into();
        let task_id_clone = task_id.clone();
        let cancellation_token = CancellationToken::new();
        let child_token = cancellation_token.clone();

        let tasks = Arc::clone(&self.tasks);

        let join_handle = tokio::spawn(async move {
            task_fn(child_token).await;
            tokio::spawn(async move {
                tracing::info!("Task {task_id} completed. Cleaning up...");
                tasks.write().await.remove(&task_id);
            });
        });

        if join_handle.is_finished() {
            tracing::warn!("Task {task_id_clone} finished immediately, not registering.");
            return;
        }
        let task = RunningTask {
            task_id: task_id_clone.clone(),
            join_handle,
            cancellation_token,
            created_at: chrono::Utc::now(),
        };
        self.tasks.write().await.insert(task_id_clone, task);
    }

    pub async fn has_task(&self, task_id: impl Into<String>) -> bool {
        let task_id: String = task_id.into();
        self.tasks.read().await.contains_key(&task_id)
    }

    pub async fn register_task(
        &self,
        task_id: impl Into<String>,
        join_handle: JoinHandle<()>,
        cancellation_token: CancellationToken,
    ) {
        let task_id = task_id.into();
        let task = RunningTask {
            task_id: task_id.clone(),
            join_handle,
            cancellation_token,
            created_at: chrono::Utc::now(),
        };

        self.tasks.write().await.insert(task_id.clone(), task);
        tracing::info!("Registered task for thread: {task_id}");
    }

    pub async fn cancel_task(&self, task_id: impl Into<String>) -> Result<bool, String> {
        let mut tasks = self.tasks.write().await;
        let task_id: String = task_id.into();

        if let Some(task) = tasks.remove(&task_id) {
            task.cancellation_token.cancel();
            let timeout = tokio::time::sleep(self.stop_timeout);
            let mut join_handle = task.join_handle;
            tracing::info!("Sent cancellation signal for thread: {task_id}");
            tokio::select! {
                _ = &mut join_handle => {
                    tracing::info!("Task for thread: {task_id} has been cancelled successfully.");
                }
                _ = timeout => {
                    tracing::warn!("Task for thread: {task_id} did not complete in time, forcefully aborting...");
                    join_handle.abort();
                }
            }

            Ok(true)
        } else {
            tracing::warn!("Task not found for thread: {task_id}");
            Ok(false)
        }
    }

    pub async fn remove_task(&self, task_id: impl Into<String>) {
        let task_id: String = task_id.into();
        self.tasks.write().await.remove(&task_id);
        tracing::info!("Removed task for thread: {task_id}");
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
