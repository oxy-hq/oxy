# Agentic Subsystem

## Architecture

Three-layer design — each layer has strict dependency rules:

```
          domains (analytics, builder)
               │
          pipeline (facade / composition)
               │
          runtime (execution infrastructure)
               │
          core (pure FSM)
```

| Layer | Crates | May depend on | Must NOT depend on |
| ------- | -------- | --------------- | ------------------- |
| **Core** | `agentic-core` | External only (serde, tokio) | Any `agentic-*` crate |
| **Runtime** | `agentic-runtime` | `core` | analytics, builder, connector, llm, pipeline, http |
| **Infrastructure** | `agentic-connector`, `agentic-llm` | `core` | analytics, builder, runtime, pipeline, http |
| **Domains** | `agentic-analytics`, `agentic-builder` | `core`, `runtime`, `connector`, `llm` | Each other, pipeline, http |
| **Pipeline** | `agentic-pipeline` | All agentic crates, `oxy` | `http` |
| **HTTP** | `agentic-http` | `pipeline`, `runtime`, `oxy`, `oxy-auth` | analytics, builder, connector, llm, core, entity |

## Crate Responsibilities

| Crate | What it owns | Key types |
| ------- | ------------- | ----------- |
| `core` | FSM framework | `Domain`, `DomainSolver`, `Orchestrator`, `ProblemState`, `CoreEvent`, `UiBlock` |
| `runtime` | Run lifecycle, persistence, event streaming | `RuntimeState`, `PipelineHandle`, `EventRegistry`, `StreamProcessor`, entity models |
| `pipeline` | Pipeline setup, config resolution, type erasure | `PipelineBuilder`, `StartedPipeline`, `ThinkingMode` |
| `analytics` | Analytics solver, semantic layer, extension table | `AnalyticsSolver`, `AnalyticsEvent`, `SchemaCatalog`, `AnalyticsMigrator` |
| `builder` | Builder solver, file tools, propose_change | `BuilderSolver`, `BuilderEvent`, `BuilderTestRunner` |
| `connector` | Database backends | `DatabaseConnector`, `ConnectorConfig`, `SchemaInfo` |
| `llm` | LLM provider abstraction | `LlmClient`, `LlmProvider`, `ThinkingConfig` |
| `http` | Axum route handlers | `AgenticState`, `router()`, route handlers |
| `workflow` | Procedure runner | `OxyProcedureRunner`, `WorkflowEventBridge` |

## Migration Strategy

Three independent SeaORM migrators with separate tracking tables:

| Migrator | Tracking table | Location | Owns |
| ---------- | --------------- | ---------- | ------ |
| Central | `seaql_migrations` | `crates/migration/` | Platform + conversation tables |
| Runtime | `seaql_migrations_orchestrator` | `agentic-runtime` | `agentic_runs`, `agentic_run_events`, `agentic_run_suspensions` |
| Analytics | `seaql_migrations_analytics` | `agentic-analytics` | `analytics_run_extensions` |

Startup order: Central -> Runtime -> Analytics.

## Domain Extension Pattern

Domain-specific run data lives in extension tables, not on the generic `agentic_runs` table:

```
agentic_runs (runtime)          analytics_run_extensions (analytics domain)
├── id (PK)                     ├── run_id (PK, FK → agentic_runs.id)
├── question                    ├── agent_id
├── status                      ├── spec_hint (JSONB)
├── answer                      └── thinking_mode
├── source_type
├── metadata (JSONB)
└── ...
```

New domains add their own extension table with their own migrator. The runtime table stays generic.

## Adding a New Domain

1. Create `crates/agentic/<domain>/` implementing `DomainSolver` from `core`
2. Add `start_pipeline()` returning `runtime::PipelineHandle`
3. Register event handler via `event_handler()` returning `DomainHandler`
4. (Optional) Add extension table with own migrator
5. Wire into `agentic-pipeline`: add to `PipelineBuilder` + `ErasedHandle` + `build_event_registry()`
6. **No changes needed** to `runtime`, `core`, or `http`

## Key Rules

- **Runtime is transport-agnostic** — no axum, no HTTP types. Works from HTTP, CLI, gRPC, or tests.
- **Entities are domain-private** — `agentic-http` has zero `entity` crate imports.
- **Cross-domain references are loose** — plain UUID columns, no FK constraints. Application-level cleanup.
- **Events are serialized as `(event_type, payload JSON)`** — the `EventRegistry` handles domain-specific deserialization at read time via registered `RowProcessor` callbacks.
