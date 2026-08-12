//! `compiled_references` — denormalised cross-entity reference graph.
//! Powers fast "who calls X" / "what does Y depend on" without
//! scanning every entity's JSON body. The Context Graph page maps
//! naturally onto this table; today it re-derives the same data from
//! FS walks.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
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
