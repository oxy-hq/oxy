use crate::github::background_tasks;
use apalis::prelude::*;
use oxy_shared::errors::OxyError;
use tokio::time::sleep;
use tracing::info;

/// Start the Apalis worker service for background tasks with PostgreSQL
pub async fn start_apalis_worker() -> Result<(), OxyError> {
    info!("Starting Apalis worker for background tasks with PostgreSQL");

    // Initialize the background task manager infrastructure
    // This ensures the storage and job tables are set up properly
    let instance = background_tasks::get_instance().await?;

    info!("Background task manager initialized for worker mode");
    let storage = instance.lock().await.get_storage().clone();

    // testing long clone job
    sleep(std::time::Duration::from_secs(10)).await;

    let worker = WorkerBuilder::new("oxy-clone-worker-postgres")
        .data(0usize)
        .backend(storage)
        .build_fn(background_tasks::process_clone_job);

    Monitor::new()
        .register(worker)
        .run()
        .await
        .map_err(|e| OxyError::InitializationError(format!("Worker failed: {e}")))?;

    Ok(())
}
