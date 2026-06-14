//! Durable task queue backing the coordinator/worker pipeline.

use sea_orm::{
    ActiveValue::*, ColumnTrait, ConnectionTrait, DatabaseConnection, DbErr, EntityTrait,
    QueryFilter, QueryOrder,
};

use crate::lifecycle::crud::now;
use crate::orchestrator::entity::task_queue;

/// Ownership regime for a queued task — decides whether the global/recovery
/// claim path may pick it up.
///
/// Stamped onto `agentic_task_queue.scope_owned` at INSERT and preserved
/// across `claimed -> queued` reaping. The global `claim_task` filters
/// `scope_owned = false`, so a [`TaskScope::Scoped`] task can never be
/// poached out from under the co-located coordinator that owns its tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskScope {
    /// A co-located scoped coordinator owns this task's tree (every
    /// interactive HTTP/CLI run today). The global claim path must skip it.
    Scoped,
    /// No in-process driver owns this task — scheduler-seeded or
    /// crash-orphaned. The global/recovery loop is responsible for it.
    Global,
}

impl TaskScope {
    /// `true` for [`TaskScope::Scoped`] — the value written to
    /// `agentic_task_queue.scope_owned`.
    pub fn is_owned(self) -> bool {
        matches!(self, TaskScope::Scoped)
    }
}

/// Insert a new task into the durable queue with status `queued`.
pub async fn enqueue_task(
    db: &impl ConnectionTrait,
    task_id: &str,
    run_id: &str,
    parent_task_id: Option<&str>,
    spec: &agentic_core::delegation::TaskSpec,
    policy: Option<&agentic_core::delegation::TaskPolicy>,
    scope: TaskScope,
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
        scope_owned: Set(scope.is_owned()),
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

/// Atomically claim the oldest queued task **that no co-located scoped
/// coordinator owns** (`scope_owned = false`). Returns `None` if no such
/// task is available. Uses `FOR UPDATE SKIP LOCKED` to avoid contention
/// between concurrent workers.
///
/// This is the unscoped/global claim path (the standalone worker and the
/// recovery loop). The `scope_owned = false` predicate is what prevents it
/// from poaching an interactive run's task out from under the per-request
/// coordinator that owns its tree — those rows are stamped `scope_owned =
/// true` at enqueue (see [`TaskScope`]) and are claimed only via
/// [`claim_task_under_root`], which deliberately ignores `scope_owned`.
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
              AND scope_owned = false \
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

/// Like [`claim_task`] but only considers tasks under the given root —
/// either the root itself (`task_id = $root`) or its descendants
/// (`task_id LIKE '$root.%'`).
///
/// Used by per-run scoped workers so they don't accidentally claim tasks
/// from a sibling run's coordinator. See `DurableTransport::task_id_root`
/// for the full context — without this, workers spawned per HTTP request
/// race for queued tasks across runs and the wrong coordinator silently
/// drops the outcome.
pub async fn claim_task_under_root(
    db: &DatabaseConnection,
    worker_id: &str,
    root_task_id: &str,
) -> Result<Option<task_queue::Model>, DbErr> {
    use sea_orm::{DatabaseBackend, FromQueryResult, Statement};

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
              AND (task_id = $2 OR task_id LIKE $2 || '.%') \
            ORDER BY created_at \
            LIMIT 1 \
            FOR UPDATE SKIP LOCKED \
        ) \
        RETURNING *";

    let result = task_queue::Model::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        sql,
        [worker_id.into(), root_task_id.into()],
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

/// Delete terminal task-queue rows older than their retention window, so the
/// queue doesn't grow unbounded with the history of every job ever run.
///
/// Two windows on purpose: `completed_ttl` covers the bulk happy-path rows
/// (`completed`, `cancelled`), `dead_ttl` covers `failed`/`dead` rows which
/// stay longer because they're the dead-letter triage surface. A `None` window
/// disables pruning for that class (keep forever). Only ever touches TERMINAL
/// rows — never `queued`/`claimed` — so an in-flight task can't be deleted out
/// from under a worker. FK-safe: `agentic_task_outcomes` references
/// `agentic_runs`, not this table.
pub async fn purge_old_terminal_tasks(
    db: &DatabaseConnection,
    completed_ttl: Option<std::time::Duration>,
    dead_ttl: Option<std::time::Duration>,
) -> Result<u64, DbErr> {
    use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};

    let mut total = 0u64;

    if let Some(ttl) = completed_ttl {
        let secs = ttl.as_secs() as i64;
        let res = db
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "DELETE FROM agentic_task_queue \
                 WHERE queue_status IN ('completed', 'cancelled') \
                   AND updated_at < now() - ($1 * interval '1 second')",
                [secs.into()],
            ))
            .await?;
        total += res.rows_affected();
    }

    if let Some(ttl) = dead_ttl {
        let secs = ttl.as_secs() as i64;
        let res = db
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "DELETE FROM agentic_task_queue \
                 WHERE queue_status IN ('failed', 'dead') \
                   AND updated_at < now() - ($1 * interval '1 second')",
                [secs.into()],
            ))
            .await?;
        total += res.rows_affected();
    }

    Ok(total)
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

/// **Workspace-scoped** by joining `agentic_task_queue.run_id` →
/// `agentic_runs.workspace_id`. The queue table itself doesn't carry a
/// `workspace_id` column today; the JOIN keeps the dashboard
/// tenant-correct without a schema change.
pub async fn get_queue_stats(
    db: &DatabaseConnection,
    workspace_id: uuid::Uuid,
) -> Result<QueueStats, DbErr> {
    use sea_orm::{DatabaseBackend, FromQueryResult, Statement};

    // Count by status in a single query.
    #[derive(Debug, FromQueryResult)]
    struct StatusCount {
        queue_status: String,
        cnt: i64,
    }

    let rows = StatusCount::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "SELECT q.queue_status, COUNT(*) as cnt \
         FROM agentic_task_queue q \
         INNER JOIN agentic_runs r ON r.id = q.run_id \
         WHERE r.workspace_id = $1 \
         GROUP BY q.queue_status",
        [workspace_id.into()],
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
    stats.stale_tasks = task_queue::Model::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "SELECT q.* FROM agentic_task_queue q \
         INNER JOIN agentic_runs r ON r.id = q.run_id \
         WHERE r.workspace_id = $1 \
           AND q.queue_status = 'claimed' \
           AND q.last_heartbeat < now() - (q.visibility_timeout_secs || ' seconds')::interval \
         ORDER BY q.last_heartbeat \
         LIMIT 50",
        [workspace_id.into()],
    ))
    .all(db)
    .await?
    .into_iter()
    .map(QueueTaskRow::from)
    .collect();

    // Fetch dead-lettered tasks (most recent first).
    stats.dead_tasks = task_queue::Model::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "SELECT q.* FROM agentic_task_queue q \
         INNER JOIN agentic_runs r ON r.id = q.run_id \
         WHERE r.workspace_id = $1 AND q.queue_status = 'dead' \
         ORDER BY q.updated_at DESC",
        [workspace_id.into()],
    ))
    .all(db)
    .await?
    .into_iter()
    .map(QueueTaskRow::from)
    .collect();

    Ok(stats)
}
