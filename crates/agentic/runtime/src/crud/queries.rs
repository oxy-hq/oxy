//! Read-side queries over runs, events, and thread history.

use sea_orm::{
    ColumnTrait, Condition, DatabaseConnection, DbErr, EntityTrait, QueryFilter, QueryOrder,
};
use serde_json::Value;
use uuid::Uuid;

use crate::entity::{run, run_event};

use super::user_facing_status;

pub struct ToolExchangeRow {
    pub name: String,
    pub input: String,
    pub output: String,
}

pub struct ThreadHistoryTurn {
    pub question: String,
    pub answer: String,
    /// Full run metadata — callers extract domain-specific fields.
    pub metadata: Option<Value>,
}

pub async fn get_run(db: &DatabaseConnection, run_id: &str) -> Result<Option<run::Model>, DbErr> {
    run::Entity::find_by_id(run_id.to_string()).one(db).await
}

pub async fn get_run_by_thread(
    db: &DatabaseConnection,
    thread_id: Uuid,
) -> Result<Option<run::Model>, DbErr> {
    run::Entity::find()
        .filter(run::Column::ThreadId.eq(thread_id))
        .order_by_desc(run::Column::CreatedAt)
        .one(db)
        .await
}

pub async fn get_runs_by_thread(
    db: &DatabaseConnection,
    thread_id: Uuid,
) -> Result<Vec<run::Model>, DbErr> {
    run::Entity::find()
        .filter(run::Column::ThreadId.eq(thread_id))
        .order_by_asc(run::Column::CreatedAt)
        .all(db)
        .await
}

/// List recent runs across all threads, ordered newest-first.
pub async fn list_recent_runs(
    db: &DatabaseConnection,
    limit: u64,
) -> Result<Vec<run::Model>, DbErr> {
    use sea_orm::QuerySelect;
    run::Entity::find()
        .order_by_desc(run::Column::CreatedAt)
        .limit(limit)
        .all(db)
        .await
}

/// List root runs with optional filters, paginated. Returns (runs, total_count).
pub async fn list_runs_filtered(
    db: &DatabaseConnection,
    status_filter: Option<&[&str]>,
    source_type_filter: Option<&str>,
    offset: u64,
    limit: u64,
) -> Result<(Vec<run::Model>, u64), DbErr> {
    use sea_orm::{PaginatorTrait, QuerySelect};

    let mut query = run::Entity::find().filter(run::Column::ParentRunId.is_null());

    if let Some(statuses) = status_filter {
        // Map user-facing statuses to internal task_status values.
        let mut cond = Condition::any();
        for s in statuses {
            match *s {
                "running" => {
                    cond = cond
                        .add(run::Column::TaskStatus.eq("running"))
                        .add(run::Column::TaskStatus.eq("delegating"))
                        .add(run::Column::TaskStatus.eq("waiting_on_child"))
                        .add(run::Column::TaskStatus.eq("waiting_on_children"));
                }
                "suspended" => {
                    cond = cond.add(run::Column::TaskStatus.eq("awaiting_input"));
                }
                "done" => {
                    cond = cond.add(run::Column::TaskStatus.eq("done"));
                }
                "failed" => {
                    cond = cond
                        .add(run::Column::TaskStatus.eq("failed"))
                        .add(run::Column::TaskStatus.eq("timed_out"));
                }
                "cancelled" => {
                    cond = cond.add(run::Column::TaskStatus.eq("cancelled"));
                }
                _ => {}
            }
        }
        query = query.filter(cond);
    }

    if let Some(src) = source_type_filter {
        query = query.filter(run::Column::SourceType.eq(src));
    }

    let total = query.clone().count(db).await?;
    let runs = query
        .order_by_desc(run::Column::CreatedAt)
        .offset(offset)
        .limit(limit)
        .all(db)
        .await?;

    Ok((runs, total))
}

/// List root runs that are currently active (not in a terminal state).
/// Used by the coordinator dashboard to show in-flight pipelines.
pub async fn list_active_runs(db: &DatabaseConnection) -> Result<Vec<run::Model>, DbErr> {
    run::Entity::find()
        .filter(run::Column::ParentRunId.is_null())
        .filter(
            Condition::any()
                .add(run::Column::TaskStatus.eq("running"))
                .add(run::Column::TaskStatus.eq("delegating"))
                .add(run::Column::TaskStatus.eq("awaiting_input"))
                .add(run::Column::TaskStatus.eq("waiting_on_child"))
                .add(run::Column::TaskStatus.eq("waiting_on_children"))
                .add(run::Column::TaskStatus.eq("needs_resume"))
                .add(run::Column::TaskStatus.eq("shutdown")),
        )
        .order_by_desc(run::Column::UpdatedAt)
        .all(db)
        .await
}

async fn get_last_run_event(
    db: &DatabaseConnection,
    run_id: &str,
) -> Result<Option<run_event::Model>, DbErr> {
    run_event::Entity::find()
        .filter(run_event::Column::RunId.eq(run_id))
        .order_by_desc(run_event::Column::Seq)
        .one(db)
        .await
}

fn terminal_error_message(
    event: Option<&run_event::Model>,
    fallback: Option<&str>,
) -> Option<String> {
    event
        .and_then(|row| row.payload["message"].as_str())
        .map(ToOwned::to_owned)
        .or_else(|| fallback.map(ToOwned::to_owned))
}

pub async fn get_effective_run_state(
    db: &DatabaseConnection,
    run: &run::Model,
) -> Result<(String, Option<String>), DbErr> {
    if run.answer.is_some() {
        return Ok(("done".to_string(), None));
    }

    let last_event = get_last_run_event(db, &run.id).await?;
    let effective = match last_event.as_ref().map(|event| event.event_type.as_str()) {
        Some("done") => ("done".to_string(), None),
        Some("error") => (
            "failed".to_string(),
            terminal_error_message(last_event.as_ref(), run.error_message.as_deref()),
        ),
        _ => (
            user_facing_status(run.task_status.as_deref()).to_string(),
            run.error_message.clone(),
        ),
    };

    Ok(effective)
}

pub async fn get_thread_history(
    db: &DatabaseConnection,
    thread_id: Uuid,
    limit: u64,
) -> Result<Vec<ThreadHistoryTurn>, DbErr> {
    use sea_orm::QuerySelect;
    let models = run::Entity::find()
        .filter(run::Column::ThreadId.eq(thread_id))
        .filter(run::Column::TaskStatus.is_in(["done", "failed", "cancelled", "timed_out"]))
        .order_by_asc(run::Column::CreatedAt)
        .limit(limit)
        .all(db)
        .await?;
    Ok(models
        .into_iter()
        .filter_map(|m| {
            let answer = turn_answer(m.task_status.as_deref(), &m.answer, &m.error_message)?;
            Some(ThreadHistoryTurn {
                question: m.question,
                answer,
                metadata: m.metadata,
            })
        })
        .collect())
}

/// Render the conversation-history answer text for a terminal run.
///
/// Returns `None` only when the status is non-terminal or has no useful
/// content to surface. `cancelled` and `timed_out` runs without an explicit
/// answer/error_message still yield a synthetic marker so follow-up turns
/// know the prior run did not complete.
fn turn_answer(
    task_status: Option<&str>,
    answer: &Option<String>,
    error_message: &Option<String>,
) -> Option<String> {
    if let Some(ans) = answer {
        return Some(ans.clone());
    }
    match task_status {
        Some("failed") | Some("timed_out") => Some(format!(
            "Error: {}",
            error_message.as_deref().unwrap_or("run failed")
        )),
        Some("cancelled") => Some(
            error_message
                .as_deref()
                .map(|m| format!("Cancelled: {m}"))
                .unwrap_or_else(|| "Cancelled by user".to_string()),
        ),
        Some("done") => error_message.as_deref().map(|e| format!("Error: {e}")),
        _ => None,
    }
}

pub async fn get_thread_history_with_events(
    db: &DatabaseConnection,
    thread_id: Uuid,
    limit: u64,
) -> Result<Vec<(String, String, Vec<ToolExchangeRow>)>, DbErr> {
    use sea_orm::QuerySelect;
    let runs = run::Entity::find()
        .filter(run::Column::ThreadId.eq(thread_id))
        .filter(run::Column::TaskStatus.is_in(["done", "failed", "cancelled", "timed_out"]))
        .order_by_asc(run::Column::CreatedAt)
        .limit(limit)
        .all(db)
        .await?;

    let mut result = Vec::new();
    for r in runs {
        let (status, error_message) = get_effective_run_state(db, &r).await?;
        let answer = match (status.as_str(), r.answer, error_message) {
            ("done", Some(answer), _) => answer,
            ("done", None, Some(error)) => format!("Error: {}", error),
            ("failed", _, Some(error)) => format!("Error: {}", error),
            ("failed", _, None) => "Error: run failed".to_string(),
            ("cancelled", _, Some(msg)) => format!("Cancelled: {msg}"),
            ("cancelled", _, None) => "Cancelled by user".to_string(),
            _ => continue,
        };

        let events = run_event::Entity::find()
            .filter(run_event::Column::RunId.eq(&r.id))
            .order_by_asc(run_event::Column::Seq)
            .all(db)
            .await?;

        let mut exchanges: Vec<ToolExchangeRow> = Vec::new();
        let mut pending_call: Option<(String, String)> = None;
        for event in events {
            match event.event_type.as_str() {
                "tool_call" => {
                    let name = event.payload["name"].as_str().unwrap_or("").to_string();
                    let input = event.payload["input"].as_str().unwrap_or("{}").to_string();
                    pending_call = Some((name, input));
                }
                "tool_result" => {
                    if let Some((name, input)) = pending_call.take() {
                        let output = event.payload["output"].as_str().unwrap_or("").to_string();
                        exchanges.push(ToolExchangeRow {
                            name,
                            input,
                            output,
                        });
                    }
                }
                _ => {}
            }
        }

        result.push((r.question, answer, exchanges));
    }
    Ok(result)
}
