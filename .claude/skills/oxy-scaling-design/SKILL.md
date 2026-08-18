---
name: oxy-scaling-design
description: Use when the user asks about Oxy's multi-instance scaling, the split fleet, worker fleet, horizontal scaling, high availability, or how the serve/ide/worker roles divide work. Triggers include "scale Oxy", "scale oxygen", "multi-instance", "split fleet", "worker fleet", "horizontal scaling", "high availability", "OXY_ROLE", "stateful vs HA", "compile boundary", "stateless serving", "durable execution", "shard workspaces", "ephemeral environments", "internal jobs admin".
---

# Oxy multi-instance scaling — quick reference

**The current architecture in one line:** Oxy runs as a **split fleet** keyed by `OXY_ROLE` — a single stateful `ide` (the Factory: working copy + `.git` + compile + new-run execution), a horizontally-scaled stateless `serve` fleet (reads Postgres + S3 only), and a `worker` queue drainer. This is a Postgres primary/read-replica shape: the `ide` is the primary, the serve fleet is the read replicas, and the **compile boundary** (compiled `*_definitions` rows + S3 blobs, keyed by `revision_id`) is the replicated read model.

**Read these first when grounding a decision:**
- `internal-docs/multi-instance-fleet.md` — how the fleet works today (roles, the stateful-vs-HA matrix, route classification, the `super_read_only` guard, graceful degradation, code map). **The primary reference.**
- `internal-docs/ephemeral-workspace-environments.md` — the forward implementation plan (the crate/binary split that makes "FS in a stateless service" a compile error).
- `internal-docs/multi-instance-fleet.md` — the original phase ledger + the rejected-alternatives list.
- `internal-docs/compile-boundary.md` — operator runbook.

## What is built (the current reality)

- **Split fleet via `OXY_ROLE`** (`ide | serve | worker | all`). One knob selects the topology; everything else derives from it. A `serve` replica derives in-process workers + the periodic global driver OFF; every other role ON. The legacy flags (`OXY_DISABLE_INPROCESS_WORKERS`, `OXY_INPROC_GLOBAL_WORKER`, `--no-workers`) survive only as two-directional overrides.
- **Compile boundary (compile-complete stateless serving).** The `ide` compiles the working copy → Postgres `*_definitions` rows + S3 blobs per `revision_id` → promotes via `workspaces.current_revision_id`. The serve fleet reads the promoted revision and **never walks the workspace FS**. This is why the read path needs no working copy — and therefore no per-request clone.
- **Self-routing.** `role_manifest::classify` is the single routing authority; a serve replica reverse-proxies `IdeOnly` requests to `OXY_IDE_UPSTREAM` (`ide_proxy`). Replaces the drift-prone external route table that caused three outages.
- **Route classification + HA carve-out.** `IDE_ONLY_PATTERNS` (FS/exec/live-stream → ide) + `FLEET_OK_READ_PATTERNS` (Postgres/S3 reads buried under an IdeOnly wildcard stay HA) + drift tests + the `super_read_only` runtime guard. See the `oxy-route-classification` skill.
- **Runtime-artifact S3 mirror** (`runtime_artifact.rs`) — charts/results/app-data mirror to the compile-boundary bucket so any replica serves them; ide-down charts degrade to the S3 mirror.
- **Schedules/monitors fire without a leader.** The periodic global driver runs on every eligible node; `tick_schedules`/`tick_monitor_schedules` CAS-advance `next_run_at` so firing is exactly-once across replicas. (Leader election was tried and removed — the CAS already guarantees it, and running on all eligible nodes is better HA.)
- **Backpressure** — admission control (global ceiling + per-tenant fairness, 503 + `Retry-After`), worker HPA on outstanding work (queued + claimed) against capacity — see the recipe in `internal-docs/worker-fleet.md`; queue depth alone goes absent when the fleet keeps up.
- **Migrations** — a dedicated migrate Job owns the schema; `serve`/`compile`/`worker` honour `OXY_SKIP_MIGRATIONS`; a startup advisory lock serialises co-booting nodes.

## What is pending

- **Ephemeral-env crate/binary split** (the strongest enforcement): make the serve binary link no FS code so "FS in a stateless service" is a *compile error*, not a runtime miss. Reviewed, staged — see the ephemeral-env design. Keep env-count at **one** Factory until a measured production limit forces a small sharded pool.
- **Durable execution** — replay-deterministic agentic-runtime orchestrator (independent).
- **Generated artifacts → S3** — charts/results/app-data already mirror via `runtime_artifact`; embeddings + parquet caches still pending.
- **gix migration** of `crates/git/` read paths — general git-read modernisation, in flight.

## Hard constraints (don't violate)

1. **Code-first is sacred** — the filesystem IS the data model (like dbt). Agents/automations/apps/semantic views live as YAML in git. **Never introduce a parallel source of truth** (S3 snapshots, DB-backed file storage of definitions). Generated artifacts are different — those go to S3.
2. **Git is the source of truth** — GitHub origin in cloud, local repo in local mode. PRs, branches, commits stay first-class ide actions. The Factory's local disk is a cache, re-cloned from origin on restart.
3. **HTTP is stateless beyond the request** — anything a serve replica needs must be in Postgres, S3, or reconstructable from origin. Reads serve from any replica; only writes/execution touch the Factory.
4. **One fencing primitive for the Factory:** the StatefulSet `replicas: 1` at-most-one guarantee. There is **no workspace-ownership lease** (it was built and reverted 2026-06-14 — at replicas=1 it guards a multi-producer race that can't occur) and **no leader election** (the `next_run_at` CAS gives exactly-once). The task-claim lease in `agentic_task_queue` is the worker's, and is unrelated.

## What was explicitly rejected (don't re-debate)

- **Smart cloning** for the serve/worker read path (partial/shallow/sparse clone + LRU clone cache + mirror updater). The compile boundary makes the read path need no working copy, so it needs no clone. A working copy lives only on the Factory, which already has its checkout. (A shelved implementation exists on `feat/smart-cloning` if cold-start cloning is ever needed for the *build* plane — not a scaling plan.)
- Sourcegraph gitserver — license flipped proprietary; dead upstream.
- Gitaly + Praefect — assumes GitLab Rails as auth source; nobody runs it standalone.
- Mononoke (Meta) — GPL-2.0, no outside production deployments, exotic build.
- libgit2 / git2-rs — superseded by gix; Cargo migrated off it.
- Apalis / Hatchet / Temporal / River / pgmq — Oxy has its own orchestrator; no parallel queue framework.
- S3 snapshot as workspace truth — violates code-first; sync nightmare with git.
- EFS/NFS shared filesystem — git over network FS is fragile.

## When this skill applies

- "How do we scale Oxy?" / "is it HA?" → this skill + `multi-instance-fleet.md`.
- A change that touches role split, the worker fleet, the compile boundary, or workspace ownership → ground it here, then read the relevant doc before implementing.
- "Why does X exist / why not Y?" → trace to the constraints + rejected list above.
- A route/handler change → defer to the `oxy-route-classification` skill (IdeOnly vs FleetOk vs the HA carve-out).
- Long-running/background work → the `oxy-task-spec-default` skill (TaskSpec on the worker fleet, not `tokio::spawn` in a handler).

## Refs

- Fleet guide (primary): `internal-docs/multi-instance-fleet.md`
- Forward plan: `internal-docs/ephemeral-workspace-environments.md`
- Phase ledger + rejected alternatives: `internal-docs/multi-instance-fleet.md`
- Operator runbook: `internal-docs/compile-boundary.md`
- Worker fleet dev guide: `internal-docs/worker-fleet.md`; scope survey: `internal-docs/worker-fleet.md`
- Backend architecture rules: `internal-docs/backend-architecture.md`
