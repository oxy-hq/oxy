//! Fan-out execution: concurrent or serial solve+execute over multiple specs.

use std::sync::Arc;

use tracing::Instrument;

use crate::back_target::{BackTarget, RetryContext};
use crate::domain::Domain;
use crate::events::{CoreEvent, DomainEvents, EventStream, Outcome};
use crate::solver::{DomainSolver, FanoutWorker};
use crate::state::ProblemState;

use super::{RunContext, SessionMemory, TransitionResult, child_trace_id, emit, next_trace_id};

/// Execute solve+execute for each spec in `specs`, merge the results, and
/// return a [`TransitionResult`] pointing at the Interpreting state.
///
/// Emits [`CoreEvent::FanOut`], [`CoreEvent::SubSpecStart`], and
/// [`CoreEvent::SubSpecEnd`] events for each sub-spec.  On any sub-spec
/// failure or merge failure, returns a `TransitionResult` that routes through
/// [`ProblemState::Diagnosing`].
///
/// # Panics
///
/// Panics (via `.expect`) if `ctx_intent` is `None`, which must not happen
/// when Specifying is reached normally (the orchestrator sets `ctx.intent`
/// after Clarifying).
pub async fn run_fanout<D, S, Ev>(
    solver: &mut S,
    specs: Vec<D::Spec>,
    ctx: &RunContext<D>,
    mem: &SessionMemory<D>,
    ctx_intent: Option<D::Intent>,
    events: &Option<EventStream<Ev>>,
) -> TransitionResult<D>
where
    D: Domain,
    S: DomainSolver<D>,
    Ev: DomainEvents,
{
    let total = specs.len();
    let fan_trace = next_trace_id();
    emit(
        events,
        CoreEvent::FanOut {
            spec_count: total,
            trace_id: fan_trace.clone(),
        },
    )
    .await;

    // ── Try concurrent path ──────────────────────────────────────────────
    if let Some(worker) = solver.fanout_worker::<Ev>() {
        // Build owned copies for the concurrent tasks.
        let ctx_owned = RunContext {
            intent: ctx.intent.clone(),
            spec: ctx.spec.clone(),
            retry_ctx: ctx.retry_ctx.clone(),
        };
        let mem_owned = mem.clone_shallow();
        let max_retries = solver.max_fanout_retries();
        let results = run_fanout_concurrent::<D, Ev>(
            worker,
            specs,
            total,
            fan_trace.clone(),
            Arc::new(ctx_owned),
            Arc::new(mem_owned),
            events.clone(),
            max_retries,
        )
        .await;

        return collect_fanout_results::<D, S>(solver, results, ctx_intent);
    }

    // ── Serial fallback ──────────────────────────────────────────────────
    let mut results = Vec::with_capacity(total);
    for (index, spec) in specs.into_iter().enumerate() {
        let sub_trace = child_trace_id(&fan_trace, index);
        emit(
            events,
            CoreEvent::SubSpecStart {
                index,
                total,
                trace_id: sub_trace.clone(),
            },
        )
        .await;

        emit(
            events,
            CoreEvent::StateEnter {
                state: "solving".into(),
                revision: 0,
                trace_id: sub_trace.clone(),
                sub_spec_index: None,
            },
        )
        .await;
        let solution = match solver.solve(spec, ctx, mem).await {
            Ok(s) => s,
            Err((err, back)) => {
                emit(
                    events,
                    CoreEvent::StateExit {
                        state: "solving".into(),
                        outcome: Outcome::Failed,
                        trace_id: sub_trace,
                        sub_spec_index: None,
                    },
                )
                .await;
                return TransitionResult::diagnosing(ProblemState::Diagnosing { error: err, back });
            }
        };
        emit(
            events,
            CoreEvent::StateExit {
                state: "solving".into(),
                outcome: Outcome::Advanced,
                trace_id: sub_trace.clone(),
                sub_spec_index: None,
            },
        )
        .await;

        emit(
            events,
            CoreEvent::StateEnter {
                state: "executing".into(),
                revision: 0,
                trace_id: sub_trace.clone(),
                sub_spec_index: None,
            },
        )
        .await;
        let result = match solver.execute(solution, ctx, mem).await {
            Ok(r) => r,
            Err((err, back)) => {
                emit(
                    events,
                    CoreEvent::StateExit {
                        state: "executing".into(),
                        outcome: Outcome::Failed,
                        trace_id: sub_trace,
                        sub_spec_index: None,
                    },
                )
                .await;
                return TransitionResult::diagnosing(ProblemState::Diagnosing { error: err, back });
            }
        };
        emit(
            events,
            CoreEvent::StateExit {
                state: "executing".into(),
                outcome: Outcome::Advanced,
                trace_id: sub_trace.clone(),
                sub_spec_index: None,
            },
        )
        .await;

        emit(
            events,
            CoreEvent::SubSpecEnd {
                index,
                trace_id: sub_trace,
            },
        )
        .await;
        results.push(result);
    }

    match solver.merge_results(results) {
        Ok(merged) => TransitionResult::ok_to(ProblemState::Interpreting(merged), "interpreting"),
        Err(err) => {
            let intent_for_back = ctx_intent.expect("intent must be set before specifying");
            TransitionResult::diagnosing(ProblemState::Diagnosing {
                error: err,
                back: BackTarget::Specify(intent_for_back, Default::default()),
            })
        }
    }
}

// ── Concurrent fan-out helpers ───────────────────────────────────────────────

/// Spawn concurrent tasks for each spec and collect results.
///
/// Each sub-spec is retried up to `max_retries` times on failure.  The error
/// message from the failed attempt is injected into [`RetryContext`] so the
/// LLM can correct its SQL on the next attempt.  Only the failed sub-spec is
/// retried; successful ones are kept.
async fn run_fanout_concurrent<D, Ev>(
    worker: Arc<dyn FanoutWorker<D, Ev>>,
    specs: Vec<D::Spec>,
    total: usize,
    fan_trace: String,
    ctx: Arc<RunContext<D>>,
    mem: Arc<SessionMemory<D>>,
    events: Option<EventStream<Ev>>,
    max_retries: u32,
) -> Vec<(usize, Result<D::Result, (D::Error, BackTarget<D>)>)>
where
    D: Domain,
    Ev: DomainEvents,
{
    let mut handles = Vec::with_capacity(total);
    for (index, spec) in specs.into_iter().enumerate() {
        let w = Arc::clone(&worker);
        let ev = events.clone();
        let sub_trace = child_trace_id(&fan_trace, index);
        let ctx = Arc::clone(&ctx);
        let mem = Arc::clone(&mem);

        let sub_span = tracing::info_span!(
            "fanout.sub_spec",
            oxy.name = "fanout.sub_spec",
            sub_spec_index = index,
            total,
        );
        let handle = tokio::spawn(
            async move {
                emit(
                    &ev,
                    CoreEvent::SubSpecStart {
                        index,
                        total,
                        trace_id: sub_trace.clone(),
                    },
                )
                .await;

                // Retry loop: on failure, re-attempt solve+execute with error context.
                let mut retry_ctx: Option<RetryContext> = ctx.retry_ctx.clone();

                for attempt in 0..=max_retries {
                    let attempt_run_ctx = RunContext {
                        intent: ctx.intent.clone(),
                        spec: ctx.spec.clone(),
                        retry_ctx: retry_ctx.clone(),
                    };

                    let result = w
                        .solve_and_execute(spec.clone(), index, total, &ev, &attempt_run_ctx, &mem)
                        .await;

                    match result {
                        Ok(r) => {
                            emit(
                                &ev,
                                CoreEvent::SubSpecEnd {
                                    index,
                                    trace_id: sub_trace,
                                },
                            )
                            .await;
                            return (index, Ok(r));
                        }
                        Err((err, back)) => {
                            if attempt >= max_retries {
                                emit(
                                    &ev,
                                    CoreEvent::SubSpecEnd {
                                        index,
                                        trace_id: sub_trace,
                                    },
                                )
                                .await;
                                return (index, Err((err, back)));
                            }

                            let err_msg = err.to_string();
                            emit(
                                &ev,
                                CoreEvent::BackEdge {
                                    from: "executing".into(),
                                    to: "solving".into(),
                                    reason: err_msg.clone(),
                                    trace_id: sub_trace.clone(),
                                },
                            )
                            .await;

                            retry_ctx = Some(match retry_ctx.take() {
                                Some(existing) => existing.advance(err_msg),
                                None => RetryContext {
                                    errors: vec![err_msg],
                                    attempt: 1,
                                    previous_output: None,
                                },
                            });
                        }
                    }
                }

                unreachable!("retry loop must exit via return")
            }
            .instrument(sub_span),
        );
        handles.push(handle);
    }

    let mut outcomes = Vec::with_capacity(total);
    for handle in handles {
        match handle.await {
            Ok(result) => outcomes.push(result),
            Err(join_err) => {
                // Task panicked — shouldn't happen, but handle gracefully.
                eprintln!("fanout task panicked: {join_err}");
            }
        }
    }
    outcomes
}

/// Collect concurrent fan-out results: merge successes or route to diagnosing.
fn collect_fanout_results<D, S>(
    solver: &S,
    outcomes: Vec<(usize, Result<D::Result, (D::Error, BackTarget<D>)>)>,
    ctx_intent: Option<D::Intent>,
) -> TransitionResult<D>
where
    D: Domain,
    S: DomainSolver<D>,
{
    let mut successes = Vec::new();
    let mut first_error: Option<(D::Error, BackTarget<D>)> = None;

    for (_index, result) in outcomes {
        match result {
            Ok(r) => successes.push(r),
            Err((err, back)) => {
                if first_error.is_none() {
                    first_error = Some((err, back));
                }
                // Continue collecting — we let all tasks finish.
            }
        }
    }

    if let Some((err, back)) = first_error {
        // At least one sub-spec failed. Route to diagnosing with the first error.
        return TransitionResult::diagnosing(ProblemState::Diagnosing { error: err, back });
    }

    match solver.merge_results(successes) {
        Ok(merged) => TransitionResult::ok_to(ProblemState::Interpreting(merged), "interpreting"),
        Err(err) => {
            let intent_for_back = ctx_intent.expect("intent must be set before specifying");
            TransitionResult::diagnosing(ProblemState::Diagnosing {
                error: err,
                back: BackTarget::Specify(intent_for_back, Default::default()),
            })
        }
    }
}
