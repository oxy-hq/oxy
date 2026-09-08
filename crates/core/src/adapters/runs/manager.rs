use indexmap::IndexMap;
use uuid::Uuid;

use crate::{
    adapters::runs::{
        database::RunsDatabaseStorage,
        storage::{RunsNoopStorage, RunsStorage, RunsStorageImpl},
    },
    database::client::establish_connection,
    types::{
        pagination::{Paginated, Pagination},
        run::{RunDetails, RunInfo},
    },
};
use oxy_shared::errors::OxyError;

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

    /// Creates a `RunsManager` that does not require a database connection.
    /// Run history and checkpoints are not persisted. Retry operations are not supported.
    pub fn noop() -> Self {
        RunsManager {
            storage: RunsStorageImpl::Noop(RunsNoopStorage),
        }
    }

    pub async fn list_runs(
        &self,
        source_id: &str,
        pagination: &Pagination,
    ) -> Result<Paginated<RunInfo>, OxyError> {
        self.storage.list_runs(source_id, pagination).await
    }
    pub async fn find_run_details(
        &self,
        source_id: &str,
        run_index: Option<i32>,
    ) -> Result<Option<RunDetails>, OxyError> {
        self.storage.find_run_details(source_id, run_index).await
    }

    pub async fn lookup(&self, lookup_id: &str) -> Result<Option<RunDetails>, OxyError> {
        self.storage.lookup(lookup_id).await
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
        lookup_id: Option<Uuid>,
        user_id: Option<Uuid>,
    ) -> Result<RunInfo, OxyError> {
        self.storage
            .new_run(source_id, None, variables, lookup_id, user_id)
            .await
    }
    pub async fn delete_run(&self, source_id: &str, run_index: i32) -> Result<(), OxyError> {
        self.storage.delete_run(source_id, run_index).await
    }

    pub async fn bulk_delete_runs(&self, run_ids: Vec<(String, i32)>) -> Result<u64, OxyError> {
        self.storage.bulk_delete_runs(run_ids).await
    }
}
