use indexmap::IndexMap;
use uuid::Uuid;

use crate::{
    adapters::runs::database::RunsDatabaseStorage,
    types::{
        block::Group,
        pagination::{Paginated, Pagination},
        run::{RootReference, RunDetails, RunInfo},
    },
};
use oxy_shared::errors::OxyError;

#[enum_dispatch::enum_dispatch]
pub trait RunsStorage {
    async fn last_run(&self, source_id: &str) -> Result<Option<RunInfo>, OxyError>;
    async fn new_run(
        &self,
        source_id: &str,
        root_ref: Option<RootReference>,
        variables: Option<IndexMap<String, serde_json::Value>>,
        lookup_id: Option<Uuid>,
        user_id: Option<Uuid>,
    ) -> Result<RunInfo, OxyError>;
    async fn upsert_run(&self, group: Group, user_id: Option<Uuid>) -> Result<(), OxyError>;
    async fn update_run_variables(
        &self,
        source_id: &str,
        run_index: i32,
        variables: Option<IndexMap<String, serde_json::Value>>,
    ) -> Result<RunInfo, OxyError>;
    async fn update_run_output(
        &self,
        source_id: &str,
        run_index: i32,
        task_name: String,
        output: serde_json::Value,
    ) -> Result<(), OxyError>;
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
    async fn lookup(&self, lookup_id: &str) -> Result<Option<RunDetails>, OxyError>;
    async fn delete_run(&self, source_id: &str, run_index: i32) -> Result<(), OxyError>;
    async fn bulk_delete_runs(&self, run_ids: Vec<(String, i32)>) -> Result<u64, OxyError>;
}

#[enum_dispatch::enum_dispatch(RunsStorage)]
#[derive(Debug, Clone)]
pub enum RunsStorageImpl {
    DatabaseStorage(RunsDatabaseStorage),
}
