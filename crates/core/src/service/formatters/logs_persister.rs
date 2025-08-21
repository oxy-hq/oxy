use crate::{
    errors::OxyError,
    execute::types::{EventKind, Source},
};
use chrono;
use entity::logs;
use sea_orm::{ActiveModelTrait, ActiveValue, DatabaseConnection};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

pub struct LogsPersister {
    connection: DatabaseConnection,
    prompts: String,
    thread_id: Uuid,
    user_id: Uuid,
    queries: Arc<Mutex<Vec<Value>>>,
    log_id: Arc<Mutex<Option<Uuid>>>,
}

impl LogsPersister {
    pub fn new(
        connection: DatabaseConnection,
        prompts: String,
        thread_id: Uuid,
        user_id: Uuid,
    ) -> Self {
        Self {
            connection,
            prompts,
            thread_id,
            user_id,
            queries: Arc::new(Mutex::new(Vec::new())),
            log_id: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn save_log(&self, _source: &Source, event_kind: &EventKind) -> Result<(), OxyError> {
        if let EventKind::SQLQueryGenerated {
            query,
            is_verified,
            database,
            source,
        } = event_kind
        {
            self.save_query(query, *is_verified, source, database)
                .await?;
        }
        Ok(())
    }

    pub async fn save_query(
        &self,
        query: &str,
        is_verified: bool,
        source: &str,
        database: &str,
    ) -> Result<(), OxyError> {
        let mut queries = self.queries.lock().await;
        queries.push(json!({
            "query": query,
            "is_verified": is_verified,
            "source": source,
            "database": database
        }));

        let mut log_id_guard = self.log_id.lock().await;

        if let Some(existing_id) = *log_id_guard {
            let log = logs::ActiveModel {
                id: ActiveValue::Set(existing_id),
                log: ActiveValue::Set(json!({"queries": queries.clone()})),
                updated_at: ActiveValue::Set(chrono::Utc::now().into()),
                ..Default::default()
            };
            log.update(&self.connection)
                .await
                .map_err(|e| OxyError::DBError(format!("Failed to update log: {e}")))?;
        } else {
            let new_id = Uuid::new_v4();
            let log = logs::ActiveModel {
                id: ActiveValue::Set(new_id),
                user_id: ActiveValue::Set(self.user_id),
                prompts: ActiveValue::Set(self.prompts.clone()),
                thread_id: ActiveValue::Set(self.thread_id),
                log: ActiveValue::Set(json!({"queries": queries.clone()})),
                created_at: ActiveValue::Set(chrono::Utc::now().into()),
                updated_at: ActiveValue::Set(chrono::Utc::now().into()),
                ..Default::default()
            };
            log.insert(&self.connection)
                .await
                .map_err(|e| OxyError::DBError(format!("Failed to save log: {e}")))?;

            *log_id_guard = Some(new_id);
        }

        Ok(())
    }
}
