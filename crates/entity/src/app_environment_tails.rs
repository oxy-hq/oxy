use sea_orm::entity::prelude::*;

/// One open live tail of a custom-app environment
/// (`GET /api/customer-apps/{id}/environments/{name}/tail`): the lease that
/// tells every producing process to `NOTIFY` that environment's function
/// output. One row per stream, renewed while it lives and deleted when it
/// ends; a row past `expires_at` no longer counts. Control plane only — no
/// line of output is stored here (`server::api::custom_apps_tail::watchers`).
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "app_environment_tails")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub app_id: Uuid,
    /// `production`, `staging` or `dev-<handle>`.
    pub environment: String,
    pub expires_at: DateTimeWithTimeZone,
    pub created_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
