//! Core FSM loop (`run_pipeline_inner`) that drives the pipeline state machine.
//!
//! The [`Orchestrator`] struct itself and the public entry points live in
//! [`super::api`].

use std::collections::HashMap;
use std::sync::Arc;

use crate::back_target::{BackTarget, RetryContext};
use crate::delegation::SuspendReason;
use crate::domain::Domain;
use crate::events::{CoreEvent, DomainEvents, EventStream, HumanInputQuestion, Outcome};
use crate::solver::DomainSolver;
use crate::state::ProblemState;

use super::{
    Orchestrator, OrchestratorError, PipelineOutput, RunContext, emit, run_fanout, stage_order,
    state_name, transitions::PipelineResult,
};

impl<D: Domain, S: DomainSolver<D> + 'static, Ev: DomainEvents> Orchestrator<D, S, Ev> {
    /// Core FSM loop that drives the pipeline from an arbitrary initial state.
    ///
    /// Callers provide the `initial_state` (e.g. `Clarifying` for a full run,
    /// `Specifying` when clarify has already completed) and a pre-populated
    /// [`RunContext`].  The FSM loop iterates until `Done`, a fatal error, or
    /// `max_iterations` is exceeded.
    ///
    /// When `stop_before` is `Some(stage)`, the loop halts and returns
    /// [`PipelineResult::Stopped`] just before executing that stage.  Used by
    /// [`Orchestrator::run_subpipeline`] to run a partial pipeline.
    pub(super) async fn run_pipeline_inner(
        &mut self,
        initial_state: ProblemState<D>,
        trace_id: &str,
        run_ctx: RunContext<D>,
        stop_before: Option<&'static str>,
    ) -> Result<PipelineResult<D>, OrchestratorError<D>>
    where
        D::Intent: Clone,
        D::Spec: Clone,
        D::Answer: Clone,
    {
        let mut run_ctx = run_ctx;
        let mut state = initial_state;
        // `current_stage` is the handler key to dispatch next iteration.
        // Maintained separately from `state` so handlers can route explicitly
        // via `TransitionResult::next_stage` without relying on state_name().
        let mut current_stage: &'static str = state_name(&state);
        let mut iterations: usize = 0;
        let mut revisions: HashMap<&'static str, u32> = HashMap::new();
        // Tracks the last active worker stage for Diagnosing back-edge events.
        let mut last_worker_stage: &'static str = current_stage;

        loop {
            if iterations >= self.max_iterations {
                let prompt = "Max iterations reached. Would you like to continue?";
                let suggestions = vec!["continue".to_string(), "stop".to_string()];
                match self.human_input.request_sync(prompt, &suggestions) {
                    Ok(answer)
                        if answer.trim().eq_ignore_ascii_case("continue")
                            || answer.trim() == "1" =>
                    {
                        self.max_iterations += self.initial_max_iterations;
                    }
                    _ => {
                        emit(
                            &self.event_tx,
                            CoreEvent::Error {
                                message: "max iterations exceeded".to_string(),
                                trace_id: trace_id.to_string(),
                            },
                        )
                        .await;
                        return Err(OrchestratorError::MaxIterationsExceeded);
                    }
                }
            }
            iterations += 1;

            match current_stage {
                // ── Terminal ──────────────────────────────────────────────────
                "done" => {
                    let answer = match state {
                        ProblemState::Done(a) => a,
                        _ => unreachable!("'done' current_stage with non-Done state data"),
                    };
                    emit(
                        &self.event_tx,
                        CoreEvent::Done {
                            trace_id: trace_id.to_string(),
                        },
                    )
                    .await;
                    return Ok(PipelineResult::Done(PipelineOutput {
                        answer,
                        intent: run_ctx
                            .intent
                            .expect("intent must be set by Clarifying before Done"),
                        spec: run_ctx.spec,
                    }));
                }

                // ── Legacy Diagnosing arm ─────────────────────────────────────
                // Default handlers produce ProblemState::Diagnosing on failure
                // so that DomainSolver::diagnose is called here, preserving
                // backward compatibility with existing DomainSolver impls.
                "diagnosing" => {
                    let (error, back) = match state {
                        ProblemState::Diagnosing { error, back } => (error, back),
                        _ => unreachable!(
                            "'diagnosing' current_stage with non-Diagnosing state data"
                        ),
                    };
                    let from = last_worker_stage;

                    // ── Suspension short-circuit ───────────────────────────────
                    // Must happen BEFORE retry_ctx() is called (Suspend has no
                    // meaningful RetryContext and the unreachable! would fire).
                    if let BackTarget::Suspend { reason } = &back {
                        let mut data = self.solver.take_suspension_data()
                            .expect("solver must call store_suspension_data before returning BackTarget::Suspend");
                        data.trace_id = trace_id.to_string();

                        // Emit awaiting_input for ALL suspend reasons so the
                        // frontend always sees an open/close pair with the
                        // input_resolved event emitted on resume.
                        let questions = match reason {
                            SuspendReason::HumanInput { questions } => questions.clone(),
                            SuspendReason::Delegation { request, .. } => {
                                vec![HumanInputQuestion {
                                    prompt: request.clone(),
                                    suggestions: vec![],
                                }]
                            }
                            SuspendReason::ParallelDelegation { targets, .. } => targets
                                .iter()
                                .map(|t| HumanInputQuestion {
                                    prompt: t.request.clone(),
                                    suggestions: vec![],
                                })
                                .collect(),
                        };
                        emit(
                            &self.event_tx,
                            CoreEvent::AwaitingHumanInput {
                                questions,
                                from_state: from.to_string(),
                                trace_id: trace_id.to_string(),
                            },
                        )
                        .await;

                        emit(
                            &self.event_tx,
                            CoreEvent::StateExit {
                                state: from.to_string(),
                                outcome: Outcome::Suspended,
                                trace_id: trace_id.to_string(),
                                sub_spec_index: None,
                            },
                        )
                        .await;
                        return Err(OrchestratorError::Suspended {
                            reason: reason.clone(),
                            resume_data: data,
                            trace_id: trace_id.to_string(),
                        });
                    }

                    let retry_ctx = back.retry_ctx().clone();
                    let back_edge_reason: String = match retry_ctx.errors.last() {
                        Some(e) => e.clone(),
                        None => format!("{error}"),
                    };
                    match self.solver.diagnose(error, back, &run_ctx).await {
                        Ok(recovered) => {
                            run_ctx.retry_ctx = Some(retry_ctx);
                            let to = state_name(&recovered);
                            let outcome = if to == from {
                                Outcome::Retry
                            } else if stage_order(to) > stage_order(from) {
                                // The recovered state is *ahead* of the failing state
                                // (e.g. executing → interpreting via ValueAnomaly).
                                // Treat this as a forward advance, not a back-edge.
                                Outcome::Advanced
                            } else {
                                Outcome::BackTracked
                            };
                            emit(
                                &self.event_tx,
                                CoreEvent::StateExit {
                                    state: from.into(),
                                    outcome: outcome.clone(),
                                    trace_id: trace_id.to_string(),
                                    sub_spec_index: None,
                                },
                            )
                            .await;
                            // Only emit BackEdge for genuine backward or retry
                            // transitions — forward advances are not back-edges.
                            if outcome != Outcome::Advanced {
                                emit(
                                    &self.event_tx,
                                    CoreEvent::BackEdge {
                                        from: from.into(),
                                        to: to.into(),
                                        reason: back_edge_reason,
                                        trace_id: trace_id.to_string(),
                                    },
                                )
                                .await;
                            }
                            current_stage = to;
                            state = recovered;
                        }
                        Err(fatal) => {
                            emit(
                                &self.event_tx,
                                CoreEvent::StateExit {
                                    state: from.into(),
                                    outcome: Outcome::Failed,
                                    trace_id: trace_id.to_string(),
                                    sub_spec_index: None,
                                },
                            )
                            .await;
                            emit(
                                &self.event_tx,
                                CoreEvent::Error {
                                    message: format!("fatal error from diagnose: {fatal}"),
                                    trace_id: trace_id.to_string(),
                                },
                            )
                            .await;
                            // Store checkpoint for retry.
                            if let Some(cp) = self.solver.build_checkpoint(from, &run_ctx, None) {
                                self.solver.store_suspension_data(cp);
                            }
                            return Err(OrchestratorError::Fatal(fatal));
                        }
                    }
                }

                // ── Table-driven worker states ─────────────────────────────────
                sname => {
                    // ── Sub-pipeline stop ─────────────────────────────────────
                    // Halt before executing this stage if the caller requested it.
                    if stop_before == Some(sname) {
                        return Ok(PipelineResult::Stopped { state, run_ctx });
                    }

                    // Update RunContext from state data before execute / should_skip.
                    match &state {
                        ProblemState::Clarifying(intent) => {
                            // Capture the raw intent as a fallback for the GeneralInquiry
                            // shortcut, which jumps directly from clarifying → done without
                            // ever entering the specifying stage.
                            run_ctx.intent = Some(intent.clone());
                        }
                        ProblemState::Specifying(intent) => {
                            run_ctx.intent = Some(intent.clone());
                        }
                        ProblemState::Solving(spec) => {
                            run_ctx.spec = Some(spec.clone());
                        }
                        _ => {}
                    }

                    // ── Skip check (static SKIP_STATES / dynamic should_skip) ──
                    // The solver may bypass this state entirely.  No events are
                    // emitted and execute is not called when a skip occurs.
                    if let Some(next_state) = self.solver.should_skip(sname, &state, &run_ctx) {
                        current_stage = state_name(&next_state);
                        state = next_state;
                        continue;
                    }

                    last_worker_stage = sname;

                    // Clone the Arcs so we can release the borrow on
                    // self.handlers before taking &mut self.solver.
                    let (execute_fn, diagnose_fn, handler_next) = {
                        let h = self
                            .handlers
                            .get(sname)
                            .unwrap_or_else(|| panic!("no handler for state '{sname}'"));
                        (Arc::clone(&h.execute), h.diagnose.clone(), h.next)
                    };

                    let _rev = Self::enter(&self.event_tx, sname, &mut revisions, trace_id).await;

                    let result = execute_fn(
                        &mut self.solver,
                        state,
                        &self.event_tx,
                        &run_ctx,
                        &self.memory,
                    )
                    .await;

                    match result.errors {
                        // ── Success ───────────────────────────────────────────
                        None => {
                            // ── Fan-out path ──────────────────────────────────
                            // When the specifying handler produced multiple specs
                            // it returns them via `fan_out` instead of calling
                            // run_fanout inline, so that StateExit for the current
                            // state fires *before* any sub-spec work begins.
                            if let Some(specs) = result.fan_out {
                                run_ctx.retry_ctx = None;
                                emit(
                                    &self.event_tx,
                                    CoreEvent::ValidationPass {
                                        state: sname.into(),
                                    },
                                )
                                .await;
                                emit(
                                    &self.event_tx,
                                    CoreEvent::StateExit {
                                        state: sname.into(),
                                        outcome: Outcome::Advanced,
                                        trace_id: trace_id.to_string(),
                                        sub_spec_index: None,
                                    },
                                )
                                .await;

                                let fanout = run_fanout(
                                    &mut self.solver,
                                    specs,
                                    &run_ctx,
                                    &self.memory,
                                    run_ctx.intent.clone(),
                                    &self.event_tx,
                                )
                                .await;

                                match fanout.errors {
                                    None => {
                                        current_stage = fanout.next_stage.unwrap_or(handler_next);
                                        state = fanout.state_data;
                                    }
                                    Some(_) => {
                                        // Fan-out failed.  Handle diagnosing inline to
                                        // avoid emitting a second StateExit for sname.
                                        let (error, back) = match fanout.state_data {
                                            ProblemState::Diagnosing { error, back } => {
                                                (error, back)
                                            }
                                            _ => unreachable!(
                                                "run_fanout failure must produce Diagnosing state"
                                            ),
                                        };

                                        // Suspension short-circuit (same as main diagnosing arm).
                                        if let BackTarget::Suspend { reason } = &back {
                                            let mut data = self
                                                .solver
                                                .take_suspension_data()
                                                .expect("solver must call store_suspension_data before returning BackTarget::Suspend");
                                            data.trace_id = trace_id.to_string();
                                            let questions = match reason {
                                                SuspendReason::HumanInput { questions } => {
                                                    questions.clone()
                                                }
                                                SuspendReason::Delegation { request, .. } => {
                                                    vec![HumanInputQuestion {
                                                        prompt: request.clone(),
                                                        suggestions: vec![],
                                                    }]
                                                }
                                                SuspendReason::ParallelDelegation {
                                                    targets,
                                                    ..
                                                } => targets
                                                    .iter()
                                                    .map(|t| HumanInputQuestion {
                                                        prompt: t.request.clone(),
                                                        suggestions: vec![],
                                                    })
                                                    .collect(),
                                            };
                                            emit(
                                                &self.event_tx,
                                                CoreEvent::AwaitingHumanInput {
                                                    questions,
                                                    from_state: sname.to_string(),
                                                    trace_id: trace_id.to_string(),
                                                },
                                            )
                                            .await;
                                            return Err(OrchestratorError::Suspended {
                                                reason: reason.clone(),
                                                resume_data: data,
                                                trace_id: trace_id.to_string(),
                                            });
                                        }

                                        let retry_ctx = back.retry_ctx().clone();
                                        let back_edge_reason = match retry_ctx.errors.last() {
                                            Some(e) => e.clone(),
                                            None => format!("{error}"),
                                        };
                                        match self.solver.diagnose(error, back, &run_ctx).await {
                                            Ok(recovered) => {
                                                run_ctx.retry_ctx = Some(retry_ctx);
                                                let to = state_name(&recovered);
                                                emit(
                                                    &self.event_tx,
                                                    CoreEvent::BackEdge {
                                                        from: sname.into(),
                                                        to: to.into(),
                                                        reason: back_edge_reason,
                                                        trace_id: trace_id.to_string(),
                                                    },
                                                )
                                                .await;
                                                current_stage = to;
                                                state = recovered;
                                            }
                                            Err(fatal) => {
                                                emit(
                                                    &self.event_tx,
                                                    CoreEvent::Error {
                                                        message: format!(
                                                            "fatal error from fan-out diagnose: {fatal}"
                                                        ),
                                                        trace_id: trace_id.to_string(),
                                                    },
                                                )
                                                .await;
                                                // Store checkpoint for retry.
                                                // TODO: pass partial fanout results for sub-spec level retry.
                                                if let Some(cp) = self
                                                    .solver
                                                    .build_checkpoint(sname, &run_ctx, None)
                                                {
                                                    self.solver.store_suspension_data(cp);
                                                }
                                                return Err(OrchestratorError::Fatal(fatal));
                                            }
                                        }
                                    }
                                }
                                continue;
                            }

                            // ── Normal success path ───────────────────────────
                            // Update RunContext from the output of this stage.
                            match &result.state_data {
                                ProblemState::Specifying(intent) => {
                                    run_ctx.intent = Some(intent.clone());
                                }
                                ProblemState::Solving(spec) => {
                                    run_ctx.spec = Some(spec.clone());
                                }
                                _ => {}
                            }
                            // Clear retry context: this stage succeeded.
                            run_ctx.retry_ctx = None;
                            emit(
                                &self.event_tx,
                                CoreEvent::ValidationPass {
                                    state: sname.into(),
                                },
                            )
                            .await;
                            emit(
                                &self.event_tx,
                                CoreEvent::StateExit {
                                    state: sname.into(),
                                    outcome: Outcome::Advanced,
                                    trace_id: trace_id.to_string(),
                                    sub_spec_index: None,
                                },
                            )
                            .await;
                            // Explicit routing: next_stage override takes precedence
                            // over the handler's default `next` key.  Enables fan-out
                            // to jump directly to "interpreting" without relying on
                            // state_name(state_data) for dispatch.
                            current_stage = result.next_stage.unwrap_or(handler_next);
                            state = result.state_data;
                        }

                        // ── Failure ───────────────────────────────────────────
                        Some(errors) => {
                            emit(
                                &self.event_tx,
                                CoreEvent::ValidationFail {
                                    state: sname.into(),
                                    errors: errors.iter().map(|e| e.to_string()).collect(),
                                },
                            )
                            .await;

                            if errors.is_empty() {
                                // Empty-error sentinel: state_name() on state_data
                                // determines the next dispatch key.  When state_data
                                // is ProblemState::Diagnosing this routes to "diagnosing".
                                current_stage = state_name(&result.state_data);
                                state = result.state_data;
                            } else {
                                // Non-empty errors: call handler.diagnose.
                                let retry_count = revisions.get(sname).copied().unwrap_or(0);
                                let diagnose_result = match diagnose_fn {
                                    Some(ref f) => f(&errors, retry_count, result.state_data),
                                    None => Some(result.state_data),
                                };
                                match diagnose_result {
                                    Some(recovery) => {
                                        let to = state_name(&recovery);
                                        let outcome = if to == sname {
                                            Outcome::Retry
                                        } else {
                                            Outcome::BackTracked
                                        };
                                        let reason = errors
                                            .last()
                                            .map(|e| e.to_string())
                                            .unwrap_or_else(|| "back-edge".into());
                                        emit(
                                            &self.event_tx,
                                            CoreEvent::StateExit {
                                                state: sname.into(),
                                                outcome,
                                                trace_id: trace_id.to_string(),
                                                sub_spec_index: None,
                                            },
                                        )
                                        .await;
                                        emit(
                                            &self.event_tx,
                                            CoreEvent::BackEdge {
                                                from: sname.into(),
                                                to: to.into(),
                                                reason,
                                                trace_id: trace_id.to_string(),
                                            },
                                        )
                                        .await;
                                        // Set retry context so the retried stage sees
                                        // the errors that caused the back-edge.
                                        run_ctx.retry_ctx = Some(RetryContext {
                                            attempt: retry_count,
                                            rate_limit_attempt: 0,
                                            errors: errors.iter().map(|e| e.to_string()).collect(),
                                            previous_output: None,
                                        });
                                        current_stage = to;
                                        state = recovery;
                                    }
                                    None => {
                                        // handler.diagnose escalated.
                                        let fatal = errors
                                            .into_iter()
                                            .next()
                                            .expect("non-empty errors on None diagnose");
                                        emit(
                                            &self.event_tx,
                                            CoreEvent::StateExit {
                                                state: sname.into(),
                                                outcome: Outcome::Failed,
                                                trace_id: trace_id.to_string(),
                                                sub_spec_index: None,
                                            },
                                        )
                                        .await;
                                        emit(
                                            &self.event_tx,
                                            CoreEvent::Error {
                                                message: format!(
                                                    "fatal error from handler diagnose: {fatal}"
                                                ),
                                                trace_id: trace_id.to_string(),
                                            },
                                        )
                                        .await;
                                        // Store checkpoint for retry.
                                        if let Some(cp) =
                                            self.solver.build_checkpoint(sname, &run_ctx, None)
                                        {
                                            self.solver.store_suspension_data(cp);
                                        }
                                        return Err(OrchestratorError::Fatal(fatal));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Emit a StateEnter event and return the revision number used.
    pub(super) async fn enter(
        tx: &Option<EventStream<Ev>>,
        sname: &'static str,
        revisions: &mut HashMap<&'static str, u32>,
        trace_id: &str,
    ) -> u32 {
        let rev = *revisions.get(sname).unwrap_or(&0);
        emit(
            tx,
            CoreEvent::StateEnter {
                state: sname.into(),
                revision: rev,
                trace_id: trace_id.to_string(),
                sub_spec_index: None,
            },
        )
        .await;
        *revisions.entry(sname).or_insert(0) += 1;
        rev
    }
}
