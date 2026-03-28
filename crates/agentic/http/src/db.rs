//! Database helpers for the agentic HTTP module (SeaORM).

use agentic_core::human_input::SuspendedRunData;
use sea_orm::sea_query::{Expr, OnConflict};
use sea_orm::{
    ActiveValue::*, ColumnTrait, Condition, DatabaseConnection, DbErr, EntityTrait, QueryFilter,
    QueryOrder, TransactionTrait,
};
use serde_json::Value;
use uuid::Uuid;

use agentic_db::entity::{agentic_run, agentic_run_event, agentic_run_suspension};

fn now() -> chrono::DateTime<chrono::FixedOffset> {
    chrono::Utc::now().fixed_offset()
}

// ── Row type returned by get_events_after ────────────────────────────────────

pub struct EventRow {
    pub seq: i64,
    pub event_type: String,
    pub payload: Value,
}

// ── Run lifecycle ─────────────────────────────────────────────────────────────

pub async fn insert_run(
    db: &DatabaseConnection,
    run_id: &str,
    agent_id: &str,
    question: &str,
    thread_id: Option<Uuid>,
) -> Result<(), DbErr> {
    let ts = now();
    let run = agentic_run::ActiveModel {
        id: Set(run_id.to_string()),
        agent_id: Set(agent_id.to_string()),
        question: Set(question.to_string()),
        status: Set("running".to_string()),
        answer: Set(None),
        error_message: Set(None),
        thread_id: Set(thread_id),
        created_at: Set(ts),
        updated_at: Set(ts),
    };
    agentic_run::Entity::insert(run).exec(db).await?;
    Ok(())
}

pub async fn update_run_done(
    db: &DatabaseConnection,
    run_id: &str,
    answer: &str,
) -> Result<(), DbErr> {
    let run = agentic_run::ActiveModel {
        id: Set(run_id.to_string()),
        status: Set("done".to_string()),
        answer: Set(Some(answer.to_string())),
        updated_at: Set(now()),
        ..Default::default()
    };
    agentic_run::Entity::update(run).exec(db).await?;
    Ok(())
}

pub async fn update_run_failed(
    db: &DatabaseConnection,
    run_id: &str,
    error: &str,
) -> Result<(), DbErr> {
    let run = agentic_run::ActiveModel {
        id: Set(run_id.to_string()),
        status: Set("failed".to_string()),
        error_message: Set(Some(error.to_string())),
        updated_at: Set(now()),
        ..Default::default()
    };
    agentic_run::Entity::update(run).exec(db).await?;
    Ok(())
}

pub async fn update_run_suspended(db: &DatabaseConnection, run_id: &str) -> Result<(), DbErr> {
    let run = agentic_run::ActiveModel {
        id: Set(run_id.to_string()),
        status: Set("suspended".to_string()),
        updated_at: Set(now()),
        ..Default::default()
    };
    agentic_run::Entity::update(run).exec(db).await?;
    Ok(())
}

pub async fn update_run_running(db: &DatabaseConnection, run_id: &str) -> Result<(), DbErr> {
    let run = agentic_run::ActiveModel {
        id: Set(run_id.to_string()),
        status: Set("running".to_string()),
        updated_at: Set(now()),
        ..Default::default()
    };
    agentic_run::Entity::update(run).exec(db).await?;
    Ok(())
}

// ── Events ────────────────────────────────────────────────────────────────────

/// Single-event insert (used for the solver-build-failure fast path only).
pub async fn insert_event(
    db: &DatabaseConnection,
    run_id: &str,
    seq: i64,
    event_type: &str,
    payload: &Value,
) -> Result<(), DbErr> {
    let event = agentic_run_event::ActiveModel {
        id: NotSet,
        run_id: Set(run_id.to_string()),
        seq: Set(seq),
        event_type: Set(event_type.to_string()),
        payload: Set(payload.clone()),
        created_at: Set(now()),
    };
    match agentic_run_event::Entity::insert(event)
        .on_conflict(
            OnConflict::columns([
                agentic_run_event::Column::RunId,
                agentic_run_event::Column::Seq,
            ])
            .do_nothing()
            .to_owned(),
        )
        .exec(db)
        .await
    {
        Ok(_) | Err(DbErr::RecordNotInserted) => {}
        Err(e) => return Err(e),
    }
    Ok(())
}

/// Batch-insert a slice of `(seq, event_type, payload_json_string)` tuples
/// inside a single transaction.
///
/// One transaction vs. N transactions reduces fsync overhead from O(N) to
/// O(1), making token-heavy streams ~50-100× faster to persist.
pub async fn batch_insert_events(
    db: &DatabaseConnection,
    run_id: &str,
    events: &[(i64, String, String)],
) -> Result<(), DbErr> {
    if events.is_empty() {
        return Ok(());
    }
    let ts = now();
    let txn = db.begin().await?;
    for (seq, event_type, payload_str) in events {
        let payload: Value = serde_json::from_str(payload_str).unwrap_or(Value::Null);
        let event = agentic_run_event::ActiveModel {
            id: NotSet,
            run_id: Set(run_id.to_string()),
            seq: Set(*seq),
            event_type: Set(event_type.clone()),
            payload: Set(payload),
            created_at: Set(ts),
        };
        let res = agentic_run_event::Entity::insert(event)
            .on_conflict(
                OnConflict::columns([
                    agentic_run_event::Column::RunId,
                    agentic_run_event::Column::Seq,
                ])
                .do_nothing()
                .to_owned(),
            )
            .exec(&txn)
            .await;
        match res {
            Ok(_) | Err(DbErr::RecordNotInserted) => {}
            Err(e) => {
                txn.rollback().await.ok();
                return Err(e);
            }
        }
    }
    txn.commit().await?;
    Ok(())
}

pub async fn get_events_after(
    db: &DatabaseConnection,
    run_id: &str,
    after_seq: i64,
) -> Result<Vec<EventRow>, DbErr> {
    let models = agentic_run_event::Entity::find()
        .filter(agentic_run_event::Column::RunId.eq(run_id))
        .filter(agentic_run_event::Column::Seq.gt(after_seq))
        .order_by_asc(agentic_run_event::Column::Seq)
        .all(db)
        .await?;
    Ok(models
        .into_iter()
        .map(|m| EventRow {
            seq: m.seq,
            event_type: m.event_type,
            payload: m.payload,
        })
        .collect())
}

pub async fn get_all_events(db: &DatabaseConnection, run_id: &str) -> Result<Vec<EventRow>, DbErr> {
    let models = agentic_run_event::Entity::find()
        .filter(agentic_run_event::Column::RunId.eq(run_id))
        .order_by_asc(agentic_run_event::Column::Seq)
        .all(db)
        .await?;
    Ok(models
        .into_iter()
        .map(|m| EventRow {
            seq: m.seq,
            event_type: m.event_type,
            payload: m.payload,
        })
        .collect())
}

/// Delete events with `seq >= from_seq` for a run.
///
/// Used by the retry flow to remove the terminal error event (and any
/// preceding failed-state events) before appending new retry events.
pub async fn delete_events_from_seq(
    db: &DatabaseConnection,
    run_id: &str,
    from_seq: i64,
) -> Result<u64, DbErr> {
    let result = agentic_run_event::Entity::delete_many()
        .filter(agentic_run_event::Column::RunId.eq(run_id))
        .filter(agentic_run_event::Column::Seq.gte(from_seq))
        .exec(db)
        .await?;
    Ok(result.rows_affected)
}

/// Return the maximum `seq` value for a run, or -1 if no events exist.
pub async fn get_max_seq(db: &DatabaseConnection, run_id: &str) -> Result<i64, DbErr> {
    let last = agentic_run_event::Entity::find()
        .filter(agentic_run_event::Column::RunId.eq(run_id))
        .order_by_desc(agentic_run_event::Column::Seq)
        .one(db)
        .await?;
    Ok(last.map(|m| m.seq).unwrap_or(-1))
}

/// Load the suspension/checkpoint record for a run, if any.
pub async fn get_suspension(
    db: &DatabaseConnection,
    run_id: &str,
) -> Result<Option<SuspendedRunData>, DbErr> {
    let row = agentic_run_suspension::Entity::find_by_id(run_id.to_string())
        .one(db)
        .await?;
    Ok(row.and_then(|r| serde_json::from_value(r.resume_data).ok()))
}

/// Load the run record by ID.
pub async fn get_run(
    db: &DatabaseConnection,
    run_id: &str,
) -> Result<Option<agentic_run::Model>, DbErr> {
    agentic_run::Entity::find_by_id(run_id.to_string())
        .one(db)
        .await
}

// ── Lookup by thread ──────────────────────────────────────────────────────────

/// Find the most recent run for a given thread_id, if any.
pub async fn get_run_by_thread(
    db: &DatabaseConnection,
    thread_id: Uuid,
) -> Result<Option<agentic_run::Model>, DbErr> {
    agentic_run::Entity::find()
        .filter(agentic_run::Column::ThreadId.eq(thread_id))
        .order_by_desc(agentic_run::Column::CreatedAt)
        .one(db)
        .await
}

/// Find all runs for a given thread_id, ordered oldest-first.
pub async fn get_runs_by_thread(
    db: &DatabaseConnection,
    thread_id: Uuid,
) -> Result<Vec<agentic_run::Model>, DbErr> {
    agentic_run::Entity::find()
        .filter(agentic_run::Column::ThreadId.eq(thread_id))
        .order_by_asc(agentic_run::Column::CreatedAt)
        .all(db)
        .await
}

// ── Suspension ────────────────────────────────────────────────────────────────

pub async fn upsert_suspension(
    db: &DatabaseConnection,
    run_id: &str,
    prompt: &str,
    suggestions: &[String],
    resume_data: &SuspendedRunData,
) -> Result<(), DbErr> {
    let suggestions_val: Value = serde_json::to_value(suggestions).unwrap();
    let resume_val: Value = serde_json::to_value(resume_data).unwrap();
    let suspension = agentic_run_suspension::ActiveModel {
        run_id: Set(run_id.to_string()),
        prompt: Set(prompt.to_string()),
        suggestions: Set(suggestions_val),
        resume_data: Set(resume_val),
        created_at: Set(now()),
    };
    agentic_run_suspension::Entity::insert(suspension)
        .on_conflict(
            OnConflict::column(agentic_run_suspension::Column::RunId)
                .update_columns([
                    agentic_run_suspension::Column::Prompt,
                    agentic_run_suspension::Column::Suggestions,
                    agentic_run_suspension::Column::ResumeData,
                ])
                .to_owned(),
        )
        .exec(db)
        .await?;
    Ok(())
}

/// Return completed (status = "done") runs for a thread, oldest-first.
///
/// Used to populate `AnalyticsIntent::history` so the clarifying stage has
/// access to prior Q/A turns when resolving pronoun or reference ambiguity.
/// Only runs with a non-null answer are included.  `limit` caps the number of
/// turns returned; pass `10` for a typical session window.
pub async fn get_thread_history(
    db: &DatabaseConnection,
    thread_id: Uuid,
    limit: u64,
) -> Result<Vec<(String, String)>, DbErr> {
    use sea_orm::QuerySelect;
    let models = agentic_run::Entity::find()
        .filter(agentic_run::Column::ThreadId.eq(thread_id))
        .filter(agentic_run::Column::Status.is_in(["done", "failed"]))
        .order_by_asc(agentic_run::Column::CreatedAt)
        .limit(limit)
        .all(db)
        .await?;
    Ok(models
        .into_iter()
        .filter_map(|m| match (m.answer, m.error_message) {
            (Some(ans), _) => Some((m.question, ans)),
            (None, Some(err)) => Some((m.question, format!("Error: {}", err))),
            (None, None) => None,
        })
        .collect())
}

// ── Startup cleanup ───────────────────────────────────────────────────────────

/// Mark any runs left in a non-terminal state as failed.
///
/// Called once at server startup to recover from process crashes: runs that
/// were `running` or `suspended` when the server died will never complete, so
/// we transition them to `failed` so clients get a definitive status instead
/// of waiting forever.
///
/// Returns the number of rows updated.
pub async fn cleanup_stale_runs(db: &DatabaseConnection) -> Result<u64, DbErr> {
    let result = agentic_run::Entity::update_many()
        .col_expr(agentic_run::Column::Status, Expr::value("failed"))
        .col_expr(
            agentic_run::Column::ErrorMessage,
            Expr::value("server restarted: run was interrupted"),
        )
        .col_expr(agentic_run::Column::UpdatedAt, Expr::value(now()))
        .filter(
            Condition::any()
                .add(agentic_run::Column::Status.eq("running"))
                .add(agentic_run::Column::Status.eq("suspended")),
        )
        .exec(db)
        .await?;
    Ok(result.rows_affected)
}
