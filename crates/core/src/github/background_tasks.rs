use crate::github::types::*;
use crate::{errors::OxyError, state_dir::get_state_dir};
use apalis::prelude::*;
use apalis_sql::sqlite::SqliteStorage;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

/// Job payload for repository cloning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloneRepositoryJob {
    pub repository: GitHubRepository,
    pub task_id: String,
}

/// Background task manager using Apalis for handling repository operations
pub struct BackgroundTaskManager {
    storage: SqliteStorage<CloneRepositoryJob>,
}

impl BackgroundTaskManager {
    /// Create a new background task manager
    pub async fn new() -> Result<Self, OxyError> {
        // Create a SQLite connection for Apalis
        let db_url = std::env::var("OXY_DATABASE_URL").unwrap_or_else(|_| {
            let state_dir = get_state_dir();
            format!("sqlite://{}/db.sqlite", state_dir.to_str().unwrap())
        });

        let pool = SqlitePool::connect(&db_url).await.map_err(|e| {
            OxyError::InitializationError(format!("Failed to connect to SQLite: {e}"))
        })?;

        let _ = SqliteStorage::setup(&pool).await;

        let storage = SqliteStorage::new(pool);

        Ok(Self { storage })
    }

    /// Start a background repository cloning task
    pub async fn start_clone_task(
        &mut self,
        repository: GitHubRepository,
    ) -> Result<String, OxyError> {
        let task_id = format!("clone_{}", repository.id);
        let job = CloneRepositoryJob {
            repository: repository.clone(),
            task_id: task_id.clone(),
        };

        // Push job to Apalis queue
        self.storage
            .push(job)
            .await
            .map_err(|e| OxyError::JobError(format!("Failed to enqueue clone job: {e}")))?;

        Ok(task_id)
    }

    /// Get the storage for use with Apalis worker
    pub fn get_storage(&self) -> &SqliteStorage<CloneRepositoryJob> {
        &self.storage
    }
}

/// Process a clone repository job - this is the actual job handler for Apalis
pub async fn process_clone_job(job: CloneRepositoryJob, _data: Data<usize>) -> Result<(), Error> {
    info!(
        "Processing clone job for repository ID: {}",
        job.repository.id
    );

    let _repository = &job.repository;
    let _task_id = &job.task_id;
    Ok(())
}

/// Start a background repository cloning task using the global instance
pub async fn start_clone_task(repository: GitHubRepository) -> Result<String, OxyError> {
    let instance = get_instance().await?;
    let mut manager = instance.lock().await;
    manager.start_clone_task(repository).await
}

/// Initialize the global background task manager instance
/// This should be called once during server startup
pub async fn initialize_background_task_manager() -> Result<(), OxyError> {
    // This will trigger the lazy initialization
    get_instance().await?;
    info!("Background task manager initialized successfully");
    Ok(())
}

/// Get the global background task manager instance
pub async fn get_instance() -> Result<Arc<Mutex<BackgroundTaskManager>>, OxyError> {
    static INSTANCE: OnceCell<Arc<Mutex<BackgroundTaskManager>>> = OnceCell::new();

    match INSTANCE.get() {
        Some(instance) => Ok(instance.clone()),
        None => {
            let manager = BackgroundTaskManager::new().await?;
            let arc_manager = Arc::new(Mutex::new(manager));
            match INSTANCE.set(arc_manager.clone()) {
                Ok(_) => Ok(arc_manager),
                Err(_) => Err(OxyError::InitializationError(
                    "Failed to initialize background task manager singleton".to_string(),
                )),
            }
        }
    }
}
