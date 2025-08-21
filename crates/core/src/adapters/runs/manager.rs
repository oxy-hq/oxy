use crate::{
    adapters::runs::{
        database::RunsDatabaseStorage,
        storage::{RunsStorage, RunsStorageImpl},
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
    pub async fn default() -> Result<Self, OxyError> {
        let storage = RunsStorageImpl::DatabaseStorage(RunsDatabaseStorage::new(
            establish_connection().await.map_err(|e| {
                OxyError::DBError(format!("Failed to establish database connection: {e}"))
            })?,
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
    pub async fn new_run(&self, source_id: &str) -> Result<RunInfo, OxyError> {
        self.storage.new_run(source_id, None).await
    }
    pub async fn nested_run(
        &self,
        source_id: &str,
        root_ref: RootReference,
    ) -> Result<RunInfo, OxyError> {
        self.storage.new_run(source_id, Some(root_ref)).await
    }
}
