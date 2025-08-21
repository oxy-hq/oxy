use crate::{
    adapters::runs::database::RunsDatabaseStorage,
    errors::OxyError,
    service::types::{
        block::Group,
        pagination::{Paginated, Pagination},
        run::{RootReference, RunDetails, RunInfo},
    },
};

#[enum_dispatch::enum_dispatch]
pub trait RunsStorage {
    async fn last_run(&self, source_id: &str) -> Result<Option<RunInfo>, OxyError>;
    async fn new_run(
        &self,
        source_id: &str,
        root_ref: Option<RootReference>,
    ) -> Result<RunInfo, OxyError>;
    async fn upsert_run(&self, group: Group) -> Result<(), OxyError>;
    async fn find_run(
        &self,
        source_id: &str,
        run_index: Option<i32>,
    ) -> Result<Option<RunInfo>, OxyError>;
    async fn find_run_details(
        &self,
        source_id: &str,
        run_index: Option<i32>,
    ) -> Result<Option<RunDetails>, OxyError>;
    async fn list_runs(
        &self,
        source_id: &str,
        pagination: &Pagination,
    ) -> Result<Paginated<RunInfo>, OxyError>;
}

#[enum_dispatch::enum_dispatch(RunsStorage)]
#[derive(Debug, Clone)]
pub enum RunsStorageImpl {
    DatabaseStorage(RunsDatabaseStorage),
}
