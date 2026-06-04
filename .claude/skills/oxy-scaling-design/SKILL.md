---
name: oxy-scaling-design
description: Use when the user asks about Oxy's multi-instance scaling, worker fleet, workspace ownership leases, horizontal scaling design, or any topic from the scaling design doc. Triggers include "scale Oxy", "scale oxygen", "multi-instance", "worker fleet", "lease table", "horizontal scaling", "Phase 1/2/3/4/5/6/7 of scaling", "gix migration", "smart cloning", "durable execution", "shard workspaces", "Envoy ring hash", "internal jobs admin", "workspace ownership".
---

# Oxy multi-instance scaling — quick reference

Full design at `internal-docs/2026-05-31-scaling-oxy-multi-instance-architecture.md`. Read that file first when grounding a decision.

## Current phase status (2026-05-31)

| Phase | Description | Status |
|-------|-------------|--------|
| 1 | Worker fleet separation (`oxy worker` standalone + `oxy serve --no-workers` + Internal Jobs admin UI) | ✅ DONE — PR #2409 |
| 1.5 | Workspace ownership lease table | ⏳ Pending |
| 1.75 | Move ad-hoc background work into TaskSpec (scope survey lists candidates) | ⏳ Pending |
| 2 | Multi-instance HTTP with Envoy ring-hash routing + 307 fallback | ⏳ Pending (depends on 1.5) |
| 3 | Smart cloning — `--filter=blob:none --depth N` + cone-mode sparse-checkout | ⏳ Pending (depends on 4) |
| 4 | gix migration of `crates/git/` read paths | 🔄 In flight |
| 5 | LRU clone cache + background mirror updater | ⏳ Pending (depends on 3) |
| 6 | Durable execution — replay-deterministic agentic-runtime orchestrator | ⏳ Pending (independent) |
| 7 | Generated artifacts → S3 (embeddings, parquet caches, build outputs) | ⏳ Pending (independent) |

## Hard constraints (don't violate)

1. **Code-first is sacred** — the filesystem IS the data model (like dbt). Agents/procedures/apps/semantic views live as YAML files in git. **Never introduce a parallel source of truth** (S3 snapshots, DB-backed file storage of definitions). Generated artifacts are different — those can go to S3.
2. **Git is the source of truth** — GitHub origin in cloud mode, local-only repo in local mode. PRs, branches, commits stay first-class IDE actions.
3. **Two distinct lease primitives** — task-claim (worker, exists in `agentic_task_queue`) and workspace-ownership (HTTP, Phase 1.5 will add). They are NOT the same lease. Workers do NOT consult the workspace-ownership lease.
4. **HTTP is stateless beyond the lease** — anything else must move to Postgres, S3, or be reconstructable from origin.

## Operational primitives (Phase 1 + in-flight)

- **`oxy worker`** — standalone worker process. `OXY_DATABASE_URL`, `OXY_WORKER_MAX_INFLIGHT`, `--health-port`. Drain queue, no HTTP.
- **`oxy serve --no-workers`** — HTTP-only frontend.
- **Internal Jobs admin UI** — `/admin/internal-jobs`, gated by `OXY_OWNER`. Worker fleet health, queue stats, dead-letter management. Distinct from the customer-facing Coordinator/Orchestrator UI.
- **`worker_id = <hostname>@<pid>`** — stable instance identity threaded through every span and structured log.

## Architecture refinements that override the original sketch

Read sections A–H of the design doc. The key surprises from Phase 1 implementation:

- **A**: Workspace ownership lease (Phase 1.5) is HTTP-only. Workers use task-claim semantics, not the workspace lease.
- **B**: Recovery loop is currently split (HTTP driver + worker reaper). Should unify into the worker fleet eventually; blocker is workspace enumeration from the worker process in cloud mode.
- **C**: Backpressure is now explicit — queue-depth alerts, worker HPA on queue depth, HTTP-side admission control with 503 + `Retry-After`.
- **D**: Customer vs system task distinction propagates — add `task_kind = "customer" | "system"` on task events, separate metrics/SLOs.
- **E**: `worker_id` enables per-worker traces, capacity planning, version drift detection. Add `claimed_by` and `claimed_at` columns to `agentic_task_queue` to formalize.
- **F**: Workers need their OWN per-worker clone cache (Phase 3 + Phase 4 are preconditions for fully separating worker fleet from HTTP fleet).
- **G**: Phase ordering is parallel, not linear: {1.5} + {1.75} + {4→3→5} + {6} + {7} run independently post-Phase 1.
- **H**: New features default to TaskSpec — see the `oxy-task-spec-default` skill.

## What was explicitly rejected (don't re-debate)

- Sourcegraph gitserver — license flipped to proprietary; dead upstream.
- Gitaly + Praefect — assumes GitLab Rails as auth source; nobody runs it standalone.
- Mononoke (Meta) — GPL-2.0, no outside production deployments, exotic build.
- libgit2 / git2-rs — superseded by gix; Cargo migrated off it.
- Apalis / Hatchet / Temporal / River / pgmq — Oxy has its own orchestrator already; no parallel queue framework.
- S3 snapshot as workspace truth — violates code-first; creates sync nightmare with git.
- EFS/NFS shared filesystem — git over network FS is fragile.

## When this skill applies

- User asks "how do we scale Oxy?" → point at this skill + the design doc.
- User proposes a change that conflicts with the design → reference the relevant refinement (A–H).
- User asks "why does X exist?" → trace to phase + design rationale.
- User asks to add multi-instance support to some specific feature → check which phase it depends on.

## Refs

- Full design: `internal-docs/2026-05-31-scaling-oxy-multi-instance-architecture.md`
- Worker fleet dev guide: `internal-docs/worker-fleet.md`
- Scope survey: `internal-docs/2026-05-28-worker-fleet-scope-survey.md`
- Internal Jobs admin design: `internal-docs/internal-jobs-admin-design.md`
- Backend architecture rules: `internal-docs/backend-architecture.md`
