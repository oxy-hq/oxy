//! `monitor_configs` — the compiled view of `.monitor.yml`, one row
//! per revision. The full payload (top-level `schedule` + `monitors`)
//! lives in a single JSONB column; the runtime reconstructs a
//! `MonitorConfig` from it with `serde_json::from_value`.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "monitor_configs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub revision_id: Uuid,
    pub definition: Json,
    #[sea_orm(
        belongs_to,
        from = "revision_id",
        to = "revision_id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    #[serde(skip)]
    pub revisions: BelongsTo<super::revisions::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
