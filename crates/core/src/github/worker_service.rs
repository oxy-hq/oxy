use crate::errors::OxyError;
use crate::github::background_tasks::{self, TaskStorage};
use apalis::prelude::*;
use tokio::time::sleep;
use tracing::info;

/// Start the Apalis worker service for background tasks  
pub async fn start_apalis_worker() -> Result<(), OxyError> {
    info!("Starting Apalis worker for background tasks");

    // Initialize the background task manager infrastructure
    // This ensures the storage and job tables are set up properly
    let _instance = crate::github::background_tasks::get_instance().await?;

    info!("Background task manager initialized for worker mode");
    let storage = _instance.lock().await.get_storage().clone();

    // testing long clone job
    sleep(std::time::Duration::from_secs(10)).await;

    match storage {
        TaskStorage::Postgres(postgres_storage) => start_postgres_worker(postgres_storage).await,
        TaskStorage::Sqlite(sqlite_storage) => start_sqlite_worker(sqlite_storage).await,
    }
}

async fn start_postgres_worker(
    storage: apalis_sql::postgres::PostgresStorage<background_tasks::CloneRepositoryJob>,
) -> Result<(), OxyError> {
    let worker = WorkerBuilder::new("oxy-clone-worker-postgres")
        .data(0usize)
        .backend(storage)
        .build_fn(background_tasks::process_clone_job);

    Monitor::new()
        .register(worker)
        .run()
        .await
        .map_err(|e| OxyError::InitializationError(format!("Postgres worker failed: {e}")))?;

    Ok(())
}

async fn start_sqlite_worker(
    storage: apalis_sql::sqlite::SqliteStorage<background_tasks::CloneRepositoryJob>,
) -> Result<(), OxyError> {
    let worker = WorkerBuilder::new("oxy-clone-worker-sqlite")
        .data(0usize)
        .backend(storage)
        .build_fn(background_tasks::process_clone_job);

    Monitor::new()
        .register(worker)
        .run()
        .await
        .map_err(|e| OxyError::InitializationError(format!("SQLite worker failed: {e}")))?;

    Ok(())
}
