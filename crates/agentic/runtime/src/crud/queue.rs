//! Durable task queue backing the coordinator/worker pipeline.

use sea_orm::{
    ActiveValue::*, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter, QueryOrder,
};

use crate::entity::task_queue;

use super::now;

/// Insert a new task into the durable queue with status `queued`.
pub async fn enqueue_task(
    db: &DatabaseConnection,
    task_id: &str,
    run_id: &str,
    parent_task_id: Option<&str>,
    spec: &agentic_core::delegation::TaskSpec,
    policy: Option<&agentic_core::delegation::TaskPolicy>,
) -> Result<(), DbErr> {
    let now = now();
    let model = task_queue::ActiveModel {
        task_id: Set(task_id.to_string()),
        run_id: Set(run_id.to_string()),
        parent_task_id: Set(parent_task_id.map(String::from)),
        queue_status: Set("queued".to_string()),
        spec: Set(serde_json::to_value(spec).unwrap()),
        policy: Set(policy.map(|p| serde_json::to_value(p).unwrap())),
        worker_id: Set(None),
        last_heartbeat: Set(None),
        claimed_at: Set(None),
        visibility_timeout_secs: Set(60),
        claim_count: Set(0),
        max_claims: Set(3),
        created_at: Set(now),
        updated_at: Set(now),
    };
    task_queue::Entity::insert(model)
        .on_conflict(
            sea_orm::sea_query::OnConflict::column(task_queue::Column::TaskId)
                .update_columns([
                    task_queue::Column::QueueStatus,
                    task_queue::Column::Spec,
                    task_queue::Column::Policy,
                    task_queue::Column::WorkerId,
                    task_queue::Column::LastHeartbeat,
                    task_queue::Column::ClaimedAt,
                    task_queue::Column::ClaimCount,
                    task_queue::Column::UpdatedAt,
                ])
                .to_owned(),
        )
        .exec(db)
        .await?;
    Ok(())
}

/// Atomically claim the oldest queued task. Returns `None` if no tasks
/// are available. Uses `FOR UPDATE SKIP LOCKED` to avoid contention
/// between concurrent workers.
pub async fn claim_task(
    db: &DatabaseConnection,
    worker_id: &str,
) -> Result<Option<task_queue::Model>, DbErr> {
    use sea_orm::{DatabaseBackend, FromQueryResult, Statement};

    // Single atomic UPDATE ... RETURNING using a subquery with FOR UPDATE SKIP LOCKED.
    let sql = "\
        UPDATE agentic_task_queue \
        SET queue_status = 'claimed', \
            worker_id = $1, \
            claimed_at = now(), \
            last_heartbeat = now(), \
            claim_count = claim_count + 1, \
            updated_at = now() \
        WHERE task_id = ( \
            SELECT task_id FROM agentic_task_queue \
            WHERE queue_status = 'queued' \
            ORDER BY created_at \
            LIMIT 1 \
            FOR UPDATE SKIP LOCKED \
        ) \
        RETURNING *";

    let result = task_queue::Model::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        sql,
        [worker_id.into()],
    ))
    .one(db)
    .await?;

    Ok(result)
}

/// Retrieve a queue entry by task_id.
pub async fn get_queue_entry(
    db: &DatabaseConnection,
    task_id: &str,
) -> Result<Option<task_queue::Model>, DbErr> {
    task_queue::Entity::find_by_id(task_id).one(db).await
}

/// Update the heartbeat timestamp for a claimed task.
pub async fn update_queue_heartbeat(db: &DatabaseConnection, task_id: &str) -> Result<(), DbErr> {
    let model = task_queue::ActiveModel {
        task_id: Set(task_id.to_string()),
        last_heartbeat: Set(Some(now())),
        updated_at: Set(now()),
        ..Default::default()
    };
    task_queue::Entity::update(model).exec(db).await?;
    Ok(())
}

/// Mark a claimed task as completed.
pub async fn complete_queue_task(db: &DatabaseConnection, task_id: &str) -> Result<(), DbErr> {
    let model = task_queue::ActiveModel {
        task_id: Set(task_id.to_string()),
        queue_status: Set("completed".to_string()),
        updated_at: Set(now()),
        ..Default::default()
    };
    task_queue::Entity::update(model).exec(db).await?;
    Ok(())
}

/// Mark a claimed task as failed.
pub async fn fail_queue_task(db: &DatabaseConnection, task_id: &str) -> Result<(), DbErr> {
    let model = task_queue::ActiveModel {
        task_id: Set(task_id.to_string()),
        queue_status: Set("failed".to_string()),
        updated_at: Set(now()),
        ..Default::default()
    };
    task_queue::Entity::update(model).exec(db).await?;
    Ok(())
}

/// Re-enqueue a task that was previously claimed or failed. Resets queue_status
/// to `queued`, clears worker_id/heartbeat, and updates the spec. Used during
/// recovery to re-launch tasks from their original spec.
pub async fn requeue_task(
    db: &DatabaseConnection,
    task_id: &str,
    spec: &agentic_core::delegation::TaskSpec,
) -> Result<(), DbErr> {
    use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
    // Use upsert: INSERT if no row exists, UPDATE if it does.
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "INSERT INTO agentic_task_queue \
             (task_id, run_id, queue_status, spec, worker_id, last_heartbeat, \
              claimed_at, visibility_timeout_secs, claim_count, max_claims, created_at, updated_at) \
         VALUES ($1, $1, 'queued', $2, NULL, NULL, NULL, 60, 0, 3, now(), now()) \
         ON CONFLICT (task_id) DO UPDATE SET \
             queue_status = 'queued', \
             spec = $2, \
             worker_id = NULL, \
             last_heartbeat = NULL, \
             claimed_at = NULL, \
             claim_count = 0, \
             updated_at = now()",
        [
            task_id.into(),
            serde_json::to_value(spec).unwrap().into(),
        ],
    ))
    .await?;
    Ok(())
}

/// Cancel a queued (not yet claimed) task.
pub async fn cancel_queued_task(db: &DatabaseConnection, task_id: &str) -> Result<(), DbErr> {
    use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "UPDATE agentic_task_queue SET queue_status = 'cancelled', updated_at = now() \
         WHERE task_id = $1 AND queue_status IN ('queued', 'claimed')",
        [task_id.into()],
    ))
    .await?;
    Ok(())
}

/// Reap stale claimed tasks whose heartbeat has expired past their
/// visibility timeout. Tasks that have exceeded `max_claims` are
/// dead-lettered instead of re-queued.
///
/// Returns the number of tasks affected.
pub async fn reap_stale_tasks(db: &DatabaseConnection) -> Result<u64, DbErr> {
    use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};

    // Dead-letter tasks that have been claimed too many times.
    let dead = db
        .execute(Statement::from_string(
            DatabaseBackend::Postgres,
            "UPDATE agentic_task_queue \
             SET queue_status = 'dead', worker_id = NULL, claimed_at = NULL, updated_at = now() \
             WHERE queue_status = 'claimed' \
               AND claim_count >= max_claims \
               AND last_heartbeat < now() - (visibility_timeout_secs || ' seconds')::interval"
                .to_string(),
        ))
        .await?;

    // Re-queue tasks that are still under max_claims.
    let requeued = db
        .execute(Statement::from_string(
            DatabaseBackend::Postgres,
            "UPDATE agentic_task_queue \
             SET queue_status = 'queued', worker_id = NULL, claimed_at = NULL, updated_at = now() \
             WHERE queue_status = 'claimed' \
               AND claim_count < max_claims \
               AND last_heartbeat < now() - (visibility_timeout_secs || ' seconds')::interval"
                .to_string(),
        ))
        .await?;

    Ok(dead.rows_affected() + requeued.rows_affected())
}

/// A plain DTO for a task queue entry, avoiding leaking entity types.
pub struct QueueTaskRow {
    pub task_id: String,
    pub run_id: String,
    pub queue_status: String,
    pub worker_id: Option<String>,
    pub claim_count: i32,
    pub max_claims: i32,
    pub last_heartbeat: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub updated_at: chrono::DateTime<chrono::FixedOffset>,
}

impl From<task_queue::Model> for QueueTaskRow {
    fn from(m: task_queue::Model) -> Self {
        Self {
            task_id: m.task_id,
            run_id: m.run_id,
            queue_status: m.queue_status,
            worker_id: m.worker_id,
            claim_count: m.claim_count,
            max_claims: m.max_claims,
            last_heartbeat: m.last_heartbeat,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}

/// Queue status counts and stale/dead-lettered task details for the dashboard.
pub struct QueueStats {
    pub queued: u64,
    pub claimed: u64,
    pub completed: u64,
    pub failed: u64,
    pub cancelled: u64,
    pub dead: u64,
    /// Tasks claimed but with heartbeat older than their visibility timeout.
    pub stale_tasks: Vec<QueueTaskRow>,
    /// Tasks that have been dead-lettered.
    pub dead_tasks: Vec<QueueTaskRow>,
}

pub async fn get_queue_stats(db: &DatabaseConnection) -> Result<QueueStats, DbErr> {
    use sea_orm::{DatabaseBackend, FromQueryResult, Statement};

    // Count by status in a single query.
    #[derive(Debug, FromQueryResult)]
    struct StatusCount {
        queue_status: String,
        cnt: i64,
    }

    let rows = StatusCount::find_by_statement(Statement::from_string(
        DatabaseBackend::Postgres,
        "SELECT queue_status, COUNT(*) as cnt FROM agentic_task_queue GROUP BY queue_status"
            .to_string(),
    ))
    .all(db)
    .await?;

    let mut stats = QueueStats {
        queued: 0,
        claimed: 0,
        completed: 0,
        failed: 0,
        cancelled: 0,
        dead: 0,
        stale_tasks: vec![],
        dead_tasks: vec![],
    };

    for row in &rows {
        let count = row.cnt as u64;
        match row.queue_status.as_str() {
            "queued" => stats.queued = count,
            "claimed" => stats.claimed = count,
            "completed" => stats.completed = count,
            "failed" => stats.failed = count,
            "cancelled" => stats.cancelled = count,
            "dead" => stats.dead = count,
            _ => {}
        }
    }

    // Fetch stale tasks (claimed but heartbeat expired).
    stats.stale_tasks = task_queue::Model::find_by_statement(Statement::from_string(
        DatabaseBackend::Postgres,
        "SELECT * FROM agentic_task_queue \
         WHERE queue_status = 'claimed' \
           AND last_heartbeat < now() - (visibility_timeout_secs || ' seconds')::interval \
         ORDER BY last_heartbeat \
         LIMIT 50"
            .to_string(),
    ))
    .all(db)
    .await?
    .into_iter()
    .map(QueueTaskRow::from)
    .collect();

    // Fetch dead-lettered tasks (most recent first).
    stats.dead_tasks = task_queue::Entity::find()
        .filter(task_queue::Column::QueueStatus.eq("dead"))
        .order_by_desc(task_queue::Column::UpdatedAt)
        .all(db)
        .await?
        .into_iter()
        .map(QueueTaskRow::from)
        .collect();

    Ok(stats)
}
