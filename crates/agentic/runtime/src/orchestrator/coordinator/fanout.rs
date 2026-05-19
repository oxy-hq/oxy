//! Per-parent child-result accumulation and resume logic.

use std::collections::HashMap;

use agentic_core::delegation::{ChildCompletion, FanoutFailurePolicy, TaskAssignment, TaskSpec};
use serde_json::{Value, json};

use crate::crud;
use crate::lifecycle::state::RunStatus;

use super::{ChildResult, Coordinator, TaskStatus};

impl Coordinator {
    /// Record a child's result and check if the fan-out is complete.
    ///
    /// For single-child delegations this behaves exactly as before: the parent
    /// is resumed immediately when the one child finishes.
    ///
    /// For multi-child fan-outs:
    /// - **FailFast**: first child failure cancels siblings and resumes parent.
    /// - **BestEffort**: waits for all children, then resumes with aggregated results.
    pub(super) async fn record_child_result(
        &mut self,
        parent_id: &str,
        child_id: &str,
        result: ChildResult,
    ) {
        // Loop-iteration metadata (only set on children that are
        // one iteration of a loop_sequential step's fan-out). Capture
        // *before* the mutable-borrow block below so we can emit the
        // per-iteration progress event without re-borrowing.
        let loop_iter_meta = self
            .tasks
            .get(child_id)
            .and_then(|n| n.loop_iteration.clone());

        // child_id → original loop iteration index, for any sibling
        // children that carry `loop_iteration`. `serialize_completed`
        // uses this to stamp the *original* iteration index on each
        // entry — not the position in the fan-out — so the decider's
        // fold can map a partial fan-out (e.g. forcing iteration 7
        // out of 30) back to the right `items[i]`.
        //
        // Only built when the parent is actually in `WaitingOnChildren`.
        // Both single- and multi-child delegations suspend the parent
        // into `WaitingOnChildren` (see `suspension.rs`), so the happy
        // path always falls through here. The `_ => ResumeImmediate`
        // arm below covers defensive edges — a child outcome landing
        // after the parent has already been resumed — and skipping the
        // lookup there avoids cloning every loop child's `task_id` on
        // those late-arrival events. Peek at the status with a shared
        // borrow before the mut-borrow block claims `self.tasks`.
        let needs_loop_index_lookup = self
            .tasks
            .get(parent_id)
            .map(|n| matches!(n.status, TaskStatus::WaitingOnChildren { .. }))
            .unwrap_or(false);
        let loop_index_lookup: HashMap<String, usize> = if needs_loop_index_lookup {
            self.tasks
                .iter()
                .filter_map(|(id, n)| n.loop_iteration.as_ref().map(|m| (id.clone(), m.index)))
                .collect()
        } else {
            HashMap::new()
        };
        enum NextAction {
            ResumeImmediate {
                answer: String,
                /// `"done"` for `ChildResult::Done`, `"failed"` for
                /// `ChildResult::Failed`. Plumbed through to the workflow
                /// decider so its fold path can take its `Fail` branch.
                child_status: &'static str,
            },
            FailFast {
                siblings: Vec<String>,
                parent_run_id: String,
                meta: Value,
            },
            AllDone {
                parent_run_id: String,
                meta: Value,
            },
            StillWaiting {
                parent_run_id: String,
                meta: Value,
            },
        }

        // Determine next action while holding the mutable borrow, then release.
        let action = {
            let Some(parent_node) = self.tasks.get_mut(parent_id) else {
                return;
            };

            match &mut parent_node.status {
                TaskStatus::WaitingOnChildren {
                    child_task_ids,
                    completed,
                    failure_policy,
                } => {
                    completed.insert(child_id.to_string(), result.clone());
                    let total = child_task_ids.len();
                    let done_count = completed.len();

                    tracing::info!(
                        target: "coordinator",
                        parent_id,
                        child_id,
                        done_count,
                        total,
                        "child result recorded"
                    );

                    let completed_json =
                        Self::serialize_completed(completed, child_task_ids, |id| {
                            loop_index_lookup.get(id).copied()
                        });
                    let child_ids_json: Vec<String> = child_task_ids.clone();
                    let meta = json!({
                        "child_task_ids": child_ids_json,
                        "completed": completed_json,
                        "failure_policy": serde_json::to_value(&*failure_policy).unwrap_or_default(),
                    });
                    let parent_run_id = parent_node.run_id.clone();

                    let is_failure = matches!(result, ChildResult::Failed(_));
                    let should_fail_fast =
                        is_failure && matches!(failure_policy, FanoutFailurePolicy::FailFast);
                    let all_done = done_count >= total;

                    if should_fail_fast {
                        let siblings: Vec<String> = child_task_ids
                            .iter()
                            .filter(|id| !completed.contains_key(id.as_str()))
                            .cloned()
                            .collect();
                        NextAction::FailFast {
                            siblings,
                            parent_run_id,
                            meta,
                        }
                    } else if all_done {
                        NextAction::AllDone {
                            parent_run_id,
                            meta,
                        }
                    } else {
                        NextAction::StillWaiting {
                            parent_run_id,
                            meta,
                        }
                    }
                }
                _ => {
                    // Parent is not waiting on children — resume directly.
                    // Borrow rather than move so the subsequent
                    // iteration-completion emit (outside the match) can
                    // still inspect `result`. Cloning the contained
                    // strings is cheap and keeps the two consumers
                    // decoupled.
                    let (answer, child_status) = match &result {
                        ChildResult::Done(a) => (a.clone(), "done"),
                        ChildResult::Failed(msg) => (format!("Delegation failed: {msg}"), "failed"),
                    };
                    NextAction::ResumeImmediate {
                        answer,
                        child_status,
                    }
                }
            }
        };

        // Mutable borrow released — do async work.
        //
        // Emit the per-iteration completion event FIRST (before the
        // action's resume_parent / persist_task_status), so the FE
        // sees the cell flip from running → done/failed in real time
        // rather than waiting for the aggregated decide() call that
        // fires once the whole fan-out finishes. This is what makes
        // the loop progress bar update incrementally.
        if let Some(meta) = loop_iter_meta {
            let (status, error) = match &result {
                ChildResult::Done(_) => ("done", None),
                ChildResult::Failed(msg) => ("failed", Some(msg.clone())),
            };
            self.emit_iteration_completed(parent_id, &meta, status, error.as_deref())
                .await;
        }

        match action {
            NextAction::ResumeImmediate {
                answer,
                child_status,
            } => {
                self.resume_parent(parent_id, answer, child_status).await;
            }
            NextAction::FailFast {
                siblings,
                parent_run_id,
                meta,
            } => {
                self.persist_task_status(&parent_run_id, "delegating", Some(meta))
                    .await;
                for sibling_id in &siblings {
                    self.transport.cancel(sibling_id).await.ok();
                    if let Some(sibling_node) = self.tasks.get_mut(sibling_id) {
                        sibling_node.status = TaskStatus::Failed;
                        sibling_node.suspended_at = None;
                    }
                }
                let answer = self.aggregate_child_results(parent_id);
                // FailFast triggers when at least one child failed — the
                // step as a whole is a failure regardless of how many
                // siblings had completed before the cancel.
                self.resume_parent(parent_id, answer, "failed").await;
            }
            NextAction::AllDone {
                parent_run_id,
                meta,
            } => {
                self.persist_task_status(&parent_run_id, "delegating", Some(meta))
                    .await;
                let answer = self.aggregate_child_results(parent_id);
                let child_status = if self.parent_has_any_failure(parent_id) {
                    "failed"
                } else {
                    "done"
                };
                self.resume_parent(parent_id, answer, child_status).await;
            }
            NextAction::StillWaiting {
                parent_run_id,
                meta,
            } => {
                self.persist_task_status(&parent_run_id, "delegating", Some(meta))
                    .await;
            }
        }
    }

    /// Emit `subrun_step_iteration_completed` for one loop child
    /// as it lands. Persists to the parent's run-event stream (the
    /// run the loop_sequential step belongs to), bumping the parent
    /// node's `next_seq`. Pairs with the
    /// `subrun_step_iteration_started` events the decider emits
    /// at fan-out time — together they drive the live loop progress
    /// bar in the diagram.
    async fn emit_iteration_completed(
        &mut self,
        parent_id: &str,
        meta: &super::LoopIterationMeta,
        status: &str,
        error: Option<&str>,
    ) {
        let Some(node) = self.tasks.get_mut(parent_id) else {
            return;
        };
        let seq = node.next_seq;
        node.next_seq += 1;
        let mut payload = json!({
            "step": meta.step_name,
            "index": meta.index,
            "status": status,
        });
        if let (Some(obj), Some(err)) = (payload.as_object_mut(), error) {
            obj.insert("error".to_string(), Value::String(err.to_string()));
        }
        let run_id = node.run_id.clone();
        if let Err(e) = crud::insert_event(
            &self.db,
            &run_id,
            seq,
            "subrun_step_iteration_completed",
            &payload,
            self.attempt,
        )
        .await
        {
            tracing::error!(
                target: "coordinator",
                parent_id,
                run_id = %run_id,
                step = %meta.step_name,
                index = meta.index,
                error = %e,
                "failed to persist iteration_completed event"
            );
        }
        self.state.notify(&run_id);
    }

    /// True iff the parent's `WaitingOnChildren` has at least one failed entry.
    /// Used by `BestEffort` fan-outs to flip the aggregated step status to
    /// `"failed"` when any iteration broke — without this, a partial-success
    /// loop would silently pass and the workflow would continue.
    fn parent_has_any_failure(&self, parent_id: &str) -> bool {
        let Some(parent_node) = self.tasks.get(parent_id) else {
            return false;
        };
        match &parent_node.status {
            TaskStatus::WaitingOnChildren { completed, .. } => completed
                .values()
                .any(|r| matches!(r, ChildResult::Failed(_))),
            _ => false,
        }
    }

    /// Aggregate completed child results into a single answer string.
    ///
    /// For single-child delegations: returns the child's answer directly.
    /// For multi-child: returns a JSON object `{ "child_id": { "status": ..., "answer"|"error": ... } }`.
    pub(super) fn aggregate_child_results(&self, parent_id: &str) -> String {
        let Some(parent_node) = self.tasks.get(parent_id) else {
            return "No results".to_string();
        };
        let TaskStatus::WaitingOnChildren {
            child_task_ids,
            completed,
            ..
        } = &parent_node.status
        else {
            return "No results".to_string();
        };

        // Single-child: return the answer directly (backward compatible).
        if child_task_ids.len() == 1
            && let Some(result) = completed.get(&child_task_ids[0])
        {
            return match result {
                ChildResult::Done(a) => a.clone(),
                ChildResult::Failed(msg) => format!("Delegation failed: {msg}"),
            };
        }

        // Multi-child: aggregate as JSON, fan-out-ordered. Loop fan-outs
        // need entries stamped with the *original* iteration index (see
        // `serialize_completed`), so resolve via the children's
        // `loop_iteration.index`.
        let aggregated = Self::serialize_completed(completed, child_task_ids, |id| {
            self.tasks
                .get(id)
                .and_then(|n| n.loop_iteration.as_ref())
                .map(|m| m.index)
        });
        serde_json::to_string(&aggregated).unwrap_or_else(|_| "{}".to_string())
    }

    /// Build the aggregated `{child_id: entry}` map sent back to the
    /// decider as the fan-out's answer.
    ///
    /// Each entry carries an `index` field. For loop fan-outs this MUST
    /// be the original loop iteration index (so the decider's fold can
    /// map back to the right `items[i]` for cache attribution), NOT the
    /// position in `child_ids_in_order`. For a partial cache hit those
    /// two are different — only forced indices delegate, so a fan-out
    /// of "iteration 7 only" still needs to carry `index: 7`. The
    /// caller passes `loop_index_of` which resolves a child_id to its
    /// original loop index by reading `TaskNode.loop_iteration.index`;
    /// for non-loop fan-outs it returns `None` and we fall back to the
    /// child's position in `child_ids_in_order`.
    pub(super) fn serialize_completed(
        completed: &HashMap<String, ChildResult>,
        child_ids_in_order: &[String],
        loop_index_of: impl Fn(&str) -> Option<usize>,
    ) -> Value {
        let mut obj = serde_json::Map::new();
        for (pos, child_id) in child_ids_in_order.iter().enumerate() {
            let Some(result) = completed.get(child_id) else {
                continue;
            };
            let idx = loop_index_of(child_id).unwrap_or(pos);
            let entry = match result {
                ChildResult::Done(answer) => {
                    json!({ "status": "done", "answer": answer, "index": idx })
                }
                ChildResult::Failed(error) => {
                    json!({ "status": "failed", "error": error, "index": idx })
                }
            };
            obj.insert(child_id.clone(), entry);
        }
        Value::Object(obj)
    }

    /// Walk parent links from `task_id` up to the tree root and return that
    /// root's `run_id`.
    fn root_run_id_of(&self, task_id: &str) -> Option<String> {
        let mut current = task_id.to_string();
        loop {
            let node = self.tasks.get(&current)?;
            match &node.parent_task_id {
                Some(p) => current = p.clone(),
                None => return Some(node.run_id.clone()),
            }
        }
    }

    /// True if the root ancestor of `task_id` has been user-cancelled.
    ///
    /// `RuntimeState::cancel` marks `statuses[root_run_id] = Cancelled`
    /// synchronously; this lookup lets the coordinator short-circuit parent
    /// resumes when a delegated child finishes after the user clicked cancel.
    pub(super) fn is_subtree_user_cancelled(&self, task_id: &str) -> bool {
        let Some(root_run_id) = self.root_run_id_of(task_id) else {
            return false;
        };
        self.state
            .statuses
            .get(&root_run_id)
            .map(|r| matches!(r.value(), RunStatus::Cancelled))
            .unwrap_or(false)
    }

    /// Resume a suspended parent task by assigning a `TaskSpec::Resume` to the
    /// worker.
    ///
    /// Always emits an `input_resolved` event to pair with the
    /// `awaiting_input` event the orchestrator emitted on suspension. This
    /// applies to both human answers and delegation completions — the
    /// awaiting/resolved pair is suspend-reason-agnostic.
    pub(super) async fn resume_parent(
        &mut self,
        parent_id: &str,
        answer: String,
        child_status: &str,
    ) {
        // Short-circuit: if the user cancelled the root while this parent was
        // suspended on a delegation, don't rebuild the pipeline just because
        // the child happened to finish. Finalise the parent via
        // `handle_cancelled`, which also cascades the cancellation up to the
        // root run.
        //
        // `handle_cancelled` → `record_child_result` → `resume_parent` forms
        // an async cycle; `Box::pin` breaks the unbounded recursive future
        // size so the compiler is happy.
        if self.is_subtree_user_cancelled(parent_id) {
            let already_finalised = self
                .tasks
                .get(parent_id)
                .map(|n| matches!(n.status, TaskStatus::Failed))
                .unwrap_or(true);
            if already_finalised {
                return;
            }
            tracing::info!(
                target: "coordinator",
                parent_id,
                "root run user-cancelled; skipping parent resume"
            );
            Box::pin(self.handle_cancelled(parent_id)).await;
            return;
        }

        tracing::info!(
            target: "coordinator",
            parent_id,
            answer_len = answer.len(),
            "resuming parent task"
        );
        // Extract all needed data from the mutable borrow, then release it.
        let (run_id, resume_data, seq, child_task_id_hint) = {
            let Some(parent_node) = self.tasks.get_mut(parent_id) else {
                return;
            };

            // For workflow decision chaining, grab the child task ID before
            // transitioning to Running (it lives in WaitingOnChildren).
            let child_task_id_hint = match &parent_node.status {
                TaskStatus::WaitingOnChildren { child_task_ids, .. } => {
                    child_task_ids.first().cloned()
                }
                _ => None,
            };

            parent_node.status = TaskStatus::Running;
            parent_node.suspended_at = None;
            let run_id = parent_node.run_id.clone();

            let resume_data = match parent_node.suspend_data.take() {
                Some(data) => data,
                None => {
                    tracing::error!(target: "coordinator", parent_id, "no suspend data for resume");
                    return;
                }
            };

            let seq = parent_node.next_seq;
            parent_node.next_seq += 1;
            (run_id, resume_data, seq, child_task_id_hint)
        };

        // Persist task_status transition back to running (single write).
        self.persist_task_status(&run_id, "running", None).await;
        let payload = json!({ "answer": &answer, "trace_id": &resume_data.trace_id });
        crud::insert_event(
            &self.db,
            &run_id,
            seq,
            "input_resolved",
            &payload,
            self.attempt,
        )
        .await
        .ok();
        self.state.notify(&run_id);

        self.state
            .statuses
            .insert(run_id.clone(), RunStatus::Running);

        // ── Temporal-style workflow decision task: enqueue WorkflowDecision ─
        if resume_data.from_state == "workflow_decision" {
            let step_name = resume_data
                .stage_data
                .get("step_name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let step_index = resume_data
                .stage_data
                .get("step_index")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;

            let pending_child_answer = child_task_id_hint.map(|ctid| ChildCompletion {
                child_task_id: ctid,
                step_index,
                step_name,
                status: child_status.to_string(),
                answer: answer.clone(),
            });

            let assignment = TaskAssignment {
                task_id: parent_id.to_string(),
                parent_task_id: None,
                run_id: run_id.clone(),
                spec: TaskSpec::WorkflowDecision {
                    run_id: run_id.clone(),
                    pending_child_answer,
                },
                policy: None,
            };
            if let Err(e) = self.transport.assign(assignment).await {
                self.fail_parent_on_assign_error(parent_id, &run_id, "WorkflowDecision", e)
                    .await;
            }
            return;
        }

        // ── Non-workflow: assign TaskSpec::Resume ───────────────────────────
        //
        // For analytics/builder pipelines that have a SuspendedRunData checkpoint,
        // assign a TaskSpec::Resume so a fresh pipeline is built from that data.
        let assignment = TaskAssignment {
            task_id: parent_id.to_string(),
            parent_task_id: None,
            run_id: run_id.clone(),
            spec: TaskSpec::Resume {
                run_id: run_id.clone(),
                resume_data,
                answer,
            },
            policy: None,
        };

        if let Err(e) = self.transport.assign(assignment).await {
            self.fail_parent_on_assign_error(parent_id, &run_id, "Resume", e)
                .await;
        }
    }

    /// Fail the parent run when scheduling its next task fails.
    ///
    /// Previously we logged and returned, leaving the run in `task_status =
    /// running` forever: the decision task's queue row had already been
    /// released by the prior worker, so the reaper couldn't resurrect it and
    /// the UI saw a permanent "still running" state. Flipping the run to
    /// `failed` here surfaces the error rather than hanging.
    pub(super) async fn fail_parent_on_assign_error(
        &self,
        parent_id: &str,
        run_id: &str,
        target: &str,
        err: impl std::fmt::Display,
    ) {
        let err_text = format!("failed to schedule {target}: {err}");
        tracing::error!(
            target: "coordinator",
            parent_id,
            run_id,
            assign_target = target,
            error = %err,
            "failed to assign follow-up task to worker — failing parent run"
        );
        if let Err(persist_err) = crud::update_run_failed(&self.db, run_id, &err_text).await {
            tracing::error!(
                target: "coordinator",
                parent_id,
                run_id,
                error = %persist_err,
                "failed to persist parent failure after assign error"
            );
        }
        self.state
            .statuses
            .insert(run_id.to_string(), RunStatus::Failed(err_text));
        self.state.notify(run_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A partial loop fan-out (e.g. forcing iteration 7 of 30) used to
    /// stamp `"index": 0` on the single delegated entry because
    /// `serialize_completed` enumerated `child_ids_in_order`. The decider's
    /// fold then mapped the fresh outcome to `items[0]` and the user
    /// saw the cached value for iteration 7. Regression guard: the
    /// `loop_index_of` lookup must override the enum position.
    #[test]
    fn serialize_completed_uses_loop_index_for_partial_fanout() {
        let mut completed = HashMap::new();
        completed.insert(
            "child-abc".to_string(),
            ChildResult::Done("fresh-answer".into()),
        );
        let child_ids = vec!["child-abc".to_string()];

        let original_idx: HashMap<String, usize> =
            [("child-abc".to_string(), 7)].into_iter().collect();
        let aggregated = Coordinator::serialize_completed(&completed, &child_ids, |id| {
            original_idx.get(id).copied()
        });

        let entry = aggregated.get("child-abc").expect("entry present");
        assert_eq!(entry["index"], 7);
        assert_eq!(entry["answer"], "fresh-answer");
        assert_eq!(entry["status"], "done");
    }

    /// Non-loop fan-outs (or recovery paths missing metadata) fall
    /// back to enum position so the existing aggregator contract still
    /// holds.
    #[test]
    fn serialize_completed_falls_back_to_position_when_lookup_returns_none() {
        let mut completed = HashMap::new();
        completed.insert("c0".to_string(), ChildResult::Done("a0".into()));
        completed.insert("c1".to_string(), ChildResult::Failed("oops".into()));
        let child_ids = vec!["c0".to_string(), "c1".to_string()];

        let aggregated = Coordinator::serialize_completed(&completed, &child_ids, |_| None);
        assert_eq!(aggregated["c0"]["index"], 0);
        assert_eq!(aggregated["c1"]["index"], 1);
        assert_eq!(aggregated["c1"]["status"], "failed");
        assert_eq!(aggregated["c1"]["error"], "oops");
    }
}
