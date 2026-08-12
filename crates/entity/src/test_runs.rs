use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "test_runs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub source_id: String,
    pub run_index: i32,
    pub project_id: Uuid,
    pub name: Option<String>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    pub project_run_id: Option<Uuid>,
    #[sea_orm(
        belongs_to,
        from = "project_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "NoAction"
    )]
    #[serde(skip)]
    pub workspaces: BelongsTo<super::workspaces::Entity>,
    #[sea_orm(has_many)]
    #[serde(skip)]
    pub test_run_cases: HasMany<super::test_run_cases::Entity>,
    #[sea_orm(
        belongs_to,
        from = "project_run_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "SetNull"
    )]
    #[serde(skip)]
    pub test_project_runs: BelongsTo<Option<super::test_project_runs::Entity>>,
}

impl ActiveModelBehavior for ActiveModel {}
