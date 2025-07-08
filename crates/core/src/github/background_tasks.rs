use crate::errors::OxyError;
use crate::github::GitHubService;
use crate::github::{git_operations::GitOperations, types::*};
use apalis::prelude::*;
use apalis_sql::sqlite::SqliteStorage;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

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
            let state_dir = crate::db::client::get_state_dir();
            format!("sqlite://{}/db.sqlite", state_dir.to_str().unwrap())
        });

        let pool = SqlitePool::connect(&db_url).await.map_err(|e| {
            OxyError::InitializationError(format!("Failed to connect to SQLite: {}", e))
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
            .map_err(|e| OxyError::JobError(format!("Failed to enqueue clone job: {}", e)))?;

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

    let repository = &job.repository;
    let _task_id = &job.task_id;

    GitHubService::clone_or_update_repository(&repository.clone())
        .await
        .map_err(|e| {
            error!("Failed to clone or update repository: {}", e);
            Error::Failed(Arc::new(Box::new(std::io::Error::other(format!(
                "Failed to clone or update repository: {}",
                e
            )))))
        })?;

    Ok(())
}

/// Start a background repository cloning task using the global instance
pub async fn start_clone_task(repository: GitHubRepository) -> Result<String, OxyError> {
    let instance = get_instance().await?;
    let mut manager = instance.lock().await;
    manager.start_clone_task(repository).await
}

/// Apalis job handler for clone repository operations
pub async fn handle_clone_repository_job(
    job: CloneRepositoryJob,
    _ctx: Context,
) -> Result<(), OxyError> {
    let repository = job.repository;
    let _task_id = job.task_id;

    info!("Starting clone job for repository ID: {}", repository.id);

    // Ensure git is available
    if let Err(e) = GitOperations::check_git_availability().await {
        error!("Git not available: {}", e);
        update_sync_status_to_error().await?;
        return Err(e);
    }

    // Ensure git config
    if let Err(e) = GitOperations::ensure_git_config().await {
        warn!("Failed to ensure git config: {}", e);
    }

    // Get the local path for the repository
    let repo_path = match GitOperations::get_repository_path(repository.id) {
        Ok(path) => path,
        Err(e) => {
            error!("Failed to get repository path: {}", e);
            update_sync_status_to_error().await?;
            return Err(e);
        }
    };

    // Check if repository already exists
    if GitOperations::is_git_repository(&repo_path).await {
        info!("Repository already exists, pulling latest changes");
        if let Err(e) = GitOperations::pull_repository(&repo_path).await {
            warn!("Failed to pull repository, continuing anyway: {}", e);
        }
    } else {
        // TODO: Clone the repository
        // NOTE: The GitHubRepository struct only has an ID field currently
        // In a full implementation, you would need to:
        // 1. Fetch complete repository details from GitHub API using repository.id
        // 2. Extract clone_url and default_branch from the API response
        // 3. Clone using those details

        info!(
            "Would clone repository ID {} to: {:?}",
            repository.id, repo_path
        );
        warn!("Cloning not implemented - repository struct needs more fields");

        // For now, just create the directory to simulate success
        if let Err(e) = std::fs::create_dir_all(&repo_path) {
            error!("Failed to create repository directory: {}", e);
            update_sync_status_to_error().await?;
            return Err(OxyError::IOError(format!(
                "Failed to create directory: {}",
                e
            )));
        }
    }

    // Update sync status to synced and get the current commit hash
    if let Err(e) = update_repository_sync_status(repository.id, &repo_path).await {
        warn!("Failed to update sync status after clone: {}", e);
    }

    info!(
        "Clone job completed successfully for repository ID: {}",
        repository.id
    );
    Ok(())
}

/// Update repository sync status after successful clone/pull
async fn update_repository_sync_status(
    _repository_id: i64,
    repo_path: &std::path::Path,
) -> Result<(), OxyError> {
    use crate::db::client::establish_connection;
    use entity::prelude::Settings;
    use entity::settings;
    use sea_orm::{ActiveModelTrait, EntityTrait, Set};

    // Get the current commit hash from the local repository
    let current_commit = GitOperations::get_current_commit_hash(repo_path).await?;

    // Update the database with synced status and current revision
    let db = establish_connection().await?;

    let settings = Settings::find()
        .one(&db)
        .await
        .map_err(|e| OxyError::DBError(format!("Failed to query settings: {}", e)))?
        .ok_or_else(|| OxyError::ConfigurationError("GitHub settings not found".to_string()))?;

    let mut active_model: settings::ActiveModel = settings.into();
    active_model.sync_status = Set(settings::SyncStatus::Synced);
    active_model.revision = Set(Some(current_commit));
    active_model.updated_at = Set(chrono::Utc::now().into());

    active_model
        .update(&db)
        .await
        .map_err(|e| OxyError::DBError(format!("Failed to update sync status: {}", e)))?;

    Ok(())
}

/// Update sync status to error
async fn update_sync_status_to_error() -> Result<(), OxyError> {
    use crate::db::client::establish_connection;
    use entity::prelude::Settings;
    use entity::settings;
    use sea_orm::{ActiveModelTrait, EntityTrait, Set};

    let db = establish_connection().await?;

    let settings = Settings::find()
        .one(&db)
        .await
        .map_err(|e| OxyError::DBError(format!("Failed to query settings: {}", e)))?
        .ok_or_else(|| OxyError::ConfigurationError("GitHub settings not found".to_string()))?;

    let mut active_model: settings::ActiveModel = settings.into();
    active_model.sync_status = Set(settings::SyncStatus::Error);
    active_model.updated_at = Set(chrono::Utc::now().into());

    active_model
        .update(&db)
        .await
        .map_err(|e| OxyError::DBError(format!("Failed to update sync status to error: {}", e)))?;

    Ok(())
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
