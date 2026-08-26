//! `schema_migration_definitions` — compiled `schemas/*.sql` files.
//!
//! One row per migration file per revision. Ordering is by `file_path`, which
//! is why the convention is a zero-padded numeric prefix (`0001_orders.sql`):
//! lexical order is the apply order, with no separate sequence to keep in sync.
//!
//! Plain SQL rather than a schema DSL on purpose — these are authored by Oxy
//! engineers in a vibe-coding flow, and every model writes Postgres DDL
//! fluently while none has seen a bespoke DSL.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "schema_migration_definitions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub revision_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub file_path: String,
    /// Content hash. The applier compares this against the ledger so an edited
    /// already-applied migration is caught rather than silently ignored.
    pub content_sha256: String,
    pub content: String,
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
