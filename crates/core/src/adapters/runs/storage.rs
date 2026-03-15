use indexmap::IndexMap;
use uuid::Uuid;

use crate::{
    adapters::runs::database::RunsDatabaseStorage,
    types::{
        block::Group,
        pagination::{Paginated, Pagination},
        run::{RootReference, RunDetails, RunInfo, RunStatus},
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
    Noop(RunsNoopStorage),
}

/// No-op storage for use when no database connection is available (e.g. `oxy run` without
/// `OXY_DATABASE_URL`). Run history and checkpoints are not persisted; retry operations are
/// not supported.
#[derive(Debug, Clone)]
pub struct RunsNoopStorage;

impl RunsStorage for RunsNoopStorage {
    async fn last_run(&self, _source_id: &str) -> Result<Option<RunInfo>, OxyError> {
        Ok(None)
    }

    async fn new_run(
        &self,
        source_id: &str,
        root_ref: Option<RootReference>,
        variables: Option<IndexMap<String, serde_json::Value>>,
        lookup_id: Option<Uuid>,
        user_id: Option<Uuid>,
    ) -> Result<RunInfo, OxyError> {
        // run_index is always 1 for noop storage. This is acceptable because noop
        // mode does not persist run history or support retries; the index is only
        // used as a placeholder to satisfy interfaces that expect a valid run.
        let now = chrono::Utc::now();
        Ok(RunInfo {
            root_ref,
            metadata: None,
            source_id: source_id.to_string(),
            run_index: Some(1),
            lookup_id: lookup_id.map(|id| id.to_string()),
            user_id,
            status: RunStatus::Pending,
            variables,
            created_at: now,
            updated_at: now,
        })
    }

    async fn upsert_run(&self, _group: Group, _user_id: Option<Uuid>) -> Result<(), OxyError> {
        Ok(())
    }

    async fn update_run_variables(
        &self,
        source_id: &str,
        _run_index: i32,
        variables: Option<IndexMap<String, serde_json::Value>>,
    ) -> Result<RunInfo, OxyError> {
        // Return a stub RunInfo. This path is hit when a workflow with sub-workflow
        // steps runs without a database (normal `oxy run` without --retry flags).
        // Explicit retry operations always use RunsManager::default() with a real DB.
        let now = chrono::Utc::now();
        Ok(RunInfo {
            root_ref: None,
            metadata: None,
            source_id: source_id.to_string(),
            run_index: Some(1),
            lookup_id: None,
            user_id: None,
            status: RunStatus::Pending,
            variables,
            created_at: now,
            updated_at: now,
        })
    }

    async fn update_run_output(
        &self,
        _source_id: &str,
        _run_index: i32,
        _task_name: String,
        _output: serde_json::Value,
    ) -> Result<(), OxyError> {
        Ok(())
    }

    async fn find_run(
        &self,
        _source_id: &str,
        _run_index: Option<i32>,
    ) -> Result<Option<RunInfo>, OxyError> {
        Ok(None)
    }

    async fn find_run_details(
        &self,
        _source_id: &str,
        _run_index: Option<i32>,
    ) -> Result<Option<RunDetails>, OxyError> {
        Ok(None)
    }

    async fn list_runs(
        &self,
        _source_id: &str,
        pagination: &Pagination,
    ) -> Result<Paginated<RunInfo>, OxyError> {
        Ok(Paginated {
            items: vec![],
            pagination: Pagination {
                page: pagination.page,
                size: pagination.size,
                num_pages: Some(0),
            },
        })
    }

    async fn lookup(&self, _lookup_id: &str) -> Result<Option<RunDetails>, OxyError> {
        Ok(None)
    }

    async fn delete_run(&self, _source_id: &str, _run_index: i32) -> Result<(), OxyError> {
        Ok(())
    }

    async fn bulk_delete_runs(&self, _run_ids: Vec<(String, i32)>) -> Result<u64, OxyError> {
        Ok(0)
    }
}
