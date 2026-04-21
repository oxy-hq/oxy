//! Default table of [`StateHandler`]s — one per pipeline state.

use std::collections::HashMap;
use std::sync::Arc;

use crate::domain::Domain;
use crate::events::DomainEvents;
use crate::solver::DomainSolver;
use crate::state::ProblemState;

use super::{StateHandler, TransitionResult};

/// Build the default set of state handlers that delegate to the corresponding
/// [`DomainSolver`] methods.
///
/// On failure each handler wraps the `(error, BackTarget)` pair in
/// `ProblemState::Diagnosing` and passes it through the legacy Diagnosing arm
/// in the orchestrator loop, which in turn calls `DomainSolver::diagnose`.
pub fn build_default_handlers<D, S, Ev>() -> HashMap<&'static str, StateHandler<D, S, Ev>>
where
    D: Domain + 'static,
    S: DomainSolver<D> + 'static,
    Ev: DomainEvents,
{
    let mut map: HashMap<&'static str, StateHandler<D, S, Ev>> = HashMap::new();

    // ── clarifying ────────────────────────────────────────────────────────────
    map.insert(
        "clarifying",
        StateHandler {
            next: "specifying",
            execute: Arc::new(|solver, state, _events, run_ctx, memory| {
                Box::pin(async move {
                    let data = match state {
                        ProblemState::Clarifying(d) => d,
                        _ => unreachable!("clarifying handler called with wrong state"),
                    };
                    match solver.clarify(data, run_ctx, memory).await {
                        Ok(output) => TransitionResult::ok(ProblemState::Specifying(output)),
                        Err((err, back)) => {
                            TransitionResult::diagnosing(ProblemState::Diagnosing {
                                error: err,
                                back,
                            })
                        }
                    }
                })
            }),
            diagnose: None,
        },
    );

    // ── specifying ────────────────────────────────────────────────────────────
    map.insert(
        "specifying",
        StateHandler {
            next: "solving",
            execute: Arc::new(|solver, state, _events, run_ctx, memory| {
                // Extract intent once so it can be used in multiple error branches.
                let ctx_intent = run_ctx.intent.clone();
                Box::pin(async move {
                    let data = match state {
                        ProblemState::Specifying(d) => d,
                        _ => unreachable!("specifying handler called with wrong state"),
                    };
                    match solver.specify(data, run_ctx, memory).await {
                        Ok(specs) if specs.len() == 1 => {
                            // Fast path: single spec → standard Solving transition.
                            TransitionResult::ok(ProblemState::Solving(
                                specs.into_iter().next().unwrap(),
                            ))
                        }
                        Ok(specs) if specs.is_empty() => {
                            // Empty specs is an error — specify must return at least one.
                            // Use the empty-vec sentinel so the orchestrator retries.
                            let intent_for_back = ctx_intent
                                .clone()
                                .expect("intent must be set before specifying");
                            TransitionResult {
                                state_data: ProblemState::Specifying(intent_for_back),
                                errors: Some(vec![]),
                                next_stage: None,
                                fan_out: None,
                            }
                        }
                        Ok(specs) => {
                            // Fan-out: yield control back to orchestrator so that
                            // StateExit for "specifying" fires before any sub-spec
                            // work begins.  The orchestrator calls run_fanout after
                            // emitting the exit event.
                            let intent_placeholder =
                                ctx_intent.expect("intent must be set before specifying");
                            TransitionResult::pending_fan_out(
                                specs,
                                ProblemState::Specifying(intent_placeholder),
                            )
                        }
                        Err((err, back)) => {
                            TransitionResult::diagnosing(ProblemState::Diagnosing {
                                error: err,
                                back,
                            })
                        }
                    }
                })
            }),
            diagnose: None,
        },
    );

    // ── solving ───────────────────────────────────────────────────────────────
    map.insert(
        "solving",
        StateHandler {
            next: "executing",
            execute: Arc::new(|solver, state, _events, run_ctx, memory| {
                Box::pin(async move {
                    let data = match state {
                        ProblemState::Solving(d) => d,
                        _ => unreachable!("solving handler called with wrong state"),
                    };
                    match solver.solve(data, run_ctx, memory).await {
                        Ok(output) => TransitionResult::ok(ProblemState::Executing(output)),
                        Err((err, back)) => {
                            TransitionResult::diagnosing(ProblemState::Diagnosing {
                                error: err,
                                back,
                            })
                        }
                    }
                })
            }),
            diagnose: None,
        },
    );

    // ── executing ─────────────────────────────────────────────────────────────
    map.insert(
        "executing",
        StateHandler {
            next: "interpreting",
            execute: Arc::new(|solver, state, _events, run_ctx, memory| {
                Box::pin(async move {
                    let data = match state {
                        ProblemState::Executing(d) => d,
                        _ => unreachable!("executing handler called with wrong state"),
                    };
                    match solver.execute(data, run_ctx, memory).await {
                        Ok(output) => TransitionResult::ok(ProblemState::Interpreting(output)),
                        Err((err, back)) => {
                            TransitionResult::diagnosing(ProblemState::Diagnosing {
                                error: err,
                                back,
                            })
                        }
                    }
                })
            }),
            diagnose: None,
        },
    );

    // ── interpreting ──────────────────────────────────────────────────────────
    map.insert(
        "interpreting",
        StateHandler {
            next: "done",
            execute: Arc::new(|solver, state, _events, run_ctx, memory| {
                Box::pin(async move {
                    let data = match state {
                        ProblemState::Interpreting(d) => d,
                        _ => unreachable!("interpreting handler called with wrong state"),
                    };
                    match solver.interpret(data, run_ctx, memory).await {
                        Ok(output) => TransitionResult::ok(ProblemState::Done(output)),
                        Err((err, back)) => {
                            TransitionResult::diagnosing(ProblemState::Diagnosing {
                                error: err,
                                back,
                            })
                        }
                    }
                })
            }),
            diagnose: None,
        },
    );

    map
}
