use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "test_run_cases")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub test_run_id: Uuid,
    pub case_index: i32,
    #[sea_orm(column_type = "Text")]
    pub prompt: String,
    #[sea_orm(column_type = "Text")]
    pub expected: String,
    #[sea_orm(column_type = "Text", nullable)]
    pub actual_output: Option<String>,
    pub score: f64,
    pub verdict: String,
    pub passing_runs: i32,
    pub total_runs: i32,
    pub avg_duration_ms: Option<f64>,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub judge_reasoning: Option<Json>,
    pub errors: Option<Json>,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::test_runs::Entity",
        from = "Column::TestRunId",
        to = "super::test_runs::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    TestRuns,
}

impl Related<super::test_runs::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::TestRuns.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
