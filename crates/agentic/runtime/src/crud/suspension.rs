//! CRUD on the `agentic_run_suspensions` table.

use agentic_core::human_input::SuspendedRunData;
use sea_orm::sea_query::OnConflict;
use sea_orm::{ActiveValue::*, DatabaseConnection, DbErr, EntityTrait};
use serde_json::Value;

use crate::entity::run_suspension;

use super::now;

pub async fn upsert_suspension(
    db: &DatabaseConnection,
    run_id: &str,
    prompt: &str,
    suggestions: &[String],
    resume_data: &SuspendedRunData,
) -> Result<(), DbErr> {
    let suggestions_val: Value = serde_json::to_value(suggestions).unwrap();
    let resume_val: Value = serde_json::to_value(resume_data).unwrap();
    let model = run_suspension::ActiveModel {
        run_id: Set(run_id.to_string()),
        prompt: Set(prompt.to_string()),
        suggestions: Set(suggestions_val),
        resume_data: Set(resume_val),
        created_at: Set(now()),
    };
    run_suspension::Entity::insert(model)
        .on_conflict(
            OnConflict::column(run_suspension::Column::RunId)
                .update_columns([
                    run_suspension::Column::Prompt,
                    run_suspension::Column::Suggestions,
                    run_suspension::Column::ResumeData,
                ])
                .to_owned(),
        )
        .exec(db)
        .await?;
    Ok(())
}

pub async fn get_suspension(
    db: &DatabaseConnection,
    run_id: &str,
) -> Result<Option<SuspendedRunData>, DbErr> {
    let row = run_suspension::Entity::find_by_id(run_id.to_string())
        .one(db)
        .await?;
    Ok(row.and_then(|r| serde_json::from_value(r.resume_data).ok()))
}
