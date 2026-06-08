# oxy-cameras

Camera fleet domain crate. Owns:

- **Postgres entities + migrator** for the `Fleet` aggregate
  (Site → EdgeBox, Site → Camera).
- **Control-plane HTTP routes** under `/control/*` consumed by edge
  workers and the operator UI.
- **Service layer** — registration, config fetch, event ingest,
  zone PATCH, UniFi onboarding.
- **Airhouse-side DDL** for the high-volume time-series tables
  (camera_events / *_health / compliance_reports); created per-tenant
  by `TenantProvisioner`.
- **Per-device JWT auth middleware**.

## Where things go

| Concern | Location |
|---|---|
| SeaORM entities | `entities/{sites,edge_boxes,cameras,device_registry,device_claims}.rs` |
| Per-domain migrator | `migration.rs` → `seaql_migrations_cameras` |
| Postgres business logic | `src/service/` |
| Airhouse ingest + DDL | `src/airhouse/` |
| Axum routes (mounted via app) | `src/routes/` |
| Per-device JWT auth | `src/auth/` |
| Vendor onboarding logic | `src/service/onboarding.rs` (uses `oxy-unifi`) |

## Dependency rules

- **Allowed**: `oxy-shared`, `oxy-unifi`, `sea-orm`, `axum`, plus
  whatever the existing platform crates use (`reqwest`, `serde`, etc.).
- **Forbidden**: imports from `agentic-*` (per
  `backend-architecture.md` — platform crates never import agentic).
- **Cross-aggregate refs**: `sites.workspace_id` is a loose `Uuid`
  column with no FK constraint (per `domain-boundaries.md` P3).
  Application code cleans up cross-aggregate state.

## Data split — Postgres vs Airhouse

| Table | Where | Why |
|---|---|---|
| `sites` | Postgres | Slow-changing config, references `workspaces` (loose) |
| `edge_boxes` | Postgres | Slow-changing config |
| `cameras` | Postgres | Config-heavy (name, vendor IDs, zones_json) |
| `device_registry` | Postgres | Per-device HMAC secret — must be in app DB |
| `device_claims` | Postgres | Operator-claimed device ↔ edge_box mapping |
| `camera_events` | Airhouse | High-volume time-series (~216M/day at scale) |
| `camera_health` | Airhouse | High-volume time-series |
| `box_health` | Airhouse | Moderate time-series |
| `camera_compliance_reports` | Airhouse | Analytics access pattern |

Writes to Airhouse go through `AirhouseTokenBroker.mint_for_system(workspace_id,
SystemPurpose::EdgeIngest)` — no Airhouse credentials on the edge.

## Reference

- [`internal-docs/video-processing-fleet-architecture.md`](../../internal-docs/video-processing-fleet-architecture.md)
- [`internal-docs/domain-boundaries.md`](../../internal-docs/domain-boundaries.md)
- [`internal-docs/airhouse-integration.md`](../../internal-docs/airhouse-integration.md)
