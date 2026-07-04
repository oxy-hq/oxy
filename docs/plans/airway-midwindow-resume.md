# Airway backfill mid-window resume (P2c)

**Status:** proposed · **Date:** 2026-07-03 · **Repos:** oxy-internal + airway-internal

## Goal

A backfill **chunk** that fails part-way through its `[from, to)` window should, on
reset-in-place retry (P2a/P2b), **resume where it left off** instead of re-extracting the
whole window. Today resume is chunk-granular only, and a failed chunk re-pulls everything.

## Why it's a two-repo build (from the cross-repo map)

- **Engine persists state once, at end of run** (`airway` `pipeline/mod.rs`: `persist()`
  fires only after `writer.finish()`/successful load). No mid-run checkpoint; a failure
  saves nothing.
- **Toast source freezes + discards the cursor during a backfill** (`toast.rs`
  `freeze_cursor → None`); it paginates the whole window in one pass with no persisted
  progress. The freeze is deliberate — it protects the *live* incremental cursor.
- **`airway_run_extensions.resume_state` (P1) is inert** — nothing reads/writes it.

So a host-side run-scoped store is necessary but **not sufficient**: without an engine
mid-run persist hook and a source that emits/consumes an in-window resume point, there's
nothing to persist or resume from.

## Design

### Resume unit = in-window high-water (business_date), run-scoped

The Toast backfill loops `restaurants × ≤30-day sub-windows × pages`. The resume point is a
**high-water `business_date`** *within* the window. On retry, `orders_window`/`cursor_window`
starts from `max(backfill_from, resume_high_water)` instead of `backfill_from`. airhouse is
**merge-on-read**, so re-pulling the partially-loaded boundary day is idempotent (deduped on
fold) — the resume can be coarse (day-granular) and still correct.

Crucially this cursor is **run-scoped** (persisted per `run_id`), *never* the live pipeline
cursor — so advancing it during a backfill can't corrupt the live incremental position (the
exact hazard the freeze was guarding against).

### 1. Toast source (airway) — resumable-backfill mode

- Add `with_resumable_backfill_window(start, end)` (or a flag on the existing method) that
  pins `[start,end)` **and** enables cursor advance (the window is run-scoped now).
- In resumable mode: `freeze_cursor` **does not** freeze — it emits the advanced high-water
  (max `business_date` loaded). `cursor_window`/`orders_window` consult the prior run-scoped
  state as the resume floor: start at `max(backfill_start, prior_high_water)`.
- Non-resumable backfill (QuickBooks, or if disabled) keeps today's freeze behavior.

### 2. Engine (airway) — mid-run persist hook (validated mechanism)

The streaming engine (`pipeline/mod.rs run_source_streaming`) drives each resource in a
spawned task that yields `SinkMsg::Batch` per batch and one `SinkMsg::ResourceDone {state}`
at stream end (the cursor is read once from the resource's `state_handle`). The sink writes
batches (airhouse commits per write) and folds/persists the cursor only after
`writer.finish()`. So the cursor is emitted **once per resource, at the end** — no
mid-resource checkpoint. Concretely:

- **New `SinkMsg::ResourceProgress { resource, cursor }`.** The source publishes an
  intra-resource checkpoint by updating the shared `state_handle` after each completed
  `(restaurant, sub-window)`; the driver polls the handle after each batch it forwards and,
  when it changes, emits `ResourceProgress`. Because the source updates the handle only in
  **resumable-backfill** mode, normal runs never change it mid-stream → no `ResourceProgress`
  → unchanged once-at-end behavior. **Gating is implicit** (no engine flag needed).
- **Sink handles `ResourceProgress`** by `fold_resource_state(resource, cursor)` + a
  mid-run `persist()`. Ordering is safe by construction: the mpsc channel is FIFO and the
  source updates the handle only *after* a window's batches are sent, so the checkpoint
  arrives (and persists) only after those batches have been written/committed —
  persist-after-commit. A crash after commit + before persist re-pulls at most one window
  (deduped by merge-on-read).
- **Toast source (P2c-2 companion):** `stream_orders`/`stream_time_entries` take the
  `state_handle` and, in resumable mode, set it to the folded high-water after each
  `(restaurant, sub-window)`; the final cursor fill at stream end is unchanged.

Risk: this touches the engine's core streaming loop + at-least-once ordering. It is bounded
(Toast is the only resumable source; gating is implicit) but MUST land with a streaming
integration test asserting the persist-after-commit ordering (crash mid-window → retry
resumes from the last committed window, no re-pull of the committed prefix, no gap).

### 3. Host (oxy) — run-scoped hybrid state store

- New `AirwayRunScopedStateStore { db, run_id, pipeline_name }` implementing `StateStore`:
  - `load()`: **cursor** (`PipelineState`) from `airway_run_extensions.resume_state` keyed by
    `run_id` (default when NULL); **schema** delegated read-only from the pipeline-global
    `airway_pipeline_state` row (keyed by `pipeline_name`).
  - `save(state, schema, _)`: write **cursor → `resume_state`** only. Do **not** write the
    global schema (a backfill must not clobber/ race the live schema); the chunk uses the
    loaded schema and the destination migration adds any new columns additively.
  - Single-writer per run → no optimistic-concurrency needed on `resume_state`.
- Thread `assignment.run_id` + the backfill flag (`backfill_from.is_some()`) through
  `execute_airway` → `AirwayWorker` → the `with_state_store` call, and select the run-scoped
  store for backfill runs (the pipeline-global store for normal runs).

## Correctness

- **At-least-once preserved:** cursor persists only after a batch commits; a crash re-pulls
  at most the in-flight window (deduped by merge-on-read).
- **Live cursor untouched:** the run-scoped cursor lives on the run extension, never on
  `airway_pipeline_state`; a backfill can't advance the live position.
- **Idempotent resume:** merge-on-read makes re-pulling the boundary window safe.

## Phases

- **P2c-1 (oxy):** `AirwayRunScopedStateStore` + thread run_id/backfill flag + select it for
  backfill runs. Self-contained; inert until P2c-2/3 land, but the seam + testable round-trip.
- **P2c-2 (airway):** Toast source resumable-backfill (emit/consume in-window high-water).
- **P2c-3 (airway):** engine mid-run persist-after-commit hook (run-scoped path only).
- **P2c-4:** airway release + oxy bump + end-to-end test (fail a chunk mid-window → retry →
  resumes, doesn't re-pull the completed prefix) + the deferred retry_run/re-drive e2e tests.

## Risks / open decisions

- **Engine at-least-once change** is the riskiest — the persist-after-commit ordering must be
  exact. Scope the hook to the run-scoped path so normal runs are untouched.
- **Resume granularity:** day-level (business_date) is simplest and merge-on-read-safe;
  per-page is finer but needs page tokens the API may not give stably. Start day-level.
- **QuickBooks:** keep non-resumable (freeze) for now; resumable is Toast-first.
- **Two-repo release coordination:** P2c-2/3 ship in an airway release; oxy bumps + P2c-1
  wiring activates it. Until then P2c-1 is inert (safe).
