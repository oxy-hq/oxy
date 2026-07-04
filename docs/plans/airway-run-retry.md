# Airway run retry — reset-in-place + per-run cursor state

**Branch:** `feat/airway-run-retry` · **Status:** in progress (P1) · **Date:** 2026-07-03

## Problem

Two coupled issues surfaced by the chunked backfill:

1. **No per-chunk/per-run cursor.** A run (esp. a backfill chunk) extracts its whole
   window; on failure it has nothing persisted, so a retry re-extracts everything. The
   incremental cursor lives per-*pipeline* (`airway_pipeline_state`, PK `pipeline_name`),
   frozen for backfills — the run itself records nothing about where it left off.
2. **Retry clones a new run.** `retry_run` (`pipeline/src/retry.rs`) is *clone-and-reseed*:
   it seeds a brand-new `agentic_runs` row tagged `metadata.retry_of=<orig>` and leaves the
   original as-is. So retries proliferate runs, and there's no single durable run whose
   state you update. The partial "retry failed tables" button starts yet another scoped new
   run (`StartAirwayRequest.resources`).

## Decisions (locked)

- Cursor state lives on the **run extension** (`airway_run_extensions`, PK `run_id`, FK→
  `agentic_runs` `ON DELETE CASCADE`).
- Retry is **reset-in-place** for **all airway runs** (not just backfill): re-drive the
  same `run_id`, don't mint a new one. Fully replace clone-reseed for airway, with a
  **fallback to reseed only if the run's queue row / spec was reaped**.
- **Track `retry_count`** on the run extension and **show it in the UI**.
- Per-run **cursor resume** is **backfill-scoped**; normal incremental runs keep the
  pipeline-global cursor.
- Partial "retry failed tables" is **removed** — a whole-run reset-in-place retry re-runs
  all streams and the cursor skips what's already done.

## Feasibility (verified)

Reset-in-place needs no new plumbing — the primitives exist, just aren't wired to retry:
- `requeue_task` (`orchestrator/crud/queue.rs:223`) re-enqueues by `task_id`; for an airway
  root `task_id == run_id` (`airway_run.rs` enqueue).
- `transition_run` (`lifecycle/crud/mod.rs:65`) is unguarded — sets a terminal run back to
  `running`.
- Crash-recovery (`recovery.rs`) already resets-in-place, but only for non-terminal stuck
  runs (`find_stuck_runs` excludes `failed/done/cancelled`).

## Design

1. **Cursor + retry count on the run extension** (P1). Add `resume_state jsonb null` and
   `retry_count integer not null default 0` to `airway_run_extensions`. Worker persists the
   per-run cursor as it loads; retry reads it to resume. `retry_count` bumped on each
   reset-in-place retry.
2. **Retry = reset-in-place** (P2, airway). `retry_airway` →
   `transition_run(run,"running")` + `requeue_task(run_id, spec)` + bump `retry_count`,
   same `run_id`. Fallback to clone-reseed only if the queue row/spec is gone (reaped).
   Automation/workflow retry unchanged.
3. **Backfill driver re-drives the existing run** (P2). `resume_backfill_range` currently
   spawns a new run per not-`done` chunk; re-drive the chunk's existing `run_id` in place so
   the checkpoint `run_id` is stable and the chunk resumes from its cursor.
4. **Drop partial retry** (P3). Remove `RetryFailedTablesButton` + the `resources`-subset
   new-run path (call sites `Elt/index.tsx:103`, `airway/index.tsx:281`).
5. **UI** (P3): whole-run **Retry** button on the orchestrator run detail (`RunHeader`) and
   the IDE airway/ELT detail; the orchestrator list **retry-selected** already hits the
   retry endpoint. Show `retry_count` on the run views.

Folds in the **schema reset** (clears `airway_pipeline_state`) as a UI action, sequenced
after the state model settles.

## Phases

- **P1 (this):** migration + entity for `resume_state` + `retry_count`; accessor helpers.
- **P2:** worker persists/reads `resume_state`; `retry_airway` reset-in-place (+ reap
  fallback); backfill driver re-drives existing run; bump `retry_count`.
- **P3:** UI retry buttons + retry_count display; remove partial retry; schema-reset button.

## Files

- `crates/agentic/airway/src/extension/migration.rs` — Migration 5.
- `crates/agentic/airway/src/extension/run_extension.rs` — entity + accessors.
- `crates/agentic/pipeline/src/retry.rs` — `retry_airway` reset-in-place.
- `crates/agentic/pipeline/src/airway_run.rs` / `backfill.rs` — re-drive existing run.
- `crates/agentic/airway/src/worker.rs` — persist/read `resume_state`.
- web-app: `RunDetail`/`RunHeader`, `pages/airway`, `Runs` list, drop `RetryFailedTablesButton`.
