//! `procedure_definitions` — covers `.procedure.yml`, `.workflow.yml`,
//! and `.automation.yml`. The latter two are legacy extensions for the
//! same parsed type; `extension` is recorded so a future deprecation
//! pass can grep for the legacy ones.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "procedure_definitions")]
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
