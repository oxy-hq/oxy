use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// The org set a **bounded** platform grant reaches — the `where` half of
/// `(capabilities × scope)`.
///
/// Only consulted when the parent grant's `scope_all` is `false`. Rows here never widen
/// a grant beyond its role's capabilities: scope narrows, capabilities gate. A grant
/// with `scope_all = false` and no rows reaches nothing, which is the fail-closed
/// direction — see `m20260806_000001_platform_grants` for why that flag is a column
/// rather than an inference from this table being empty.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "app_admin_scope_orgs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub app_admin_id: Uuid,
    pub org_id: Uuid,
    pub created_at: DateTimeWithTimeZone,
    pub created_by: Option<Uuid>,
    #[sea_orm(
        belongs_to,
        from = "app_admin_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    #[serde(skip)]
    pub app_admins: BelongsTo<super::app_admins::Entity>,
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
