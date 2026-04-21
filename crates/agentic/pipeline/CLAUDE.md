# agentic-pipeline

High-level facade for starting and driving agentic pipelines. This is the **composition layer** — the only crate that knows about all domains. Both `agentic-http` and the CLI depend on this crate instead of importing domain crates directly.

## Key Types

```rust
// Callers assemble the platform adapter + builder bridges in their host
// crate (for Oxy: `app::agentic_wiring`) and pass them in.
let project_ctx = Arc::new(OxyProjectContext::new(workspace_manager));
let platform: Arc<dyn PlatformContext> = project_ctx.clone();
let bridges = build_builder_bridges(project_ctx);

// Fluent builder for pipeline setup
PipelineBuilder::new(platform.clone())
    .with_builder_bridges(bridges)   // required for builder domain
    .analytics("agent_id")            // or .builder(model)
    .question("What is revenue?")
    .thread(thread_uuid)
    .thinking_mode(ThinkingMode::Auto)
    .start(&db).await?                // → StartedPipeline

// Type-erased handle — hides domain event type
started_pipeline.drive(db, state, answer_rx, cancel_rx).await;
```

## What This Crate Provides

| Function | What it does |
| ---------- | ------------- |
| `PipelineBuilder::start()` | Config loading + connector resolution + solver building + DB insert + pipeline startup |
| `StartedPipeline::drive()` | Type-erased `drive_pipeline()` call — works for any domain |
| `build_event_registry()` | Creates `EventRegistry` with all domains pre-registered |
| `insert_run()` | Runtime run insert + analytics extension insert |
| `update_run_done()` | Runtime update + analytics spec_hint extension |
| `update_run_thinking_mode()` | Analytics extension update |
| `get_thread_history()` | Runtime query + analytics extension join |
| `get_analytics_extension[s]()` | Extension table queries |
| `run_agentic_eval()` | Headless pipeline execution for eval/testing |
| `platform::resolve_connectors()` | Resolve database names → `ConnectorConfig` via `ProjectContext` |
| `platform::build_llm_client()` | Map a `ResolvedModelInfo` to a concrete `LlmClient` |

## Config Path Resolution

`PipelineBuilder::start_analytics()` resolves config paths with extension fallback:

1. Try literal path: `base_dir/agent_id`
2. If not found, try: `base_dir/agent_id.agentic.yml`
3. If neither exists, return the original path (produces a clear error)

## Platform Boundary

This crate has **zero `oxy::*` dependencies**. All access to host-project
state goes through three traits declared in [`platform`]:

| Trait | Purpose |
| ------- | --------- |
| `platform::ProjectContext` | `resolve_connector`, `resolve_model`, `resolve_secret` |
| `platform::ThreadOwnerLookup` | `thread_owner(thread_id)` — used by HTTP for auth |
| `platform::PlatformContext` | Supertrait: `ProjectContext + agentic_workflow::WorkspaceContext`. Anywhere pipeline needs both, take `Arc<dyn PlatformContext>`. |

Plus a bundle for the builder domain:

| Type | Contents |
| ------ | ---------- |
| `platform::BuilderBridges` | The four `Arc<dyn Builder{Database,SchemaProvider,SemanticCompiler,ProjectValidator}>` impls the builder pipeline needs |

Adapters live in the host. For Oxy they live in
[`app::agentic_wiring`](../../app/src/agentic_wiring/) — `OxyProjectContext`
implements `PlatformContext`, `OxyThreadOwnerLookup` implements
`ThreadOwnerLookup`, and `build_builder_bridges()` assembles the bridges.
Platform-refactor work happens there, not here.

When adding a new platform touchpoint, add a method to the relevant port
trait in this crate and an impl in the host adapter. Do not add `oxy` as a
dependency to this crate.

## Rules

- This crate MAY import all agentic crates — it's the composition layer.
- This crate MUST NOT import `oxy`, `oxy-shared`, `entity`, or any other platform crate.
- Consumers (`http`, `app/cli`) should import `agentic-pipeline`, NOT domain crates.
- Re-exports domain types that consumers need: `ThinkingMode`, `SchemaCatalog`, `BuilderTestRunner`.

## Testing

- Integration tests: `OXY_DATABASE_URL=... cargo nextest run -p agentic-pipeline --test integration_tests` (8 tests)
- The workflow-recovery test provides its own `FakePlatform` impl of `PlatformContext` rather than constructing a real `WorkspaceManager` — the test deliberately exercises only the DB-driven executor arm.
