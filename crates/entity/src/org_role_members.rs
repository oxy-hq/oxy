//! Who holds a role, and where.
//!
//! `location_id` is NULL for a franchisor-scope role: a Corporate user holds it
//! across the org rather than at one store, and a sentinel location would make
//! every location query have to know about it.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "org_role_members")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(indexed)]
    pub org_id: Uuid,
    pub role_id: Uuid,
    #[sea_orm(indexed)]
    pub user_id: Uuid,
    /// NULL for a franchisor-scope role — see the module docs.
    #[sea_orm(indexed)]
    pub location_id: Option<Uuid>,
    pub created_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
