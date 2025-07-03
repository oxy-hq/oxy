use crate::errors::OxyError;
use crate::workflow::loggers::types::LogItem;
use entity::threads::{ActiveModel, Model};
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
            current_output
        };

        let mut thread_guard = self.thread.lock().await;

        let mut temp_model = thread_guard.clone();
        temp_model.output =
            ActiveValue::Set(serde_json::to_string(&current_output.clone()).unwrap());

        temp_model.update(&self.connection).await.map_err(|err| {
            OxyError::DBError(format!("Failed to update streaming message: {err}"))
        })?;

        thread_guard.output = ActiveValue::Set(serde_json::to_string(&*current_output).unwrap());
        {
            let mut last_updated = self.last_updated.lock().await;
            *last_updated = Instant::now();
        }

        Ok(())
    }
}
