use indexmap::IndexMap;
use uuid::Uuid;

use crate::{
    adapters::{
        checkpoint::types::RetryStrategy,
        runs::{
            database::RunsDatabaseStorage,
            storage::{RunsStorage, RunsStorageImpl},
        },
    },
    db::client::establish_connection,
    errors::OxyError,
    service::types::{
        block::Group,
        pagination::{Paginated, Pagination},
        run::{RootReference, RunDetails, RunInfo},
    },
};

#[derive(Debug, Clone)]
pub struct RunsManager {
    storage: RunsStorageImpl,
}

impl RunsManager {
    pub async fn default(project_id: Uuid, branch_id: Uuid) -> Result<Self, OxyError> {
        let storage = RunsStorageImpl::DatabaseStorage(RunsDatabaseStorage::new(
            establish_connection().await.map_err(|e| {
                OxyError::DBError(format!("Failed to establish database connection: {e}"))
            })?,
            project_id,
            branch_id,
        ));
        Ok(RunsManager { storage })
    }

    pub async fn list_runs(
        &self,
        source_id: &str,
        pagination: &Pagination,
    ) -> Result<Paginated<RunInfo>, OxyError> {
        self.storage.list_runs(source_id, pagination).await
    }
    pub async fn upsert_run(&self, group: Group) -> Result<(), OxyError> {
        self.storage.upsert_run(group).await
    }
    pub async fn find_run_details(
        &self,
        source_id: &str,
        run_index: Option<i32>,
    ) -> Result<Option<RunDetails>, OxyError> {
        self.storage.find_run_details(source_id, run_index).await
    }

    pub async fn update_run_variables(
        &self,
        source_id: &str,
        run_index: i32,
        variables: Option<IndexMap<String, serde_json::Value>>,
    ) -> Result<RunInfo, OxyError> {
        self.storage
            .update_run_variables(source_id, run_index, variables)
            .await
    }

    pub async fn update_run_output(
        &self,
        source_id: &str,
        run_index: i32,
        task_name: String,
        output: serde_json::Value,
    ) -> Result<(), OxyError> {
        self.storage
            .update_run_output(source_id, run_index, task_name, output)
            .await
    }

    pub async fn find_run(
        &self,
        source_id: &str,
        run_index: Option<i32>,
    ) -> Result<Option<RunInfo>, OxyError> {
        self.storage.find_run(source_id, run_index).await
    }
    pub async fn last_run(&self, source_id: &str) -> Result<Option<RunInfo>, OxyError> {
        self.storage.last_run(source_id).await
    }
    pub async fn new_run(
        &self,
        source_id: &str,
        variables: Option<IndexMap<String, serde_json::Value>>,
    ) -> Result<RunInfo, OxyError> {
        self.storage.new_run(source_id, None, variables).await
    }
    pub async fn nested_run(
        &self,
        source_id: &str,
        root_ref: RootReference,
        variables: Option<IndexMap<String, serde_json::Value>>,
    ) -> Result<RunInfo, OxyError> {
        self.storage
            .new_run(source_id, Some(root_ref), variables)
            .await
    }
    pub async fn get_run_info(
        &self,
        source_id: &str,
        retry_strategy: &RetryStrategy,
    ) -> Result<(RunInfo, Option<String>), OxyError> {
        match retry_strategy {
            RetryStrategy::Retry {
                replay_id,
                run_index,
            } => {
                let run_info = self
                    .find_run(
                        &source_id,
                        Some((*run_index).try_into().map_err(|_| {
                            OxyError::RuntimeError("Run index conversion failed".to_string())
                        })?),
                    )
                    .await?;
                match run_info {
                    Some(run_info) => Ok((run_info, replay_id.clone())),
                    None => Err(OxyError::RuntimeError(format!(
                        "Run with index {run_index} not found for workflow {source_id}"
                    ))),
                }
            }
            RetryStrategy::RetryWithVariables {
                replay_id,
                run_index,
                variables,
            } => {
                let run_info = self
                    .update_run_variables(
                        &source_id,
                        (*run_index).try_into().map_err(|_| {
                            OxyError::RuntimeError("Run index conversion failed".to_string())
                        })?,
                        variables.clone(),
                    )
                    .await?;
                Ok((run_info, replay_id.clone()))
            }
            RetryStrategy::LastFailure => {
                let run_info = self.last_run(&source_id).await?;
                match run_info {
                    Some(run_info) => Ok((run_info, None)),
                    None => Err(OxyError::RuntimeError(format!(
                        "Last failure run not found for workflow {source_id}"
                    ))),
                }
            }
            RetryStrategy::NoRetry { variables } => self
                .new_run(&source_id, variables.clone())
                .await
                .map(|run| (run, None)),
            RetryStrategy::Preview => {
                todo!("Preview mode is not implemented yet")
            }
        }
    }
}
