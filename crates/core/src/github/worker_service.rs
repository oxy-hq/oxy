use crate::errors::OxyError;
use crate::github::background_tasks;
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

    let worker = WorkerBuilder::new("oxy-clone-worker")
        .data(0usize)
        .backend(storage.clone())
        .build_fn(background_tasks::process_clone_job);

    // testing long clone job
    sleep(std::time::Duration::from_secs(10)).await;

    Monitor::new()
        .register(worker)
        .run()
        .await
        .map_err(|e| OxyError::InitializationError(format!("Worker failed: {e}")))?;

    Ok(())
}
