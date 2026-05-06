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
pub async fn recover_active_runs(
    db: DatabaseConnection,
    state: Arc<RuntimeState>,
    platform: Arc<dyn PlatformContext>,
    builder_bridges: Option<BuilderBridges>,
    schema_cache: Option<Arc<Mutex<HashMap<String, agentic_analytics::SchemaCatalog>>>>,
    builder_test_runner: Option<Arc<dyn agentic_builder::BuilderTestRunner>>,
    builder_app_runner: Option<Arc<dyn agentic_builder::BuilderAppRunner>>,
) -> usize {
    // Pre-pass: clean up stale queue entries from the previous server lifetime.
    // Tasks "claimed" by now-dead workers get re-queued or dead-lettered.
    let transport = DurableTransport::new(db.clone());
    let reaped = transport.run_reaper().await;
    if reaped > 0 {
        tracing::info!(target: "recovery", count = reaped, "reaper pre-pass: cleaned stale queue entries");
    }

    let roots = match agentic_runtime::crud::get_resumable_root_runs(&db).await {
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

async fn recover_single_run(
    root: &agentic_runtime::entity::run::Model,
    db: DatabaseConnection,
    state: Arc<RuntimeState>,
    platform: Arc<dyn PlatformContext>,
    builder_bridges: Option<BuilderBridges>,
    schema_cache: Option<Arc<Mutex<HashMap<String, agentic_analytics::SchemaCatalog>>>>,
    builder_test_runner: Option<Arc<dyn agentic_builder::BuilderTestRunner>>,
    builder_app_runner: Option<Arc<dyn agentic_builder::BuilderAppRunner>>,
) -> Result<(), String> {
    use agentic_core::transport::{CoordinatorTransport, WorkerTransport};

    let transport = DurableTransport::new(db.clone());
    let executor = Arc::new(PipelineTaskExecutor {
        platform,
        builder_bridges,
        schema_cache,
        builder_test_runner,
        builder_app_runner,
        db: db.clone(),
        state: Some(state.clone()),
    });

    // ── Step 0: Transparent recovery — clean up partial events ──────────
    //
    // Recovery is transparent: no attempt increment, no attempt_started event.
    // Instead, delete partial events from the interrupted execution (e.g. a
    // step_started without its step_end) and emit a lightweight recovery_resumed
    // marker. This prevents duplicate events in the frontend reasoning trace.
    let attempt = root.attempt; // Same attempt — no increment
    tracing::info!(target: "recovery", run_id = %root.id, attempt, "starting transparent recovery");

    // Delete partial events: find the last completed boundary and remove
    // everything after it. This cleans up step_started events that were
    // emitted before the crash but never got their corresponding step_end.
    {
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
                    | "procedure_completed"
                    | "procedure_step_completed"
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
    }

    // Emit lightweight recovery marker on the root run (same attempt number).
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

                if suspend_data.is_some() || task_run.id == root.id {
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
    // For Temporal-style workflow runs, the coordinator's resume_parent will
    // enqueue a WorkflowDecision task when it processes these resumes — no
    // in-memory channel needed. For analytics/builder runs, resume_parent
    // assigns a TaskSpec::Resume which the worker handles.
    //
    // The pending_resumes are processed by the coordinator when it starts up
    // (via its from_db logic), so no explicit action is needed here.

    // ── Step 4: Register in RuntimeState + spawn coordinator + worker ───
    // Without registration the SSE endpoint finds no notifier and exits
    // immediately, so recovered runs appear "dead" to connected clients.
    {
        let (answer_tx, _answer_rx) = tokio::sync::mpsc::channel::<String>(1);
        let (cancel_tx, _cancel_rx) = tokio::sync::watch::channel(false);
        state.register(&root.id, answer_tx, cancel_tx);
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
