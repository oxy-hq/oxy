//! `SeaORM` Entity for the OIDC single-use jti ledger.
//!
//! Each accepted trusted-publishing token's `jti` is inserted here; a PK conflict
//! on insert means the token is being replayed. DB-backed (not in-memory) because
//! Oxy is multi-replica. See `customer_apps_publish_oidc`.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "oidc_used_jti")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub jti: String,
    pub expires_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
