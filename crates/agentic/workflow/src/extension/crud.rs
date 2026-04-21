//! CRUD operations for the workflow state extension table.

use sea_orm::{
    ActiveValue::*, ColumnTrait, DatabaseConnection, DatabaseTransaction, DbErr, EntityTrait,
    QueryFilter, TransactionTrait,
};
use serde_json::Value;

use super::entity;

/// Insert the initial workflow state row.
pub async fn insert_state(
    db: &DatabaseConnection,
    run_id: &str,
    yaml_hash: &str,
    workflow_config: Value,
    workflow_context: Value,
    variables: Option<Value>,
    trace_id: &str,
) -> Result<(), DbErr> {
    let now = chrono::Utc::now();
    let model = entity::ActiveModel {
        run_id: Set(run_id.to_string()),
        workflow_yaml_hash: Set(yaml_hash.to_string()),
        workflow_config: Set(workflow_config),
        workflow_context: Set(workflow_context),
        variables: Set(variables),
        trace_id: Set(trace_id.to_string()),
        current_step: Set(0),
        results: Set(serde_json::json!({})),
        render_context: Set(serde_json::json!({})),
        pending_children: Set(serde_json::json!({})),
        decision_version: Set(0),
        updated_at: Set(now),
    };
    entity::Entity::insert(model)
        .on_conflict(
            sea_orm::sea_query::OnConflict::column(entity::Column::RunId)
                .update_columns([
                    entity::Column::WorkflowYamlHash,
                    entity::Column::WorkflowConfig,
                    entity::Column::WorkflowContext,
                    entity::Column::Variables,
                    entity::Column::TraceId,
                    entity::Column::CurrentStep,
                    entity::Column::Results,
                    entity::Column::RenderContext,
                    entity::Column::PendingChildren,
                    entity::Column::DecisionVersion,
                    entity::Column::UpdatedAt,
                ])
                .to_owned(),
        )
        .exec(db)
        .await?;
    Ok(())
}

/// Load the full workflow state for a run.
pub async fn load_state(
    db: &DatabaseConnection,
    run_id: &str,
) -> Result<Option<entity::Model>, DbErr> {
    entity::Entity::find_by_id(run_id.to_string()).one(db).await
}

/// Update mutable workflow state fields with optimistic concurrency.
///
/// Only succeeds if the current `decision_version` in the DB matches
/// `expected_version`. Returns `Ok(true)` on success, `Ok(false)` on version
/// mismatch (another worker already updated), `Err` on DB failure.
pub async fn update_state(
    db: &DatabaseConnection,
    run_id: &str,
    expected_version: i64,
    current_step: i32,
    results: Value,
    render_context: Value,
    pending_children: Value,
) -> Result<bool, DbErr> {
    let txn = db.begin().await?;
    let updated = update_state_in_txn(
        &txn,
        run_id,
        expected_version,
        current_step,
        results,
        render_context,
        pending_children,
    )
    .await?;
    txn.commit().await?;
    Ok(updated)
}

pub async fn update_state_in_txn(
    txn: &DatabaseTransaction,
    run_id: &str,
    expected_version: i64,
    current_step: i32,
    results: Value,
    render_context: Value,
    pending_children: Value,
) -> Result<bool, DbErr> {
    use sea_orm::{ConnectionTrait, Statement};

    let new_version = expected_version + 1;
    let now = chrono::Utc::now();

    let stmt = Statement::from_sql_and_values(
        sea_orm::DatabaseBackend::Postgres,
        r#"
        UPDATE agentic_workflow_state
        SET current_step = $1,
            results = $2,
            render_context = $3,
            pending_children = $4,
            decision_version = $5,
            updated_at = $6
        WHERE run_id = $7
          AND decision_version = $8
        "#,
        [
            current_step.into(),
            results.into(),
            render_context.into(),
            pending_children.into(),
            new_version.into(),
            now.into(),
            run_id.into(),
            expected_version.into(),
        ],
    );

    let result = txn.execute(stmt).await?;
    Ok(result.rows_affected() == 1)
}

/// Delete the state row (called when a workflow run is cleaned up).
#[allow(dead_code)]
pub async fn delete_state(db: &DatabaseConnection, run_id: &str) -> Result<(), DbErr> {
    entity::Entity::delete_many()
        .filter(entity::Column::RunId.eq(run_id))
        .exec(db)
        .await?;
    Ok(())
}
