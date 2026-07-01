//! Startup recovery: resume in-flight tasks after server crash/restart.
//!
//! Uses a top-down tree-walk approach:
//! 1. Reconstruct coordinator from DB via `from_db()`
//! 2. Walk the task tree, classify each task
//! 3. Re-launch tasks that have checkpoints
//! 4. Mark stale tasks as failed (parent will re-delegate)
//! 5. Process PendingResumes (children done, parent not yet resumed)

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use agentic_runtime::coordinator::Coordinator;
use agentic_runtime::state::RuntimeState;
use agentic_runtime::transport::DurableTransport;
use agentic_runtime::worker::Worker;
use sea_orm::DatabaseConnection;

use crate::executor::PipelineTaskExecutor;
use crate::platform::{BuilderBridges, PlatformContext};

/// Recover all in-flight runs on server startup.
///
/// `workspace_id` — when `Some`, only resume runs owned by that
/// workspace (cloud-mode startup iterates per workspace, using the
/// per-workspace `PlatformContext` so a recovered run never gets driven
/// with the wrong workspace's connectors/secrets). `None` resumes every
/// workspace's runs — appropriate for local mode (single workspace) and
/// tests.
#[allow(clippy::too_many_arguments)]
pub async fn recover_active_runs(
    db: DatabaseConnection,
    state: Arc<RuntimeState>,
    platform: Arc<dyn PlatformContext>,
    builder_bridges: Option<BuilderBridges>,
    schema_cache: Option<Arc<Mutex<HashMap<String, agentic_analytics::SchemaCatalog>>>>,
    builder_test_runner: Option<Arc<dyn agentic_builder::BuilderTestRunner>>,
    builder_app_runner: Option<Arc<dyn agentic_builder::BuilderAppRunner>>,
    router: Arc<dyn agentic_runtime::router::TaskRouter>,
    workspace_id: Option<uuid::Uuid>,
    custom_executors: Option<Arc<agentic_runtime::worker::CustomTaskRegistry>>,
) -> usize {
    // Pre-pass: clean up stale queue entries from the previous server lifetime.
    // Tasks "claimed" by now-dead workers get re-queued or dead-lettered.
    let transport = DurableTransport::with_router(db.clone(), router.clone(), None);
    let reaped = transport.run_reaper().await;
    if reaped > 0 {
        tracing::info!(target: "recovery", count = reaped, "reaper pre-pass: cleaned stale queue entries");
    }

    let roots = match agentic_runtime::crud::get_resumable_root_runs(&db, workspace_id).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(target: "recovery", error = %e, "failed to query resumable runs");
            return 0;
        }
    };

    if roots.is_empty() {
        return 0;
    }

    tracing::info!(target: "recovery", count = roots.len(), "found resumable runs");

    let mut recovered = 0;
    for root in roots {
        let run_id = root.id.clone();
        match recover_single_run(
            &root,
            db.clone(),
            state.clone(),
            platform.clone(),
            builder_bridges.clone(),
            schema_cache.clone(),
            builder_test_runner.clone(),
            builder_app_runner.clone(),
            router.clone(),
            custom_executors.clone(),
        )
        .await
        {
            Ok(()) => {
                recovered += 1;
                tracing::info!(target: "recovery", run_id = %run_id, "run recovered");
            }
            Err(e) => {
                tracing::error!(target: "recovery", run_id = %run_id, error = %e, "failed to recover run");
                agentic_runtime::crud::mark_recovery_failed(&db, &run_id, &e)
                    .await
                    .ok();
            }
        }
    }

    recovered
}

/// Periodic global-driver entrypoint: drive **stranded** runs only.
///
/// Unlike [`recover_active_runs`] (startup: every coordinator is dead, so
/// it selects via `get_resumable_root_runs`), this is called repeatedly
/// while the process is alive. It must never touch a run a live per-request
/// coordinator is driving, so it selects via `find_stuck_runs` — which
/// requires `NOT EXISTS (active queue entry)`. A live interactive run
/// always has a `claimed`/heart-beating queue entry and is excluded by
/// construction. The reaper pre-pass first frees entries claimed by dead
/// workers, so a crashed interactive run becomes stranded → eligible.
///
/// `workspace_id` — see [`recover_active_runs`]. The cloud-mode periodic
/// loop passes the iteration's workspace id so it never drives a
/// foreign workspace's row with this context.
#[allow(clippy::too_many_arguments)]
pub async fn recover_stranded_runs(
    db: DatabaseConnection,
    state: Arc<RuntimeState>,
    platform: Arc<dyn PlatformContext>,
    builder_bridges: Option<BuilderBridges>,
    schema_cache: Option<Arc<Mutex<HashMap<String, agentic_analytics::SchemaCatalog>>>>,
    builder_test_runner: Option<Arc<dyn agentic_builder::BuilderTestRunner>>,
    builder_app_runner: Option<Arc<dyn agentic_builder::BuilderAppRunner>>,
    router: Arc<dyn agentic_runtime::router::TaskRouter>,
    workspace_id: Option<uuid::Uuid>,
    custom_executors: Option<Arc<agentic_runtime::worker::CustomTaskRegistry>>,
) -> usize {
    /// A run untouched for this long with no queue entry is genuinely
    /// stranded (not a worker mid-commit between state write and enqueue).
    const STRANDED_GRACE_SECS: u64 = 30;

    // Free entries claimed by workers that died — turns a crashed
    // interactive run into a stranded one this same pass.
    let transport = DurableTransport::with_router(db.clone(), router.clone(), None);
    let reaped = transport.run_reaper().await;
    if reaped > 0 {
        tracing::info!(target: "recovery", count = reaped, "global loop: reaped stale queue entries");
    }

    let stuck = match agentic_runtime::crud::find_stuck_runs(&db, STRANDED_GRACE_SECS, workspace_id)
        .await
    {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(target: "recovery", error = %e, "global loop: find_stuck_runs failed");
            return 0;
        }
    };
    if stuck.is_empty() {
        return 0;
    }
    tracing::info!(target: "recovery", count = stuck.len(), "global loop: found stranded runs");

    let mut recovered = 0;
    for s in stuck {
        let root = match agentic_runtime::crud::get_run(&db, &s.run_id).await {
            Ok(Some(r)) => r,
            Ok(None) => continue,
            Err(e) => {
                tracing::warn!(target: "recovery", run_id = %s.run_id, error = %e, "global loop: get_run failed");
                continue;
            }
        };
        match recover_single_run(
            &root,
            db.clone(),
            state.clone(),
            platform.clone(),
            builder_bridges.clone(),
            schema_cache.clone(),
            builder_test_runner.clone(),
            builder_app_runner.clone(),
            router.clone(),
            custom_executors.clone(),
        )
        .await
        {
            Ok(()) => {
                recovered += 1;
                tracing::info!(target: "recovery", run_id = %s.run_id, "global loop: drove stranded run");
            }
            Err(e) => {
                tracing::error!(target: "recovery", run_id = %s.run_id, error = %e, "global loop: failed to drive stranded run");
                agentic_runtime::crud::mark_recovery_failed(&db, &s.run_id, &e)
                    .await
                    .ok();
            }
        }
    }
    recovered
}

/// §12 FU4c latency-worker entrypoint. Drives runs that already have a
/// `queued scope_owned = false` queue entry — freshly-seeded Global runs
/// (scheduler tick / `run-now`) — at claim-time, without the periodic
/// loop's grace window. The driving mechanism is the same as
/// `recover_stranded_runs` (lease-CAS-acquire + spawn coordinator); the
/// only difference is the selection predicate.
///
/// Safe to interleave with the periodic loop: both call
/// `recover_single_run`, which CAS-acquires the driver lease — the loser
/// of any race skips cleanly. No double-drive.
///
/// `workspace_id` — see [`recover_active_runs`]. The local-mode latency
/// worker passes `None` (single workspace); the cloud-mode worker passes
/// the iteration's workspace id so it routes per-row to the right
/// `PlatformContext`.
#[allow(clippy::too_many_arguments)]
pub async fn recover_pending_global_runs(
    db: DatabaseConnection,
    state: Arc<RuntimeState>,
    platform: Arc<dyn PlatformContext>,
    builder_bridges: Option<BuilderBridges>,
    schema_cache: Option<Arc<Mutex<HashMap<String, agentic_analytics::SchemaCatalog>>>>,
    builder_test_runner: Option<Arc<dyn agentic_builder::BuilderTestRunner>>,
    builder_app_runner: Option<Arc<dyn agentic_builder::BuilderAppRunner>>,
    router: Arc<dyn agentic_runtime::router::TaskRouter>,
    workspace_id: Option<uuid::Uuid>,
    custom_executors: Option<Arc<agentic_runtime::worker::CustomTaskRegistry>>,
) -> usize {
    let pending = match agentic_runtime::crud::find_pending_global_runs(&db, workspace_id).await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(target: "recovery", error = %e, "latency loop: find_pending_global_runs failed");
            return 0;
        }
    };
    if pending.is_empty() {
        return 0;
    }
    tracing::debug!(target: "recovery", count = pending.len(), "latency loop: found pending Global runs");

    let mut driven = 0;
    for s in pending {
        let root = match agentic_runtime::crud::get_run(&db, &s.run_id).await {
            Ok(Some(r)) => r,
            Ok(None) => continue,
            Err(e) => {
                tracing::warn!(target: "recovery", run_id = %s.run_id, error = %e, "latency loop: get_run failed");
                continue;
            }
        };
        match recover_single_run(
            &root,
            db.clone(),
            state.clone(),
            platform.clone(),
            builder_bridges.clone(),
            schema_cache.clone(),
            builder_test_runner.clone(),
            builder_app_runner.clone(),
            router.clone(),
            custom_executors.clone(),
        )
        .await
        {
            Ok(()) => {
                driven += 1;
                tracing::info!(target: "recovery", run_id = %s.run_id, "latency loop: drove Global run");
            }
            Err(e) => {
                tracing::error!(target: "recovery", run_id = %s.run_id, error = %e, "latency loop: failed to drive Global run");
                agentic_runtime::crud::mark_recovery_failed(&db, &s.run_id, &e)
                    .await
                    .ok();
            }
        }
    }
    driven
}

#[allow(clippy::too_many_arguments)]
async fn recover_single_run(
    root: &agentic_runtime::entity::run::Model,
    db: DatabaseConnection,
    state: Arc<RuntimeState>,
    platform: Arc<dyn PlatformContext>,
    builder_bridges: Option<BuilderBridges>,
    schema_cache: Option<Arc<Mutex<HashMap<String, agentic_analytics::SchemaCatalog>>>>,
    builder_test_runner: Option<Arc<dyn agentic_builder::BuilderTestRunner>>,
    builder_app_runner: Option<Arc<dyn agentic_builder::BuilderAppRunner>>,
    router: Arc<dyn agentic_runtime::router::TaskRouter>,
    custom_executors: Option<Arc<agentic_runtime::worker::CustomTaskRegistry>>,
) -> Result<(), String> {
    use agentic_core::transport::{CoordinatorTransport, WorkerTransport};

    // Acquire the driver lease before touching the run. If a *live* driver
    // already owns it (another replica, or — once Task 6 lands — a
    // concurrent recovery tick), skip: driving it here would double-drive
    // the run. The lease auto-clears on terminal `transition_run`; if this
    // recovery is itself interrupted the lease goes stale and the run
    // becomes resumable again. Heartbeating the lease while driving is the
    // periodic loop's job (Task 6).
    let driver_id = format!("recovery-{}", uuid::Uuid::new_v4());
    match agentic_runtime::crud::try_acquire_driver(&db, &root.id, &driver_id).await {
        Ok(true) => {}
        Ok(false) => {
            tracing::info!(
                target: "recovery",
                run_id = %root.id,
                "skipping recovery: a live driver already holds the lease"
            );
            return Ok(());
        }
        Err(e) => return Err(format!("driver lease acquire failed: {e}")),
    }

    // Scope to this run's task tree so recovery's worker can't poach
    // a sibling run's queued root. See `drive_with_coordinator` in
    // `lib.rs` for the full explanation of why this matters under
    // LISTEN/NOTIFY-driven wake.
    let transport = DurableTransport::with_router(db.clone(), router, Some(root.id.clone()));
    let executor = Arc::new(PipelineTaskExecutor {
        platform,
        builder_bridges,
        schema_cache,
        builder_test_runner,
        builder_app_runner,
        db: db.clone(),
        state: Some(state.clone()),
        custom_executors,
    });

    // ── Step 0: Transparent recovery — clean up partial events ──────────
    //
    // Recovery is transparent: no attempt increment, no attempt_started event.
    // Instead, delete partial events from the interrupted execution (e.g. a
    // step_started without its step_end) and emit a lightweight recovery_resumed
    // marker. This prevents duplicate events in the frontend reasoning trace.
    //
    // Skip the marker entirely for runs that have NEVER executed — a
    // freshly-seeded Global run (scheduler tick / run-now) hits the
    // latency worker before its first event, and "Resuming from server
    // restart" is misleading for a run that's about to start for the
    // first time.
    let attempt = root.attempt; // Same attempt — no increment
    let prior_event_count = agentic_runtime::crud::get_max_seq(&db, &root.id)
        .await
        .unwrap_or(-1)
        + 1;
    let is_fresh_seed = prior_event_count == 0;
    tracing::info!(target: "recovery", run_id = %root.id, attempt, prior_event_count, "starting transparent recovery");

    // Delete partial events: find the last completed boundary and remove
    // everything after it. This cleans up step_started events that were
    // emitted before the crash but never got their corresponding step_end.
    if !is_fresh_seed {
        let all_events = agentic_runtime::crud::get_all_events(&db, &root.id)
            .await
            .unwrap_or_default();
        if let Some(last_complete) = all_events.iter().rev().find(|e| {
            matches!(
                e.event_type.as_str(),
                "step_end"
                    | "done"
                    | "error"
                    | "cancelled"
                    | "subrun_completed"
                    | "subrun_step_completed"
            )
        }) {
            let delete_from = last_complete.seq + 1;
            if delete_from <= all_events.last().map(|e| e.seq).unwrap_or(0) {
                tracing::info!(
                    target: "recovery",
                    run_id = %root.id,
                    from_seq = delete_from,
                    "deleting partial events from interrupted execution"
                );
                agentic_runtime::crud::delete_events_from_seq(&db, &root.id, delete_from)
                    .await
                    .ok();
            }
        }

        // Emit recovery marker on the root run (same attempt number) —
        // only for runs that actually have prior events to resume from.
        let next_seq = agentic_runtime::crud::get_max_seq(&db, &root.id)
            .await
            .unwrap_or(-1)
            + 1;
        agentic_runtime::crud::insert_event(
            &db,
            &root.id,
            next_seq,
            "recovery_resumed",
            &serde_json::json!({"message": "Resuming from server restart"}),
            attempt,
        )
        .await
        .ok();
    }

    // Also emit recovery_resumed on non-terminal child runs so their SSE
    // streams (e.g. builder delegation panel) close interrupted steps.
    {
        let child_tree = agentic_runtime::crud::load_task_tree(&db, &root.id)
            .await
            .unwrap_or_default();
        for child in &child_tree {
            if child.id == root.id {
                continue;
            }
            if matches!(
                child.task_status.as_deref(),
                Some("done") | Some("failed") | Some("cancelled")
            ) {
                continue;
            }
            let child_seq = agentic_runtime::crud::get_max_seq(&db, &child.id)
                .await
                .unwrap_or(-1)
                + 1;
            agentic_runtime::crud::insert_event(
                &db,
                &child.id,
                child_seq,
                "recovery_resumed",
                &serde_json::json!({"message": "Resuming from server restart"}),
                attempt,
            )
            .await
            .ok();
        }
    }

    // ── Step 1: Reconstruct coordinator from DB ─────────────────────────
    let (coordinator, pending_resumes) = Coordinator::from_db(
        db.clone(),
        state.clone(),
        transport.clone() as Arc<dyn CoordinatorTransport>,
        &root.id,
    )
    .await
    .map_err(|e| format!("failed to reconstruct coordinator: {e}"))?;
    // `from_db` returns a coordinator with the default no-op
    // completion policy + resolver — recovered runs may still
    // complete with `workflow_continue` metadata and may still
    // suspend on automation delegations, so re-attach both the
    // automation policy and resolver before driving.
    let coordinator = coordinator
        .with_completion_policy(Arc::new(agentic_automation::AutomationCompletionPolicy))
        .with_delegation_resolver(Arc::new(agentic_automation::AutomationDelegationResolver));

    // ── Step 2: Walk tree and classify each task ────────────────────────
    let tree = agentic_runtime::crud::load_task_tree(&db, &root.id)
        .await
        .map_err(|e| format!("failed to load task tree: {e}"))?;

    let pending_parent_ids: std::collections::HashSet<String> = pending_resumes
        .iter()
        .map(|pr| pr.parent_task_id.clone())
        .collect();

    for task_run in &tree {
        match task_run.task_status.as_deref() {
            Some("done") | Some("failed") => continue,

            Some("awaiting_input") => {
                tracing::debug!(target: "recovery", task_id = %task_run.id, "leaving HITL-suspended task");
                continue;
            }

            Some("delegating") => {
                if pending_parent_ids.contains(&task_run.id) {
                    re_launch_task(&db, &state, &executor, &transport, task_run).await?;
                } else {
                    tracing::debug!(target: "recovery", task_id = %task_run.id, "parent still waiting");
                }
            }

            _ => {
                // running / needs_resume / shutdown / unknown

                // Check if this task has non-terminal children. If so, it was
                // delegating before the crash and the reaper changed its status.
                // Don't re-launch it — the coordinator's WaitingOnChildren state
                // handles it; children complete → coordinator resumes this parent.
                let has_active_children = tree.iter().any(|t| {
                    t.parent_run_id.as_deref() == Some(task_run.id.as_str())
                        && !matches!(t.task_status.as_deref(), Some("done") | Some("failed"))
                });
                if has_active_children {
                    // Restore the correct DB status — this task was delegating
                    // before the crash but the reaper set it to needs_resume.
                    agentic_runtime::crud::update_task_status(
                        &db,
                        &task_run.id,
                        "delegating",
                        None,
                    )
                    .await
                    .ok();
                    tracing::info!(
                        target: "recovery",
                        task_id = %task_run.id,
                        "skipping re-launch: has active children (restored to delegating)"
                    );
                    continue;
                }

                let suspend_data = agentic_runtime::crud::get_suspension(&db, &task_run.id)
                    .await
                    .ok()
                    .flatten();

                // Root task gets the same checkpoint-presence check as
                // children. The old "always re-launch root" rule was wrong
                // for freshly-seeded Global runs (scheduler tick / run-now):
                // they have no suspension, no workflow_state, no
                // task_metadata.original_spec — `resume_from_state` errors
                // out with "no saved state" and the run is force-failed.
                // For those, the queue entry for the root is still
                // `queued`; the Worker spawned at step 4 below will claim
                // it and execute the spec fresh. Same logic for
                // reaper-requeued roots — the worker re-executes
                // idempotently.
                let is_root_with_queued_entry = task_run.id == root.id
                    && matches!(
                        agentic_runtime::crud::get_queue_entry(&db, &task_run.id).await,
                        Ok(Some(q)) if q.queue_status == "queued"
                    );

                if suspend_data.is_some() {
                    re_launch_task(&db, &state, &executor, &transport, task_run).await?;
                } else if is_root_with_queued_entry {
                    tracing::debug!(
                        target: "recovery",
                        run_id = %task_run.id,
                        "root has no checkpoint and queue entry is queued; worker will claim and execute fresh"
                    );
                } else if task_run.id == root.id {
                    // Root with no suspension AND no queued entry — the
                    // queue row was either claimed-and-orphaned or never
                    // enqueued. Fall back to the legacy re_launch path so
                    // the existing crash-recovery tests still pass; that
                    // path will surface a clear error if there's truly
                    // nothing to resume.
                    re_launch_task(&db, &state, &executor, &transport, task_run).await?;
                } else if let Some(spec) = extract_original_spec(task_run) {
                    // Child task was running with no checkpoint but has an original
                    // TaskSpec (stored on creation). Re-enqueue it — the worker will
                    // re-execute from scratch (idempotent, like Temporal activity retry).
                    tracing::info!(
                        target: "recovery",
                        task_id = %task_run.id,
                        source_type = ?task_run.source_type,
                        "re-enqueueing checkpointless child task from original spec"
                    );
                    reenqueue_child(&db, &transport, task_run, spec).await?;
                } else {
                    tracing::debug!(target: "recovery", task_id = %task_run.id, "no checkpoint and no original spec, marking failed");
                    fail_stale_child(&db, task_run).await;
                }
            }
        }
    }

    // ── Step 3: Process pending resumes ─────────────────────────────────
    //
    // For Temporal-style automation runs, the coordinator's resume_parent will
    // enqueue an AutomationDecision task when it processes these resumes — no
    // in-memory channel needed. For analytics/builder runs, resume_parent
    // assigns a TaskSpec::Resume which the worker handles.
    //
    // The pending_resumes are processed by the coordinator when it starts up
    // (via its from_db logic), so no explicit action is needed here.

    // ── Step 4: Register in RuntimeState + spawn coordinator + worker ───
    // Without registration the SSE endpoint finds no notifier and exits
    // immediately, so recovered runs appear "dead" to connected clients.
    let cancel_rx = {
        let (answer_tx, _answer_rx) = tokio::sync::mpsc::channel::<String>(1);
        let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
        state.register(&root.id, answer_tx, cancel_tx);
        cancel_rx
    };

    // Cross-process cancel forwarder (§12 FU4a). A recovered / Global run
    // is driven here, out-of-process from the API that handles the cancel
    // request — so the in-memory `state.cancel` watch may have no listener
    // in this process. Observe BOTH the in-proc watch (same-process fast
    // path) and the durable `cancel_requested_at` DB flag (cross-process),
    // and tear down the subtree on either. Self-terminates once the run is
    // terminal so it doesn't poll forever.
    {
        let cf_transport = transport.clone();
        let cf_db = db.clone();
        let cf_run = root.id.clone();
        let mut cf_rx = cancel_rx;
        tokio::spawn(async move {
            let poll = std::time::Duration::from_secs(5);
            loop {
                let cancelled = tokio::select! {
                    changed = cf_rx.changed() => changed.is_ok() && *cf_rx.borrow(),
                    _ = tokio::time::sleep(poll) => {
                        agentic_runtime::crud::is_cancel_requested(&cf_db, &cf_run)
                            .await
                            .unwrap_or(false)
                    }
                };
                if cancelled {
                    tracing::info!(
                        target: "recovery",
                        run_id = %cf_run,
                        "cancel observed (cross-process); cancelling subtree"
                    );
                    let _ = cf_transport.cancel_subtree(&cf_run).await;
                    break;
                }
                if let Ok(Some(r)) = agentic_runtime::crud::get_run(&cf_db, &cf_run).await
                    && matches!(
                        r.task_status.as_deref(),
                        Some("done") | Some("failed") | Some("cancelled") | Some("timed_out")
                    )
                {
                    break;
                }
            }
        });
    }
    // Register notifiers for non-terminal child runs so their SSE streams
    // work after recovery. Without this, the frontend opens an SSE connection
    // for a recovered child (e.g. builder delegation) and gets no notifier →
    // the stream exits immediately.
    for task_run in &tree {
        if task_run.id == root.id {
            continue; // Already registered above.
        }
        if matches!(
            task_run.task_status.as_deref(),
            Some("done") | Some("failed") | Some("cancelled")
        ) {
            continue;
        }
        state.register_notifier(&task_run.id);
    }

    // Heartbeat the driver lease while this run is being driven. Closes the
    // Task 5 seam: without this, a run that takes longer than
    // DRIVER_LEASE_TTL_SECS would have its lease go stale mid-flight and a
    // concurrent recovery tick (Task 6) could double-drive it. The task
    // self-terminates the moment the lease is no longer ours — either it
    // was stolen (heartbeat returns false) or the run reached a terminal
    // state (terminal `transition_run` nulls `driver_id`, so the next
    // heartbeat returns false). On process exit the runtime is torn down;
    // the lease then goes stale within the TTL and is reclaimable, which is
    // exactly the desired crash/restart behavior.
    {
        // TTL / 3 — three missed heartbeats before another driver may steal.
        let interval = std::time::Duration::from_secs(
            (agentic_runtime::crud::DRIVER_LEASE_TTL_SECS / 3).max(1) as u64,
        );
        let hb_db = db.clone();
        let hb_run_id = root.id.clone();
        let hb_driver_id = driver_id.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                match agentic_runtime::crud::heartbeat_driver(&hb_db, &hb_run_id, &hb_driver_id)
                    .await
                {
                    Ok(true) => {}
                    Ok(false) => break, // lost the lease or run terminal
                    Err(e) => {
                        tracing::warn!(
                            target: "recovery",
                            run_id = %hb_run_id,
                            error = %e,
                            "driver heartbeat failed; retrying next tick"
                        );
                    }
                }
            }
        });
    }

    let worker = Worker::new(transport.clone() as Arc<dyn WorkerTransport>, executor);
    tokio::spawn(async move { worker.run().await });

    let pending_count = pending_resumes.len();
    tokio::spawn(async move {
        let mut coord = coordinator;
        coord.process_pending_resumes(pending_resumes).await;
        coord.run().await;
    });

    tracing::info!(
        target: "recovery",
        run_id = %root.id,
        tree_size = tree.len(),
        pending = pending_count,
        "recovery complete"
    );

    Ok(())
}

/// Re-launch a single task from its saved state.
async fn re_launch_task(
    db: &DatabaseConnection,
    _state: &Arc<RuntimeState>,
    executor: &Arc<PipelineTaskExecutor>,
    transport: &Arc<DurableTransport>,
    task_run: &agentic_runtime::entity::run::Model,
) -> Result<(), String> {
    use agentic_core::transport::WorkerTransport;
    use agentic_runtime::worker::TaskExecutor;

    let suspend_data = agentic_runtime::crud::get_suspension(db, &task_run.id)
        .await
        .ok()
        .flatten();

    let executing = executor
        .resume_from_state(task_run, suspend_data)
        .await
        .map_err(|e| format!("failed to resume task {}: {e}", task_run.id))?;

    agentic_runtime::crud::update_run_running(db, &task_run.id)
        .await
        .ok();
    agentic_runtime::crud::update_task_status(db, &task_run.id, "running", None)
        .await
        .ok();

    spawn_virtual_worker(
        transport.clone() as Arc<dyn WorkerTransport>,
        &task_run.id,
        executing,
    );

    tracing::info!(
        target: "recovery",
        task_id = %task_run.id,
        source_type = ?task_run.source_type,
        "re-launched task"
    );

    Ok(())
}

/// Mark a stale child as failed and write an outcome for its parent.
async fn fail_stale_child(db: &DatabaseConnection, task_run: &agentic_runtime::entity::run::Model) {
    agentic_runtime::crud::mark_recovery_failed(
        db,
        &task_run.id,
        "stale child; parent will re-delegate",
    )
    .await
    .ok();

    if let Some(ref parent_id) = task_run.parent_run_id {
        agentic_runtime::crud::insert_task_outcome(
            db,
            &task_run.id,
            parent_id,
            "failed",
            Some("stale child; parent will re-delegate"),
        )
        .await
        .ok();
    }
}

/// Extract the original TaskSpec from a child run's task_metadata.
///
/// The coordinator stores `original_spec` in task_metadata when spawning children
/// (for retry/fallback). We reuse it here to re-enqueue checkpointless tasks.
fn extract_original_spec(
    task_run: &agentic_runtime::entity::run::Model,
) -> Option<agentic_core::delegation::TaskSpec> {
    let meta = task_run.task_metadata.as_ref()?;
    let spec_val = meta.get("original_spec")?;
    serde_json::from_value(spec_val.clone()).ok()
}

/// Re-enqueue a child task through the durable queue using its original TaskSpec.
///
/// The task gets a fresh execution — the worker will pick it up and run it from
/// scratch. This is the Temporal-style "activity retry" pattern: the task is
/// idempotent, so re-running it produces the correct result.
async fn reenqueue_child(
    db: &sea_orm::DatabaseConnection,
    transport: &std::sync::Arc<DurableTransport>,
    task_run: &agentic_runtime::entity::run::Model,
    spec: agentic_core::delegation::TaskSpec,
) -> Result<(), String> {
    // Reset task_status to running so the coordinator tracks it correctly.
    agentic_runtime::crud::transition_run(db, &task_run.id, "running", None, None, None)
        .await
        .ok();

    // Use requeue_task (upsert) instead of enqueue_task (insert) — the queue
    // row already exists from the original execution and would cause a PK
    // violation on insert.
    agentic_runtime::crud::requeue_task(db, &task_run.id, &spec)
        .await
        .map_err(|e| format!("failed to requeue child {}: {e}", task_run.id))?;

    // Wake the worker so it picks up the re-queued task immediately.
    transport.notify_new_task();

    Ok(())
}

/// Forward an ExecutingTask's events/outcomes to the coordinator via transport.
fn spawn_virtual_worker(
    transport: Arc<dyn agentic_core::transport::WorkerTransport>,
    task_id: &str,
    executing: agentic_runtime::worker::ExecutingTask,
) {
    use agentic_core::delegation::TaskOutcome;
    use agentic_core::transport::WorkerMessage;

    let task_id = task_id.to_string();
    let transport_clone = transport.clone();
    let task_id_clone = task_id.clone();

    // Spawn heartbeat loop for the recovered task.
    let heartbeat_cancel = transport.spawn_heartbeat(&task_id, std::time::Duration::from_secs(15));

    tokio::spawn(async move {
        let mut events = executing.events;
        while let Some((event_type, payload)) = events.recv().await {
            if transport_clone
                .send(WorkerMessage::Event {
                    task_id: task_id_clone.clone(),
                    event_type,
                    payload,
                })
                .await
                .is_err()
            {
                break;
            }
        }
    });

    let task_id_for_outcomes = task_id;
    tokio::spawn(async move {
        let mut outcomes = executing.outcomes;
        while let Some(outcome) = outcomes.recv().await {
            let is_terminal = matches!(
                outcome,
                TaskOutcome::Done { .. } | TaskOutcome::Failed(_) | TaskOutcome::Cancelled
            );
            let _ = transport
                .send(WorkerMessage::Outcome {
                    task_id: task_id_for_outcomes.clone(),
                    outcome,
                })
                .await;
            if is_terminal {
                heartbeat_cancel.cancel();
                break;
            }
        }
    });
}
