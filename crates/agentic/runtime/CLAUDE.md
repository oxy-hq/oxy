# agentic-runtime

Transport-agnostic execution infrastructure for agentic pipelines. Provides run lifecycle management, event persistence, and streaming — used by both HTTP and CLI.

## Layout

Stage 1 of the airway/airform extraction split the crate into two sub-layers; pick the layer your code actually needs.

| Layer | Modules | Purpose |
| -------- | --------- | --------- |
| `lifecycle/` | `state`, `handle`, `bridge`, `event_registry`, `entity/{run,run_event,run_suspension}`, `crud/{runs,events,suspension,queries}` | The "what a run *is*" half: run row + event log + suspensions + SSE plumbing. Zero orchestrator deps. |
| `orchestrator/` | `coordinator/`, `worker`, `router/`, `transport/`, `circuit_breaker`, `background`, `entity/{task_queue,task_outcome}`, `crud/{queue,outcomes,recovery}` | The "how a run *executes*" half: durable task queue + coordinator + worker pool + transports. Built on top of lifecycle. |
| (root) | `migration` | Single SeaORM migrator covering both layers (`seaql_migrations_orchestrator` tracking table). |

The legacy flat paths (`agentic_runtime::coordinator`, `agentic_runtime::state`, `agentic_runtime::crud::*`, etc.) are still re-exported at the crate root for back-compat with the ~180 external callsites — new external code should prefer the canonical `lifecycle::…` / `orchestrator::…` paths so the layering shows up in `use` lists. Inside this crate the canonical paths are mandatory; the flat aliases are external-only.

## Rules

- **Never import domain crates** (analytics, builder, connector, llm). Only depends on `agentic-core`.
- **Never import HTTP types** (axum). This crate is transport-agnostic.
- Domain-specific behavior is injected via:
  - `serialize_fn: Fn(&Event<Ev>) -> (String, Value)` for bridge task serialization
  - `RowProcessor` closures registered in `EventRegistry` for deserialization
  - `OnResumeFn` callback for domain-specific resume logic
- The `agentic_runs` table has no domain-specific columns — only `source_type` and `metadata` (JSONB).
- `PipelineHandle<Ev>` requires `Ev: DomainEvents` — domains provide the concrete type, runtime handles it generically.

## Key Types

```rust
// Transport-agnostic state — used by HTTP server and CLI
pub struct RuntimeState {
    pub notifiers: DashMap<String, Arc<Notify>>,  // wake SSE/CLI subscribers
    pub answer_txs: DashMap<String, Sender<String>>,  // HITL answers
    pub cancel_txs: DashMap<String, watch::Sender<bool>>,  // cancellation
    pub statuses: DashMap<String, RunStatus>,  // in-memory cache
}

// Domain-agnostic pipeline handle
pub struct PipelineHandle<Ev: DomainEvents> {
    pub events: Receiver<Event<Ev>>,
    pub outcomes: Receiver<PipelineOutcome>,
    pub answers: Sender<String>,
    pub cancel: CancellationToken,
    pub join: JoinHandle<()>,
}
```

## Integrating a new system

If you're standing up a new top-level system on top of this runtime
(airform, future ELT/transformation runners — airway is already on the
queue-driven pattern below), follow the walkthrough in
[`internal-docs/agentic-runtime-integration.md`](../../../internal-docs/agentic-runtime-integration.md).
It covers three patterns:

- **Pipeline-style** (analytics, builder) — `DomainSolver` +
  `PipelineHandle`. Lifecycle only; no orchestrator queue.
- **One-shot queue work** (per-workspace health eval) —
  `TaskSpec::Custom { kind }` + a `CustomTaskRegistry` executor registered by
  the host. One durable unit, no FSM, no resume.
- **Queue-driven** (automation and airway; airform still to come) — dedicated
  `TaskSpec` variants drained off the durable task queue; coordinator does the
  fan-out + resume. **Currently a stub** in that doc — read
  `crates/agentic/automation/` and `crates/agentic/airway/` for now.

The integration doc has the API surface for both layers, the
contributor checklist, and the rules around extension tables and
migrators.

## Testing

- Unit tests: `cargo nextest run -p agentic-runtime`
- Integration tests: `OXY_DATABASE_URL=... cargo nextest run -p agentic-runtime --test integration_tests` (requires PostgreSQL — use testcontainers, never the dev DB)
