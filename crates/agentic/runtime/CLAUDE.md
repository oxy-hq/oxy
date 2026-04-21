# agentic-runtime

Transport-agnostic execution infrastructure for agentic pipelines. Provides run lifecycle management, event persistence, and streaming — used by both HTTP and CLI.

## Modules

| Module | Purpose |
| -------- | --------- |
| `entity/` | SeaORM models for `agentic_runs`, `agentic_run_events`, `agentic_run_suspensions` |
| `crud` | All database operations (insert/update/query runs, events, suspensions) |
| `migration` | Migrator with `seaql_migrations_orchestrator` tracking table |
| `state` | `RuntimeState` — in-memory run management (notifiers, channels, status cache) |
| `handle` | `PipelineHandle<Ev>` and `PipelineOutcome` — domain-agnostic pipeline interface |
| `bridge` | `run_bridge()` — event channel → batch DB writes → notify subscribers |
| `outcome` | `drive_pipeline()` — outcome loop (done/suspended/failed/cancelled → DB state) |
| `event_registry` | `EventRegistry` + `StreamProcessor` — domain-aware event deserialization for SSE |

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

## Testing

- Unit tests: `cargo nextest run -p agentic-runtime` (10 tests, no DB required)
- Integration tests: `OXY_DATABASE_URL=... cargo nextest run -p agentic-runtime --test integration_tests` (8 tests, requires PostgreSQL)
