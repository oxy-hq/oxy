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

/// Update mutable workflow state using an incremental result delta.
///
/// Instead of overwriting the full `results` JSONB column, this merges only
/// the single new step result via PostgreSQL's `||` operator.  For a workflow
/// with S steps and an average result size of R bytes the old full-replace
/// approach wrote 1+2+...+S × R ≈ O(S²·R) bytes total; this function writes
/// O(S·R) — one new entry per step.
///
/// `result_delta` must be either:
/// - A single-key JSON object `{"step_name": result}` when a new result was
///   produced this decision, or
/// - An empty object `{}` when no result changed (delegation/wait decisions).
///
/// `render_context` is always written as `'{}'` — it is derived from `results`
/// at load time, so there is no need to persist it separately.
pub async fn apply_result_delta_in_txn(
    txn: &DatabaseTransaction,
    run_id: &str,
    expected_version: i64,
    current_step: i32,
    result_delta: Value,
    pending_children: Value,
) -> Result<bool, DbErr> {
    use sea_orm::{ConnectionTrait, Statement};

    // The SQL below uses `results || $2::jsonb`. Postgres' `||` operator
    // requires both sides to be objects; passing an array or scalar would
    // surface as an opaque runtime error. Reject early with a clear message.
    if !result_delta.is_object() {
        return Err(DbErr::Custom(format!(
            "result_delta must be a JSON object, got: {result_delta:?}"
        )));
    }

    let new_version = expected_version + 1;
    let now = chrono::Utc::now();

    let stmt = Statement::from_sql_and_values(
        sea_orm::DatabaseBackend::Postgres,
        r#"
        UPDATE agentic_workflow_state
        SET current_step     = $1,
            results          = results || $2::jsonb,
            render_context   = '{}'::jsonb,
            pending_children = $3,
            decision_version = $4,
            updated_at       = $5
        WHERE run_id         = $6
          AND decision_version = $7
        "#,
        [
            current_step.into(),
            result_delta.into(),
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
