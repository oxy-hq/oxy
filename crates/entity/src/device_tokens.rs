//! Where a push would go, if push delivery is configured.
//!
//! Collected independently of whether anything can send yet: a token is only
//! obtainable from a device the user is holding, so gathering them has to work
//! before delivery does, or the first thing a push integration needs is a
//! user base that has already granted permission — which it cannot have.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "device_tokens")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(indexed)]
    pub user_id: Uuid,
    /// `apns` | `fcm` | `web`.
    ///
    /// Web push is a real third platform rather than a variant: different key
    /// format, different endpoint, and it is what an installed PWA uses — which
    /// is the surface the frontline pilot actually ships on.
    pub platform: String,
    pub token: String,
    pub device_name: Option<String>,
    pub created_at: DateTimeWithTimeZone,
    pub last_seen_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
