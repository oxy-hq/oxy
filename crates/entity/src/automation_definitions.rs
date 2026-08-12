//! `automation_definitions` — covers `.automation.yml` and `.procedure.yml`.
//! The latter is a legacy extension for the
//! same parsed type; `extension` is recorded so a future deprecation
//! pass can grep for the legacy ones.
//!
//! The physical table was renamed from `procedure_definitions` by
//! migration `m20260623_000001_rename_procedures_to_automations`, which
//! also leaves a back-compat `procedure_definitions` view. The module
//! `entity::procedure_definitions` is kept as an alias of this one (see
//! `lib.rs`) so existing call sites keep compiling.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "automation_definitions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub revision_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub file_path: String,
    pub name: String,
    pub extension: String,
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
