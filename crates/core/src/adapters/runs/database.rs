use std::collections::HashMap;

use entity::runs::Variables;
use indexmap::IndexMap;
use sea_orm::{
    ActiveValue, Condition, QueryOrder,
    prelude::*,
    sea_query::{Expr, OnConflict},
};

use crate::{
    adapters::runs::storage::RunsStorage,
    types::{
        block::{Block, GroupKind},
        pagination::{Paginated, Pagination},
        run::{RootReference, RunDetails, RunInfo, RunStatus},
    },
};
use oxy_shared::errors::OxyError;
use sea_orm::ExprTrait;

#[derive(Debug, Clone)]
pub struct RunsDatabaseStorage {
    connection: DatabaseConnection,
    project_id: Uuid,
    branch_id: Uuid,
}

impl RunsDatabaseStorage {
    pub fn new(connection: DatabaseConnection, project_id: Uuid, branch_id: Uuid) -> Self {
        RunsDatabaseStorage {
            connection,
            project_id,
            branch_id,
        }
    }

    /// Atomically increments and returns the next run index for the given
    /// source.  A single `INSERT … ON CONFLICT DO UPDATE … RETURNING` is
    /// sufficient: the DB guarantees atomicity, so no advisory lock or
    /// application-level mutex is required.
    async fn next_run_index(&self, source_id: &str) -> Result<i32, OxyError> {
        let row = entity::run_sequences::Entity::insert(entity::run_sequences::ActiveModel {
            project_id: ActiveValue::Set(self.project_id),
            branch_id: ActiveValue::Set(self.branch_id),
            source_id: ActiveValue::Set(source_id.to_string()),
            last_value: ActiveValue::Set(1),
        })
        .on_conflict(
            OnConflict::columns([
                entity::run_sequences::Column::ProjectId,
                entity::run_sequences::Column::BranchId,
                entity::run_sequences::Column::SourceId,
            ])
            .value(
                entity::run_sequences::Column::LastValue,
                Expr::col((
                    entity::run_sequences::Entity,
                    entity::run_sequences::Column::LastValue,
                ))
                .add(1),
            )
            .to_owned(),
        )
        .exec_with_returning(&self.connection)
        .await;
        let row =
            row.map_err(|e| OxyError::DBError(format!("Failed to advance run sequence: {e}")))?;
        Ok(row.last_value)
    }
}

impl RunsStorage for RunsDatabaseStorage {
    async fn last_run(&self, source_id: &str) -> Result<Option<RunInfo>, OxyError> {
        let run = entity::runs::Entity::find()
            .filter(entity::runs::Column::SourceId.eq(source_id))
            .filter(entity::runs::Column::ProjectId.eq(self.project_id))
            .filter(entity::runs::Column::BranchId.eq(self.branch_id))
            .order_by_desc(entity::runs::Column::RunIndex)
            .one(&self.connection)
            .await;
        let run =
            run.map_err(|err| OxyError::DBError(format!("Failed to fetch last run: {err}")))?;
        if run.is_none() {
            return Ok(None);
        }

        let run = run.unwrap();
        let status = match (&run.blocks, &run.error) {
            (_, Some(_)) => RunStatus::Failed,
            (Some(_), None) => RunStatus::Completed,
            (None, None) => RunStatus::Pending,
        };
        let root_ref = run.root_source_id.as_ref().map(|source_id| RootReference {
            source_id: source_id.clone(),
            run_index: run.root_run_index,
            replay_ref: run.root_replay_ref.unwrap_or_default(),
        });
        Ok(Some(RunInfo {
            metadata: None,
            root_ref,
            source_id: run.source_id,
            run_index: run.run_index,
            status,
            lookup_id: run.lookup_id.map(|id| id.to_string()),
            user_id: run.user_id,
            variables: run.variables.map(|v| v.to_inner()),
            created_at: run.created_at.into(),
            updated_at: run.updated_at.into(),
        }))
    }

    async fn new_run(
        &self,
        source_id: &str,
        root_ref: Option<RootReference>,
        variables: Option<IndexMap<String, serde_json::Value>>,
        lookup_id: Option<Uuid>,
        user_id: Option<Uuid>,
    ) -> Result<RunInfo, OxyError> {
        let run_index = self.next_run_index(source_id).await?;
        let mut run = entity::runs::ActiveModel {
            id: ActiveValue::Set(uuid::Uuid::new_v4()),
            source_id: ActiveValue::Set(source_id.to_string()),
            run_index: ActiveValue::Set(Some(run_index)),
            metadata: ActiveValue::Set(None),
            blocks: ActiveValue::Set(None),
            error: ActiveValue::Set(None),
            project_id: ActiveValue::Set(self.project_id),
            branch_id: ActiveValue::Set(self.branch_id),
            user_id: ActiveValue::Set(user_id),
            created_at: ActiveValue::Set(chrono::Utc::now().into()),
            updated_at: ActiveValue::Set(chrono::Utc::now().into()),
            variables: ActiveValue::Set(variables.clone().map(Variables)),
            lookup_id: ActiveValue::Set(lookup_id),
            ..Default::default()
        };
        match root_ref {
            Some(ref root) => {
                run.root_source_id = ActiveValue::Set(Some(root.source_id.clone()));
                run.root_run_index = ActiveValue::Set(root.run_index);
                run.root_replay_ref = ActiveValue::Set(Some(root.replay_ref.clone()));
            }
            None => {
                run.root_source_id = ActiveValue::Set(None);
                run.root_run_index = ActiveValue::Set(None);
                run.root_replay_ref = ActiveValue::Set(None);
            }
        }
        run.insert(&self.connection)
            .await
            .map_err(|err| OxyError::DBError(format!("Failed to create run: {err}")))?;
        Ok(RunInfo {
            metadata: None,
            root_ref,
            source_id: source_id.to_string(),
            run_index: Some(run_index),
            lookup_id: lookup_id.map(|id| id.to_string()),
            user_id,
            variables,
            status: RunStatus::Pending,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
    }

    async fn find_run(
        &self,
        source_id: &str,
        run_index: Option<i32>,
    ) -> Result<Option<RunInfo>, OxyError> {
        let run_index_operator = run_index
            .map(|index| entity::runs::Column::RunIndex.eq(Some(index)))
            .unwrap_or(entity::runs::Column::RunIndex.is_null());
        let run = entity::runs::Entity::find()
            .filter(
                entity::runs::Column::SourceId
                    .eq(source_id)
                    .and(run_index_operator)
                    .and(entity::runs::Column::ProjectId.eq(self.project_id))
                    .and(entity::runs::Column::BranchId.eq(self.branch_id)),
            )
            .one(&self.connection)
            .await;
        let run = run
            .map_err(|err| OxyError::DBError(format!("Failed to fetch run: {err}")))?
            .map(|run| RunInfo {
                metadata: None,
                root_ref: run.root_source_id.as_ref().map(|source_id| RootReference {
                    source_id: source_id.clone(),
                    run_index: run.root_run_index,
                    replay_ref: run.root_replay_ref.unwrap_or_default(),
                }),
                source_id: run.source_id,
                run_index: run.run_index,
                lookup_id: run.lookup_id.map(|id| id.to_string()),
                user_id: run.user_id,
                status: match (run.blocks, run.error) {
                    (_, Some(_)) => RunStatus::Failed,
                    (Some(_), None) => RunStatus::Completed,
                    (None, None) => RunStatus::Pending,
                },
                variables: run.variables.map(|v| v.to_inner()),
                created_at: run.created_at.into(),
                updated_at: run.updated_at.into(),
            });
        Ok(run)
    }

    async fn find_run_details(
        &self,
        source_id: &str,
        run_index: Option<i32>,
    ) -> Result<Option<RunDetails>, OxyError> {
        let run_index_operator = run_index
            .map(|index| entity::runs::Column::RunIndex.eq(Some(index)))
            .unwrap_or(entity::runs::Column::RunIndex.is_null());
        let run = entity::runs::Entity::find()
            .filter(
                entity::runs::Column::SourceId
                    .eq(source_id)
                    .and(entity::runs::Column::ProjectId.eq(self.project_id))
                    .and(entity::runs::Column::BranchId.eq(self.branch_id))
                    .and(run_index_operator),
            )
            .one(&self.connection)
            .await
            .map_err(|err| OxyError::DBError(format!("Failed to fetch run: {err}")))?;
        if run.is_none() {
            return Ok(None);
        }
        let run = run.unwrap();

        let status = match (&run.blocks, &run.error) {
            (_, Some(_)) => RunStatus::Failed,
            (Some(_), None) => RunStatus::Completed,
            (None, None) => RunStatus::Pending,
        };
        let blocks = run
            .blocks
            .map(|blocks_json| {
                serde_json::from_value::<HashMap<String, Block>>(blocks_json).map_err(|err| {
                    OxyError::SerializerError(format!("Failed to deserialize blocks: {err}"))
                })
            })
            .transpose()?;
        let children = run
            .children
            .map(|children_json| {
                serde_json::from_value::<Vec<String>>(children_json).map_err(|err| {
                    OxyError::SerializerError(format!("Failed to deserialize children: {err}"))
                })
            })
            .transpose()?;

        Ok(Some(RunDetails {
            run_info: RunInfo {
                metadata: run
                    .metadata
                    .as_ref()
                    .and_then(|json| serde_json::from_value::<GroupKind>(json.clone()).ok()),
                root_ref: run.root_source_id.as_ref().map(|source_id| RootReference {
                    source_id: source_id.clone(),
                    run_index: run.root_run_index,
                    replay_ref: run.root_replay_ref.unwrap_or_default(),
                }),
                variables: run.variables.map(|v| v.to_inner()),
                source_id: run.source_id,
                run_index: run.run_index,
                lookup_id: run.lookup_id.map(|id| id.to_string()),
                user_id: run.user_id,
                status,
                created_at: run.created_at.into(),
                updated_at: run.updated_at.into(),
            },
            output: run
                .output
                .as_ref()
                .and_then(|output_json| serde_json::to_value(output_json.clone()).ok()),
            children,
            blocks,
            error: run.error,
        }))
    }

    async fn lookup(&self, lookup_id: &str) -> Result<Option<RunDetails>, OxyError> {
        let lookup_id = Uuid::parse_str(lookup_id).map_err(|err| {
            OxyError::ArgumentError(format!("Invalid lookup_id format, must be UUID: {err}"))
        })?;
        let run = entity::runs::Entity::find()
            .filter(
                entity::runs::Column::LookupId
                    .eq(lookup_id)
                    .and(entity::runs::Column::ProjectId.eq(self.project_id))
                    .and(entity::runs::Column::BranchId.eq(self.branch_id)),
            )
            .one(&self.connection)
            .await
            .map_err(|err| OxyError::DBError(format!("Failed to fetch run by lookup_id: {err}")))?;
        if run.is_none() {
            return Ok(None);
        }
        let run = run.unwrap();

        let status = match (&run.blocks, &run.error) {
            (_, Some(_)) => RunStatus::Failed,
            (Some(_), None) => RunStatus::Completed,
            (None, None) => RunStatus::Pending,
        };
        let blocks = run
            .blocks
            .map(|blocks_json| {
                serde_json::from_value::<HashMap<String, Block>>(blocks_json).map_err(|err| {
                    OxyError::SerializerError(format!("Failed to deserialize blocks: {err}"))
                })
            })
            .transpose()?;
        let children = run
            .children
            .map(|children_json| {
                serde_json::from_value::<Vec<String>>(children_json).map_err(|err| {
                    OxyError::SerializerError(format!("Failed to deserialize children: {err}"))
                })
            })
            .transpose()?;

        Ok(Some(RunDetails {
            run_info: RunInfo {
                metadata: run
                    .metadata
                    .as_ref()
                    .and_then(|json| serde_json::from_value::<GroupKind>(json.clone()).ok()),
                root_ref: run.root_source_id.as_ref().map(|source_id| RootReference {
                    source_id: source_id.clone(),
                    run_index: run.root_run_index,
                    replay_ref: run.root_replay_ref.unwrap_or_default(),
                }),
                variables: run.variables.map(|v| v.to_inner()),
                source_id: run.source_id,
                run_index: run.run_index,
                lookup_id: run.lookup_id.map(|id| id.to_string()),
                user_id: run.user_id,
                status,
                created_at: run.created_at.into(),
                updated_at: run.updated_at.into(),
            },
            output: run
                .output
                .as_ref()
                .and_then(|output_json| serde_json::to_value(output_json.clone()).ok()),
            children,
            blocks,
            error: run.error,
        }))
    }

    async fn list_runs(
        &self,
        source_id: &str,
        pagination: &Pagination,
    ) -> Result<Paginated<RunInfo>, OxyError> {
        tracing::info!(
            "Listing runs for source_id: {}, page: {}, size: {}",
            source_id,
            pagination.page,
            pagination.size
        );
        let query = entity::runs::Entity::find()
            .filter(
                entity::runs::Column::SourceId
                    .eq(source_id)
                    .and(entity::runs::Column::ProjectId.eq(self.project_id))
                    .and(entity::runs::Column::BranchId.eq(self.branch_id))
                    .and(entity::runs::Column::RunIndex.is_not_null()),
            )
            .order_by_desc(entity::runs::Column::RunIndex)
            .paginate(&self.connection, pagination.size as u64);
        let num_pages = query
            .num_pages()
            .await
            .map_err(|err| OxyError::DBError(format!("Failed to get number of pages: {err}")))?;
        let runs = query
            .fetch_page(pagination.page as u64 - 1)
            .await
            .map_err(|err| OxyError::DBError(format!("Failed to list runs: {err}")))?;
        let run_infos = runs
            .into_iter()
            .map(|run| RunInfo {
                metadata: None,
                root_ref: run.root_source_id.as_ref().map(|source_id| RootReference {
                    source_id: source_id.clone(),
                    run_index: run.root_run_index,
                    replay_ref: run.root_replay_ref.unwrap_or_default(),
                }),
                source_id: run.source_id,
                run_index: run.run_index,
                lookup_id: run.lookup_id.map(|id| id.to_string()),
                user_id: run.user_id,
                status: match (run.blocks, run.error) {
                    (_, Some(_)) => RunStatus::Failed,
                    (Some(_), None) => RunStatus::Completed,
                    (None, None) => RunStatus::Pending,
                },
                variables: run.variables.map(|v| v.to_inner()),
                created_at: run.created_at.into(),
                updated_at: run.updated_at.into(),
            })
            .collect();
        Ok(Paginated {
            items: run_infos,
            pagination: Pagination {
                page: pagination.page,
                size: pagination.size,
                num_pages: Some(num_pages as usize),
            },
        })
    }

    async fn delete_run(&self, source_id: &str, run_index: i32) -> Result<(), OxyError> {
        let result = entity::runs::Entity::delete_many()
            .filter(
                entity::runs::Column::SourceId
                    .eq(source_id)
                    .and(entity::runs::Column::RunIndex.eq(Some(run_index)))
                    .and(entity::runs::Column::ProjectId.eq(self.project_id))
                    .and(entity::runs::Column::BranchId.eq(self.branch_id)),
            )
            .exec(&self.connection)
            .await
            .map_err(|err| OxyError::DBError(format!("Failed to delete run: {err}")))?;

        if result.rows_affected == 0 {
            return Err(OxyError::DBError(format!(
                "No run found for source_id: {} with run_index: {}",
                source_id, run_index
            )));
        }

        tracing::info!(
            "Deleted run for source_id: {}, run_index: {}",
            source_id,
            run_index
        );
        Ok(())
    }

    async fn bulk_delete_runs(&self, run_ids: Vec<(String, i32)>) -> Result<u64, OxyError> {
        if run_ids.is_empty() {
            return Ok(0);
        }

        // Build the condition for bulk deletion
        let mut condition = Condition::any();
        for (source_id, run_index) in run_ids {
            let run_condition = Condition::all()
                .add(entity::runs::Column::SourceId.eq(source_id))
                .add(entity::runs::Column::RunIndex.eq(Some(run_index)));
            condition = condition.add(run_condition);
        }

        let final_condition = Condition::all()
            .add(condition)
            .add(entity::runs::Column::ProjectId.eq(self.project_id))
            .add(entity::runs::Column::BranchId.eq(self.branch_id));

        let result = entity::runs::Entity::delete_many()
            .filter(final_condition)
            .exec(&self.connection)
            .await
            .map_err(|err| OxyError::DBError(format!("Failed to bulk delete runs: {err}")))?;

        tracing::info!("Bulk deleted {} runs", result.rows_affected);
        Ok(result.rows_affected)
    }
}
