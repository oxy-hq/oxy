use std::sync::Arc;

use database::DatabaseStorage;
use file::FileStorage;
use indexmap::IndexMap;
use itertools::Itertools;
use noop::NoopStorage;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tokio::sync::OnceCell;

use crate::{
    adapters::runs::RunsManager,
    execute::{builders::checkpoint::CheckpointId, types::Event},
    types::run::{RootReference, RunInfo as PublicRunInfo},
};
use oxy_shared::errors::OxyError;

mod database;
mod file;
mod noop;
pub mod types;

pub struct CheckpointBuilder {
    storage: Option<CheckpointStorageImpl>,
    run_storage: Option<RunsManager>,
}

impl CheckpointBuilder {
    pub async fn from_runs_manager(
        runs_manager: &RunsManager,
    ) -> Result<CheckpointManager, OxyError> {
        let storage = if runs_manager.is_noop() {
            CheckpointStorageImpl::Noop(NoopStorage)
        } else {
            CheckpointStorageImpl::DatabaseStorage(DatabaseStorage::default().await?)
        };
        CheckpointBuilder {
            storage: Some(storage),
            run_storage: Some(runs_manager.clone()),
        }
        .build()
    }

    fn build(self) -> Result<CheckpointManager, OxyError> {
        let storage = self.storage.ok_or(OxyError::RuntimeError(
            "Storage source is required".to_string(),
        ))?;
        let run_storage = self.run_storage.ok_or(OxyError::RuntimeError(
            "Run storage is required".to_string(),
        ))?;

        Ok(CheckpointManager {
            storage,
            run_storage,
        })
    }
}

#[derive(Debug, Clone)]
pub struct CheckpointManager {
    storage: CheckpointStorageImpl,
    run_storage: RunsManager,
}

impl CheckpointManager {
    pub async fn write_success_marker(&self, run_info: &RunInfo) -> Result<(), OxyError> {
        self.storage.write_success_marker(run_info).await
    }

    pub fn new_context(&self, run_info: RunInfo, user_id: Option<uuid::Uuid>) -> CheckpointContext {
        CheckpointContext::new(
            run_info,
            self.storage.clone(),
            self.run_storage.clone(),
            user_id,
        )
    }
}

#[derive(Debug, Clone)]
pub struct CheckpointContext {
    root_ref: Option<RootReference>,
    run_info: RunInfo,
    current_ref: Vec<String>,
    storage: CheckpointStorageImpl,
    has_checkpoints: Arc<OnceCell<bool>>,
    run_storage: RunsManager,
    user_id: Option<uuid::Uuid>,
}

impl CheckpointContext {
    fn new(
        run_info: RunInfo,
        storage: CheckpointStorageImpl,
        run_storage: RunsManager,
        user_id: Option<uuid::Uuid>,
    ) -> Self {
        CheckpointContext {
            root_ref: None,
            run_info,
            storage,
            current_ref: vec![],
            has_checkpoints: Arc::new(OnceCell::new()),
            run_storage,
            user_id,
        }
    }

    pub fn nested(&self, run_info: RunInfo) -> Self {
        let root_ref = self.get_root_ref();
        CheckpointContext {
            root_ref: Some(root_ref),
            run_info,
            storage: self.storage.clone(),
            current_ref: vec![],
            has_checkpoints: Arc::new(OnceCell::new()),
            run_storage: self.run_storage.clone(),
            user_id: self.user_id,
        }
    }

    pub fn with_current_ref(&self, child_ref: &str) -> Self {
        let mut current_ref = self.current_ref.clone();
        current_ref.push(child_ref.to_string());

        CheckpointContext {
            root_ref: self.root_ref.clone(),
            run_info: self.run_info.clone(),
            current_ref,
            storage: self.storage.clone(),
            has_checkpoints: self.has_checkpoints.clone(),
            run_storage: self.run_storage.clone(),
            user_id: self.user_id,
        }
    }

    pub fn get_root_ref(&self) -> RootReference {
        match self.root_ref {
            Some(ref root) => RootReference {
                source_id: root.source_id.clone(),
                run_index: root.run_index,
                replay_ref: self.current_ref_str(),
            },
            None => RootReference {
                source_id: self.run_info.source_id.clone(),
                run_index: Some(self.run_info.run_index.try_into().unwrap_or_else(|e| {
                    tracing::error!("Failed to convert run_index: {}", e);
                    0
                })),
                replay_ref: self.current_ref_str(),
            },
        }
    }

    pub fn current_ref_str(&self) -> String {
        self.current_ref.join(".")
    }

    pub(crate) fn storage(&self) -> &CheckpointStorageImpl {
        &self.storage
    }

    pub(crate) fn run_info(&self) -> &RunInfo {
        &self.run_info
    }

    pub fn get_full_ref(&self, replay_id: &str) -> String {
        self.current_ref
            .iter()
            .chain(std::iter::once(&replay_id.to_string()))
            .join(".")
    }

    pub fn get_replay_id(&self, target_replay_id: &str) -> Option<String> {
        if let Some(replay_id) = &self.run_info.replay_id {
            return replay_id
                .strip_prefix(&self.get_full_ref(target_replay_id))
                .map(|s| s.trim_start_matches(".").to_string());
        }
        None
    }

    pub async fn get_child_run_info(
        &self,
        replay_id: &str,
        source_id: &str,
        variables: Option<IndexMap<String, serde_json::Value>>,
    ) -> Result<RunInfo, OxyError> {
        let checkpoint_data = match self.has_any_checkpoint().await {
            Ok(true) => self
                .storage
                .read_checkpoint::<serde_json::Value>(&self.run_info, &self.get_full_ref(replay_id))
                .await
                .ok(),
            Ok(false) => None,
            Err(err) => {
                tracing::warn!(
                    "Failed to determine checkpoint existence for child run lookup; falling back to direct read: {}",
                    err
                );
                self.storage
                    .read_checkpoint::<serde_json::Value>(
                        &self.run_info,
                        &self.get_full_ref(replay_id),
                    )
                    .await
                    .ok()
            }
        };
        let mut root_ref = self.get_root_ref();
        root_ref.replay_ref = self.get_full_ref(replay_id);
        let mut run_info: RunInfo = match checkpoint_data {
            Some(checkpoint_data) => match checkpoint_data.run_info {
                Some(run_info) => self
                    .run_storage
                    .update_run_variables(
                        &run_info.get_source_id(),
                        run_info.get_run_index() as i32,
                        variables,
                    )
                    .await?
                    .try_into()?,
                None => self
                    .run_storage
                    .nested_run(source_id, root_ref, variables, self.user_id)
                    .await?
                    .try_into()?,
            },
            None => self
                .run_storage
                .nested_run(source_id, root_ref, variables, self.user_id)
                .await?
                .try_into()?,
        };
        run_info.set_replay_id(self.get_replay_id(replay_id));
        tracing::info!(
            "Getting child run info: {:?}\n{:?}.{replay_id}",
            run_info,
            self.current_ref
        );
        Ok(run_info)
    }

    pub async fn create_checkpoint<T: Serialize + Send>(
        &self,
        checkpoint: CheckpointData<T>,
    ) -> Result<(), OxyError> {
        let mut checkpoint = checkpoint;
        checkpoint.replay_id = self.current_ref_str();
        self.storage
            .create_checkpoint(&self.run_info, checkpoint)
            .await
    }

    pub async fn read_checkpoint<T: DeserializeOwned, C: CheckpointId>(
        &self,
        input: &C,
    ) -> Result<CheckpointData<T>, OxyError> {
        if self.is_replay() {
            tracing::info!("Skip read from a replay run {}", self.current_ref.join("."));
            return Err(OxyError::ArgumentError(format!(
                "Skip read from a replay run {}",
                self.run_info.run_index
            )));
        }
        if !self.has_any_checkpoint().await? {
            return Err(OxyError::RuntimeError("Checkpoint not found".to_string()));
        }
        let replay_id = self.current_ref_str();
        let checkpoint_data = self
            .storage
            .read_checkpoint::<T>(&self.run_info, &replay_id)
            .await?;
        let checkpoint_hash = input.checkpoint_hash();
        if checkpoint_data.checkpoint_hash != checkpoint_hash {
            return Err(OxyError::ArgumentError(format!(
                "Checkpoint hash mismatch: expected {}, got {}",
                checkpoint_hash, &checkpoint_data.checkpoint_hash
            )));
        }
        Ok(checkpoint_data)
    }

    fn is_replay(&self) -> bool {
        if let Some(replay_id) = &self.run_info.replay_id {
            if replay_id.is_empty() {
                return true;
            }

            let current_ref = self.current_ref.join(".");
            return replay_id.starts_with(&current_ref);
        }
        false
    }

    async fn has_any_checkpoint(&self) -> Result<bool, OxyError> {
        let storage = self.storage.clone();
        let run_info = self.run_info.clone();

        self.has_checkpoints
            .get_or_try_init(|| async move { storage.has_any_checkpoint(&run_info).await })
            .await
            .copied()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunInfo {
    source_id: String,
    run_index: u32,
    variables: Option<IndexMap<String, serde_json::Value>>,
    // Nested replay ID for checkpoints
    // This is used to identify the specific run in a replay context
    // It can be empty if this is a replay all
    replay_id: Option<String>,
    success: bool,
    // Root reference for nested runs
    root_ref: Option<RootReference>,
}

impl RunInfo {
    pub fn new(
        source_id: String,
        run_index: u32,
        replay_id: Option<String>,
        success: bool,
        variables: Option<IndexMap<String, serde_json::Value>>,
        root_ref: Option<RootReference>,
    ) -> Self {
        RunInfo {
            source_id,
            run_index,
            replay_id,
            success,
            variables,
            root_ref,
        }
    }

    pub fn is_success(&self) -> bool {
        self.success
    }

    pub fn set_replay_id(&mut self, replay_id: Option<String>) {
        self.replay_id = replay_id;
    }

    pub fn get_source_id(&self) -> String {
        self.source_id.to_string()
    }

    pub fn get_replay_id(&self) -> Option<String> {
        self.replay_id.clone()
    }

    pub fn get_run_index(&self) -> u32 {
        self.run_index
    }

    pub fn get_variables(&self) -> Option<IndexMap<String, serde_json::Value>> {
        self.variables.clone()
    }

    pub fn get_root_ref(&self) -> Option<RootReference> {
        self.root_ref.clone()
    }

    pub fn task_id(&self) -> String {
        format!("{}::{}", self.source_id, self.run_index)
    }
}

impl TryFrom<PublicRunInfo> for RunInfo {
    type Error = OxyError;

    fn try_from(value: PublicRunInfo) -> Result<Self, Self::Error> {
        let is_completed = value.is_completed();
        Ok(RunInfo::new(
            value.source_id,
            value
                .run_index
                .ok_or(OxyError::RuntimeError("Run index is required".to_string()))?
                .try_into()
                .map_err(|e| {
                    OxyError::RuntimeError(format!("Failed to convert run_index to u32: {e}"))
                })?,
            None,
            is_completed,
            value.variables,
            value.root_ref,
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointData<T> {
    pub replay_id: String,
    pub checkpoint_hash: String,
    pub output: Option<T>,
    pub events: Vec<Event>,

    // Nested replay ID for checkpoints
    pub run_info: Option<RunInfo>,
    // Loop values for the current run
    pub loop_values: Option<Vec<serde_json::Value>>,
}

impl<T: Serialize> CheckpointData<T> {
    pub fn into_value(self) -> Result<CheckpointData<serde_json::Value>, OxyError> {
        let output = self
            .output
            .map(|v| {
                serde_json::to_value(v).map_err(|e| {
                    OxyError::SerializerError(format!("Failed to serialize checkpoint output: {e}"))
                })
            })
            .transpose()?;
        Ok(CheckpointData {
            replay_id: self.replay_id,
            checkpoint_hash: self.checkpoint_hash,
            output,
            events: self.events,
            run_info: self.run_info,
            loop_values: self.loop_values,
        })
    }
}

#[enum_dispatch::enum_dispatch]
#[allow(async_fn_in_trait)]
pub trait CheckpointStorage {
    async fn write_success_marker(&self, run_info: &RunInfo) -> Result<(), OxyError>;
    async fn has_any_checkpoint(&self, run_info: &RunInfo) -> Result<bool, OxyError>;
    async fn create_checkpoint<T: Serialize + Send>(
        &self,
        run_info: &RunInfo,
        checkpoint: CheckpointData<T>,
    ) -> Result<(), OxyError>;
    async fn create_checkpoints_batch(
        &self,
        items: Vec<(RunInfo, CheckpointData<serde_json::Value>)>,
    ) -> Result<(), OxyError>;
    async fn read_checkpoint<T: DeserializeOwned>(
        &self,
        run_info: &RunInfo,
        replay_id: &str,
    ) -> Result<CheckpointData<T>, OxyError>;
}

#[enum_dispatch::enum_dispatch(CheckpointStorage)]
#[derive(Debug, Clone)]
pub(crate) enum CheckpointStorageImpl {
    FileStorage(FileStorage),
    DatabaseStorage(DatabaseStorage),
    Noop(NoopStorage),
}
