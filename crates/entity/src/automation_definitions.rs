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
