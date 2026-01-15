use crate::loggers::types::LogItem;
use entity::threads::{ActiveModel, Model};
use oxy_shared::errors::OxyError;
use sea_orm::{ActiveModelTrait, ActiveValue, DatabaseConnection};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};

const DEFAULT_FLUSH_INTERVAL: Duration = Duration::from_millis(1000);

#[derive(Debug, Clone)]
pub struct StreamingWorkflowPersister {
    connection: DatabaseConnection,
    thread: Arc<Mutex<ActiveModel>>,
    current_output: Arc<Mutex<Vec<LogItem>>>,
    last_updated: Arc<Mutex<Instant>>,
}

impl StreamingWorkflowPersister {
    pub async fn new(connection: DatabaseConnection, thread: Model) -> Result<Self, OxyError> {
        let thread_model: ActiveModel = thread.clone().into();
        Ok(Self {
            connection,
            thread: Arc::new(Mutex::new(thread_model)),
            current_output: Arc::new(Mutex::new(Vec::new())),
            last_updated: Arc::new(Mutex::new(Instant::now())),
        })
    }

    pub async fn append_output(&self, output: &LogItem) -> Result<(), OxyError> {
        {
            let mut current_output = self.current_output.lock().await;
            current_output.push(output.clone());
        }

        let should_flush = {
            let last_updated = self.last_updated.lock().await;
            last_updated.elapsed() > DEFAULT_FLUSH_INTERVAL
        };

        if should_flush {
            self.update_output().await?;
        }

        Ok(())
    }

    pub async fn update_output(&self) -> Result<(), OxyError> {
        let current_output = {
            let current_output = self.current_output.lock().await;
            if current_output.is_empty() {
                {
                    let mut last_updated = self.last_updated.lock().await;
                    *last_updated = Instant::now();
                }
                return Ok(());
            }
            current_output.clone()
        };

        let mut thread_guard = self.thread.lock().await;

        let mut temp_model = thread_guard.clone();
        temp_model.output =
            ActiveValue::Set(serde_json::to_string(&current_output).unwrap_or_else(|e| {
                tracing::error!("Failed to serialize workflow output: {}", e);
                "[]".to_string()
            }));

        temp_model.update(&self.connection).await.map_err(|err| {
            tracing::error!("Failed to update streaming workflow message: {}", err);
            OxyError::DBError(format!(
                "Failed to update streaming workflow message: {err}"
            ))
        })?;

        // Only clear the buffer and update timestamp after successful database update
        {
            let mut current_output_guard = self.current_output.lock().await;
            current_output_guard.clear();
        }

        thread_guard.output =
            ActiveValue::Set(serde_json::to_string(&current_output).unwrap_or_else(|e| {
                tracing::error!(
                    "Failed to serialize workflow output for thread guard: {}",
                    e
                );
                "[]".to_string()
            }));

        {
            let mut last_updated = self.last_updated.lock().await;
            *last_updated = Instant::now();
        }

        tracing::debug!(
            "Successfully updated workflow output with {} items",
            current_output.len()
        );
        Ok(())
    }

    /// Force flush any pending output and clear the buffer
    pub async fn finalize(&self) -> Result<(), OxyError> {
        self.update_output().await?;

        // Final cleanup
        {
            let mut current_output = self.current_output.lock().await;
            current_output.clear();
        }

        Ok(())
    }
}
