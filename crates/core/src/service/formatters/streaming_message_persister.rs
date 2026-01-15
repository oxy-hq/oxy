use entity::messages;
use oxy_shared::errors::OxyError;
use sea_orm::{ActiveModelTrait, ActiveValue, DatabaseConnection};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};
use uuid::Uuid;

const DEFAULT_FLUSH_INTERVAL: Duration = Duration::from_millis(1000);
const DEFAULT_FLUSH_SIZE: usize = 1000;

pub struct StreamingMessagePersister {
    connection: DatabaseConnection,
    message: Arc<Mutex<messages::ActiveModel>>,
    pending_content: Arc<Mutex<String>>,
    last_flush: Arc<Mutex<Instant>>,
    message_id: Uuid,
    cancelled: Arc<Mutex<bool>>,
}

impl StreamingMessagePersister {
    pub async fn new(
        connection: DatabaseConnection,
        thread_id: Uuid,
        initial_content: String,
    ) -> Result<Self, OxyError> {
        let message_id = Uuid::new_v4();

        let new_message = messages::ActiveModel {
            id: ActiveValue::Set(message_id),
            content: ActiveValue::Set(initial_content.clone()),
            is_human: ActiveValue::Set(false),
            thread_id: ActiveValue::Set(thread_id),
            created_at: ActiveValue::default(),
            input_tokens: ActiveValue::Set(0),
            output_tokens: ActiveValue::Set(0),
        };

        let insert_message = new_message.clone();

        insert_message.insert(&connection).await.map_err(|err| {
            OxyError::DBError(format!("Failed to create streaming message: {err}"))
        })?;

        Ok(Self {
            connection,
            message: Arc::new(Mutex::new(new_message)),
            pending_content: Arc::new(Mutex::new(String::new())),
            last_flush: Arc::new(Mutex::new(Instant::now())),
            message_id,
            cancelled: Arc::new(Mutex::new(false)),
        })
    }

    pub async fn cancel(&self, content: &str) -> Result<(), OxyError> {
        self.append_content(content).await?;
        self.flush_pending_content().await?;
        {
            let mut cancelled = self.cancelled.lock().await;
            *cancelled = true;
        }
        Ok(())
    }

    pub async fn append_content(&self, content: &str) -> Result<(), OxyError> {
        {
            let cancelled = self.cancelled.lock().await;
            if *cancelled {
                return Ok(());
            }
        }
        {
            let mut pending = self.pending_content.lock().await;
            pending.push_str(content);
        }

        let should_flush = {
            let last_flush = self.last_flush.lock().await;
            let pending = self.pending_content.lock().await;

            last_flush.elapsed() > DEFAULT_FLUSH_INTERVAL || pending.len() > DEFAULT_FLUSH_SIZE
        };

        if should_flush {
            self.flush_pending_content().await?;
        }

        Ok(())
    }

    async fn flush_pending_content(&self) -> Result<(), OxyError> {
        let pending_content = {
            let mut pending = self.pending_content.lock().await;
            if pending.is_empty() {
                return Ok(());
            }
            let content = pending.clone();
            pending.clear();
            content
        };

        let mut message_guard = self.message.lock().await;
        let mut current_content = match &message_guard.content {
            ActiveValue::Set(val) => val.clone(),
            ActiveValue::Unchanged(val) => val.clone(),
            _ => String::new(),
        };

        current_content.push_str(&pending_content);

        let mut temp_model = message_guard.clone();
        temp_model.content = ActiveValue::Set(current_content.clone());

        temp_model.update(&self.connection).await.map_err(|err| {
            OxyError::DBError(format!("Failed to update streaming message: {err}"))
        })?;

        message_guard.content = ActiveValue::Set(current_content);
        {
            let mut last_flush = self.last_flush.lock().await;
            *last_flush = Instant::now();
        }

        Ok(())
    }

    pub async fn update_usage(
        &self,
        input_tokens: i32,
        output_tokens: i32,
    ) -> Result<(), OxyError> {
        {
            let cancelled = self.cancelled.lock().await;
            if *cancelled {
                return Ok(());
            }
        }
        self.flush_pending_content().await?;

        let mut message_guard = self.message.lock().await;

        let mut temp_model = message_guard.clone();
        temp_model.input_tokens = ActiveValue::Set(input_tokens);
        temp_model.output_tokens = ActiveValue::Set(output_tokens);

        temp_model
            .update(&self.connection)
            .await
            .map_err(|err| OxyError::DBError(format!("Failed to update message tokens: {err}")))?;

        message_guard.input_tokens = ActiveValue::Set(input_tokens);
        message_guard.output_tokens = ActiveValue::Set(output_tokens);

        Ok(())
    }

    pub fn get_message_id(&self) -> Uuid {
        self.message_id
    }

    pub async fn get_message(&self) -> messages::ActiveModel {
        let message_guard = self.message.lock().await;
        message_guard.clone()
    }

    pub fn get_connection(&self) -> &DatabaseConnection {
        &self.connection
    }
}
