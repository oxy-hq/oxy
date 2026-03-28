//! Integration tests for [`Orchestrator`] using a minimal mock domain whose
//! associated types are all simple string-based values.

use std::sync::Arc;

use agentic_core::{
    BackTarget, CoreEvent, Domain, DomainEvents, DomainSolver, Event, EventStream, Orchestrator,
    OrchestratorError, ProblemState,
};
use async_trait::async_trait;
use tokio::sync::mpsc;

// ── Mock domain ───────────────────────────────────────────────────────────────

struct MockDomain;

/// The spec for the mock domain.
#[derive(Clone, Debug)]
struct MockSpec {
    intent: String,
    requirements: Vec<String>,
}

impl Domain for MockDomain {
    type Intent = String;
    type Spec = MockSpec;
    type Solution = Vec<String>;
    type Result = String;
    type Answer = String;
    type Catalog = ();
    type Error = String;
}

// ── Helper: assert solver call counts ────────────────────────────────────────

#[derive(Default, Debug)]
struct CallCounts {
    clarify: u32,
    specify: u32,
    solve: u32,
    execute: u32,
    interpret: u32,
    diagnose: u32,
}

// ═════════════════════════════════════════════════════════════════════════════
// 1. Happy path — every stage succeeds on the first attempt
// ═════════════════════════════════════════════════════════════════════════════

struct HappySolver {
    calls: CallCounts,
}

impl HappySolver {
    fn new() -> Self {
        Self {
            calls: Default::default(),
        }
    }
}

#[async_trait]
impl DomainSolver<MockDomain> for HappySolver {
    async fn clarify(
        &mut self,
        intent: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        self.calls.clarify += 1;
        Ok(format!("clarified: {intent}"))
    }

    async fn specify_single(
        &mut self,
        intent: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<MockSpec, (String, BackTarget<MockDomain>)> {
        self.calls.specify += 1;
        Ok(MockSpec {
            intent,
            requirements: vec!["req-A".into(), "req-B".into()],
        })
    }

    async fn solve(
        &mut self,
        spec: MockSpec,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<Vec<String>, (String, BackTarget<MockDomain>)> {
        self.calls.solve += 1;
        Ok(vec![format!("step-1 for {}", spec.intent), "step-2".into()])
    }

    async fn execute(
        &mut self,
        solution: Vec<String>,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        self.calls.execute += 1;
        Ok(solution.join(", "))
    }

    async fn interpret(
        &mut self,
        result: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        self.calls.interpret += 1;
        Ok(format!("answer: {result}"))
    }

    async fn diagnose(
        &mut self,
        error: String,
        _back: BackTarget<MockDomain>,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
    ) -> Result<ProblemState<MockDomain>, String> {
        self.calls.diagnose += 1;
        Err(error) // should never be reached on the happy path
    }
}

#[tokio::test]
async fn happy_path_returns_answer() {
    let mut orch = Orchestrator::<MockDomain, _>::new(HappySolver::new());
    let answer = orch.run("sort a list".into()).await.unwrap();
    assert_eq!(answer, "answer: step-1 for clarified: sort a list, step-2");
}

#[tokio::test]
async fn happy_path_calls_each_stage_exactly_once() {
    let mut orch = Orchestrator::<MockDomain, _>::new(HappySolver::new());
    orch.run("sum a list".into()).await.unwrap();

    let counts = &orch.solver().calls;
    assert_eq!(counts.clarify, 1);
    assert_eq!(counts.specify, 1);
    assert_eq!(counts.solve, 1);
    assert_eq!(counts.execute, 1);
    assert_eq!(counts.interpret, 1);
    assert_eq!(counts.diagnose, 0);
}

// ═════════════════════════════════════════════════════════════════════════════
// 2. Back-edge: Clarify → Clarify (intent needs a second pass)
// ═════════════════════════════════════════════════════════════════════════════

struct RetryClarifySolver {
    clarify_calls: u32,
}

#[async_trait]
impl DomainSolver<MockDomain> for RetryClarifySolver {
    async fn clarify(
        &mut self,
        intent: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        self.clarify_calls += 1;
        if self.clarify_calls < 3 {
            // Signal "not ready yet" — send the intent back to Clarify.
            Err((
                "ambiguous intent".into(),
                BackTarget::Clarify(intent, Default::default()),
            ))
        } else {
            Ok(format!("clarified({}): {intent}", self.clarify_calls))
        }
    }

    async fn specify_single(
        &mut self,
        intent: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<MockSpec, (String, BackTarget<MockDomain>)> {
        Ok(MockSpec {
            intent,
            requirements: vec![],
        })
    }

    async fn solve(
        &mut self,
        spec: MockSpec,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<Vec<String>, (String, BackTarget<MockDomain>)> {
        Ok(vec![spec.intent])
    }

    async fn execute(
        &mut self,
        solution: Vec<String>,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(solution.join("|"))
    }

    async fn interpret(
        &mut self,
        result: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(result)
    }

    async fn diagnose(
        &mut self,
        _error: String,
        back: BackTarget<MockDomain>,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
    ) -> Result<ProblemState<MockDomain>, String> {
        Ok(match back {
            BackTarget::Clarify(intent, _) => ProblemState::Clarifying(intent),
            _ => unreachable!("unexpected back target in retry-clarify test"),
        })
    }
}

#[tokio::test]
async fn back_edge_clarify_retries_until_success() {
    let mut orch = Orchestrator::<MockDomain, _>::new(RetryClarifySolver { clarify_calls: 0 });
    let answer = orch.run("find duplicates".into()).await.unwrap();
    // clarify succeeds on the 3rd call; answer carries the call count
    assert!(
        answer.contains("clarified(3)"),
        "expected clarified(3) in answer, got: {answer}"
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// 3. Back-edge: Solve → Specify using HasIntent
//    Demonstrates that the spec's HasIntent impl lets the solver recover the
//    intent without storing it anywhere outside the spec.
// ═════════════════════════════════════════════════════════════════════════════

struct SolveBackToSpecifySolver {
    solve_calls: u32,
    specify_calls: u32,
}

#[async_trait]
impl DomainSolver<MockDomain> for SolveBackToSpecifySolver {
    async fn clarify(
        &mut self,
        intent: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(intent)
    }

    async fn specify_single(
        &mut self,
        intent: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<MockSpec, (String, BackTarget<MockDomain>)> {
        self.specify_calls += 1;
        Ok(MockSpec {
            requirements: vec![format!("req-v{}", self.specify_calls)],
            intent,
        })
    }

    async fn solve(
        &mut self,
        spec: MockSpec,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<Vec<String>, (String, BackTarget<MockDomain>)> {
        self.solve_calls += 1;
        if self.solve_calls < 2 {
            // Use HasIntent to extract the intent from the spec.
            // This is the primary motivation for the HasIntent constraint.
            let recovered_intent = spec.intent.clone();
            Err((
                "spec insufficient, re-specifying".into(),
                BackTarget::Specify(recovered_intent, Default::default()),
            ))
        } else {
            Ok(vec![format!("solution(spec-v{})", self.specify_calls)])
        }
    }

    async fn execute(
        &mut self,
        solution: Vec<String>,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(solution.join(","))
    }

    async fn interpret(
        &mut self,
        result: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(result)
    }

    async fn diagnose(
        &mut self,
        _error: String,
        back: BackTarget<MockDomain>,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
    ) -> Result<ProblemState<MockDomain>, String> {
        Ok(match back {
            BackTarget::Specify(intent, _) => ProblemState::Specifying(intent),
            _ => unreachable!("unexpected back target in solve-to-specify test"),
        })
    }
}

#[tokio::test]
async fn back_edge_solve_to_specify_via_has_intent() {
    let mut orch = Orchestrator::<MockDomain, _>::new(SolveBackToSpecifySolver {
        solve_calls: 0,
        specify_calls: 0,
    });
    let answer = orch.run("hard problem".into()).await.unwrap();
    // specify is called twice (once originally, once after the back-edge)
    assert_eq!(answer, "solution(spec-v2)");
    assert_eq!(orch.solver().specify_calls, 2);
    assert_eq!(orch.solver().solve_calls, 2);
}

// ═════════════════════════════════════════════════════════════════════════════
// 4. Back-edge: Execute → Solve (transient execution failure)
// ═════════════════════════════════════════════════════════════════════════════

struct ExecuteBackToSolveSolver {
    execute_calls: u32,
}

#[async_trait]
impl DomainSolver<MockDomain> for ExecuteBackToSolveSolver {
    async fn clarify(
        &mut self,
        intent: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(intent)
    }

    async fn specify_single(
        &mut self,
        intent: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<MockSpec, (String, BackTarget<MockDomain>)> {
        Ok(MockSpec {
            intent,
            requirements: vec![],
        })
    }

    async fn solve(
        &mut self,
        spec: MockSpec,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<Vec<String>, (String, BackTarget<MockDomain>)> {
        Ok(vec![
            format!("plan({})", self.execute_calls + 1),
            spec.intent,
        ])
    }

    async fn execute(
        &mut self,
        solution: Vec<String>,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        self.execute_calls += 1;
        if self.execute_calls < 2 {
            Err((
                "transient failure".into(),
                BackTarget::Solve(
                    MockSpec {
                        intent: solution[1].clone(),
                        requirements: vec![],
                    },
                    Default::default(),
                ),
            ))
        } else {
            Ok(format!(
                "executed after {} attempts: {}",
                self.execute_calls, solution[0]
            ))
        }
    }

    async fn interpret(
        &mut self,
        result: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(result)
    }

    async fn diagnose(
        &mut self,
        _error: String,
        back: BackTarget<MockDomain>,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
    ) -> Result<ProblemState<MockDomain>, String> {
        Ok(match back {
            BackTarget::Solve(spec, _) => ProblemState::Solving(spec),
            _ => unreachable!(),
        })
    }
}

#[tokio::test]
async fn back_edge_execute_to_solve_on_transient_failure() {
    let mut orch =
        Orchestrator::<MockDomain, _>::new(ExecuteBackToSolveSolver { execute_calls: 0 });
    let answer = orch.run("deploy service".into()).await.unwrap();
    assert!(
        answer.contains("executed after 2 attempts"),
        "got: {answer}"
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// 5. Fatal error — diagnose propagates Err
// ═════════════════════════════════════════════════════════════════════════════

struct FatalSolver;

#[async_trait]
impl DomainSolver<MockDomain> for FatalSolver {
    async fn clarify(
        &mut self,
        intent: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Err((
            "unrecoverable".into(),
            BackTarget::Clarify(intent, Default::default()),
        ))
    }

    async fn specify_single(
        &mut self,
        intent: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<MockSpec, (String, BackTarget<MockDomain>)> {
        Ok(MockSpec {
            intent,
            requirements: vec![],
        })
    }

    async fn solve(
        &mut self,
        spec: MockSpec,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<Vec<String>, (String, BackTarget<MockDomain>)> {
        Ok(vec![spec.intent])
    }

    async fn execute(
        &mut self,
        solution: Vec<String>,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(solution.join(","))
    }

    async fn interpret(
        &mut self,
        result: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(result)
    }

    async fn diagnose(
        &mut self,
        error: String,
        _back: BackTarget<MockDomain>,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
    ) -> Result<ProblemState<MockDomain>, String> {
        Err(error) // propagate the error as fatal
    }
}

#[tokio::test]
async fn fatal_error_from_diagnose_terminates_run() {
    let mut orch = Orchestrator::<MockDomain, _>::new(FatalSolver);
    let err = orch.run("doomed task".into()).await.unwrap_err();
    assert_eq!(err, OrchestratorError::Fatal("unrecoverable".into()));
}

// ═════════════════════════════════════════════════════════════════════════════
// 6. Max iterations guard — prevents runaway back-edge cycles
// ═════════════════════════════════════════════════════════════════════════════

struct InfiniteLoopSolver;

#[async_trait]
impl DomainSolver<MockDomain> for InfiniteLoopSolver {
    async fn clarify(
        &mut self,
        intent: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(intent)
    }

    async fn specify_single(
        &mut self,
        intent: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<MockSpec, (String, BackTarget<MockDomain>)> {
        // Always fail back to Specify — creates an infinite loop.
        Err((
            "forever stuck".into(),
            BackTarget::Specify(intent, Default::default()),
        ))
    }

    async fn solve(
        &mut self,
        spec: MockSpec,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<Vec<String>, (String, BackTarget<MockDomain>)> {
        Ok(vec![spec.intent])
    }

    async fn execute(
        &mut self,
        solution: Vec<String>,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(solution.join(","))
    }

    async fn interpret(
        &mut self,
        result: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(result)
    }

    async fn diagnose(
        &mut self,
        _error: String,
        back: BackTarget<MockDomain>,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
    ) -> Result<ProblemState<MockDomain>, String> {
        Ok(match back {
            BackTarget::Specify(intent, _) => ProblemState::Specifying(intent),
            BackTarget::Clarify(intent, _) => ProblemState::Clarifying(intent),
            _ => unreachable!(),
        })
    }
}

#[tokio::test]
async fn max_iterations_exceeded_terminates_loop() {
    let mut orch = Orchestrator::<MockDomain, _>::with_max_iterations(InfiniteLoopSolver, 10);
    let err = orch.run("loop forever".into()).await.unwrap_err();
    assert_eq!(err, OrchestratorError::<MockDomain>::MaxIterationsExceeded);
}

// ═════════════════════════════════════════════════════════════════════════════
// 7. into_solver — orchestrator yields solver after the run
// ═════════════════════════════════════════════════════════════════════════════

struct CountingSolver {
    runs: u32,
}

#[async_trait]
impl DomainSolver<MockDomain> for CountingSolver {
    async fn clarify(
        &mut self,
        i: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        self.runs += 1;
        Ok(i)
    }
    async fn specify_single(
        &mut self,
        i: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<MockSpec, (String, BackTarget<MockDomain>)> {
        Ok(MockSpec {
            intent: i,
            requirements: vec![],
        })
    }
    async fn solve(
        &mut self,
        s: MockSpec,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<Vec<String>, (String, BackTarget<MockDomain>)> {
        Ok(vec![s.intent])
    }
    async fn execute(
        &mut self,
        s: Vec<String>,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(s.join(""))
    }
    async fn interpret(
        &mut self,
        r: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(r)
    }
    async fn diagnose(
        &mut self,
        e: String,
        _: BackTarget<MockDomain>,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
    ) -> Result<ProblemState<MockDomain>, String> {
        Err(e)
    }
}

#[tokio::test]
async fn into_solver_returns_solver_with_accumulated_state() {
    let mut orch = Orchestrator::<MockDomain, _>::new(CountingSolver { runs: 0 });
    orch.run("task".into()).await.unwrap();
    let solver = orch.into_solver();
    assert_eq!(solver.runs, 1);
}

// ═════════════════════════════════════════════════════════════════════════════
// Event-system tests
// ═════════════════════════════════════════════════════════════════════════════

// ── Helper: drain a channel into a Vec ───────────────────────────────────────

async fn drain<Ev: DomainEvents>(rx: &mut mpsc::Receiver<Event<Ev>>) -> Vec<Event<Ev>> {
    let mut events = Vec::new();
    while let Ok(e) = rx.try_recv() {
        events.push(e);
    }
    events
}

// ═════════════════════════════════════════════════════════════════════════════
// 8. Event stream — happy path produces correct StateEnter/StateExit sequence
// ═════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn event_stream_happy_path_state_sequence() {
    let (tx, mut rx) = mpsc::channel::<Event<()>>(256);

    let mut orch = Orchestrator::<MockDomain, _>::new(HappySolver::new()).with_events(tx);

    orch.run("task".into()).await.unwrap();

    let events = drain(&mut rx).await;

    // Collect just the state names from StateEnter events in order.
    let entered: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            Event::Core(agentic_core::CoreEvent::StateEnter { state, .. }) => Some(state.clone()),
            _ => None,
        })
        .collect();

    assert_eq!(
        entered,
        [
            "clarifying",
            "specifying",
            "solving",
            "executing",
            "interpreting"
        ]
    );

    // Each entered state must have a matching StateExit with outcome Advanced.
    let exited: Vec<(String, agentic_core::Outcome)> = events
        .iter()
        .filter_map(|e| match e {
            Event::Core(agentic_core::CoreEvent::StateExit { state, outcome, .. }) => {
                Some((state.clone(), outcome.clone()))
            }
            _ => None,
        })
        .collect();

    assert_eq!(exited.len(), 5);
    assert!(exited
        .iter()
        .all(|(_, o)| *o == agentic_core::Outcome::Advanced));

    // There must be a terminal Done event.
    let has_done = events
        .iter()
        .any(|e| matches!(e, Event::Core(agentic_core::CoreEvent::Done { .. })));
    assert!(has_done, "expected a Done event");
}

// ═════════════════════════════════════════════════════════════════════════════
// 9. Event stream — StateEnter revision increments on re-entry
// ═════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn event_stream_revision_increments_on_reentry() {
    let (tx, mut rx) = mpsc::channel::<Event<()>>(256);

    // RetryClarifySolver retries Clarify twice before succeeding.
    let mut orch =
        Orchestrator::<MockDomain, _>::new(RetryClarifySolver { clarify_calls: 0 }).with_events(tx);

    orch.run("task".into()).await.unwrap();

    let events = drain(&mut rx).await;

    let clarify_revisions: Vec<u32> = events
        .iter()
        .filter_map(|e| match e {
            Event::Core(agentic_core::CoreEvent::StateEnter {
                state, revision, ..
            }) if state == "clarifying" => Some(*revision),
            _ => None,
        })
        .collect();

    // Clarify entered 3 times: revision 0, 1, 2.
    assert_eq!(clarify_revisions, [0, 1, 2]);
}

// ═════════════════════════════════════════════════════════════════════════════
// 10. Event stream — back-edge produces BackEdge event with correct from/to
// ═════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn event_stream_back_edge_emits_back_edge_event() {
    let (tx, mut rx) = mpsc::channel::<Event<()>>(256);

    let mut orch = Orchestrator::<MockDomain, _>::new(SolveBackToSpecifySolver {
        solve_calls: 0,
        specify_calls: 0,
    })
    .with_events(tx);

    orch.run("task".into()).await.unwrap();

    let events = drain(&mut rx).await;

    let back_edges: Vec<(String, String)> = events
        .iter()
        .filter_map(|e| match e {
            Event::Core(agentic_core::CoreEvent::BackEdge { from, to, .. }) => {
                Some((from.clone(), to.clone()))
            }
            _ => None,
        })
        .collect();

    assert_eq!(back_edges.len(), 1);
    assert_eq!(back_edges[0].0, "solving");
    assert_eq!(back_edges[0].1, "specifying");
}

// ═════════════════════════════════════════════════════════════════════════════
// 11. Event stream — retry back-edge (same stage) uses Outcome::Retry
// ═════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn event_stream_retry_uses_retry_outcome() {
    let (tx, mut rx) = mpsc::channel::<Event<()>>(256);

    // RetryClarifySolver loops Clarify → Diagnose → Clarify twice.
    let mut orch =
        Orchestrator::<MockDomain, _>::new(RetryClarifySolver { clarify_calls: 0 }).with_events(tx);

    orch.run("task".into()).await.unwrap();

    let events = drain(&mut rx).await;

    // The two Clarify failures must produce Retry outcomes (same stage → same stage).
    let retry_exits: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::Core(agentic_core::CoreEvent::StateExit { state, outcome, .. })
                if state == "clarifying" && *outcome == agentic_core::Outcome::Retry =>
            {
                Some(())
            }
            _ => None,
        })
        .collect();

    assert_eq!(
        retry_exits.len(),
        2,
        "expected 2 Retry exits for clarifying"
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// 12. Event stream — domain events travel on the same channel
// ═════════════════════════════════════════════════════════════════════════════

/// A trivial domain event to exercise the generic domain-event path.
#[derive(Debug)]
enum MockDomainEvent {
    TaskStarted { label: String },
}

impl DomainEvents for MockDomainEvent {}

/// A solver that emits a domain event during `clarify`.
struct DomainEventSolver {
    event_tx: Option<EventStream<MockDomainEvent>>,
}

#[async_trait]
impl DomainSolver<MockDomain> for DomainEventSolver {
    async fn clarify(
        &mut self,
        intent: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        if let Some(tx) = &self.event_tx {
            let _ = tx
                .send(Event::Domain(MockDomainEvent::TaskStarted {
                    label: intent.clone(),
                }))
                .await;
        }
        Ok(intent)
    }

    async fn specify_single(
        &mut self,
        i: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<MockSpec, (String, BackTarget<MockDomain>)> {
        Ok(MockSpec {
            intent: i,
            requirements: vec![],
        })
    }

    async fn solve(
        &mut self,
        s: MockSpec,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<Vec<String>, (String, BackTarget<MockDomain>)> {
        Ok(vec![s.intent])
    }

    async fn execute(
        &mut self,
        s: Vec<String>,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(s.join(""))
    }

    async fn interpret(
        &mut self,
        r: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(r)
    }

    async fn diagnose(
        &mut self,
        e: String,
        _: BackTarget<MockDomain>,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
    ) -> Result<ProblemState<MockDomain>, String> {
        Err(e)
    }
}

#[tokio::test]
async fn event_stream_domain_events_travel_on_same_channel() {
    let (tx, mut rx) = mpsc::channel::<Event<MockDomainEvent>>(256);

    let solver = DomainEventSolver {
        event_tx: Some(tx.clone()),
    };
    let mut orch = Orchestrator::<MockDomain, _, MockDomainEvent>::new(solver).with_events(tx);

    orch.run("hello".into()).await.unwrap();

    let events = drain(&mut rx).await;

    // There should be at least one Domain event.
    let domain_labels: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            Event::Domain(MockDomainEvent::TaskStarted { label }) => Some(label.clone()),
            _ => None,
        })
        .collect();

    assert_eq!(domain_labels, ["hello"]);

    // Core events must also be present on the same channel.
    let has_core = events.iter().any(|e| matches!(e, Event::Core(_)));
    assert!(
        has_core,
        "core events must travel on the same channel as domain events"
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// 13. Event stream — events arrive in chronological order
// ═════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn event_stream_events_arrive_in_order() {
    let (tx, mut rx) = mpsc::channel::<Event<()>>(256);

    let mut orch = Orchestrator::<MockDomain, _>::new(HappySolver::new()).with_events(tx);
    orch.run("ordered".into()).await.unwrap();

    let events = drain(&mut rx).await;

    // Build a simplified sequence: "enter:X" / "exit:X" / "done".
    let seq: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            Event::Core(agentic_core::CoreEvent::StateEnter { state, .. }) => {
                Some(format!("enter:{state}"))
            }
            Event::Core(agentic_core::CoreEvent::StateExit { state, .. }) => {
                Some(format!("exit:{state}"))
            }
            Event::Core(agentic_core::CoreEvent::Done { .. }) => Some("done".into()),
            _ => None,
        })
        .collect();

    let expected = [
        "enter:clarifying",
        "exit:clarifying",
        "enter:specifying",
        "exit:specifying",
        "enter:solving",
        "exit:solving",
        "enter:executing",
        "exit:executing",
        "enter:interpreting",
        "exit:interpreting",
        "done",
    ];

    assert_eq!(seq, expected);
}

// ═════════════════════════════════════════════════════════════════════════════
// 14. Static skip — SKIP_STATES bypasses solving without calling solve()
// ═════════════════════════════════════════════════════════════════════════════

/// A solver that statically declares "solving" as a never-used stage.
///
/// `should_skip` transforms `Solving(MockSpec)` → `Executing(Vec<String>)`
/// directly, bypassing the LLM-style solve step.
struct StaticSkipSolver {
    solve_calls: u32,
    execute_calls: u32,
}

#[async_trait]
impl DomainSolver<MockDomain> for StaticSkipSolver {
    const SKIP_STATES: &'static [&'static str] = &["solving"];

    async fn clarify(
        &mut self,
        intent: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(format!("clarified:{intent}"))
    }

    async fn specify_single(
        &mut self,
        intent: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<MockSpec, (String, BackTarget<MockDomain>)> {
        Ok(MockSpec {
            intent,
            requirements: vec!["static-skip-req".into()],
        })
    }

    async fn solve(
        &mut self,
        spec: MockSpec,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<Vec<String>, (String, BackTarget<MockDomain>)> {
        // Should never be reached when the state is statically skipped.
        self.solve_calls += 1;
        Ok(vec![format!("solved:{}", spec.intent)])
    }

    async fn execute(
        &mut self,
        solution: Vec<String>,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        self.execute_calls += 1;
        Ok(solution.join("|"))
    }

    async fn interpret(
        &mut self,
        result: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(format!("answer:{result}"))
    }

    async fn diagnose(
        &mut self,
        error: String,
        _back: BackTarget<MockDomain>,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
    ) -> Result<ProblemState<MockDomain>, String> {
        Err(error)
    }

    fn should_skip(
        &mut self,
        state: &str,
        data: &ProblemState<MockDomain>,
        _run_ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
    ) -> Option<ProblemState<MockDomain>> {
        if state == "solving" {
            if let ProblemState::Solving(spec) = data {
                // Produce the Executing state directly from the spec,
                // bypassing the LLM-based solve step.
                return Some(ProblemState::Executing(vec![format!(
                    "skipped-solve:{}",
                    spec.intent
                )]));
            }
        }
        None
    }
}

#[tokio::test]
async fn static_skip_states_bypasses_solving() {
    let mut orch = Orchestrator::<MockDomain, _>::new(StaticSkipSolver {
        solve_calls: 0,
        execute_calls: 0,
    });
    let answer = orch.run("skip-test".into()).await.unwrap();

    // solve must never be called — the state was statically skipped.
    assert_eq!(
        orch.solver().solve_calls,
        0,
        "solve must not be called for SKIP_STATES"
    );
    // execute must be called once with the skip-generated solution.
    assert_eq!(orch.solver().execute_calls, 1);
    // The answer must carry the skip marker rather than a real solve output.
    assert!(
        answer.contains("skipped-solve"),
        "answer must contain skip marker, got: {answer}",
    );
}

#[tokio::test]
async fn static_skip_emits_no_state_enter_exit_for_skipped_state() {
    let (tx, mut rx) = mpsc::channel::<Event<()>>(256);
    let mut orch = Orchestrator::<MockDomain, _, ()>::new(StaticSkipSolver {
        solve_calls: 0,
        execute_calls: 0,
    })
    .with_events(tx);

    orch.run("skip-events-test".into()).await.unwrap();

    let events = drain(&mut rx).await;

    // "solving" must not appear in any StateEnter event.
    let solving_enters: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::Core(agentic_core::CoreEvent::StateEnter { state, .. })
                if state == "solving" =>
            {
                Some(state.clone())
            }
            _ => None,
        })
        .collect();
    assert!(
        solving_enters.is_empty(),
        "no StateEnter must be emitted for a skipped state, got: {solving_enters:?}",
    );

    // The non-skipped states must still appear in order.
    let entered: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            Event::Core(agentic_core::CoreEvent::StateEnter { state, .. }) => Some(state.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        entered,
        ["clarifying", "specifying", "executing", "interpreting"],
        "entered states must skip 'solving'",
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// Multi-turn session memory
// ═════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn orchestrator_stores_completed_turns_in_memory() {
    let mut orch = Orchestrator::<MockDomain, _>::new(HappySolver::new());

    let _answer1 = orch.run("first question".into()).await.unwrap();
    assert_eq!(orch.memory().len(), 1);
    assert_eq!(orch.memory().turns()[0].intent, "clarified: first question");
    assert_eq!(
        orch.memory().turns()[0].answer,
        "answer: step-1 for clarified: first question, step-2"
    );

    let _answer2 = orch.run("second question".into()).await.unwrap();
    assert_eq!(orch.memory().len(), 2);
    assert_eq!(
        orch.memory().turns()[1].intent,
        "clarified: second question"
    );

    orch.clear_memory();
    assert!(orch.memory().is_empty());
}

#[tokio::test]
async fn session_memory_respects_max_turns() {
    let mut orch = Orchestrator::<MockDomain, _>::new(HappySolver::new()).with_max_memory_turns(2);

    orch.run("q1".into()).await.unwrap();
    orch.run("q2".into()).await.unwrap();
    orch.run("q3".into()).await.unwrap();

    assert_eq!(
        orch.memory().len(),
        2,
        "oldest turn should have been evicted"
    );
    // q1 was evicted; q2 is now first.
    assert_eq!(orch.memory().turns()[0].intent, "clarified: q2");
    assert_eq!(orch.memory().turns()[1].intent, "clarified: q3");
}

#[tokio::test]
async fn memory_turn_carries_trace_id() {
    let mut orch = Orchestrator::<MockDomain, _>::new(HappySolver::new());
    orch.run("trace test".into()).await.unwrap();
    let turn = &orch.memory().turns()[0];
    assert!(!turn.trace_id.is_empty());
    assert!(turn.trace_id.starts_with("trace-"));
}

#[tokio::test]
async fn memory_cleared_between_topics() {
    let mut orch = Orchestrator::<MockDomain, _>::new(HappySolver::new());
    orch.run("topic A".into()).await.unwrap();
    orch.run("still topic A".into()).await.unwrap();
    assert_eq!(orch.memory().len(), 2);

    orch.clear_memory();
    orch.run("new topic B".into()).await.unwrap();
    assert_eq!(orch.memory().len(), 1, "clearing should reset the counter");
    assert_eq!(orch.memory().turns()[0].intent, "clarified: new topic B");
}

// ═════════════════════════════════════════════════════════════════════════════
// run_pipeline refactor — memory is NOT touched by run_pipeline
// ═════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn run_pipeline_does_not_push_to_memory() {
    let mut orch = Orchestrator::<MockDomain, _>::new(HappySolver::new());

    // run_pipeline should return a PipelineOutput without touching memory.
    let output = orch
        .run_pipeline("pipeline test".into(), "test-trace-1")
        .await
        .unwrap();
    assert_eq!(
        output.answer,
        "answer: step-1 for clarified: pipeline test, step-2"
    );
    assert_eq!(output.intent, "clarified: pipeline test");
    assert!(output.spec.is_some());

    // Memory must still be empty — run_pipeline does NOT push.
    assert!(
        orch.memory().is_empty(),
        "run_pipeline must not touch session memory"
    );
}

#[tokio::test]
async fn run_pushes_to_memory_while_run_pipeline_does_not() {
    let mut orch = Orchestrator::<MockDomain, _>::new(HappySolver::new());

    // run_pipeline: no memory push.
    let _out = orch
        .run_pipeline("via pipeline".into(), "trace-p")
        .await
        .unwrap();
    assert_eq!(orch.memory().len(), 0);

    // run: pushes to memory.
    let _ans = orch.run("via run".into()).await.unwrap();
    assert_eq!(orch.memory().len(), 1);
    assert_eq!(orch.memory().turns()[0].intent, "clarified: via run");
}

#[tokio::test]
async fn run_pipeline_uses_provided_trace_id_in_events() {
    let (tx, mut rx) = mpsc::channel::<Event<()>>(256);
    let mut orch = Orchestrator::<MockDomain, _>::new(HappySolver::new()).with_events(tx);

    let custom_trace = "parent-trace-42.0";
    orch.run_pipeline("sub question".into(), custom_trace)
        .await
        .unwrap();

    let events = drain(&mut rx).await;

    // All StateEnter events must carry our custom trace ID.
    let trace_ids: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            Event::Core(agentic_core::CoreEvent::StateEnter { trace_id, .. }) => {
                Some(trace_id.clone())
            }
            _ => None,
        })
        .collect();

    assert!(!trace_ids.is_empty(), "must have StateEnter events");
    assert!(
        trace_ids.iter().all(|t| t == custom_trace),
        "all events must carry the supplied trace_id, got: {trace_ids:?}",
    );

    // The Done event must also carry the custom trace ID.
    let done_trace = events.iter().find_map(|e| match e {
        Event::Core(agentic_core::CoreEvent::Done { trace_id }) => Some(trace_id.clone()),
        _ => None,
    });
    assert_eq!(done_trace.as_deref(), Some(custom_trace));
}

#[tokio::test]
async fn run_pipeline_error_does_not_push_to_memory() {
    let mut orch = Orchestrator::<MockDomain, _>::new(FatalSolver);

    let result = orch.run_pipeline("doomed".into(), "trace-doom").await;
    assert!(result.is_err());
    assert!(
        orch.memory().is_empty(),
        "failed pipeline must not push to memory"
    );
}

#[tokio::test]
async fn run_pipeline_output_carries_spec() {
    let mut orch = Orchestrator::<MockDomain, _>::new(HappySolver::new());
    let output = orch
        .run_pipeline("spec check".into(), "trace-spec")
        .await
        .unwrap();

    let spec = output
        .spec
        .expect("spec must be set for a full pipeline run");
    assert_eq!(spec.intent, "clarified: spec check");
    assert_eq!(spec.requirements, vec!["req-A", "req-B"]);
}

// ═════════════════════════════════════════════════════════════════════════════
// child_trace_id — tested indirectly via run_pipeline custom trace IDs
// ═════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn child_trace_id_convention_in_events() {
    let (tx, mut rx) = mpsc::channel::<Event<()>>(256);
    let mut orch = Orchestrator::<MockDomain, _>::new(HappySolver::new()).with_events(tx);

    // Simulate what a scatter-gather wrapper would do:
    // parent trace: "trace-parent", sub-runs use "trace-parent.0", "trace-parent.1"
    let parent_trace = "trace-parent";
    let child_0 = format!("{parent_trace}.0");
    let child_1 = format!("{parent_trace}.1");

    orch.run_pipeline("sub-0".into(), &child_0).await.unwrap();
    let events_0 = drain(&mut rx).await;

    orch.run_pipeline("sub-1".into(), &child_1).await.unwrap();
    let events_1 = drain(&mut rx).await;

    // Sub-run 0 events all carry "trace-parent.0"
    let trace_ids_0: Vec<String> = events_0
        .iter()
        .filter_map(|e| match e {
            Event::Core(agentic_core::CoreEvent::StateEnter { trace_id, .. }) => {
                Some(trace_id.clone())
            }
            _ => None,
        })
        .collect();
    assert!(
        trace_ids_0.iter().all(|t| t == &child_0),
        "sub-0 traces: {trace_ids_0:?}"
    );

    // Sub-run 1 events all carry "trace-parent.1"
    let trace_ids_1: Vec<String> = events_1
        .iter()
        .filter_map(|e| match e {
            Event::Core(agentic_core::CoreEvent::StateEnter { trace_id, .. }) => {
                Some(trace_id.clone())
            }
            _ => None,
        })
        .collect();
    assert!(
        trace_ids_1.iter().all(|t| t == &child_1),
        "sub-1 traces: {trace_ids_1:?}"
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// Fan-out tests — specify() returns N > 1 specs
// ═════════════════════════════════════════════════════════════════════════════

// ── FanOutSolver — specify returns N specs ────────────────────────────────────

struct FanOutSolver {
    sub_specs: Vec<String>,
    /// If set, solve returns Err for the spec whose intent matches
    /// `sub_specs[fail_sub_index]`.
    fail_sub_index: Option<usize>,
}

impl FanOutSolver {
    fn new(sub_specs: Vec<String>) -> Self {
        Self {
            sub_specs,
            fail_sub_index: None,
        }
    }

    fn with_failing_sub(mut self, index: usize) -> Self {
        self.fail_sub_index = Some(index);
        self
    }
}

#[async_trait]
impl DomainSolver<MockDomain> for FanOutSolver {
    async fn clarify(
        &mut self,
        intent: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(format!("clarified: {intent}"))
    }

    async fn specify(
        &mut self,
        _intent: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<Vec<MockSpec>, (String, BackTarget<MockDomain>)> {
        // Return one spec per sub_spec — the fan-out path.
        Ok(self
            .sub_specs
            .iter()
            .map(|s| MockSpec {
                intent: s.clone(),
                requirements: vec![],
            })
            .collect())
    }

    async fn specify_single(
        &mut self,
        intent: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<MockSpec, (String, BackTarget<MockDomain>)> {
        Ok(MockSpec {
            intent,
            requirements: vec![],
        })
    }

    async fn solve(
        &mut self,
        spec: MockSpec,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<Vec<String>, (String, BackTarget<MockDomain>)> {
        // Fail for the configured failing sub-spec index.
        if let Some(fi) = self.fail_sub_index {
            if spec.intent == self.sub_specs.get(fi).cloned().unwrap_or_default() {
                return Err((
                    "sub-spec fatal".into(),
                    BackTarget::Specify(spec.intent, Default::default()),
                ));
            }
        }
        Ok(vec![spec.intent.clone()])
    }

    async fn execute(
        &mut self,
        solution: Vec<String>,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(solution.join(","))
    }

    async fn interpret(
        &mut self,
        result: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(format!("ans:{result}"))
    }

    async fn diagnose(
        &mut self,
        error: String,
        _back: BackTarget<MockDomain>,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
    ) -> Result<ProblemState<MockDomain>, String> {
        Err(error) // always fatal for simplicity
    }

    fn merge_results(&self, results: Vec<String>) -> Result<String, String> {
        Ok(results.join(" | "))
    }
}

// ── FanOutMergeErrorSolver — fan-out OK, merge fails ─────────────────────────

struct FanOutMergeErrorSolver;

#[async_trait]
impl DomainSolver<MockDomain> for FanOutMergeErrorSolver {
    async fn clarify(
        &mut self,
        i: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(format!("c:{i}"))
    }
    async fn specify(
        &mut self,
        _i: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<Vec<MockSpec>, (String, BackTarget<MockDomain>)> {
        Ok(vec![
            MockSpec {
                intent: "q1".into(),
                requirements: vec![],
            },
            MockSpec {
                intent: "q2".into(),
                requirements: vec![],
            },
        ])
    }
    async fn specify_single(
        &mut self,
        i: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<MockSpec, (String, BackTarget<MockDomain>)> {
        Ok(MockSpec {
            intent: i,
            requirements: vec![],
        })
    }
    async fn solve(
        &mut self,
        s: MockSpec,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<Vec<String>, (String, BackTarget<MockDomain>)> {
        Ok(vec![s.intent])
    }
    async fn execute(
        &mut self,
        sol: Vec<String>,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(sol.join(""))
    }
    async fn interpret(
        &mut self,
        r: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(r)
    }
    async fn diagnose(
        &mut self,
        e: String,
        _: BackTarget<MockDomain>,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
    ) -> Result<ProblemState<MockDomain>, String> {
        Err(e)
    }
    fn merge_results(&self, _results: Vec<String>) -> Result<String, String> {
        Err("merge failed".into())
    }
}

/// Test 1: fan-out with 2 sub-specs; both results merged into one turn.
#[tokio::test]
async fn fan_out_two_sub_specs_merged_into_one_turn() {
    let sub_specs = vec!["sub-A".into(), "sub-B".into()];
    let mut orch = Orchestrator::<MockDomain, _>::new(FanOutSolver::new(sub_specs));

    let answer = orch.run("compound question".into()).await.unwrap();

    // merge_results joins with " | ", then interpret wraps with "ans:".
    assert!(answer.contains("sub-A"), "answer: {answer}");
    assert!(answer.contains("sub-B"), "answer: {answer}");

    // Only one memory turn regardless of sub-spec count.
    assert_eq!(
        orch.memory().len(),
        1,
        "expected 1 turn in memory, got {}",
        orch.memory().len()
    );

    // The stored turn carries the clarified intent.
    let turn = &orch.memory().turns()[0];
    assert_eq!(turn.intent, "clarified: compound question");
}

/// Test 2: second sub-spec solve fails → Fatal error; memory stays empty.
#[tokio::test]
async fn fan_out_failing_sub_spec_returns_fatal() {
    let sub_specs = vec!["sub-ok".into(), "sub-fail".into()];
    let solver = FanOutSolver::new(sub_specs).with_failing_sub(1);
    let mut orch = Orchestrator::<MockDomain, _>::new(solver);

    let result = orch.run("compound question".into()).await;
    assert!(
        matches!(result, Err(OrchestratorError::Fatal(_))),
        "expected Fatal, got ok"
    );
    assert_eq!(
        orch.memory().len(),
        0,
        "memory must be empty after failed fan-out"
    );
}

/// Test 3: events include FanOut + SubSpecStart/End for 3 sub-specs.
#[tokio::test]
async fn fan_out_events_are_emitted() {
    use agentic_core::CoreEvent;

    let (tx, mut rx) = mpsc::channel::<Event<()>>(512);
    let sub_specs = vec!["s0".into(), "s1".into(), "s2".into()];
    let mut orch = Orchestrator::<MockDomain, _>::new(FanOutSolver::new(sub_specs)).with_events(tx);

    orch.run("three questions".into()).await.unwrap();
    let events = drain(&mut rx).await;

    let fan_out: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, Event::Core(CoreEvent::FanOut { .. })))
        .collect();
    let sub_starts: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, Event::Core(CoreEvent::SubSpecStart { .. })))
        .collect();
    let sub_ends: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, Event::Core(CoreEvent::SubSpecEnd { .. })))
        .collect();

    assert_eq!(fan_out.len(), 1, "expected 1 FanOut event");
    if let Event::Core(CoreEvent::FanOut { spec_count, .. }) = &fan_out[0] {
        assert_eq!(*spec_count, 3);
    }

    assert_eq!(sub_starts.len(), 3, "expected 3 SubSpecStart events");
    assert_eq!(sub_ends.len(), 3, "expected 3 SubSpecEnd events");

    // Verify indices 0..2 and child trace IDs follow parent.N convention.
    let parent = if let Event::Core(CoreEvent::FanOut { trace_id, .. }) = &fan_out[0] {
        trace_id.clone()
    } else {
        unreachable!()
    };

    for i in 0..3usize {
        let start = sub_starts.iter().find(
            |e| matches!(e, Event::Core(CoreEvent::SubSpecStart { index, .. }) if *index == i),
        );
        assert!(start.is_some(), "missing SubSpecStart for index {i}");
        if let Some(Event::Core(CoreEvent::SubSpecStart { trace_id, .. })) = start {
            assert_eq!(
                *trace_id,
                format!("{parent}.{i}"),
                "child trace_id mismatch at index {i}"
            );
        }
    }
}

/// Test 4: merge error → Fatal.
#[tokio::test]
async fn fan_out_merge_error_returns_fatal() {
    let mut orch = Orchestrator::<MockDomain, _>::new(FanOutMergeErrorSolver);
    let result = orch.run("compound".into()).await;
    assert!(
        matches!(result, Err(OrchestratorError::Fatal(ref e)) if e == "merge failed"),
        "expected Fatal(merge failed)"
    );
}

/// Test 5: run_pipeline does not push to memory.
#[tokio::test]
async fn fan_out_pipeline_does_not_write_memory_entries() {
    let first_solver = HappySolver::new();
    let mut orch = Orchestrator::<MockDomain, _>::new(first_solver);

    // First run (single intent) — memory = 1
    orch.run("first question".into()).await.unwrap();
    assert_eq!(orch.memory().len(), 1);

    // run_pipeline must not push to memory.
    let before = orch.memory().len();
    let _ = orch
        .run_pipeline("raw pipeline call".into(), "test-trace")
        .await
        .unwrap();
    let after = orch.memory().len();
    assert_eq!(before, after, "run_pipeline must not push to memory");
}

// ═════════════════════════════════════════════════════════════════════════════
// Forward recovery: executing → interpreting emits Advanced, not BackEdge
// ═════════════════════════════════════════════════════════════════════════════

/// Solver where `execute` fails and diagnose recovers *forward* to
/// Interpreting — simulating the ValueAnomaly pass-through pattern.
struct ForwardRecoverySolver {
    execute_calls: u32,
}

#[async_trait]
impl DomainSolver<MockDomain> for ForwardRecoverySolver {
    async fn clarify(
        &mut self,
        intent: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(format!("clarified: {intent}"))
    }

    async fn specify_single(
        &mut self,
        intent: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<MockSpec, (String, BackTarget<MockDomain>)> {
        Ok(MockSpec {
            intent,
            requirements: vec![],
        })
    }

    async fn solve(
        &mut self,
        spec: MockSpec,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<Vec<String>, (String, BackTarget<MockDomain>)> {
        Ok(vec![spec.intent])
    }

    async fn execute(
        &mut self,
        solution: Vec<String>,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        self.execute_calls += 1;
        // Always fail, pointing forward to Interpret.
        Err((
            "value anomaly".into(),
            BackTarget::Interpret(solution.join(","), Default::default()),
        ))
    }

    async fn interpret(
        &mut self,
        result: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(format!("answer: {result}"))
    }

    async fn diagnose(
        &mut self,
        _error: String,
        back: BackTarget<MockDomain>,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
    ) -> Result<ProblemState<MockDomain>, String> {
        // Forward recovery: jump ahead to Interpreting.
        match back {
            BackTarget::Interpret(result, _) => Ok(ProblemState::Interpreting(result)),
            _ => unreachable!("unexpected back target"),
        }
    }
}

#[tokio::test]
async fn diagnose_forward_transition_emits_advance_not_back_edge() {
    let (tx, mut rx) = mpsc::channel::<Event<()>>(256);

    let mut orch = Orchestrator::<MockDomain, _>::new(ForwardRecoverySolver { execute_calls: 0 })
        .with_events(tx);

    let answer = orch.run("task".into()).await.unwrap();
    assert_eq!(answer, "answer: clarified: task");

    let events = drain(&mut rx).await;

    // The executing → interpreting transition should produce an Advanced
    // outcome, NOT a BackEdge event.
    let exit_outcomes: Vec<(String, agentic_core::Outcome)> = events
        .iter()
        .filter_map(|e| match e {
            Event::Core(CoreEvent::StateExit { state, outcome, .. }) => {
                Some((state.clone(), outcome.clone()))
            }
            _ => None,
        })
        .collect();

    // Find the exit for "executing" — it must be Advanced.
    let exec_exit = exit_outcomes
        .iter()
        .find(|(s, _)| s == "executing")
        .expect("must have a StateExit for executing");
    assert_eq!(
        exec_exit.1,
        agentic_core::Outcome::Advanced,
        "executing → interpreting should be Advanced, got {:?}",
        exec_exit.1
    );

    // No BackEdge events should have been emitted.
    let back_edge_count = events
        .iter()
        .filter(|e| matches!(e, Event::Core(CoreEvent::BackEdge { .. })))
        .count();
    assert_eq!(
        back_edge_count, 0,
        "forward recovery must not emit BackEdge"
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// Backward recovery: executing → solving still emits BackEdge
// ═════════════════════════════════════════════════════════════════════════════

/// Solver where `execute` fails and diagnose recovers *backward* to Solving.
struct BackwardRecoverySolver {
    execute_calls: u32,
}

#[async_trait]
impl DomainSolver<MockDomain> for BackwardRecoverySolver {
    async fn clarify(
        &mut self,
        intent: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(format!("clarified: {intent}"))
    }

    async fn specify_single(
        &mut self,
        intent: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<MockSpec, (String, BackTarget<MockDomain>)> {
        Ok(MockSpec {
            intent,
            requirements: vec!["req".into()],
        })
    }

    async fn solve(
        &mut self,
        spec: MockSpec,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<Vec<String>, (String, BackTarget<MockDomain>)> {
        Ok(vec![spec.intent.clone()])
    }

    async fn execute(
        &mut self,
        solution: Vec<String>,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        self.execute_calls += 1;
        if self.execute_calls == 1 {
            // First attempt: fail backward to Solve.
            Err((
                "bad plan".into(),
                BackTarget::Solve(
                    MockSpec {
                        intent: solution.join(","),
                        requirements: vec![],
                    },
                    Default::default(),
                ),
            ))
        } else {
            // Second attempt: succeed.
            Ok(solution.join("|"))
        }
    }

    async fn interpret(
        &mut self,
        result: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(format!("answer: {result}"))
    }

    async fn diagnose(
        &mut self,
        _error: String,
        back: BackTarget<MockDomain>,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
    ) -> Result<ProblemState<MockDomain>, String> {
        match back {
            BackTarget::Solve(spec, _) => Ok(ProblemState::Solving(spec)),
            _ => unreachable!("unexpected back target"),
        }
    }
}

#[tokio::test]
async fn diagnose_backward_transition_still_emits_back_edge() {
    let (tx, mut rx) = mpsc::channel::<Event<()>>(256);

    let mut orch = Orchestrator::<MockDomain, _>::new(BackwardRecoverySolver { execute_calls: 0 })
        .with_events(tx);

    orch.run("task".into()).await.unwrap();

    let events = drain(&mut rx).await;

    // The executing → solving transition should produce BackTracked + BackEdge.
    let exec_exits: Vec<agentic_core::Outcome> = events
        .iter()
        .filter_map(|e| match e {
            Event::Core(CoreEvent::StateExit { state, outcome, .. }) if state == "executing" => {
                Some(outcome.clone())
            }
            _ => None,
        })
        .collect();

    // First exit from executing should be BackTracked.
    assert_eq!(
        exec_exits.first(),
        Some(&agentic_core::Outcome::BackTracked),
        "executing → solving should be BackTracked"
    );

    // A BackEdge event should have been emitted.
    let back_edges: Vec<(String, String)> = events
        .iter()
        .filter_map(|e| match e {
            Event::Core(CoreEvent::BackEdge { from, to, .. }) => Some((from.clone(), to.clone())),
            _ => None,
        })
        .collect();

    assert_eq!(back_edges.len(), 1);
    assert_eq!(back_edges[0].0, "executing");
    assert_eq!(back_edges[0].1, "solving");
}

// ═════════════════════════════════════════════════════════════════════════════
// Item 4: StateHandler.diagnose = None acts as passthrough
// ═════════════════════════════════════════════════════════════════════════════

/// Solver that succeeds unconditionally on all stages.
struct PassSolver;

#[async_trait]
impl DomainSolver<MockDomain> for PassSolver {
    async fn clarify(
        &mut self,
        intent: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(intent)
    }

    async fn specify_single(
        &mut self,
        intent: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<MockSpec, (String, BackTarget<MockDomain>)> {
        Ok(MockSpec {
            intent,
            requirements: vec![],
        })
    }

    async fn solve(
        &mut self,
        spec: MockSpec,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<Vec<String>, (String, BackTarget<MockDomain>)> {
        Ok(vec![spec.intent])
    }

    async fn execute(
        &mut self,
        solution: Vec<String>,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(solution.join(","))
    }

    async fn interpret(
        &mut self,
        result: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(format!("done: {result}"))
    }

    async fn diagnose(
        &mut self,
        error: String,
        _back: BackTarget<MockDomain>,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
    ) -> Result<ProblemState<MockDomain>, String> {
        Err(error)
    }
}

/// `diagnose: None` on a StateHandler must pass the suggested recovery state
/// through unchanged — equivalent to `|_, _, r| Some(r)`.
///
/// We inject a custom clarifying handler whose `execute` closure returns a
/// non-empty error on the first call and succeeds on the second.  With
/// `diagnose: None` the orchestrator must retry (pass recovery through) so
/// the run eventually succeeds.
#[tokio::test]
async fn state_handler_diagnose_none_acts_as_passthrough() {
    use agentic_core::{build_default_handlers, StateHandler, TransitionResult};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc as StdArc;

    // Shared counter so the closure can track calls without capturing &mut.
    let calls = StdArc::new(AtomicU32::new(0));
    let calls2 = StdArc::clone(&calls);

    let mut handlers = build_default_handlers::<MockDomain, PassSolver, ()>();

    // Replace the clarifying handler: fail once, then succeed.
    handlers.insert(
        "clarifying",
        StateHandler {
            next: "specifying",
            execute: Arc::new(move |_solver, state, _events, _ctx, _mem| {
                let n = calls2.fetch_add(1, Ordering::SeqCst);
                let data = match state {
                    ProblemState::Clarifying(d) => d,
                    _ => unreachable!(),
                };
                Box::pin(async move {
                    if n == 0 {
                        // First call: return a non-empty error, suggesting retry
                        // (recovery = Clarifying again).
                        TransitionResult {
                            state_data: ProblemState::Clarifying(data),
                            errors: Some(vec!["transient".to_string()]),
                            next_stage: None,
                            fan_out: None,
                        }
                    } else {
                        // Second call: succeed.
                        TransitionResult::ok(ProblemState::Specifying(data))
                    }
                })
            }),
            // diagnose: None → orchestrator passes recovery through unchanged.
            diagnose: None,
        },
    );

    let mut orch = Orchestrator::<MockDomain, _>::new(PassSolver).with_handlers(handlers);
    let answer = orch
        .run("input".into())
        .await
        .expect("should succeed after retry");
    assert_eq!(answer, "done: input");
    // Handler was called twice: once failing, once succeeding.
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

/// A custom `diagnose` closure that always returns `None` must escalate the
/// error to a fatal `OrchestratorError::Fatal`.
#[tokio::test]
async fn state_handler_diagnose_some_none_escalates_to_fatal() {
    use agentic_core::{build_default_handlers, StateHandler, TransitionResult};

    let mut handlers = build_default_handlers::<MockDomain, PassSolver, ()>();

    // Replace the clarifying handler so it always returns a non-empty error.
    handlers.insert(
        "clarifying",
        StateHandler {
            next: "specifying",
            execute: Arc::new(|_solver, state, _events, _ctx, _mem| {
                let data = match state {
                    ProblemState::Clarifying(d) => d,
                    _ => unreachable!(),
                };
                Box::pin(async move {
                    TransitionResult {
                        state_data: ProblemState::Clarifying(data),
                        errors: Some(vec!["permanent".to_string()]),
                        next_stage: None,
                        fan_out: None,
                    }
                })
            }),
            // diagnose: Some(closure returning None) → always escalate.
            diagnose: Some(Arc::new(|_errors, _retry, _recovery| None)),
        },
    );

    let mut orch = Orchestrator::<MockDomain, _>::new(PassSolver).with_handlers(handlers);
    let err = orch.run("input".into()).await.unwrap_err();
    assert!(
        matches!(err, OrchestratorError::Fatal(ref e) if e == "permanent"),
        "expected Fatal(permanent), got {err:?}"
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// Item 7: problem_state_from_resume returns Option
// ═════════════════════════════════════════════════════════════════════════════

/// Calling `resume` on a solver that does not override `problem_state_from_resume`
/// (which now defaults to `None`) must return `OrchestratorError::ResumeNotSupported`
/// instead of panicking.
#[tokio::test]
async fn resume_without_hitl_support_returns_resume_not_supported() {
    use agentic_core::{OrchestratorError, SuspendedRunData};

    let mut orch = Orchestrator::<MockDomain, _>::new(PassSolver);
    let dummy = SuspendedRunData {
        from_state: "clarifying".into(),
        original_input: "q".into(),
        trace_id: "t".into(),
        stage_data: serde_json::Value::Null,
        question: "what?".into(),
        suggestions: vec![],
    };
    let err = orch.resume(dummy, "answer".into()).await.unwrap_err();
    assert!(
        matches!(err, OrchestratorError::ResumeNotSupported),
        "expected ResumeNotSupported, got {err:?}"
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// Concurrent fan-out — happy path
// ═════════════════════════════════════════════════════════════════════════════

use agentic_core::solver::FanoutWorker;

/// A solver that returns multiple specs and provides a `FanoutWorker` for
/// concurrent execution.
struct ConcurrentFanoutSolver {
    sub_specs: Vec<String>,
}

#[async_trait]
impl DomainSolver<MockDomain> for ConcurrentFanoutSolver {
    async fn clarify(
        &mut self,
        intent: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(format!("clarified: {intent}"))
    }

    async fn specify(
        &mut self,
        _intent: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<Vec<MockSpec>, (String, BackTarget<MockDomain>)> {
        Ok(self
            .sub_specs
            .iter()
            .map(|s| MockSpec {
                intent: s.clone(),
                requirements: vec![],
            })
            .collect())
    }

    async fn specify_single(
        &mut self,
        intent: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<MockSpec, (String, BackTarget<MockDomain>)> {
        Ok(MockSpec {
            intent,
            requirements: vec![],
        })
    }

    async fn solve(
        &mut self,
        spec: MockSpec,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<Vec<String>, (String, BackTarget<MockDomain>)> {
        // Should not be called during concurrent fan-out.
        Ok(vec![spec.intent])
    }

    async fn execute(
        &mut self,
        solution: Vec<String>,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        // Should not be called during concurrent fan-out.
        Ok(solution.join(","))
    }

    async fn interpret(
        &mut self,
        result: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(format!("ans:{result}"))
    }

    async fn diagnose(
        &mut self,
        error: String,
        _back: BackTarget<MockDomain>,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
    ) -> Result<ProblemState<MockDomain>, String> {
        Err(error) // always fatal
    }

    fn merge_results(&self, results: Vec<String>) -> Result<String, String> {
        Ok(results.join(" | "))
    }

    fn fanout_worker<Ev: DomainEvents>(&self) -> Option<Arc<dyn FanoutWorker<MockDomain, Ev>>> {
        // We can only provide a worker for Ev = (), but the trait requires
        // a generic Ev.  Use a separate worker that is generic-compatible.
        Some(Arc::new(GenericMockFanoutWorker))
    }
}

/// A generic `FanoutWorker` that works for any `Ev: DomainEvents`.
struct GenericMockFanoutWorker;

#[async_trait]
impl<Ev: DomainEvents> FanoutWorker<MockDomain, Ev> for GenericMockFanoutWorker {
    async fn solve_and_execute(
        &self,
        spec: MockSpec,
        _index: usize,
        _total: usize,
        _events: &Option<EventStream<Ev>>,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _mem: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(format!("result:{}", spec.intent.to_uppercase()))
    }
}

#[tokio::test]
async fn test_concurrent_fanout_happy_path() {
    let (tx, mut rx) = mpsc::channel::<Event<()>>(512);

    let sub_specs = vec!["alpha".into(), "beta".into(), "gamma".into()];
    let mut orch =
        Orchestrator::<MockDomain, _>::new(ConcurrentFanoutSolver { sub_specs }).with_events(tx);

    let answer = orch.run("compound".into()).await.unwrap();

    // Each sub-spec's intent is uppercased by the worker, merged with " | ",
    // then wrapped by interpret with "ans:".
    assert!(answer.contains("result:ALPHA"), "answer: {answer}");
    assert!(answer.contains("result:BETA"), "answer: {answer}");
    assert!(answer.contains("result:GAMMA"), "answer: {answer}");

    let events = drain(&mut rx).await;

    // Verify FanOut event with spec_count = 3.
    let fan_out: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, Event::Core(CoreEvent::FanOut { .. })))
        .collect();
    assert_eq!(fan_out.len(), 1, "expected 1 FanOut event");
    if let Event::Core(CoreEvent::FanOut { spec_count, .. }) = &fan_out[0] {
        assert_eq!(*spec_count, 3);
    }

    // Verify 3 SubSpecStart and 3 SubSpecEnd events.
    let sub_starts: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, Event::Core(CoreEvent::SubSpecStart { .. })))
        .collect();
    let sub_ends: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, Event::Core(CoreEvent::SubSpecEnd { .. })))
        .collect();
    assert_eq!(sub_starts.len(), 3, "expected 3 SubSpecStart events");
    assert_eq!(sub_ends.len(), 3, "expected 3 SubSpecEnd events");

    // Verify the pipeline reaches interpreting (StateEnter for "interpreting").
    let interp_enters: Vec<_> = events
        .iter()
        .filter(|e| {
            matches!(
                e,
                Event::Core(CoreEvent::StateEnter { state, .. }) if state == "interpreting"
            )
        })
        .collect();
    assert_eq!(
        interp_enters.len(),
        1,
        "pipeline must reach interpreting after concurrent fan-out"
    );

    // Verify Done event.
    let has_done = events
        .iter()
        .any(|e| matches!(e, Event::Core(CoreEvent::Done { .. })));
    assert!(has_done, "expected a Done event");
}

// ═════════════════════════════════════════════════════════════════════════════
// Serial fan-out unchanged when fanout_worker() returns None
// ═════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_serial_fanout_unchanged() {
    // FanOutSolver does NOT override fanout_worker(), so the default (None)
    // is used — the serial fan-out path is taken.
    let (tx, mut rx) = mpsc::channel::<Event<()>>(512);

    let sub_specs = vec!["s0".into(), "s1".into()];
    let mut orch = Orchestrator::<MockDomain, _>::new(FanOutSolver::new(sub_specs)).with_events(tx);

    let answer = orch.run("serial question".into()).await.unwrap();

    // merge_results joins with " | ", then interpret wraps with "ans:".
    assert!(answer.contains("s0"), "answer: {answer}");
    assert!(answer.contains("s1"), "answer: {answer}");

    let events = drain(&mut rx).await;

    // Serial path emits StateEnter/StateExit for solving and executing
    // inside each sub-spec.
    let solving_enters: Vec<_> = events
        .iter()
        .filter(|e| {
            matches!(
                e,
                Event::Core(CoreEvent::StateEnter { state, .. }) if state == "solving"
            )
        })
        .collect();
    // Serial fan-out: 2 sub-specs, each gets a solving StateEnter.
    assert_eq!(
        solving_enters.len(),
        2,
        "serial fan-out must emit solving StateEnter per sub-spec"
    );

    // SubSpecStart/End events must still appear.
    let sub_starts: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, Event::Core(CoreEvent::SubSpecStart { .. })))
        .collect();
    let sub_ends: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, Event::Core(CoreEvent::SubSpecEnd { .. })))
        .collect();
    assert_eq!(sub_starts.len(), 2);
    assert_eq!(sub_ends.len(), 2);

    // FanOut event must be present.
    let fan_out_count = events
        .iter()
        .filter(|e| matches!(e, Event::Core(CoreEvent::FanOut { .. })))
        .count();
    assert_eq!(fan_out_count, 1);
}

// ═════════════════════════════════════════════════════════════════════════════
// Concurrent fan-out retry tests
// ═════════════════════════════════════════════════════════════════════════════

use std::sync::atomic::{AtomicU32, Ordering};

/// A `FanoutWorker` that fails the first N attempts for a specific sub-spec
/// index, then succeeds.  Tracks total call counts via shared atomics.
struct RetryMockFanoutWorker {
    /// Sub-spec index that should fail initially.
    fail_index: usize,
    /// How many times the failing sub-spec should fail before succeeding.
    fail_count: u32,
    /// Per-index attempt counters (indexed by sub-spec index).
    attempts: Vec<Arc<AtomicU32>>,
}

impl RetryMockFanoutWorker {
    fn new(total: usize, fail_index: usize, fail_count: u32) -> Self {
        Self {
            fail_index,
            fail_count,
            attempts: (0..total).map(|_| Arc::new(AtomicU32::new(0))).collect(),
        }
    }
}

#[async_trait]
impl<Ev: DomainEvents> FanoutWorker<MockDomain, Ev> for RetryMockFanoutWorker {
    async fn solve_and_execute(
        &self,
        spec: MockSpec,
        index: usize,
        _total: usize,
        _events: &Option<EventStream<Ev>>,
        ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _mem: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        let attempt = self.attempts[index].fetch_add(1, Ordering::SeqCst);

        if index == self.fail_index && attempt < self.fail_count {
            return Err((
                format!("sub-spec {index} failed on attempt {attempt}"),
                BackTarget::Solve(spec, Default::default()),
            ));
        }

        // On success, include whether retry context was present so tests can
        // verify the error was forwarded.
        let had_retry = ctx.retry_ctx.is_some();
        Ok(format!(
            "result:{}:retry={}",
            spec.intent.to_uppercase(),
            had_retry
        ))
    }
}

/// A solver that uses `RetryMockFanoutWorker`.
struct RetryFanoutSolver {
    sub_specs: Vec<String>,
    worker: Arc<RetryMockFanoutWorker>,
}

impl RetryFanoutSolver {
    fn new(sub_specs: Vec<String>, fail_index: usize, fail_count: u32) -> Self {
        let total = sub_specs.len();
        Self {
            sub_specs: sub_specs.clone(),
            worker: Arc::new(RetryMockFanoutWorker::new(total, fail_index, fail_count)),
        }
    }

    fn attempt_count(&self, index: usize) -> u32 {
        self.worker.attempts[index].load(Ordering::SeqCst)
    }
}

#[async_trait]
impl DomainSolver<MockDomain> for RetryFanoutSolver {
    async fn clarify(
        &mut self,
        intent: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(format!("clarified: {intent}"))
    }

    async fn specify(
        &mut self,
        _intent: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<Vec<MockSpec>, (String, BackTarget<MockDomain>)> {
        Ok(self
            .sub_specs
            .iter()
            .map(|s| MockSpec {
                intent: s.clone(),
                requirements: vec![],
            })
            .collect())
    }

    async fn specify_single(
        &mut self,
        intent: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<MockSpec, (String, BackTarget<MockDomain>)> {
        Ok(MockSpec {
            intent,
            requirements: vec![],
        })
    }

    async fn solve(
        &mut self,
        spec: MockSpec,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<Vec<String>, (String, BackTarget<MockDomain>)> {
        Ok(vec![spec.intent])
    }

    async fn execute(
        &mut self,
        solution: Vec<String>,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(solution.join(","))
    }

    async fn interpret(
        &mut self,
        result: String,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
        _memory: &agentic_core::orchestrator::SessionMemory<MockDomain>,
    ) -> Result<String, (String, BackTarget<MockDomain>)> {
        Ok(format!("ans:{result}"))
    }

    async fn diagnose(
        &mut self,
        error: String,
        _back: BackTarget<MockDomain>,
        _ctx: &agentic_core::orchestrator::RunContext<MockDomain>,
    ) -> Result<ProblemState<MockDomain>, String> {
        Err(error) // always fatal
    }

    fn merge_results(&self, results: Vec<String>) -> Result<String, String> {
        Ok(results.join(" | "))
    }

    fn fanout_worker<Ev: DomainEvents>(&self) -> Option<Arc<dyn FanoutWorker<MockDomain, Ev>>> {
        Some(self.worker.clone())
    }
}

/// Sub-spec #1 fails once then succeeds on retry → overall pipeline succeeds.
#[tokio::test]
async fn concurrent_fanout_retry_succeeds_after_one_failure() {
    let (tx, mut rx) = mpsc::channel::<Event<()>>(512);

    // 3 sub-specs; index 1 fails once then succeeds.
    let solver = RetryFanoutSolver::new(
        vec!["alpha".into(), "beta".into(), "gamma".into()],
        1, // fail_index
        1, // fail_count (fail once, succeed on 2nd attempt)
    );
    let mut orch = Orchestrator::<MockDomain, _>::new(solver).with_events(tx);

    let answer = orch.run("retry-test".into()).await.unwrap();

    // All three sub-specs should be in the merged answer.
    assert!(answer.contains("ALPHA"), "answer: {answer}");
    assert!(answer.contains("BETA"), "answer: {answer}");
    assert!(answer.contains("GAMMA"), "answer: {answer}");

    // The retried sub-spec should have received retry context.
    assert!(
        answer.contains("retry=true"),
        "retried sub-spec must receive retry context, answer: {answer}"
    );

    // Verify attempt counts: index 0 and 2 called once, index 1 called twice.
    assert_eq!(orch.solver().attempt_count(0), 1);
    assert_eq!(orch.solver().attempt_count(1), 2);
    assert_eq!(orch.solver().attempt_count(2), 1);

    // Verify a BackEdge event was emitted for the retry.
    let events = drain(&mut rx).await;
    let back_edges: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, Event::Core(CoreEvent::BackEdge { .. })))
        .collect();
    assert!(
        !back_edges.is_empty(),
        "expected at least 1 BackEdge event for fanout retry"
    );
}

/// Sub-spec #0 fails twice then succeeds on the 3rd attempt (max_retries=2).
#[tokio::test]
async fn concurrent_fanout_retry_succeeds_at_max_retries() {
    // fail_count=2, default max_fanout_retries=2, so 3 total attempts (0,1,2).
    // Attempt 0 and 1 fail, attempt 2 succeeds.
    let solver = RetryFanoutSolver::new(
        vec!["alpha".into(), "beta".into()],
        0, // fail_index
        2, // fail_count
    );
    let mut orch = Orchestrator::<MockDomain, _>::new(solver);

    let answer = orch.run("retry-at-limit".into()).await.unwrap();

    assert!(answer.contains("ALPHA"), "answer: {answer}");
    assert!(answer.contains("BETA"), "answer: {answer}");

    // Index 0 attempted 3 times (2 failures + 1 success), index 1 once.
    assert_eq!(orch.solver().attempt_count(0), 3);
    assert_eq!(orch.solver().attempt_count(1), 1);
}

/// Sub-spec #0 always fails → exhausts retries → pipeline returns Fatal.
#[tokio::test]
async fn concurrent_fanout_retry_exhausted_returns_fatal() {
    // fail_count=10 (more than max_retries=2), so all 3 attempts fail.
    let solver = RetryFanoutSolver::new(
        vec!["alpha".into(), "beta".into()],
        0,  // fail_index
        10, // fail_count (always fails)
    );
    let mut orch = Orchestrator::<MockDomain, _>::new(solver);

    let result = orch.run("retry-exhausted".into()).await;
    assert!(
        matches!(result, Err(OrchestratorError::Fatal(_))),
        "expected Fatal after retries exhausted, got: {result:?}"
    );

    // Index 0 should have been attempted max_retries+1 = 3 times.
    assert_eq!(orch.solver().attempt_count(0), 3);
}

/// When no sub-spec fails, no BackEdge events are emitted and each sub-spec
/// is called exactly once.
#[tokio::test]
async fn concurrent_fanout_no_retry_when_all_succeed() {
    let (tx, mut rx) = mpsc::channel::<Event<()>>(512);

    // fail_index=99 (out of range) — nothing fails.
    let solver = RetryFanoutSolver::new(
        vec!["alpha".into(), "beta".into()],
        99, // no sub-spec at this index
        1,
    );
    let mut orch = Orchestrator::<MockDomain, _>::new(solver).with_events(tx);

    let answer = orch.run("no-retry".into()).await.unwrap();
    assert!(answer.contains("ALPHA"), "answer: {answer}");
    assert!(answer.contains("BETA"), "answer: {answer}");

    // Each sub-spec called exactly once.
    assert_eq!(orch.solver().attempt_count(0), 1);
    assert_eq!(orch.solver().attempt_count(1), 1);

    // No BackEdge events.
    let events = drain(&mut rx).await;
    let back_edges: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, Event::Core(CoreEvent::BackEdge { .. })))
        .collect();
    assert!(
        back_edges.is_empty(),
        "expected no BackEdge events when all succeed"
    );
}
