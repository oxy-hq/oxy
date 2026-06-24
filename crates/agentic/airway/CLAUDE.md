# agentic-airway

Pattern B subsystem on `agentic-runtime`: queue-driven ELT pipeline
runtime that wraps the external [airway] engine. Sibling of
`agentic-automation` in shape and dependency posture.

[airway]: https://github.com/oxy-hq/airway-internal

## Status

Shipped. The `TaskSpec::Airway` variant, `AirwayEvent` taxonomy, source
and destination factories, the `airway_*` state store/aggregates, and
the real `AirwayWorker` (engine end-to-end, event bridging, load-audit
finalisation) are all in place and exercised by both the HTTP run path
and `oxy airway run`.

## Pipeline

```
TaskSpec::Airway → AirwayWorker.execute() → engine end-to-end → done|failed
```

Single queue row per run. No per-step decisions, no fan-out at the
coordinator. Resource-level fan-out happens inside
`airway::extract_parallel`.

## Aggregates owned

- **PipelineState** (`airway_pipeline_state`) — incremental cursor +
  schema state, keyed by `pipeline_name`.
- **LoadAudit** (`airway_load_audit`) — per-extraction audit row,
  keyed by `load_id`.
- **AirwayRunExtension** (`airway_run_extensions`) — per-run metadata
  extending `agentic_runs`.

Migrator: `AirwayMigrator`, tracking table `seaql_migrations_airway`.

## Rules

- Must NOT depend on `oxy`, `oxy-shared`, `entity`, or any other
  platform crate.
- Must NOT depend on `agentic-analytics`, `agentic-builder`,
  `agentic-automation`, `agentic-pipeline`, or `agentic-http`.
- May depend on `agentic-core`, `agentic-runtime`, `agentic-connector`
  (postgres feature, added in stage 3), and the external `airway`
  engine.
- Cross-aggregate refs (`airway_run_extensions.load_id` →
  `airway_load_audit.load_id`) are loose UUIDs — no DB FK to other
  aggregates.

## Testing

- Unit tests: `cargo nextest run -p agentic-airway`
- Integration tests need a real Postgres — use testcontainers, never
  the dev DB.
