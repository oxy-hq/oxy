//! `SeaORM` Entity for partner-publish consent.
//!
//! The CLIENT org's opt-in switch for third-party (partner) app publishing.
//! Default OFF — **no row means denied**. A partner with `manage_apps` and an
//! assigned client still cannot publish into that client until the client's own
//! Owner/Admin turns it on. Only a REAL org officer may set it (the
//! synthetic-operator override is rejected), so neither Oxy staff nor the partner
//! can flip it on the client's behalf.
//!
//! See `internal-docs/partner-platform.md`.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
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
    #[sea_orm(
        belongs_to,
        from = "org_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    #[serde(skip)]
    pub organizations: BelongsTo<super::organizations::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
