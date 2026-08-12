//! `SeaORM` Entity for secrets management

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "secrets")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(unique)]
    pub name: String,
    #[sea_orm(column_type = "Text")]
    pub encrypted_value: String,
    #[sea_orm(column_type = "Text", nullable)]
    pub description: Option<String>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    pub created_by: Uuid,
    pub updated_by: Option<Uuid>,
    pub project_id: Uuid,
    pub is_active: bool,
    #[sea_orm(
        belongs_to,
        from = "project_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    pub workspaces: BelongsTo<super::workspaces::Entity>,
    #[sea_orm(
        belongs_to,
        from = "created_by",
        to = "id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    pub users: BelongsTo<super::users::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
