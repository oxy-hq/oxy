//! Startup cleanup and resume enumeration for the `agentic_runs` table.

use sea_orm::{
    ActiveValue::*, ColumnTrait, Condition, DatabaseConnection, DbErr, EntityTrait, QueryFilter,
};

use crate::entity::run;

use super::{events::get_max_seq, now, transition_run};

/// Find root runs that are still active (not terminal) for restart recovery.
pub async fn get_active_root_runs(db: &DatabaseConnection) -> Result<Vec<run::Model>, DbErr> {
    run::Entity::find()
        .filter(run::Column::ParentRunId.is_null())
        .filter(run::Column::TaskStatus.is_in([
            "running",
            "suspended_human",
            "waiting_on_child",
            "waiting_on_children",
        ]))
        .all(db)
        .await
}

pub async fn cleanup_stale_runs(db: &DatabaseConnection) -> Result<u64, DbErr> {
    // Find all runs with non-terminal task_status.
    let stale_runs = run::Entity::find()
        .filter(
            Condition::any()
                .add(run::Column::TaskStatus.eq("running"))
                .add(run::Column::TaskStatus.eq("awaiting_input"))
                .add(run::Column::TaskStatus.eq("delegating"))
                .add(run::Column::TaskStatus.eq("waiting_on_child"))
                .add(run::Column::TaskStatus.eq("waiting_on_children"))
                .add(run::Column::TaskStatus.eq("needs_resume"))
                .add(run::Column::TaskStatus.eq("shutdown")),
        )
        .all(db)
        .await?;

    let mut reconciled = 0;
    for r in stale_runs {
        // Runs waiting on children whose delegation was interrupted (e.g. server
        // crash) should be failed — the child task is gone and won't complete.
        if matches!(
            r.task_status.as_deref(),
            Some("waiting_on_child") | Some("waiting_on_children")
        ) {
            let update = run::ActiveModel {
                id: Set(r.id.clone()),
                task_status: Set(Some("failed".to_string())),
                error_message: Set(Some(
                    "server restarted: delegation was interrupted".to_string(),
                )),
                updated_at: Set(now()),
                ..Default::default()
            };
            run::Entity::update(update).exec(db).await?;
            reconciled += 1;
            continue;
        }

        // Suspended runs: leave as-is for recovery.
        if matches!(r.task_status.as_deref(), Some("awaiting_input")) {
            continue;
        }

        // Check if this run has any events at all.
        let event_count = get_max_seq(db, &r.id).await.unwrap_or(-1) + 1;
        if event_count == 0 && r.parent_run_id.is_none() {
            // Root run with zero events — never started, just fail it.
            let update = run::ActiveModel {
                id: Set(r.id.clone()),
                task_status: Set(Some("failed".to_string())),
                error_message: Set(Some("server restarted: run never started".to_string())),
                updated_at: Set(now()),
                ..Default::default()
            };
            run::Entity::update(update).exec(db).await?;
            reconciled += 1;
        } else {
            // Has events — mark for resume.
            let update = run::ActiveModel {
                id: Set(r.id.clone()),
                task_status: Set(Some("needs_resume".to_string())),
                error_message: Set(Some(
                    "server restarted: run will be resumed automatically".to_string(),
                )),
                updated_at: Set(now()),
                ..Default::default()
            };
            run::Entity::update(update).exec(db).await?;
            reconciled += 1;
        }
    }

    // Second pass: clean up orphaned child tasks whose parent is terminal.
    let orphans = run::Entity::find()
        .filter(run::Column::ParentRunId.is_not_null())
        .filter(
            Condition::any()
                .add(run::Column::TaskStatus.eq("needs_resume"))
                .add(run::Column::TaskStatus.eq("running"))
                .add(run::Column::TaskStatus.eq("shutdown"))
                .add(run::Column::TaskStatus.eq("waiting_on_children"))
                .add(run::Column::TaskStatus.eq("waiting_on_child"))
                .add(run::Column::TaskStatus.eq("awaiting_input"))
                .add(run::Column::TaskStatus.eq("delegating")),
        )
        .all(db)
        .await?;

    for orphan in orphans {
        // Check if the parent is terminal.
        if let Some(ref parent_id) = orphan.parent_run_id
            && let Some(parent) = run::Entity::find_by_id(parent_id.clone()).one(db).await?
        {
            let parent_terminal =
                matches!(parent.task_status.as_deref(), Some("done") | Some("failed"));
            if parent_terminal {
                let update = run::ActiveModel {
                    id: Set(orphan.id.clone()),
                    task_status: Set(Some("failed".to_string())),
                    error_message: Set(Some(
                        "parent task completed; orphaned child cleaned up".to_string(),
                    )),
                    updated_at: Set(now()),
                    ..Default::default()
                };
                run::Entity::update(update).exec(db).await?;
                reconciled += 1;
            }
        }
    }

    Ok(reconciled)
}

/// Find root runs that are resumable after a server restart.
///
/// Includes tasks marked `"shutdown"` (graceful shutdown — always resumable)
/// and `"needs_resume"` (crash recovery — best effort).
pub async fn get_resumable_root_runs(db: &DatabaseConnection) -> Result<Vec<run::Model>, DbErr> {
    run::Entity::find()
        .filter(run::Column::ParentRunId.is_null())
        .filter(run::Column::TaskStatus.is_in([
            "running",
            "awaiting_input",
            "delegating",
            "needs_resume",
            "shutdown",
        ]))
        .all(db)
        .await
}

// ── Stuck-workflow-run sweeper ───────────────────────────────────────────────

/// A workflow run that has no active queue entry driving it forward.
#[derive(Debug, Clone)]
pub struct StuckRun {
    pub run_id: String,
    pub task_status: Option<String>,
}

/// Find workflow runs that are stranded: `task_status` is non-terminal but no
/// queue entry for the run or any descendant is in `queued`/`claimed`. These
/// runs cannot make progress on their own — nothing will re-drive them.
///
/// `grace_secs` is a lower bound on `updated_at` age to avoid racing with a
/// worker that is mid-commit (e.g. has already advanced state but has not yet
/// enqueued the follow-up).
///
/// Intentionally scoped to `source_type = 'workflow'`. Agent/analytics runs
/// that get into this state are typically unrecoverable (no idempotent
/// re-drive primitive), and a blanket sweep could false-positive on
/// long-running LLM calls. Workflow decisions are pure + `decision_version`
/// gated, so a spurious re-enqueue is always safe.
pub async fn find_stuck_workflow_runs(
    db: &DatabaseConnection,
    grace_secs: u64,
) -> Result<Vec<StuckRun>, DbErr> {
    use sea_orm::{DatabaseBackend, FromQueryResult, Statement};

    #[derive(FromQueryResult)]
    struct Row {
        id: String,
        task_status: Option<String>,
    }

    // Active statuses from `get_active_root_runs` / `cleanup_stale_runs` — a
    // run in any of these is presumed "still supposed to be making progress".
    // We intentionally exclude `awaiting_input` (HITL suspension — driven by
    // a user action, not a queue row).
    let sql = "\
        SELECT r.id, r.task_status \
        FROM agentic_runs r \
        WHERE r.source_type = 'workflow' \
          AND r.task_status IN ('running', 'delegating', 'waiting_on_child', 'waiting_on_children') \
          AND r.updated_at < now() - ($1 || ' seconds')::interval \
          AND NOT EXISTS ( \
              SELECT 1 FROM agentic_task_queue q \
              WHERE (q.task_id = r.id OR q.task_id LIKE r.id || '.%') \
                AND q.queue_status IN ('queued', 'claimed') \
          )";

    let rows = Row::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        sql,
        [(grace_secs as i64).into()],
    ))
    .all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| StuckRun {
            run_id: r.id,
            task_status: r.task_status,
        })
        .collect())
}

/// Mark a run as failed during recovery (when resume itself fails).
pub async fn mark_recovery_failed(
    db: &DatabaseConnection,
    run_id: &str,
    error: &str,
) -> Result<(), DbErr> {
    transition_run(
        db,
        run_id,
        "failed",
        None,
        None,
        Some(&format!("recovery failed: {error}")),
    )
    .await
}

/// Get the max child counter across all runs in a task tree.
///
/// Scans all `agentic_runs` whose ID starts with `root_run_id` and extracts
/// the numeric counter suffix to determine the next safe counter value.
/// This queries the DB directly rather than relying on the in-memory task tree,
/// ensuring we account for children that may have been created by previous
/// recovery attempts even if they're not in the loaded tree.
pub async fn get_max_child_counter(
    db: &DatabaseConnection,
    root_run_id: &str,
) -> Result<u64, DbErr> {
    use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};

    // Query all run IDs that are descendants of this root.
    let stmt = Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "SELECT id FROM agentic_runs WHERE id LIKE $1 AND id != $2",
        [format!("{root_run_id}.%").into(), root_run_id.into()],
    );
    let rows = db.query_all(stmt).await?;

    let mut max_counter: u64 = 0;
    for row in rows {
        let id: String = row.try_get("", "id")?;
        // Check every segment, not just the last, to catch nested IDs.
        for segment in id.split('.') {
            if let Some(gen_str) = segment.strip_prefix('a') {
                if let Some((_, c)) = gen_str.split_once('_')
                    && let Ok(c) = c.parse::<u64>()
                {
                    max_counter = max_counter.max(c);
                }
            } else if let Some(gen_str) = segment.strip_prefix('g') {
                if let Some((_, c)) = gen_str.split_once('_')
                    && let Ok(c) = c.parse::<u64>()
                {
                    max_counter = max_counter.max(c);
                }
            } else if let Ok(n) = segment.parse::<u64>() {
                max_counter = max_counter.max(n);
            }
        }
    }

    Ok(max_counter)
}

/// Increment the attempt counter for a run and return the new value.
pub async fn increment_attempt(db: &DatabaseConnection, run_id: &str) -> Result<i32, DbErr> {
    let root = run::Entity::find_by_id(run_id)
        .one(db)
        .await?
        .ok_or_else(|| DbErr::RecordNotFound(run_id.to_string()))?;
    let new_attempt = root.attempt + 1;

    let model = run::ActiveModel {
        id: Set(run_id.to_string()),
        attempt: Set(new_attempt),
        updated_at: Set(now()),
        ..Default::default()
    };
    run::Entity::update(model).exec(db).await?;

    Ok(new_attempt)
}
