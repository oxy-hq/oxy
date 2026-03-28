# Agentic-Core: A Generic FSM for LLM-Powered Problem Solving

## Philosophy

Most problem-solving follows the same epistemic arc: something vague becomes defined, then approachable, then resolved, then communicated. This crate encodes that arc as a finite state machine, parameterized by domain. The FSM tracks _where the problem is_, not _what the system is doing_. States are epistemic milestones, not activities.

The orchestrator is deterministic. The LLM is a stateless worker called within each state. Validators guard every transition. Thinking is observable but never feeds back across states.

---

## States

```
  Intent
    │
    ▼
 Clarifying ──► Specifying ──► Solving ──► Executing ──► Interpreting ──► Done
    ▲               ▲             ▲             ▲               ▲
    └───────────────┴─────────────┴─────────────┴───────────────┘
                        Diagnosing (back-edges)
```

States are named as activities (Clarifying, Solving) for readability, but each represents an **epistemic claim** about the problem's status. A state transition means the epistemic status has advanced, not merely that work was performed.

| State          | Carries              | Epistemic Claim                               | Invariant                                                     |
| -------------- | -------------------- | --------------------------------------------- | ------------------------------------------------------------- |
| `Clarifying`   | `Intent`             | Working toward a grounded question            | Intent is partial, ambiguous, or underspecified               |
| `Specifying`   | `Intent`             | Have a clear intent, grounding it into a spec | Intent is fully formed; resolving against semantic layer      |
| `Solving`      | `Spec`               | Have a grounded spec, producing a solution    | Spec is valid; every reference resolves; writing SQL          |
| `Executing`    | `Solution`           | Have a candidate solution, running it         | Solution is syntactically valid and structurally matches spec |
| `Interpreting` | `Result`             | Have validated results, producing an answer   | Results match expected shape and plausible ranges             |
| `Diagnosing`   | `Error + BackTarget` | A validator failed, routing to recovery       | Deterministic routing based on error type                     |
| `Done`         | `Answer`             | User has a comprehensible answer              | Response addresses original question                          |

### The ProblemState Enum

```rust
pub enum ProblemState<D: Domain> {
    Clarifying(D::Intent),
    Specifying(D::Intent),
    Solving(D::Spec),
    Executing(D::Solution),
    Interpreting(D::Result),
    Diagnosing {
        error: D::Error,
        back: BackTarget<D>,
    },
    Done(D::Answer),
}
```

Each variant carries only the output of its predecessor. The orchestrator accumulates and owns context across the run via `RunContext<D>`. `D::Spec` implements `HasIntent<D>` so the orchestrator can recover intent from the spec without threading it through every variant.

### Diagnosing as a First-Class State

`Diagnosing` is entered when a validator fails. It is an explicit state so that:

- The FSM is always in a well-defined state — no "between states" moment
- The event stream shows `StateEnter("diagnosing")` clearly
- The diagnosis logic is testable independently

`Diagnosing` is deterministic. It examines the error type and produces a typed `BackTarget<D>`. It never calls the LLM.

Domains may skip states by returning a `Some(next_state)` from `should_skip(state, data, run_ctx)`. The orchestrator advances past skipped states without calling handlers.

---

## Context Per State (Hourglass Pattern)

Context narrows then widens. The `ProblemState` variant carries only its predecessor's output. The orchestrator maintains `RunContext<D>` as accumulated state across the full run.

```rust
pub struct RunContext<D: Domain> {
    pub intent: Option<D::Intent>,       // set after Clarifying
    pub spec:   Option<D::Spec>,         // set after Specifying
    pub retry_ctx: Option<RetryContext>, // set on back-edges, cleared after success
}
```

`SessionMemory<D>` holds up to 10 prior completed turns (question + answer pairs), evicting oldest, so the orchestrator can inject conversation history without bloating the current state.

Each state handler receives:

| Handler      | Receives                                                                 | Produces   |
| ------------ | ------------------------------------------------------------------------ | ---------- |
| Clarifying   | raw question, session memory, prior intent (on retry), catalog           | `Intent`   |
| Specifying   | intent, catalog, retry context                                           | `Spec`     |
| Solving      | spec, dialect, retry context                                             | `Solution` |
| Executing    | solution, connector                                                      | `Result`   |
| Interpreting | result, intent (from RunContext), spec (from RunContext), session memory | `Answer`   |

---

## Semantic Layer Integration (Hybrid Approach)

The semantic layer (`.view.yml` / `.topic.yml` files) replaces raw DB schema as the primary catalog. It provides pre-defined metrics with business logic baked in, known valid metric/dimension combinations, automatic join path resolution, and direct compilation from intent to SQL for standard queries.

### Why Hybrid

Pure semantic layer breaks on anything beyond its query interface (custom window functions, multi-step calculations, novel analyses). Pure LLM gets basic business logic wrong. The hybrid approach uses the semantic layer for the common case (guaranteed correctness) and falls back to LLM-generated SQL for the long tail (with semantic context to keep it grounded).

### Two Paths Through the FSM

**Simple path** (semantic layer compiles directly):

```
Clarifying → Specifying (emits SQL) → skip Solving → Executing → Interpreting → Done
```

**Complex path** (LLM writes SQL with semantic context):

```
Clarifying → Specifying (emits context) → Solving (LLM writes SQL) → Executing → Interpreting → Done
```

**Procedure path** (pre-defined query file):

```
Clarifying → Specifying (resolves procedure) → skip Solving → Executing (runs procedure) → Interpreting → Done
```

The routing decision happens in `Specifying`, not `Clarifying`. `Clarifying`'s job is understanding intent. Whether the semantic layer can handle it is a technical routing decision.

### Catalog Architecture

```
HybridCatalog
  ├── SemanticCatalog (Optional)   — reads .view.yml / .topic.yml
  └── SchemaCatalog                — raw DB schema, column stats, join hints
```

`HybridCatalog::try_compile(intent)` tries semantic first, falls back to schema on `TooComplex`. The `solution_source` field on `QuerySpec` carries the routing decision forward so that downstream states (Executing, Diagnosing) know which path was taken.

---

## Back-Edges

Every back-edge is triggered by a validator failure inside a state handler. The handler calls `TransitionResult::fail(state, errors)`. The orchestrator calls `handler.diagnose`, which returns an `Option<ProblemState<D>>`: `Some(state)` to backtrack, `None` to escalate as fatal.

### Flow

```
Handler returns TransitionResult::fail(errors)
    → Orchestrator calls handler.diagnose(errors, attempt, current_state)
    → Returns Some(ProblemState::Diagnosing { error, back: BackTarget<D> })
    → Orchestrator processes Diagnosing: extracts BackTarget, transitions
    → Emits BackEdge event
```

### BackTarget

`BackTarget<D>` carries **typed state data** to the recovery target — not a string name. This ensures the target state handler receives well-typed input, and the compiler catches routing mistakes.

```rust
pub enum BackTarget<D: Domain> {
    Clarify(D::Intent, RetryContext),
    Specify(D::Intent, RetryContext),
    Solve(D::Spec, RetryContext),
    Execute(D::Solution, RetryContext),
    Interpret(D::Result, RetryContext),
    Suspend { prompt: String, suggestions: Vec<String> },
}

pub struct RetryContext {
    pub errors: Vec<String>,
    pub attempt: u32,
    pub previous_output: Option<String>,  // only populated after 2nd failure
}
```

`Suspend` is not a back-edge — it's a first-class HITL signal (see Human-in-the-Loop below). `RetryContext` carries NO thinking. Retries use errors and optional prior output text only.

### Routing Rules

| From         | Error Type       | Target                                                 | Rationale                                                  |
| ------------ | ---------------- | ------------------------------------------------------ | ---------------------------------------------------------- |
| Solving      | SyntaxError      | Solving (retry)                                        | Code is wrong, spec is fine                                |
| Solving      | ShapeMismatch    | Specifying                                             | Spec produced unachievable plan                            |
| Specifying   | AmbiguousColumn  | Clarifying                                             | Intent unresolvable — needs user clarification             |
| Specifying   | UnresolvedMetric | Specifying (retry with LLM path)                       | Semantic layer failed — fall back to LLM with context      |
| Executing    | EmptyResults     | Specifying                                             | Filters too narrow, wrong table, or semantic layer SQL bad |
| Executing    | ShapeMismatch    | Solving (LLM path) or Specifying (semantic layer path) | Can't retry Solving if it was skipped                      |
| Executing    | ValueAnomaly     | Solving (LLM path) or Specifying (semantic layer path) | Same routing logic based on solution_source                |
| Interpreting | ValueAnomaly     | Interpreting (retry)                                   | Interpretation was misleading, re-narrate                  |
| Any          | NeedsUserInput   | Suspend (HITL)                                         | Requires explicit user clarification to proceed            |

When Executing fails on the semantic layer path, it routes to Specifying (not Solving, which was skipped). Specifying retries via the LLM path with the failure diagnosis as additional context.

### Context Contamination Rules

- **Same-state retries:** carry error message; `previous_output` is withheld on first retry (fresh generation often outperforms patching). Populated on second failure to give the LLM more signal.
- **Cross-state back-edges:** carry `RetryContext` (diagnosis), never raw artifacts from the failed state.
- **User-driven back-edges:** carry prior intent + feedback; never carry a prior solution.

### Circuit Breaker

Max 200 orchestrator iterations per run. If exceeded, returns `OrchestratorError::MaxIterationsExceeded`. This guards against infinite back-edge cycles regardless of retry count at any single state.

---

## Human-in-the-Loop (HITL)

Some problems cannot be resolved by the agent alone: ambiguous intent, missing permissions, no matching data. Rather than hallucinating an answer, the orchestrator suspends and waits for explicit user input.

### Trigger

Any state handler may emit `BackTarget::Suspend { prompt, suggestions }` via `TransitionResult::diagnosing`. The orchestrator detects this variant, emits `AwaitingHumanInput`, and returns `OrchestratorError::Suspended` to the caller.

```rust
pub enum OrchestratorError<D: Domain> {
    MaxIterationsExceeded,
    Fatal(D::Error),
    Suspended {
        prompt: String,
        suggestions: Vec<String>,
        resume_data: SuspendedRunData,
        trace_id: String,
    },
}
```

### Resume

`SuspendedRunData` is opaque to the HTTP layer — the domain stores and retrieves it via `DomainSolver::store_suspension_data` / `take_suspension_data`. To resume:

1. Caller delivers `ResumeInput { answer: String }` to `Orchestrator::resume`
2. Domain converts answer to a `ProblemState<D>` via `problem_state_from_resume`
3. Orchestrator continues the run from that state with full `SessionMemory` and `RunContext` intact

### HTTP API Shape

```
POST   /runs          — create a run, start the orchestrator in a background task
GET    /runs/:id/events — SSE stream; replays persisted events then parks for live ones
POST   /runs/:id/answer — deliver user answer to a suspended run
```

A `DashMap<String, mpsc::Sender<String>>` in `AgenticState` connects the HTTP handler to the suspended orchestrator task.

---

## Fan-Out / Scatter-Gather

When `Specifying` decomposes a complex question into parallel sub-queries:

1. `Specifying` produces multiple specs (e.g., metric A and metric B queried separately)
2. Orchestrator emits `FanOut { spec_count, trace_id }`
3. Child orchestrators run concurrently from `Solving` onward, sharing the same `EventStream`
4. Child trace IDs: `"trace-N.0"`, `"trace-N.1"`, `"trace-N.2"`, …
5. Each child emits `SubSpecStart` / `SubSpecEnd` bookends
6. Results collected via `try_join_all`, merged via `DomainSolver::merge_results`
7. Parent continues to `Interpreting` with the merged result

Fan-out and single-path runs are indistinguishable to `Interpreting` — it always receives one `D::Result`.

---

## Architecture

### Crate Layout

```
crates/agentic/
  core/       — Generic FSM framework (domain-agnostic)
  analytics/  — Analytics domain implementation
  connector/  — DatabaseConnector trait + DuckDB impl
  db/         — Sea-ORM entity models for persistence
  http/       — Axum HTTP routes + SSE layer
```

### Layer Diagram

```
Orchestrator (FSM loop, table-driven, deterministic)
  │
  ├── State Handlers (domain workers, call LLM with scoped context)
  │     │
  │     └── LlmClient.run_with_tools()
  │           ├── Thinking tokens → ThinkingToken events (display only)
  │           ├── Content tokens → LlmToken events
  │           ├── Tool calls → ToolCall/ToolResult events
  │           ├── Encrypted thinking blobs preserved WITHIN tool loop
  │           └── Returns LlmOutput { text, thinking_summary, raw_blocks }
  │
  ├── Validators (pure deterministic functions)
  │
  ├── Diagnosis (deterministic routing, uses error types not LLM)
  │
  ├── EventStream (single mpsc channel, everything flows through)
  │
  └── UiTransform (maps CoreEvents to UiBlocks for frontend consumption)
```

### Key Principles

- **LLM doesn't drive transitions.** The orchestrator is deterministic. Validators determine success/failure. `Diagnosing` routes back-edges. The LLM only does generative work within a state.
- **Context is scoped per state.** Each handler receives exactly what it needs. The orchestrator owns accumulated context in `RunContext<D>` and `SessionMemory<D>`.
- **Thinking is read-only.** Streamed as `ThinkingToken` events for display and logging. Never re-enters the system across states.
- **Tools are scoped per state.** Each state declares available tools via `tools_for_state(state_name)`. Solving cannot call Clarifying's tools.
- **All LLM calls are stateless.** No conversation history across FSM state boundaries. Fresh prompt each time. Session history is injected explicitly where needed (Clarifying, Interpreting).
- **Typed back-edges.** `BackTarget<D>` carries the destination state's input data as a typed variant, not a string. Routing mistakes are compile errors.

---

## Generic Domain Trait

```rust
pub trait Domain: Sized + Send + 'static {
    type Intent:  Send + Clone + 'static;
    type Spec:    HasIntent<Self> + Send + Clone + 'static;
    type Solution: Send + 'static;
    type Result:  Send + 'static;
    type Answer:  Send + Clone + 'static;
    type Catalog: Send + 'static;
    type Error:   std::fmt::Display + Send + 'static;
}

pub trait HasIntent<D: Domain> {
    fn intent(&self) -> &D::Intent;
}
```

`HasIntent<D>` is minimal — it exists only so the orchestrator can recover `Intent` from a `Spec` without threading it through every state variant. It is not a chain of traits.

`DomainSolver<D>` provides the workers and hooks:

```rust
trait DomainSolver<D: Domain>: Send + Sync + 'static {
    // Stage skipping (dynamic)
    fn should_skip(state: &str, data: &ProblemState<D>, run_ctx: &RunContext<D>)
        -> Option<ProblemState<D>>;

    // Workers (called by handlers built via build_default_handlers())
    async fn clarify(input, session_memory, catalog, run_ctx, events) -> TransitionResult<D>;
    async fn specify(intent, catalog, run_ctx, events)                -> TransitionResult<D>;
    async fn solve(spec, run_ctx, events)                              -> TransitionResult<D>;
    async fn execute(solution, run_ctx, events)                        -> TransitionResult<D>;
    async fn interpret(result, run_ctx, session_memory, events)        -> TransitionResult<D>;

    // Routing (deterministic, no LLM)
    fn diagnose(error: &D::Error, back: &BackTarget<D>) -> Result<ProblemState<D>, D::Error>;

    // Tools (unix-style: each does one thing)
    fn tools_for_state(state: &str) -> Vec<ToolDef>;
    async fn execute_tool(state: &str, name: &str, params: Value, events) -> Result<Value, ToolError>;

    // Fan-out
    fn merge_results(results: Vec<D::Result>) -> Result<D::Result, Vec<D::Error>>;

    // HITL hooks
    async fn store_suspension_data(data: SuspendedRunData);
    async fn take_suspension_data(run_id: &str) -> Option<SuspendedRunData>;
    fn set_resume_data(&mut self, input: ResumeInput);
    fn problem_state_from_resume(&self, input: ResumeInput) -> ProblemState<D>;
}
```

### Analytics Domain

```rust
struct AnalyticsDomain;

impl Domain for AnalyticsDomain {
    type Intent   = AnalyticsIntent;
    type Spec     = QuerySpec;          // impl HasIntent, carries solution_source
    type Solution = AnalyticsSolution;
    type Result   = AnalyticsResult;
    type Answer   = AnalyticsAnswer;
    type Catalog  = HybridCatalog;
    type Error    = AnalyticsError;
}
```

---

## Table-Driven Orchestrator

States are registered as `StateHandler` configs, not match arms:

```rust
pub struct StateHandler<D, S, Ev> {
    pub next: &'static str,
    pub execute: Arc<dyn Fn(
        &mut S,
        ProblemState<D>,
        &Option<EventStream<Ev>>,
        &RunContext<D>,
        &SessionMemory<D>,
    ) -> BoxFuture<TransitionResult<D>> + Send + Sync>,
    pub diagnose: Arc<dyn Fn(
        &[D::Error],
        u32,
        ProblemState<D>,
    ) -> Option<ProblemState<D>> + Send + Sync>,
}
```

`diagnose` returns `Option<ProblemState<D>>`: `Some(state)` routes to recovery, `None` signals a fatal error that the orchestrator wraps as `OrchestratorError::Fatal`.

### TransitionResult

```rust
impl<D: Domain> TransitionResult<D> {
    fn ok(state: ProblemState<D>) -> Self;          // success, use handler.next
    fn ok_to(state: ProblemState<D>, next: &'static str) -> Self;  // success with explicit next (fan-out)
    fn diagnosing(state: ProblemState<D>) -> Self;  // pass-through a pre-built Diagnosing state
    fn fail(state: ProblemState<D>, errors: Vec<D::Error>) -> Self; // call handler.diagnose
}
```

### Orchestrator Loop

```
loop (max 200 iterations):
  1. Look up handler for current state name
  2. Call solver.should_skip(state, data, run_ctx) → if Some(next) skip to it
  3. Emit StateEnter { state, revision, trace_id }
  4. Call handler.execute(&mut solver, state, events, run_ctx, session_memory)
  5. Match TransitionResult:
     a. ok / ok_to → update run_ctx, advance to next state, emit StateExit(Advanced)
     b. diagnosing → if BackTarget::Suspend → emit AwaitingHumanInput, return Suspended
     c. diagnosing → route back-edge, emit BackEdge, emit StateExit(BackTracked)
     d. fail → call handler.diagnose → None → return Fatal
  6. If state == Done → persist to session_memory, emit Done, return answer
```

`build_default_handlers()` constructs the standard 5-state table from a `DomainSolver<D>`. Domains can replace individual handlers for custom logic.

---

## Events

### Core Events (every domain)

```rust
pub enum CoreEvent {
    StateEnter      { state: String, revision: u32, trace_id: String },
    StateExit       { state: String, outcome: Outcome, trace_id: String },
    BackEdge        { from: String, to: String, reason: String, trace_id: String },
    LlmStart        { state: String, prompt_tokens: u32 },
    LlmToken        { token: String },
    LlmEnd          { state: String, output_tokens: u32, duration_ms: u64 },
    ThinkingStart   { state: String },
    ThinkingToken   { token: String },   // human-readable summary only
    ThinkingEnd     { state: String },
    ToolCall        { name: String, input: Value },
    ToolResult      { name: String, output: Value, duration_ms: u64 },
    ValidationPass  { state: String },
    ValidationFail  { state: String, errors: Vec<String> },
    FanOut          { spec_count: usize, trace_id: String },
    SubSpecStart    { index: usize, total: usize, trace_id: String },
    SubSpecEnd      { index: usize, trace_id: String },
    AwaitingHumanInput { prompt: String, suggestions: Vec<String>, from_state: String, trace_id: String },
    Done            { trace_id: String },
    Error           { message: String, trace_id: String },
}

pub enum Outcome { Advanced, Retry, BackTracked, Failed }
```

### Domain Events (optional per domain)

```rust
pub trait DomainEvents: Send + 'static {}

pub enum Event<Ev: DomainEvents = ()> {
    Core(CoreEvent),
    Domain(Ev),
}
```

### UI Stream Transform

The raw event stream is lower-level than a frontend needs. `UiTransformState<D>` consumes `Event<Ev>` and emits `UiBlock<D>`:

```rust
pub enum UiBlock<D: DomainEvents> {
    StepStart   { label: String },
    StepEnd     { label: String, success: bool },
    ThinkingStart,
    ThinkingToken { token: String },
    ThinkingEnd,
    ToolCall    { name: String, input: Value },
    ToolResult  { name: String, output: Value, duration_ms: u64 },
    TextDelta   { token: String },
    AwaitingInput { prompt: String, suggestions: Vec<String> },
    Domain(D),
    Done,
    Error       { message: String },
}
```

Internal FSM noise (`BackEdge`, `LlmStart/End`, `ValidationPass/Fail`) is filtered. `FanOut`/`SubSpec*` events become user-friendly step blocks. The step label is domain-supplied (`analytics_step_label` for the analytics domain).

### Transport

Single `tokio::sync::mpsc` channel per run. The sender is passed into the orchestrator and down to workers. The consumer holds the receiver. Zero-cost when the receiver is dropped (background runs with no UI subscriber).

---

## Tools

Tools are capabilities available to LLM workers within a state. They are NOT states.

- Each domain declares tools per state via `DomainSolver::tools_for_state(state_name)`
- The LLM client runs a tool loop: call LLM → if tool calls, execute them, feed results back → repeat until text-only response
- `ToolLoopConfig { max_rounds }` prevents runaway loops
- Tool scoping is enforced by the handler — the LLM physically cannot call a tool from a different state's scope

---

## Thinking / Reasoning

### Provider Behavior

**Anthropic (Claude 4+):**

- Returns summarized thinking (human-readable) + encrypted signature
- During tool-use loops: must pass back complete thinking blocks verbatim
- Adaptive thinking recommended for Opus 4.6+

**OpenAI (o3, o4-mini, GPT-5+):**

- Returns `encrypted_content` in reasoning items
- During tool-use: must pass back encrypted reasoning items
- Chat Completions API: reasoning items discarded (stateless, degraded performance)

### Design Rules

1. **Encrypted blobs travel WITHIN a tool loop only.** `LlmClient` preserves `raw_content_blocks` between tool rounds within one state invocation.
2. **Encrypted blobs NEVER cross FSM state boundaries.** Each state makes fresh LLM calls.
3. **Human-readable summaries stream as `ThinkingToken` events.** For display and logging only.
4. **`RetryContext` carries NO thinking.** Retries use errors + optional previous output text.
5. **Each state can have its own `ThinkingConfig`** — different token budgets or effort levels per state.

### Provider Abstraction

```rust
pub enum ThinkingConfig {
    Disabled,
    Adaptive,                      // Claude 4.6+
    Manual { budget_tokens: u32 }, // Claude earlier models
    Effort(ReasoningEffort),       // OpenAI
}

pub trait LlmProvider: Send + Sync {
    async fn stream(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        thinking: &ThinkingConfig,
    ) -> Result<impl Stream<Item = Chunk>, LlmError>;
}
```

---

## Storage

Behind the `storage` feature flag. Four port traits with two adapters:

| Trait                    | Purpose                             |
| ------------------------ | ----------------------------------- |
| `TurnStore`              | Session lifecycle, save/load turns  |
| `QueryLog`               | Append-only log of executed queries |
| `PreferenceStore`        | Key-value user preferences          |
| `SuspendedPipelineStore` | HITL suspend/resume state           |

| Adapter           | Backing                                                   |
| ----------------- | --------------------------------------------------------- |
| `InMemoryStorage` | `Mutex<Vec<…>>`, used in tests                            |
| `JsonFileStorage` | Atomic writes (`write → .tmp → rename`), NDJSON query log |

Artifacts are truncated at 64 KB with `…[truncated]` suffix.

---

## What This Doesn't Solve

- **Ambiguity that resists formalization.** Vibes, emotional context, pragmatic implicature.
- **Temporal reasoning.** The state object is a snapshot, not a history.
- **Problems that don't decompose linearly.** Research-style problems where attempting a solution redefines the problem.
- **The serialization bottleneck.** Structure in the FSM graph doesn't become structure in the LLM's reasoning.
- **Semantic layer coverage gaps.** The LLM fallback path is less reliable than the semantic layer path. Expanding semantic layer coverage reduces dependence on the fallback.

The FSM is a compression and indexing strategy for context, not a representation of understanding. Use it for structured domain state. Keep session memory for tone and conversational continuity, explicit reasoning traces for provenance, and unstructured memory for things that don't fit the schema yet.

---

## References

- Olausson, T. X., Inala, J. P., Wang, C., Gao, J., & Solar-Lezama, A. (2023). "Demystifying GPT Self-Repair for Code Generation." arXiv:2306.09896. Key finding: self-repair is bottlenecked by the model's ability to provide feedback on its own code; fresh generation from spec + error often outperforms iterative repair. Informs the `RetryContext.previous_output` withholding on first retry.
- Huang, J., et al. (2023). "Large Language Models Cannot Self-Correct Reasoning Yet." Key finding: without external feedback, self-correction degrades accuracy. Informs the design principle that validators (external, deterministic) drive transitions rather than LLM self-reflection.
- Anthropic. "Building with Extended Thinking." docs.claude.com. Thinking blocks must be preserved verbatim during tool-use loops; Claude 4+ returns summarized thinking; signature field contains encrypted full thinking.
- OpenAI. "Reasoning Models." developers.openai.com. Encrypted reasoning items must be passed back during tool use; Chat Completions API discards reasoning items.
