use sea_orm::{ActiveValue, prelude::*};
use serde::{Serialize, de::DeserializeOwned};

use crate::{database::client::establish_connection, execute::types::Event};
use oxy_shared::errors::OxyError;

use super::{CheckpointData, CheckpointStorage, RunInfo};

#[derive(Debug, Clone)]
pub(super) struct DatabaseStorage {
    connection: DatabaseConnection,
}

impl DatabaseStorage {
    pub fn new(db: DatabaseConnection) -> Self {
        DatabaseStorage { connection: db }
    }

    pub async fn default() -> Result<Self, OxyError> {
        let connection = establish_connection().await.map_err(|e| {
            OxyError::InitializationError(format!("Failed to establish database connection: {e}"))
        })?;

        Ok(Self::new(connection))
    }
}

impl CheckpointStorage for DatabaseStorage {
    async fn write_success_marker(&self, _run_info: &RunInfo) -> Result<(), OxyError> {
        // In the database, we don't need to write a success marker explicitly,
        // as the run status is relying on the run's output and error.
        Ok(())
    }

    async fn create_checkpoint<T: Serialize + Send>(
        &self,
        run_info: &RunInfo,
        checkpoint: CheckpointData<T>,
    ) -> Result<(), OxyError> {
        // Find the run to create a checkpoint for
        let run_index = run_info.get_run_index();
        let run = entity::runs::Entity::find()
            .filter(
                entity::runs::Column::SourceId
                    .eq(&run_info.source_id)
                    .and(entity::runs::Column::RunIndex.eq(Some(run_index))),
            )
            .one(&self.connection)
            .await
            .map_err(|err| OxyError::DBError(format!("Failed to find run: {err}")))?
            .ok_or_else(|| OxyError::RuntimeError("Run not found".to_string()))?;
        // Serialize the checkpoint data
        let output_json = serde_json::to_value(&checkpoint.output).map_err(|err| {
            OxyError::SerializerError(format!("Failed to serialize checkpoint: {err}"))
        })?;
        let events_json = serde_json::to_value(&checkpoint.events).map_err(|err| {
            OxyError::SerializerError(format!("Failed to serialize checkpoint events: {err}"))
        })?;
        let child_info_json = checkpoint
            .run_info
            .map(|info| {
                serde_json::to_value(info).map_err(|err| {
                    OxyError::SerializerError(format!("Failed to serialize child run info: {err}"))
                })
            })
            .transpose()?;
        let loop_values_json = checkpoint
            .loop_values
            .map(|values| {
                serde_json::to_value(values).map_err(|err| {
                    OxyError::SerializerError(format!("Failed to serialize loop values: {err}"))
                })
            })
            .transpose()?;
        // Create a new checkpoint entry
        let checkpoint_entry = entity::checkpoints::ActiveModel {
            id: ActiveValue::Set(uuid::Uuid::new_v4()),
            run_id: ActiveValue::Set(run.id),
            replay_id: ActiveValue::Set(checkpoint.replay_id),
            checkpoint_hash: ActiveValue::Set(checkpoint.checkpoint_hash),
            output: ActiveValue::Set(Some(output_json)),
            events: ActiveValue::Set(Some(events_json)),
            child_run_info: ActiveValue::Set(child_info_json),
            loop_values: ActiveValue::Set(loop_values_json),
            created_at: ActiveValue::Set(chrono::Utc::now().into()),
            updated_at: ActiveValue::Set(chrono::Utc::now().into()),
            ..Default::default()
        };

        // On conflict, update the existing checkpoint
        let insert = entity::checkpoints::Entity::insert(checkpoint_entry).on_conflict(
            sea_orm::sea_query::OnConflict::columns([
                entity::checkpoints::Column::RunId,
                entity::checkpoints::Column::ReplayId,
            ])
            .update_columns([
                entity::checkpoints::Column::CheckpointHash,
                entity::checkpoints::Column::Output,
                entity::checkpoints::Column::Events,
                entity::checkpoints::Column::UpdatedAt,
            ])
            .to_owned(),
        );
        insert
            .exec(&self.connection)
            .await
            .map_err(|err| OxyError::DBError(format!("Failed to create Checkpoint: {err}")))?;

        Ok(())
    }

    async fn read_checkpoint<T: DeserializeOwned>(
        &self,
        run_info: &RunInfo,
        replay_id: &str,
    ) -> Result<CheckpointData<T>, OxyError> {
        // Find the run to read the checkpoint from
        let run_index = run_info.get_run_index();
        let run = entity::runs::Entity::find()
            .filter(entity::runs::Column::SourceId.eq(&run_info.source_id))
            .filter(entity::runs::Column::RunIndex.eq(Some(run_index)))
            .one(&self.connection)
            .await
            .map_err(|err| OxyError::DBError(format!("Failed to find run: {err}")))?
            .ok_or_else(|| OxyError::RuntimeError("Run not found".to_string()))?;
        // Read the checkpoint data
        let checkpoint = entity::checkpoints::Entity::find()
            .filter(entity::checkpoints::Column::RunId.eq(run.id))
            .filter(entity::checkpoints::Column::ReplayId.eq(replay_id))
            .one(&self.connection)
            .await
            .map_err(|err| OxyError::DBError(format!("Failed to read checkpoint: {err}")))?
            .ok_or_else(|| OxyError::RuntimeError("Checkpoint not found".to_string()))?;
        // Deserialize the checkpoint data
        let output = checkpoint
            .output
            .map(|v| {
                serde_json::from_value::<T>(v).map_err(|err| {
                    OxyError::SerializerError(format!(
                        "Failed to deserialize checkpoint output: {err}"
                    ))
                })
            })
            .transpose()?;
        let events = checkpoint
            .events
            .map(|v| {
                serde_json::from_value::<Vec<Event>>(v).map_err(|err| {
                    OxyError::SerializerError(format!(
                        "Failed to deserialize checkpoint events: {err}"
                    ))
                })
            })
            .transpose()?
            .ok_or(OxyError::RuntimeError(
                "Checkpoint events are missing".to_string(),
            ))?;
        let child_run_info = checkpoint
            .child_run_info
            .map(|v| {
                serde_json::from_value(v).map_err(|err| {
                    OxyError::SerializerError(format!(
                        "Failed to deserialize child run info: {err}"
                    ))
                })
            })
            .transpose()?;
        let loop_values = checkpoint
            .loop_values
            .map(|v| {
                serde_json::from_value(v).map_err(|err| {
                    OxyError::SerializerError(format!("Failed to deserialize loop values: {err}"))
                })
            })
            .transpose()?;

        Ok(CheckpointData {
            replay_id: checkpoint.replay_id,
            checkpoint_hash: checkpoint.checkpoint_hash,
            output,
            events,
            run_info: child_run_info,
            loop_values,
        })
    }
}
