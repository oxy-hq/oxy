use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

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
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    // Logical relation only — no FK in DB (mirrors the runs table pattern)
    #[sea_orm(
        belongs_to = "super::projects::Entity",
        from = "Column::ProjectId",
        to = "super::projects::Column::Id",
        on_update = "NoAction",
        on_delete = "NoAction"
    )]
    Projects,
    #[sea_orm(has_many = "super::test_run_cases::Entity")]
    TestRunCases,
    #[sea_orm(
        belongs_to = "super::test_project_runs::Entity",
        from = "Column::ProjectRunId",
        to = "super::test_project_runs::Column::Id",
        on_update = "NoAction",
        on_delete = "SetNull"
    )]
    TestProjectRuns,
}

impl Related<super::projects::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Projects.def()
    }
}

impl Related<super::test_run_cases::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::TestRunCases.def()
    }
}

impl Related<super::test_project_runs::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::TestProjectRuns.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
