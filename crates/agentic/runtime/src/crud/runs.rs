//! Lifecycle CRUD on the `agentic_runs` table.

use sea_orm::{ActiveValue::*, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter};
use serde_json::Value;
use uuid::Uuid;

use crate::entity::run;

use super::{now, transition_run};

pub async fn insert_run(
    db: &DatabaseConnection,
    run_id: &str,
    question: &str,
    thread_id: Option<Uuid>,
    source_type: &str,
    metadata: Option<Value>,
) -> Result<(), DbErr> {
    insert_run_inner(
        db,
        run_id,
        question,
        thread_id,
        source_type,
        metadata,
        None,
        0,
    )
    .await
}

/// Insert a child run with a parent reference for the task tree.
pub async fn insert_run_with_parent(
    db: &DatabaseConnection,
    run_id: &str,
    parent_run_id: &str,
    question: &str,
    source_type: &str,
    metadata: Option<Value>,
    attempt: i32,
) -> Result<(), DbErr> {
    insert_run_inner(
        db,
        run_id,
        question,
        None,
        source_type,
        metadata,
        Some(parent_run_id),
        attempt,
    )
    .await
}

async fn insert_run_inner(
    db: &DatabaseConnection,
    run_id: &str,
    question: &str,
    thread_id: Option<Uuid>,
    source_type: &str,
    metadata: Option<Value>,
    parent_run_id: Option<&str>,
    attempt: i32,
) -> Result<(), DbErr> {
    let ts = now();
    let model = run::ActiveModel {
        id: Set(run_id.to_string()),
        question: Set(question.to_string()),
        answer: Set(None),
        error_message: Set(None),
        thread_id: Set(thread_id),
        source_type: Set(Some(source_type.to_string())),
        metadata: Set(metadata),
        parent_run_id: Set(parent_run_id.map(ToString::to_string)),
        task_status: Set(Some("running".to_string())),
        task_metadata: Set(None),
        attempt: Set(attempt),
        recovery_requested_at: Set(None),
        created_at: Set(ts),
        updated_at: Set(ts),
    };
    run::Entity::insert(model).exec(db).await?;
    Ok(())
}

// ── Compatibility shims ─────────────────────────────────────────────────────
// These thin wrappers delegate to `transition_run` so that existing callers
// continue to compile without modification.

pub async fn update_run_done(
    db: &DatabaseConnection,
    run_id: &str,
    answer: &str,
    _metadata_patch: Option<Value>,
) -> Result<(), DbErr> {
    transition_run(db, run_id, "done", None, Some(answer), None).await
}

pub async fn update_run_failed(
    db: &DatabaseConnection,
    run_id: &str,
    error: &str,
) -> Result<(), DbErr> {
    transition_run(db, run_id, "failed", None, None, Some(error)).await
}

pub async fn update_run_suspended(db: &DatabaseConnection, run_id: &str) -> Result<(), DbErr> {
    transition_run(db, run_id, "awaiting_input", None, None, None).await
}

pub async fn update_run_running(db: &DatabaseConnection, run_id: &str) -> Result<(), DbErr> {
    transition_run(db, run_id, "running", None, None, None).await
}

/// Persist a coordinator task status transition.
pub async fn update_task_status(
    db: &DatabaseConnection,
    run_id: &str,
    task_status: &str,
    task_metadata: Option<Value>,
) -> Result<(), DbErr> {
    transition_run(db, run_id, task_status, task_metadata, None, None).await
}

/// Load all runs in a task tree (root + descendants) by following `parent_run_id`.
pub async fn load_task_tree(
    db: &DatabaseConnection,
    root_run_id: &str,
) -> Result<Vec<run::Model>, DbErr> {
    // Load the root.
    let root = run::Entity::find_by_id(root_run_id.to_string())
        .one(db)
        .await?;
    let Some(root) = root else {
        return Ok(vec![]);
    };

    // BFS to collect all descendants.
    let mut result = vec![root];
    let mut parent_ids = vec![root_run_id.to_string()];

    while !parent_ids.is_empty() {
        let children = run::Entity::find()
            .filter(run::Column::ParentRunId.is_in(&parent_ids))
            .all(db)
            .await?;
        parent_ids = children.iter().map(|c| c.id.clone()).collect();
        result.extend(children);
    }

    Ok(result)
}

pub async fn update_run_terminal_from_events(
    db: &DatabaseConnection,
    run_id: &str,
    events: &[(i64, String, String, i32)],
) -> Result<(), DbErr> {
    let Some((_, event_type, payload_str, _)) = events
        .iter()
        .rev()
        .find(|(_, event_type, _, _)| matches!(event_type.as_str(), "done" | "error"))
    else {
        return Ok(());
    };

    let payload: Value = serde_json::from_str(payload_str).unwrap_or(Value::Null);
    let model = match event_type.as_str() {
        "done" => run::ActiveModel {
            id: Set(run_id.to_string()),
            task_status: Set(Some("done".to_string())),
            error_message: Set(None),
            updated_at: Set(now()),
            ..Default::default()
        },
        "error" => run::ActiveModel {
            id: Set(run_id.to_string()),
            task_status: Set(Some("failed".to_string())),
            error_message: Set(Some(
                payload["message"]
                    .as_str()
                    .unwrap_or("unknown error")
                    .to_string(),
            )),
            updated_at: Set(now()),
            ..Default::default()
        },
        _ => return Ok(()),
    };

    run::Entity::update(model).exec(db).await?;
    Ok(())
}
