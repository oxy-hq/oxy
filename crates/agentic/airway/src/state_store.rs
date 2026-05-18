//! `AirwayPgStateStore` — implements [`airway::StateStore`] against
//! oxy's SeaORM-managed Postgres.
//!
//! Backs the airway engine's per-pipeline incremental state + schema
//! + audit log with the same database that holds `agentic_runs` and
//! the rest of the platform tables. One row per pipeline_name in
//! [`crate::extension::pipeline_state`]; one row per load in
//! [`crate::extension::load_audit`].
//!
//! Optimistic concurrency: `save` writes `version + 1` only if the
//! row's current `version` matches `expected_version`. A concurrent
//! writer that bumped the version first causes the second writer to
//! see zero rows updated and surface `AirwayError::State` so airway
//! can reload + retry. Mirrors `airway::state::postgres::PostgresStateStore`'s
//! semantics, just via SeaORM instead of raw tokio-postgres.

use std::sync::Arc;

use airway::state::{PipelineState, StateSnapshot, StateStore};
use airway::{AirwayError, Schema};
use async_trait::async_trait;
use chrono::Utc;
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};

use crate::extension::load_audit::{self, Entity as LoadAuditEntity, status as load_status};
use crate::extension::pipeline_state::{
    self, Column as PipelineStateColumn, Entity as PipelineStateEntity,
};

/// SeaORM-backed [`StateStore`] for a single pipeline_name.
///
/// Construct one per pipeline run; the worker hands this to airway via
/// [`airway::Pipeline::with_state_store`].
#[derive(Clone)]
pub struct AirwayPgStateStore {
    db: Arc<DatabaseConnection>,
    pipeline_name: String,
}

impl AirwayPgStateStore {
    pub fn new(db: Arc<DatabaseConnection>, pipeline_name: impl Into<String>) -> Self {
        Self {
            db,
            pipeline_name: pipeline_name.into(),
        }
    }

    pub fn pipeline_name(&self) -> &str {
        &self.pipeline_name
    }
}

#[async_trait]
impl StateStore for AirwayPgStateStore {
    async fn load(&self) -> Result<StateSnapshot, AirwayError> {
        let row = PipelineStateEntity::find_by_id(self.pipeline_name.clone())
            .one(self.db.as_ref())
            .await
            .map_err(|e| AirwayError::State(format!("load pipeline_state: {e}")))?;

        match row {
            None => Ok(StateSnapshot::default()),
            Some(model) => {
                let state: PipelineState = serde_json::from_value(model.state)
                    .map_err(|e| AirwayError::State(format!("deserialize PipelineState: {e}")))?;
                let schema: Schema = serde_json::from_value(model.schema_json)
                    .map_err(|e| AirwayError::State(format!("deserialize Schema: {e}")))?;
                Ok(StateSnapshot {
                    state,
                    schema: Some(schema),
                    version: model.version,
                })
            }
        }
    }

    async fn save(
        &self,
        state: &PipelineState,
        schema: &Schema,
        expected_version: i64,
    ) -> Result<(), AirwayError> {
        let state_json = serde_json::to_value(state)
            .map_err(|e| AirwayError::State(format!("serialize PipelineState: {e}")))?;
        let schema_json = serde_json::to_value(schema)
            .map_err(|e| AirwayError::State(format!("serialize Schema: {e}")))?;
        let new_version = expected_version + 1;

        // Initial insert path: expected_version 0 + no row yet.
        // Use INSERT ... ON CONFLICT DO UPDATE WHERE version = expected.
        // That single round-trip handles both insert and version-checked update.
        let model = pipeline_state::ActiveModel {
            pipeline_name: ActiveValue::Set(self.pipeline_name.clone()),
            state: ActiveValue::Set(state_json),
            schema_json: ActiveValue::Set(schema_json),
            version: ActiveValue::Set(new_version),
            updated_at: ActiveValue::Set(Utc::now()),
        };

        // `update_columns` + `target_condition` lets us enforce
        // `version = expected_version` as part of the upsert.
        let exec = PipelineStateEntity::insert(model)
            .on_conflict(
                OnConflict::column(PipelineStateColumn::PipelineName)
                    .update_columns([
                        PipelineStateColumn::State,
                        PipelineStateColumn::SchemaJson,
                        PipelineStateColumn::Version,
                        PipelineStateColumn::UpdatedAt,
                    ])
                    .target_and_where(PipelineStateColumn::Version.eq(expected_version))
                    .to_owned(),
            )
            .exec(self.db.as_ref())
            .await;

        match exec {
            Ok(_) => {
                // Insert path returns Ok; the conflict-update path also
                // returns Ok when at least one row matched. But sea-orm
                // doesn't surface "0 rows updated" as an error, so we
                // re-read to confirm the version actually advanced. This
                // matches airway::PostgresStateStore's defensive check.
                let cur = PipelineStateEntity::find_by_id(self.pipeline_name.clone())
                    .one(self.db.as_ref())
                    .await
                    .map_err(|e| AirwayError::State(format!("verify save: {e}")))?
                    .ok_or_else(|| {
                        AirwayError::State(format!(
                            "pipeline_state row vanished after save for `{}`",
                            self.pipeline_name
                        ))
                    })?;
                if cur.version != new_version {
                    return Err(AirwayError::State(format!(
                        "optimistic concurrency conflict for pipeline `{}`: \
                         expected version {expected_version}, current is {}",
                        self.pipeline_name, cur.version,
                    )));
                }
                Ok(())
            }
            Err(e) => Err(AirwayError::State(format!("save pipeline_state: {e}"))),
        }
    }

    async fn record_load_start(
        &self,
        load_id: &str,
        pipeline_name: &str,
        schema_hash: &str,
    ) -> Result<(), AirwayError> {
        // airway hands us `&str` because `Schema.version_hash` is `String`.
        // An empty value means the state store had no prior schema yet
        // (first-ever load for this pipeline) — store as NULL so the
        // audit log distinguishes that from "ran against an empty schema".
        let schema_hash = if schema_hash.is_empty() {
            None
        } else {
            Some(schema_hash.to_string())
        };
        let model = load_audit::ActiveModel {
            load_id: ActiveValue::Set(load_id.to_string()),
            pipeline_name: ActiveValue::Set(pipeline_name.to_string()),
            schema_hash: ActiveValue::Set(schema_hash),
            status: ActiveValue::Set(load_status::IN_PROGRESS.to_string()),
            error_message: ActiveValue::Set(None),
            partial: ActiveValue::Set(false),
            started_at: ActiveValue::Set(Utc::now()),
            finished_at: ActiveValue::Set(None),
        };
        LoadAuditEntity::insert(model)
            .exec(self.db.as_ref())
            .await
            .map_err(|e| AirwayError::State(format!("record_load_start: {e}")))?;
        Ok(())
    }

    async fn record_load_complete(
        &self,
        load_id: &str,
        _table_counts: &std::collections::HashMap<String, usize>,
    ) -> Result<(), AirwayError> {
        // Per-table counts are already on the `LoadCompleted` event;
        // the audit row just records the terminal status + finish ts.
        let existing = LoadAuditEntity::find()
            .filter(load_audit::Column::LoadId.eq(load_id.to_string()))
            .one(self.db.as_ref())
            .await
            .map_err(|e| AirwayError::State(format!("record_load_complete lookup: {e}")))?
            .ok_or_else(|| {
                AirwayError::State(format!("load_audit row not found for `{load_id}`"))
            })?;
        let mut active: load_audit::ActiveModel = existing.into();
        active.status = ActiveValue::Set(load_status::COMPLETED.to_string());
        active.finished_at = ActiveValue::Set(Some(Utc::now()));
        active
            .update(self.db.as_ref())
            .await
            .map_err(|e| AirwayError::State(format!("record_load_complete: {e}")))?;
        Ok(())
    }

    async fn record_load_partial(
        &self,
        load_id: &str,
        _table_counts: &std::collections::HashMap<String, usize>,
    ) -> Result<(), AirwayError> {
        // The run finished (status Completed) but some resources/tables
        // were skipped — `partial = true` distinguishes it from a clean
        // load. Per-table counts ride the events, as with complete.
        let existing = LoadAuditEntity::find()
            .filter(load_audit::Column::LoadId.eq(load_id.to_string()))
            .one(self.db.as_ref())
            .await
            .map_err(|e| AirwayError::State(format!("record_load_partial lookup: {e}")))?
            .ok_or_else(|| {
                AirwayError::State(format!("load_audit row not found for `{load_id}`"))
            })?;
        let mut active: load_audit::ActiveModel = existing.into();
        active.status = ActiveValue::Set(load_status::COMPLETED.to_string());
        active.partial = ActiveValue::Set(true);
        active.finished_at = ActiveValue::Set(Some(Utc::now()));
        active
            .update(self.db.as_ref())
            .await
            .map_err(|e| AirwayError::State(format!("record_load_partial: {e}")))?;
        Ok(())
    }

    async fn record_load_failed(&self, load_id: &str, error: &str) -> Result<(), AirwayError> {
        let existing = LoadAuditEntity::find()
            .filter(load_audit::Column::LoadId.eq(load_id.to_string()))
            .one(self.db.as_ref())
            .await
            .map_err(|e| AirwayError::State(format!("record_load_failed lookup: {e}")))?
            .ok_or_else(|| {
                AirwayError::State(format!("load_audit row not found for `{load_id}`"))
            })?;
        let mut active: load_audit::ActiveModel = existing.into();
        active.status = ActiveValue::Set(load_status::FAILED.to_string());
        active.error_message = ActiveValue::Set(Some(error.to_string()));
        active.finished_at = ActiveValue::Set(Some(Utc::now()));
        active
            .update(self.db.as_ref())
            .await
            .map_err(|e| AirwayError::State(format!("record_load_failed: {e}")))?;
        Ok(())
    }
}

// Unit tests for `AirwayPgStateStore` need a real Postgres (the impl
// is a thin SeaORM wrapper, mocking just shadows what SeaORM already
// tests). Real integration coverage lives in the testcontainers suite
// that will follow this in the worker slice.
