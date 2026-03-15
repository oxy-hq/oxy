use serde::{Serialize, de::DeserializeOwned};

use oxy_shared::errors::OxyError;

use super::{CheckpointData, CheckpointStorage, RunInfo};

/// No-op checkpoint storage for use when no database connection is available.
/// Checkpoints are not persisted, and retry/replay operations are not supported.
#[derive(Debug, Clone)]
pub(super) struct NoopStorage;

impl CheckpointStorage for NoopStorage {
    async fn write_success_marker(&self, _run_info: &RunInfo) -> Result<(), OxyError> {
        Ok(())
    }

    async fn has_any_checkpoint(&self, _run_info: &RunInfo) -> Result<bool, OxyError> {
        Ok(false)
    }

    async fn create_checkpoint<T: Serialize + Send>(
        &self,
        _run_info: &RunInfo,
        _checkpoint: CheckpointData<T>,
    ) -> Result<(), OxyError> {
        Ok(())
    }

    async fn create_checkpoints_batch(
        &self,
        _items: Vec<(RunInfo, CheckpointData<serde_json::Value>)>,
    ) -> Result<(), OxyError> {
        Ok(())
    }

    async fn read_checkpoint<T: DeserializeOwned>(
        &self,
        _run_info: &RunInfo,
        _replay_id: &str,
    ) -> Result<CheckpointData<T>, OxyError> {
        Err(OxyError::RuntimeError("Checkpoint not found".to_string()))
    }
}
