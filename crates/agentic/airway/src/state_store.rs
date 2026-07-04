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
    ActiveModelTrait, ActiveValue, ColumnTrait, ConnectionTrait, DatabaseBackend,
    DatabaseConnection, EntityTrait, QueryFilter, Statement,
};

use crate::extension::load_audit::{self, Entity as LoadAuditEntity, status as load_status};
use crate::extension::pipeline_state::{
    self, Column as PipelineStateColumn, Entity as PipelineStateEntity,
};
use crate::extension::run_extension::Entity as RunExtEntity;

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

/// Run-scoped [`StateStore`] for a backfill chunk.
///
/// Persists the incremental **cursor** to `airway_run_extensions.resume_state`
/// (keyed by `run_id`) so a reset-in-place retry resumes mid-window instead of
/// re-extracting the whole window. The **schema** and the load-audit log are
/// delegated to the pipeline-global [`AirwayPgStateStore`]: a backfill reads the
/// live schema but never writes it (so it can't clobber/race the live schema),
/// and never advances the live incremental cursor (`resume_state` is per-run).
///
/// Single writer per run, so `save` skips optimistic concurrency on
/// `resume_state` (the pipeline-global schema is never written here).
///
/// NOTE: this is the host seam for mid-window resume (P2c-1). It is inert until
/// the airway engine gains a mid-run persist hook and the Toast source stops
/// freezing the cursor during a backfill (P2c-2/3) — see
/// `docs/plans/airway-midwindow-resume.md`. Wiring it in for backfill runs is
/// safe before then: the source still emits no advanced cursor, so `resume_state`
/// just round-trips an empty cursor and the run re-extracts as it does today.
#[derive(Clone)]
pub struct AirwayRunScopedStateStore {
    db: Arc<DatabaseConnection>,
    run_id: String,
    global: AirwayPgStateStore,
}

impl AirwayRunScopedStateStore {
    pub fn new(
        db: Arc<DatabaseConnection>,
        run_id: impl Into<String>,
        pipeline_name: impl Into<String>,
    ) -> Self {
        let global = AirwayPgStateStore::new(Arc::clone(&db), pipeline_name);
        Self {
            db,
            run_id: run_id.into(),
            global,
        }
    }

    pub fn run_id(&self) -> &str {
        &self.run_id
    }
}

#[async_trait]
impl StateStore for AirwayRunScopedStateStore {
    async fn load(&self) -> Result<StateSnapshot, AirwayError> {
        // Schema + pipeline identity come from the pipeline-global row.
        let global = self.global.load().await?;
        // Cursor comes from THIS run's resume_state if a prior attempt persisted
        // one; otherwise start with an empty cursor so the source runs from the
        // window start — never inheriting the live incremental position.
        let ext = RunExtEntity::find_by_id(self.run_id.clone())
            .one(self.db.as_ref())
            .await
            .map_err(|e| AirwayError::State(format!("load run_extension: {e}")))?;
        let resume: Option<PipelineState> = ext
            .and_then(|m| m.resume_state)
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| AirwayError::State(format!("deserialize resume_state: {e}")))?;
        let state = match resume {
            Some(s) => s,
            None => {
                // Keep pipeline identity + schema_version_hash from the global
                // state, but clear the cursors so we don't inherit the live one.
                let mut s = global.state.clone();
                s.resource_states.clear();
                s
            }
        };
        Ok(StateSnapshot {
            state,
            schema: global.schema,
            // Run-scoped single writer: optimistic concurrency not needed.
            version: 0,
        })
    }

    async fn save(
        &self,
        state: &PipelineState,
        _schema: &Schema,
        _expected_version: i64,
    ) -> Result<(), AirwayError> {
        // Persist ONLY the cursor, to this run's resume_state. The schema is
        // deliberately not written — a backfill must not touch the live schema.
        let state_json = serde_json::to_value(state)
            .map_err(|e| AirwayError::State(format!("serialize resume_state: {e}")))?;
        let res = self
            .db
            .as_ref()
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "UPDATE airway_run_extensions SET resume_state = $1 WHERE run_id = $2",
                [state_json.into(), self.run_id.clone().into()],
            ))
            .await
            .map_err(|e| AirwayError::State(format!("save resume_state: {e}")))?;
        if res.rows_affected() == 0 {
            // The extension row is inserted at run start, so a miss means the run
            // is non-airway or its row was purged — the cursor is silently lost and
            // resume degrades to a full re-extract. Surface it so a future
            // regression (a store keyed off a missing run) doesn't go unnoticed.
            tracing::warn!(
                run_id = %self.run_id,
                "run-scoped resume_state UPDATE matched no airway_run_extensions row"
            );
        }
        Ok(())
    }

    async fn record_load_start(
        &self,
        load_id: &str,
        pipeline_name: &str,
        schema_hash: &str,
    ) -> Result<(), AirwayError> {
        self.global
            .record_load_start(load_id, pipeline_name, schema_hash)
            .await
    }

    async fn record_load_complete(
        &self,
        load_id: &str,
        table_counts: &std::collections::HashMap<String, usize>,
    ) -> Result<(), AirwayError> {
        self.global
            .record_load_complete(load_id, table_counts)
            .await
    }

    async fn record_load_partial(
        &self,
        load_id: &str,
        table_counts: &std::collections::HashMap<String, usize>,
    ) -> Result<(), AirwayError> {
        self.global.record_load_partial(load_id, table_counts).await
    }

    async fn record_load_failed(&self, load_id: &str, error: &str) -> Result<(), AirwayError> {
        self.global.record_load_failed(load_id, error).await
    }
}

// Unit tests for `AirwayPgStateStore` need a real Postgres (the impl
// is a thin SeaORM wrapper, mocking just shadows what SeaORM already
// tests). Real integration coverage lives in the testcontainers suite
// that will follow this in the worker slice.
