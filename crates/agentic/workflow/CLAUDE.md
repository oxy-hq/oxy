# agentic-workflow

Procedure execution bridge between the agentic analytics domain and `oxy-workflow`.

## Key Types

```rust
pub struct OxyProcedureRunner {
    project_manager: ProjectManager,
    procedure_files: Vec<PathBuf>,
    event_tx: Option<EventStream<AnalyticsEvent>>,
}

impl ProcedureRunner for OxyProcedureRunner {
    async fn run(&self, file_path: &Path) -> Result<ProcedureOutput, ProcedureError>;
    async fn search(&self, query: &str) -> Result<Vec<ProcedureRef>, ProcedureError>;
}
```

## Event Bridge

`WorkflowEventBridge` translates oxy-workflow task lifecycle events into `AnalyticsEvent` variants:

- `ProcedureStarted` — emitted with full step DAG before execution
- `ProcedureStepStarted` / `ProcedureStepCompleted` — per-task progress
- `ProcedureCompleted` — final success/failure

## Usage

Created in `agentic-pipeline` during analytics solver building:

```rust
let runner = OxyProcedureRunner::new(project_manager)
    .with_procedure_files(procedure_files)
    .with_events(event_stream);
solver.with_procedure_runner(Arc::new(runner))
```

## Rules

- Depends on `agentic-core`, `agentic-analytics` (for event types), `oxy`, `oxy-workflow`.
- Only used by the analytics domain — the builder domain does not run procedures.
