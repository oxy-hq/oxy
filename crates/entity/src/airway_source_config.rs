use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Per-source-kind airway admission config, with a sparse per-workspace
/// override.
///
/// `workspace_id IS NULL` is the **global** row for that `source_kind`; a
/// non-null one overrides it. Both `contract_policy` and `environment` are
/// nullable and merge **field by field** — a workspace row setting only
/// `environment` still inherits `contract_policy` from the global row. See
/// `agentic_pipeline::airway_config::resolve_admission`.
///
/// Only the two keys stage 2 can enforce live here. `max_rewind`,
/// `cursor_lag_floor` and per-resource restatement overrides land in stage 4,
/// with the code that honours them — a knob nothing reads is the failure
/// airway's own plan calls out.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "airway_source_config")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    /// `toast`, `quickbooks`, `weather`, `rest_api`, … — matches
    /// `AirwayPipelineSpec::source.kind`.
    pub source_kind: String,
    /// `None` = the global row for this kind.
    pub workspace_id: Option<Uuid>,
    /// `permissive` | `require_declared` | `forbid_opaque`. Parsed by
    /// `agentic_airway::AirwayAdmission::from_strings`, which errors rather
    /// than defaulting on an unknown spelling.
    pub contract_policy: Option<String>,
    /// `production` | `sandbox`. Same parser, same refusal.
    pub environment: Option<String>,
    /// Maintained by the database, not by the writer: `DEFAULT now()` on
    /// insert and a `BEFORE UPDATE` trigger
    /// (`airway_source_config_set_updated_at`) on every update, including the
    /// update half of an `ON CONFLICT DO UPDATE` upsert. A writer may `Set` it
    /// — the trigger simply overwrites with the same `now()` — but does not
    /// have to, and cannot make it lag by forgetting. That matters because the
    /// admin surface reports this value as "when this policy last changed".
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::workspaces::Entity",
        from = "Column::WorkspaceId",
        to = "super::workspaces::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Workspaces,
}

impl Related<super::workspaces::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Workspaces.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
