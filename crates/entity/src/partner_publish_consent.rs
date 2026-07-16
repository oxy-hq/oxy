//! `SeaORM` Entity for partner-publish consent.
//!
//! The CLIENT org's opt-in switch for third-party (partner) app publishing.
//! Default OFF — **no row means denied**. A partner with `manage_apps` and an
//! assigned client still cannot publish into that client until the client's own
//! Owner/Admin turns it on. Only a REAL org officer may set it (the
//! synthetic-operator override is rejected), so neither Oxy staff nor the partner
//! can flip it on the client's behalf.
//!
//! See `internal-docs/2026-07-16-partner-platform-design.md`.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "partner_publish_consent")]
pub struct Model {
    /// The CLIENT org that is (or is not) consenting.
    #[sea_orm(primary_key, auto_increment = false)]
    pub org_id: Uuid,
    /// Explicit ON. A row with `enabled = false` is an explicit revoke kept for the
    /// history; absence of a row is the default OFF.
    pub enabled: bool,
    /// The client officer who last set it.
    pub granted_by: Option<Uuid>,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::organizations::Entity",
        from = "Column::OrgId",
        to = "super::organizations::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Organizations,
}

impl Related<super::organizations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Organizations.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
