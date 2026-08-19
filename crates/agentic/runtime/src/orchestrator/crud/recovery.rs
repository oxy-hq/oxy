//! Startup cleanup and resume enumeration for the `agentic_runs` table.

use sea_orm::{
    ActiveValue::*, ColumnTrait, Condition, DatabaseConnection, DbErr, EntityTrait, QueryFilter,
};
use uuid::Uuid;

use crate::lifecycle::crud::events::get_max_seq;
use crate::lifecycle::crud::{DRIVER_LEASE_TTL_SECS, now, transition_run};
use crate::lifecycle::entity::run;

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

        let event_count = get_max_seq(db, &r.id).await.unwrap_or(-1) + 1;
        if event_count == 0 && r.parent_run_id.is_none() {
            // Root with zero events. Two sub-cases:
            //
            // (a) A scheduler-seeded or run-now-seeded Global run that
            //     hasn't been driven yet: its queue entry is still
            //     `queued` with `scope_owned = false`, waiting for the
            //     latency worker / periodic loop to pick it up. Force-
            //     failing this is a regression — the run is valid pending
            //     work, not an orphan.
            //
            // (b) Any other zero-event root with no queued entry: stale
            //     placeholder from a request that died before enqueuing.
            //     Safe to fail.
            //
            // The discriminator is whether a `queued` queue row exists
            // for this run id.
            let has_queued = crate::orchestrator::crud::queue::get_queue_entry(db, &r.id)
                .await
                .ok()
                .flatten()
                .map(|q| q.queue_status == "queued")
                .unwrap_or(false);
            if has_queued {
                // Leave as-is; the recovery loop / latency worker will
                // drive it on the next tick.
                continue;
            }
            // (b): never started AND no queued entry — fail it.
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
///
/// Excludes runs a *live* driver already owns: a run is only resumable if it
/// is unleased or its driver lease has gone stale past
/// [`DRIVER_LEASE_TTL_SECS`]. This is the F1 guard — without it, calling this
/// on an interval (the Phase 2 global loop) would re-select and double-drive
/// runs that are still in flight.
///
/// `workspace_id` — when `Some`, only return runs owned by that workspace.
/// Cloud-mode startup recovery iterates per workspace and passes the
/// current workspace id; local mode passes `None` (the single workspace
/// is identified by the nil UUID and every other row would also be nil).
pub async fn get_resumable_root_runs(
    db: &DatabaseConnection,
    workspace_id: Option<Uuid>,
) -> Result<Vec<run::Model>, DbErr> {
    let lease_cutoff = now() - chrono::Duration::seconds(DRIVER_LEASE_TTL_SECS);
    let mut query = run::Entity::find()
        .filter(run::Column::ParentRunId.is_null())
        .filter(run::Column::TaskStatus.is_in([
            "running",
            "awaiting_input",
            "delegating",
            "needs_resume",
            "shutdown",
        ]))
        .filter(
            Condition::any()
                .add(run::Column::DriverId.is_null())
                .add(run::Column::DriverHeartbeatAt.is_null())
                .add(run::Column::DriverHeartbeatAt.lt(lease_cutoff)),
        );
    if let Some(ws) = workspace_id {
        query = query.filter(run::Column::WorkspaceId.eq(ws));
    }
    query.all(db).await
}

// ── Stuck-automation-run sweeper ─────────────────────────────────────────────

/// An automation run that has no active queue entry driving it forward.
#[derive(Debug, Clone)]
pub struct StuckRun {
    pub run_id: String,
    pub task_status: Option<String>,
    /// Owning workspace — used by recovery loops to look up the right
    /// cached `PlatformContext` before driving the run. Nil UUID for
    /// local serve mode (== `LOCAL_WORKSPACE_ID`).
    pub workspace_id: Uuid,
}

/// Find automation runs that are stranded: `task_status` is non-terminal but no
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
/// long-running LLM calls. Automation decisions are pure + `decision_version`
/// gated, so a spurious re-enqueue is always safe.
pub async fn find_stuck_automation_runs(
    db: &DatabaseConnection,
    grace_secs: u64,
) -> Result<Vec<StuckRun>, DbErr> {
    use sea_orm::{DatabaseBackend, FromQueryResult, Statement};

    #[derive(FromQueryResult)]
    struct Row {
        id: String,
        task_status: Option<String>,
        workspace_id: Uuid,
    }

    // Active statuses from `get_active_root_runs` / `cleanup_stale_runs` — a
    // run in any of these is presumed "still supposed to be making progress".
    // We intentionally exclude `awaiting_input` (HITL suspension — driven by
    // a user action, not a queue row).
    let sql = "\
        SELECT r.id, r.task_status, r.workspace_id \
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
            workspace_id: r.workspace_id,
        })
        .collect())
}

/// Find runs that are stranded **and** safe for the periodic global
/// driver loop to pick up — generalized over `find_stuck_automation_runs`
/// for `workflow` + `airway` (the Phase 1/2 schedulable targets).
///
/// "Stranded" means: no queue entry is `claimed` (a live worker — the
/// dead case is the reaper's job) **and** no entry is `queued` *and*
/// `scope_owned = true`. The `scope_owned` split is load-bearing:
///
/// - `claimed` (any scope) → a live coordinator owns it → exclude (this is
///   the rung-2 anti-poaching invariant; a live per-request coordinator
///   always has a `claimed`, heart-beating entry, and a live coordinator's
///   transient not-yet-claimed children are `queued scope_owned = true`).
/// - `queued` + `scope_owned = true` → an interactive run's not-yet-claimed
///   task; its live coordinator is about to claim it (the grace window
///   covers the enqueue→start gap) → exclude.
/// - `queued` + `scope_owned = false` → a Global orphan / scheduler-seeded
///   task with **no consumer** in Phase 1 (no standalone unscoped claim
///   worker). This does NOT shield the run — it is exactly the rung-4 /
///   Phase-2 case the periodic driver must pick up and drive.
///
/// Also excludes runs whose driver lease is still fresh, so two ticks /
/// replicas don't both grab the same stranded run.
///
/// `get_resumable_root_runs` is still correct for *startup* recovery: a
/// process restart kills every in-flight coordinator, so everything
/// resumable is genuinely orphaned.
///
/// `workspace_id` — when `Some`, only return runs owned by that workspace.
/// `None` returns every workspace's stranded runs. The recovery loop in
/// cloud mode passes the per-iteration workspace_id so it doesn't try to
/// drive workspace-B's run with workspace-A's `PlatformContext`; the
/// startup pass + tests pass `None`.
pub async fn find_stuck_runs(
    db: &DatabaseConnection,
    grace_secs: u64,
    workspace_id: Option<Uuid>,
) -> Result<Vec<StuckRun>, DbErr> {
    use sea_orm::{DatabaseBackend, FromQueryResult, Statement, Value};

    #[derive(FromQueryResult)]
    struct Row {
        id: String,
        task_status: Option<String>,
        workspace_id: Uuid,
    }

    // The workspace filter is conditional, but every binding must be the
    // same number of placeholders across paths — branch on whether
    // `workspace_id` is set and append the extra clause + value.
    let mut values: Vec<Value> = vec![
        (grace_secs as i32).into(),
        (DRIVER_LEASE_TTL_SECS as i32).into(),
    ];
    let workspace_clause = if let Some(ws) = workspace_id {
        values.push(ws.into());
        " AND r.workspace_id = $3 "
    } else {
        ""
    };
    let sql = format!(
        "\
        SELECT r.id, r.task_status, r.workspace_id \
        FROM agentic_runs r \
        WHERE r.source_type IN ('workflow', 'airway') \
          AND r.parent_run_id IS NULL \
          AND r.task_status IN ('running', 'delegating', 'waiting_on_child', 'waiting_on_children', 'needs_resume', 'shutdown') \
          AND r.updated_at < now() - make_interval(secs => $1) \
          AND (r.driver_id IS NULL \
               OR r.driver_heartbeat_at IS NULL \
               OR r.driver_heartbeat_at < now() - make_interval(secs => $2)) \
          {workspace_clause} \
          AND NOT EXISTS ( \
              SELECT 1 FROM agentic_task_queue q \
              WHERE (q.task_id = r.id OR q.task_id LIKE r.id || '.%') \
                AND ( \
                  q.queue_status = 'claimed' \
                  OR (q.queue_status = 'queued' AND q.scope_owned = true) \
                ) \
          )"
    );

    let rows = Row::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        sql,
        values,
    ))
    .all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| StuckRun {
            run_id: r.id,
            task_status: r.task_status,
            workspace_id: r.workspace_id,
        })
        .collect())
}

/// Find runs that have a `queued` + `scope_owned = false` queue entry
/// AND are not currently lease-held — the §12 FU4c latency-worker
/// selection. No grace window: a queued row only exists after the seed
/// function fully commits, so there's no mid-commit race to wait out
/// (unlike `find_stuck_runs` which guards against an in-flight worker
/// that's about to enqueue).
///
/// This is intentionally narrower than `find_stuck_runs`: it picks up
/// freshly-seeded Global runs at claim-time (cron / `run-now`) so the
/// periodic loop's grace window doesn't gate them.
///
/// **Source-type policy:** this query is type-agnostic on purpose. The
/// "freshly seeded, never claimed" precondition (`queue_status='queued'`
/// + `scope_owned=false`) means no worker has yet executed the spec, so
/// the LLM-double-spend concern that justifies `find_stuck_runs`'s
/// `('workflow', 'airway')` filter does not apply here. Any new top-level
/// source type (analytics agents, future kinds) must be picked up by
/// this latency worker — otherwise scheduled / run-now runs sit
/// `queued` forever. Tests in `latency_worker_picks_up_all_source_types`
/// enforce this contract.
///
/// `workspace_id` — when `Some`, only return pending rows owned by that
/// workspace. When `None`, returns every workspace's pending rows; the
/// caller (e.g. the cloud-mode latency worker) is responsible for
/// grouping by `StuckRun.workspace_id` and routing each row to the
/// correct cached `PlatformContext`.
pub async fn find_pending_global_runs(
    db: &DatabaseConnection,
    workspace_id: Option<Uuid>,
) -> Result<Vec<StuckRun>, DbErr> {
    use sea_orm::{DatabaseBackend, FromQueryResult, Statement, Value};

    #[derive(FromQueryResult)]
    struct Row {
        id: String,
        task_status: Option<String>,
        workspace_id: Uuid,
    }

    let mut values: Vec<Value> = vec![(DRIVER_LEASE_TTL_SECS as i32).into()];
    let workspace_clause = if let Some(ws) = workspace_id {
        values.push(ws.into());
        " AND r.workspace_id = $2 "
    } else {
        ""
    };
    let sql = format!(
        "\
        SELECT r.id, r.task_status, r.workspace_id \
        FROM agentic_runs r \
        WHERE r.parent_run_id IS NULL \
          AND r.task_status IN ('running', 'delegating', 'waiting_on_child', 'waiting_on_children', 'needs_resume', 'shutdown') \
          AND (r.driver_id IS NULL \
               OR r.driver_heartbeat_at IS NULL \
               OR r.driver_heartbeat_at < now() - make_interval(secs => $1)) \
          {workspace_clause} \
          AND EXISTS ( \
              SELECT 1 FROM agentic_task_queue q \
              WHERE (q.task_id = r.id OR q.task_id LIKE r.id || '.%') \
                AND q.queue_status = 'queued' \
                AND q.scope_owned = false \
          )"
    );

    let rows = Row::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        sql,
        values,
    ))
    .all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| StuckRun {
            run_id: r.id,
            task_status: r.task_status,
            workspace_id: r.workspace_id,
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
    let rows = db.query_all_raw(stmt).await?;

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
