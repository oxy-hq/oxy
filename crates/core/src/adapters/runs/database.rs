use std::collections::HashMap;

use sea_orm::{
    ActiveValue, QueryOrder, QuerySelect, TransactionError, TransactionTrait, prelude::*,
};

use crate::{
    adapters::runs::storage::RunsStorage,
    config::constants::AGENT_RETRY_MAX_ELAPSED_TIME,
    errors::OxyError,
    service::types::{
        block::{Block, Group, GroupId, GroupKind},
        pagination::{Paginated, Pagination},
        run::{RootReference, RunDetails, RunInfo, RunStatus},
    },
};

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
}

impl RunsStorage for RunsDatabaseStorage {
    async fn last_run(&self, source_id: &str) -> Result<Option<RunInfo>, OxyError> {
        let run = entity::runs::Entity::find()
            .filter(entity::runs::Column::SourceId.eq(source_id))
            .filter(entity::runs::Column::ProjectId.eq(self.project_id))
            .filter(entity::runs::Column::BranchId.eq(self.branch_id))
            .order_by_desc(entity::runs::Column::RunIndex)
            .one(&self.connection)
            .await
            .map_err(|err| OxyError::DBError(format!("Failed to fetch last run: {err}")))?;
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
            created_at: run.created_at.into(),
            updated_at: run.updated_at.into(),
        }))
    }

    async fn new_run(
        &self,
        source_id: &str,
        root_ref: Option<RootReference>,
    ) -> Result<RunInfo, OxyError> {
        let connection = self.connection.clone();
        let project_id = self.project_id;
        let branch_id = self.branch_id;
        let source_id = source_id.to_string();
        let root_ref = root_ref.clone();

        let mut attempt = 0;

        let new_run_func = {
            let connection = connection.clone();
            let project_id = project_id;
            let branch_id = branch_id;
            let source_id = source_id.clone();
            let root_ref = root_ref.clone();
            move || {
                let connection = connection.clone();
                let project_id = project_id;
                let branch_id = branch_id;
                let source_id = source_id.clone();
                let root_ref = root_ref.clone();
                async move {
                    connection
                        .transaction::<_, RunInfo, DbErr>(move |txn| {
                            let source_id = source_id.clone();
                            let root_ref = root_ref.clone();
                            Box::pin(async move {
                                // Find run index based on last run for the new run
                                let max_run_id = entity::runs::Entity::find()
                                    .filter(
                                        entity::runs::Column::SourceId
                                            .eq(&source_id)
                                            .and(entity::runs::Column::RunIndex.is_not_null()),
                                    )
                                    .order_by_desc(entity::runs::Column::RunIndex)
                                    .lock_exclusive()
                                    .one(txn)
                                    .await
                                    .map(|opt| opt.map(|run| run.run_index))?;
                                let run_index = match max_run_id {
                                    None => Some(1),
                                    Some(index) => index.map(|id| id + 1),
                                };
                                // Create a new run with the initial state
                                let mut run = entity::runs::ActiveModel {
                                    id: ActiveValue::Set(uuid::Uuid::new_v4()),
                                    source_id: ActiveValue::Set(source_id.to_string()),
                                    run_index: ActiveValue::Set(run_index),
                                    metadata: ActiveValue::Set(None), // Start with no metadata
                                    blocks: ActiveValue::Set(None),   // Start with no blocks
                                    error: ActiveValue::Set(None),
                                    project_id: ActiveValue::Set(project_id),
                                    branch_id: ActiveValue::Set(branch_id),
                                    created_at: ActiveValue::Set(chrono::Utc::now().into()),
                                    updated_at: ActiveValue::Set(chrono::Utc::now().into()),
                                    ..Default::default()
                                };
                                match root_ref {
                                    Some(ref root) => {
                                        run.root_source_id =
                                            ActiveValue::Set(Some(root.source_id.clone()));
                                        run.root_run_index = ActiveValue::Set(root.run_index);
                                        run.root_replay_ref =
                                            ActiveValue::Set(Some(root.replay_ref.clone()));
                                    }
                                    None => {
                                        run.root_source_id = ActiveValue::Set(None);
                                        run.root_run_index = ActiveValue::Set(None);
                                        run.root_replay_ref = ActiveValue::Set(None);
                                    }
                                }
                                run.insert(txn).await?;
                                tracing::info!(
                                    "New run created successfully for source_id: {}",
                                    source_id
                                );
                                Ok(RunInfo {
                                    metadata: None,
                                    root_ref,
                                    source_id: source_id.to_string(),
                                    run_index,
                                    status: RunStatus::Pending,
                                    created_at: chrono::Utc::now(),
                                    updated_at: chrono::Utc::now(),
                                })
                            })
                        })
                        .await
                        .map_err(|err| match err {
                            TransactionError::Transaction(DbErr::Exec(RuntimeErr::SqlxError(
                                e,
                            ))) => match e {
                                sqlx::Error::Database(db_err) => {
                                    match db_err.code().map(|c| c.to_string()).as_deref() {
                                        Some("517") | Some("5") => {
                                            backoff::Error::<OxyError>::transient(
                                                OxyError::DBError(
                                                    "Database is locked, retrying...".to_string(),
                                                ),
                                            )
                                        }
                                        _ => backoff::Error::<OxyError>::permanent(
                                            OxyError::DBError(format!(
                                                "Database error({:?}): {}",
                                                db_err.code(),
                                                db_err.message()
                                            )),
                                        ),
                                    }
                                }
                                _ => backoff::Error::<OxyError>::permanent(OxyError::DBError(
                                    format!("SQLx error: {e}"),
                                )),
                            },
                            _ => backoff::Error::<OxyError>::permanent(OxyError::DBError(format!(
                                "Failed to create new run: {err}"
                            ))),
                        })
                }
            }
        };

        backoff::future::retry_notify(
            backoff::ExponentialBackoffBuilder::default()
                .with_max_elapsed_time(Some(AGENT_RETRY_MAX_ELAPSED_TIME))
                .build(),
            || new_run_func(),
            |err, b| {
                attempt += 1;
                tracing::error!(
                    "Error happened at {:?} in RunsManager new run: {:?}",
                    b,
                    err
                );
                tracing::warn!("Retrying({})...", attempt);
            },
        )
        .await
    }

    async fn upsert_run(&self, group: Group) -> Result<(), OxyError> {
        let metadata_json = serde_json::to_value(&group.group_kind).map_err(|err| {
            OxyError::SerializerError(format!("Failed to serialize metadata: {err}"))
        })?;
        let blocks_json = serde_json::to_value(&group.blocks).map_err(|err| {
            OxyError::SerializerError(format!("Failed to serialize blocks: {err}"))
        })?;
        let children_json = serde_json::to_value(&group.children).map_err(|err| {
            OxyError::SerializerError(format!("Failed to serialize children: {err}"))
        })?;

        let (source_id, run_index) =
            match group.group_id() {
                GroupId::Workflow {
                    workflow_id,
                    run_id,
                } => (
                    workflow_id,
                    Some(run_id.parse::<i32>().map_err(|_| {
                        OxyError::RuntimeError("Invalid run index format".to_string())
                    })?),
                ),
                GroupId::Artifact { artifact_id } => {
                    (artifact_id, None) // Artifact runs don't have a run index
                }
            };
        // Use runs active model to upsert the run data
        // Check if the run already exists in the database
        let existing_run = entity::runs::Entity::find()
            .filter(
                entity::runs::Column::SourceId
                    .eq(&source_id)
                    .and(entity::runs::Column::ProjectId.eq(self.project_id))
                    .and(entity::runs::Column::BranchId.eq(self.branch_id))
                    .and(entity::runs::Column::RunIndex.eq(run_index)),
            )
            .one(&self.connection)
            .await
            .map_err(|err| OxyError::DBError(format!("Failed to check existing run: {err}")))?;

        if let Some(run) = existing_run {
            // Update existing run
            let mut active_model: entity::runs::ActiveModel = run.into();
            active_model.metadata = ActiveValue::Set(Some(metadata_json));
            active_model.blocks = ActiveValue::Set(Some(blocks_json));
            active_model.children = ActiveValue::Set(Some(children_json));
            active_model.updated_at = ActiveValue::Set(chrono::Utc::now().into());
            active_model.error = ActiveValue::Set(group.error);
            active_model
                .update(&self.connection)
                .await
                .map_err(|err| OxyError::DBError(format!("Failed to update run: {err}")))?;
        } else {
            // Insert new run
            let new_run = entity::runs::ActiveModel {
                id: ActiveValue::Set(uuid::Uuid::new_v4()),
                source_id: ActiveValue::Set(source_id.to_string()),
                run_index: ActiveValue::Set(run_index),
                metadata: ActiveValue::Set(Some(metadata_json)),
                blocks: ActiveValue::Set(Some(blocks_json)),
                children: ActiveValue::Set(Some(children_json)),
                error: ActiveValue::Set(group.error),
                created_at: ActiveValue::Set(chrono::Utc::now().into()),
                updated_at: ActiveValue::Set(chrono::Utc::now().into()),
                project_id: ActiveValue::Set(self.project_id),
                branch_id: ActiveValue::Set(self.branch_id),
                ..Default::default()
            };
            new_run
                .insert(&self.connection)
                .await
                .map_err(|err| OxyError::DBError(format!("Failed to insert run: {err}")))?;
        }

        Ok(())
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
            .await
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
                status: match (run.blocks, run.error) {
                    (_, Some(_)) => RunStatus::Failed,
                    (Some(_), None) => RunStatus::Completed,
                    (None, None) => RunStatus::Pending,
                },
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
                source_id: run.source_id,
                run_index: run.run_index,
                status,
                created_at: run.created_at.into(),
                updated_at: run.updated_at.into(),
            },
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
        // Query runs from the database
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
                status: match (run.blocks, run.error) {
                    (_, Some(_)) => RunStatus::Failed,
                    (Some(_), None) => RunStatus::Completed,
                    (None, None) => RunStatus::Pending,
                },
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
}
