use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
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
    #[sea_orm(
        belongs_to,
        from = "test_run_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    #[serde(skip)]
    pub test_runs: BelongsTo<super::test_runs::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
