use sea_orm::entity::prelude::*;

/// A customer-owned hostname a custom app's production environment is also
/// served at (`oxy_custom_apps::custom_domains`). Only a row with
/// `verified_at` set routes; the TXT record at `_oxygen-challenge.<domain>`
/// must equal `verification_token` to set it. Control plane only.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "app_custom_domains")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub app_id: Uuid,
    /// Lowercase ASCII hostname, no port and no trailing dot.
    pub domain: String,
    pub verification_token: String,
    pub verified_at: Option<DateTimeWithTimeZone>,
    pub last_checked_at: Option<DateTimeWithTimeZone>,
    pub last_check_error: Option<String>,
    pub created_by: Option<Uuid>,
    pub created_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
