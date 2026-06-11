//! `compiled_references` — denormalised cross-entity reference graph.
//! Powers fast "who calls X" / "what does Y depend on" without
//! scanning every entity's JSON body. The Context Graph page maps
//! naturally onto this table; today it re-derives the same data from
//! FS walks.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "compiled_references")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub revision_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub from_kind: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub from_name: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub to_kind: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub to_name: String,
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
