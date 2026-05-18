//! `airway_run_extensions` — per-run metadata extending `agentic_runs`.
//!
//! Same shape as `analytics_run_extensions` and `agentic_workflow_state`:
//! one row per `agentic_runs.id` whose `source_type = 'airway'`, FK +
//! cascade to the runtime row.
//!
//! `load_id` is a loose reference to `airway_load_audit.load_id` — no
//! DB FK per `internal-docs/domain-boundaries.md` rule P3 (cross-aggregate
//! references are loose UUID/string columns).

use sea_orm::entity::prelude::*;
use sea_orm::{ActiveValue, ConnectionTrait, DbErr};

use crate::config::AirwayPipelineSpec;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "airway_run_extensions")]
pub struct Model {
    /// Mirror of `agentic_runs.id` for the airway run.
    #[sea_orm(primary_key, auto_increment = false)]
    pub run_id: String,
    /// The `AirwayPipelineSpec.name` used for this run.
    pub pipeline_name: String,
    /// Source file the pipeline was loaded from (`.airway.yml` path)
    /// when applicable. `None` for inline-spec runs.
    pub pipeline_ref: Option<String>,
    /// Loose ref to `airway_load_audit.load_id`. App-level cleanup —
    /// no DB FK.
    pub load_id: Option<String>,
    /// Concurrency knob the run was launched with.
    pub concurrency: i32,
    /// Selected resources (empty array = "all the source advertises").
    pub resources: Json,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

/// Insert the per-run metadata row for a fresh airway run.
///
/// Called by the pipeline facade at run-start time after the
/// `agentic_runs` row is inserted (the `run_id` FK target exists by
/// then). `load_id` is left `None` here — the worker fills it in when
/// the engine generates the load_id at the top of the run.
pub async fn insert_run_extension<C>(
    db: &C,
    run_id: &str,
    spec: &AirwayPipelineSpec,
    pipeline_ref: Option<&str>,
) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    let resources_json =
        serde_json::to_value(&spec.resources).unwrap_or(serde_json::Value::Array(Vec::new()));
    let model = ActiveModel {
        run_id: ActiveValue::Set(run_id.to_string()),
        pipeline_name: ActiveValue::Set(spec.name.clone()),
        pipeline_ref: ActiveValue::Set(pipeline_ref.map(str::to_string)),
        load_id: ActiveValue::Set(None),
        concurrency: ActiveValue::Set(spec.concurrency as i32),
        resources: ActiveValue::Set(resources_json),
    };
    Entity::insert(model).exec(db).await?;
    Ok(())
}

/// Look up the per-run metadata for an airway run. `None` when the
/// run wasn't airway (or hasn't had its extension row inserted yet).
pub async fn get_run_extension<C>(db: &C, run_id: &str) -> Result<Option<Model>, DbErr>
where
    C: ConnectionTrait,
{
    Entity::find_by_id(run_id.to_string()).one(db).await
}
