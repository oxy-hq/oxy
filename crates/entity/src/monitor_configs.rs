//! `monitor_configs` — the compiled view of `.monitor.yml`, one row
//! per revision. The full payload (top-level `schedule` + `monitors`)
//! lives in a single JSONB column; the runtime reconstructs a
//! `MonitorConfig` from it with `serde_json::from_value`.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "monitor_configs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub revision_id: Uuid,
    pub definition: Json,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::revisions::Entity",
        from = "Column::RevisionId",
        to = "super::revisions::Column::RevisionId",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Revisions,
}

impl Related<super::revisions::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Revisions.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
