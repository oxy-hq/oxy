---
name: oxy-data-architecture
description: Use when deciding where an aggregate gets materialised — writing an `execute_sql` rollup, chaining one after an `airway` ingest step, choosing rollup vs semantic `pre_aggregations`, or whether raw rows must be retained — and on the symptoms: a dashboard that aggregates at query time, a load that wrote every row yet failed, preagg that cannot bucket to 15 minutes or join two views, or no "bronze"/"silver"/"gold" in the schema. Pipeline internals: `oxy-airway-pipelines`; view syntax: `oxy-semantic-modeling`.
---

# Oxy data architecture: where results get materialised

An OLAP store wants **materialised results, not aggregation at query time.** A
dashboard that groups four million order rows on every page load is not a slow
query to tune — it is a rollup that was never built. So the work that produces
those tables has to be systematic: one place it runs, one guarantee about when it
may read, one rule for what gets kept.

That spans Airway ingest, the semantic layer, and scheduled automations, which is
why it is its own skill rather than a section in one of them — and it does not
restate them. Fold mechanics, duplicate rows and the pipeline lease are
**`oxy-airway-pipelines`**; views and measures **`oxy-semantic-modeling`**; task
syntax and schedules **`oxy-automations`**.

## The source doc, and why half of it is not teachable

The frame comes from **`internal-docs/medallion-architecture.md`** in
**`oxy-hq/oxygen-internal`** — a **private** repo (often checked out under a
directory named `oxy-internal`). You cannot fetch it like a URL and it may not be
checked out at all where you are reading this, which is why every stable fact
below is carried here rather than pointed at. Do not copy more of it in: it is
actively maintained there, and a duplicate rots and then argues with the original.

**Its status is `design`, with an open "Decisions needed" section.** So this
skill splits it: **current state** is taught below as fact, re-verified against
source and cited; the **target tiering, gap list, and open decisions** are named
at the end and deliberately *not* taught. Building against a convention that does
not exist yet is worse than building against no convention at all.

## The vocabulary bridge

**Nothing in the code says bronze, silver, gold, or medallion.** The doc
introduces that vocabulary as a *lens*, not as names you will find. Grep
`oxy-internal` for "bronze" or "silver" and you get Tailwind classes for
leaderboard medal ranks in the observability UI (`bg-rank-bronze`); grep "gold"
and you get golden-snapshot test names (`crates/oxy-compile/src/compile.rs:1042`)
and a restaurant in seed data. **Not one hit is a data tier.** An agent that
greps, finds nothing, and invents a tier structure has invented it.

| medallion word | what is actually there |
|---|---|
| (transient) bronze buffer | `<schema>_raw.<table>` append buffer — **`replacing` disposition only**, not a tier |
| **silver** | `<schema>.<table>`, the normalized read table — every disposition lands here |
| **gold** | `execute_sql` rollup tables (`*_daily_metrics`) **in the same schema** |
| the promotion | the **fold** (watermark-bounded, latest-wins, merge-on-read) — inline at end of load **and again** on the scheduled vacuum, identical SQL |
| storage | Airhouse = DuckLake on DuckDB (S3 Parquet + catalog) — **for pipelines that land in Airhouse** |

That last row is a default, not a law. A rollup lands wherever its `execute_sql`
step's `database:` points, so gold is not necessarily DuckLake: the canonical
exemplar view in `oxy-semantic-modeling` sits on a gold table
(`…___sales_daily_metrics`) whose `datasource:` is **`clickhouse`**. That view is
where the two skills meet — a view models a rollup an automation built — and it
is also the reminder to read the `database:`/`datasource:` rather than assume the
warehouse.

## Current state: silver on ingest, gold in place

**Normalization happens at ingest, so the landed table is already silver.** It is
not a later stage. Airway flattens nested JSON into parent/child relational
tables, snake_cases every name (`normalizer/mod.rs:23`), coerces types, and
injects dlt-style lineage columns — `_aw_id`, `_aw_load_id`, `_aw_parent_id`,
`_aw_root_id`, `_aw_list_idx` (`normalizer/relational.rs:153`). There is no
separate "raw → typed" step to insert your own logic into.

**The `_raw` buffer is a write-throughput optimization, not a tier.** It exists
only for `write_disposition: replacing`, to keep the write path O(batch) instead
of scanning the target (airway `types.rs:117`). It is drained on fold, so steady
state is roughly empty — and when a fold fails the rows stay durable there,
which is what makes the re-run below safe rather than what makes the failure
harmless. **You cannot rebuild silver from it.**

> **Location trap.** The buffer lives at **`<schema>_raw.<table>`** — a sibling
> *schema*, where `<schema>` is the pipeline's `dataset_name`, so one landing in
> `toast_pos` buffers to `toast_pos_raw.orders` (airway
> `connector/destinations/airhouse.rs:1003`). Several in-tree comments still say
> `<table>_raw`, oxy's own `crates/agentic/airway/src/source_factory.rs:475`
> among them. That is the pre-`raw_schema_name` location, healed in place by
> `migrate_legacy_raw`; the comments never caught up. **This is not version
> drift** — the migration predates every airway rev oxy has pinned. Follow the
> comment and you look in the wrong schema and conclude the buffer is missing.

**Gold is a `CREATE OR REPLACE TABLE` in an automation**, reading the
Airway-landed tables and writing aggregates into the *same* dataset schema —
`toast_pos.orders_daily_metrics` beside `toast_pos.orders`. No marts schema
exists today. The `execute_sql` task:
<https://www.oxygen-hq.com/docs/guide/build/automations/task-types>

## The sequencing guarantee — the mechanism that makes this systematic

This is the whole answer to "how do we do the background work systematically",
and it is one sentence in the source:

> A step completes only once the pipeline's end-of-load fold has committed, so a
> following `execute_sql` step reads a queryable table rather than a half-folded
> one.
> — `crates/core/src/config/model.rs:2435`

So ingest → rollup is a plain two-step automation, and the ordering is real
rather than hoped-for:

```yaml
tasks:
  - name: load
    type: airway
    pipeline: pipelines/restaurant_analytics.airway.yml
  - name: rollup
    type: execute_sql
    database: pokehouse
    sql_query: |
      CREATE OR REPLACE TABLE toast_pos.orders_daily_metrics AS
      SELECT business_date, restaurant_id, sum(net_amount) AS net_sales
      FROM toast_pos.orders GROUP BY 1, 2
```

Two things to know about that step:

- **`type: airway` is undocumented.** The task-types page above lists
  `execute_sql` thoroughly and mentions airway **zero** times (checked
  2026-08-19). It is also not run by `step_executor` like the other I/O tasks —
  it is delegated as a `TaskSpec::Airway` (`model.rs:2428`). Reading the docs
  and concluding you must trigger ingest some other way is the expected mistake.
- **A failed fold fails the run — and the fix is to re-run, never to
  re-extract.** Every row can be written and the load still fail, because the
  fold that promotes them into the public table is a separate step: *"what
  distinguishes 'written' from 'queryable'"*
  (`crates/agentic/airway/src/events.rs:163`). Airway emits `LoadCompleted`
  anyway, with correct counts and the per-table outcome in its `folds` field —
  the counts are real — *"and the run reports an error"* (`events.rs:153`). It
  then returns `Err`, which is *"what makes the RUN terminal state failed"*
  (airway `pipeline/mod.rs:1378`; the returns at `:1376` and `:1881`,
  `TaskOutcome::Failed` at `crates/agentic/airway/src/worker.rs:311`). So the
  run goes red and the message names the cause outright: *"load wrote every row
  but N table(s) failed the end-of-load fold … rows are durable in the staging
  buffer but not yet visible in the public schema; re-run to fold them"*
  (`pipeline/mod.rs:1291`). Take that literally. Cursors already advanced, so
  *"the re-run re-extracts nothing and the fold sweeps the pending buffer"*
  (`pipeline/mod.rs:1372`) — a full re-extract, or a Reset schema, is the
  expensive wrong move on a run that already landed every row.

## Rollup is not preagg

Asked to make a slow dashboard fast, an agent working in the semantic layer
reaches for `pre_aggregations`. Nothing warns it off: `pre_aggregations` appears
in no docs page, and `oxy-semantic-modeling` does not mention it either — so the
first thing that tells you it is the wrong tool is the SQL failing. It is a
**different mechanism** and it cannot do what a rollup does:

| | `execute_sql` rollup | semantic `pre_aggregations` |
|---|---|---|
| lives in | an automation | a `.view.yml` block |
| reads | any SQL — joins, CTEs, window functions | **one view**: `generate_build_sql` takes a single `view.source_sql()` and emits no JOIN (airlayer `engine/preagg.rs:266`) |
| time buckets | anything you can write | a calendar part only — `granularity` is interpolated straight into `date_trunc('{g}', …)` (airlayer `dialect/mod.rs:75`), so `year`/`quarter`/`month`/`week`/`day`/`hour`/`minute`/`second` and nothing between |
| refresh | your automation's schedule | `refresh_key`, by a background worker |

So **15-minute buckets and any two-table join must be a rollup.** `RollupSpec`
carries `dimensions`, `measures`, `time_dimension`, `granularity` — there is
nowhere to name a second view. Use preagg for the narrow case it fits: one view,
calendar grain, speeding up a measure that already resolves. Use a rollup for
everything else, which in practice is most gold.

## Bronze policy is a decision rule, not a preference

The right policy is a function of **re-extractability**, and it is chosen per
domain rather than applied uniformly:

| source | policy | why |
|---|---|---|
| **re-extractable** — Toast, QuickBooks, most APIs | transient `_raw` buffer is fine; **the source system is the bronze** | you can always re-pull to rebuild silver; a durable warehouse copy is redundant cost |
| **non-re-extractable** — video events, sensor streams | the raw event table **is durable bronze and must be retained** | you cannot re-pull yesterday's frames; drop the events and the history is gone |

The video domain is the worked example, and it diverges visibly: `oxy_cam_events`
is written by a direct append (`crates/cameras/src/service/ingest.rs:106`) into a
plain table with no `_raw` buffer and no fold
(`crates/cameras/src/airhouse/schema.rs:16`) — it never goes through Airway at
all. That is the shape durable bronze takes here. Before proposing a retention or
cleanup job on any raw table, run the test above; the answer for cameras is the
opposite of the answer for Toast.

## Why the shapes are these shapes: DuckLake

`PRIMARY KEY`, `UNIQUE`, and `CREATE INDEX` are all rejected
(`crates/cameras/src/airhouse/schema.rs:6`). That single constraint is why
`merge` stages then `MERGE INTO` and why `replacing` dedups on fold — no
`ON CONFLICT` to lean on, and scan pruning comes from partition/sort, not an
index. **`oxy-airway-pipelines` owns the consequences** (duplicate rows, the
single-flight lease, reset); do not re-derive them here. The one that reaches
gold: DDL in an `execute_sql` rollup must carry no constraints either.

**A corollary for rollup SQL:** join parent to child on the **propagated
business key** (`invoice_id` on `invoices__line`), never on `_aw_parent_id` — it
is regenerated every load (airway `types.rs:114`), so a join on it matches within
one load and silently stops matching across loads.

## Proposed, not built — cite, do not build against

The rest of that doc is a roadmap. Say so out loud if someone asks you to follow
it; none of the following exists:

- **A conformance layer** — shared `dim_store` / `dim_date` / `dim_camera` and
  cross-source id maps, materialised once. Cross-source keys are wired ad hoc
  per domain today, mostly hand-maintained `VALUES` tables. One camera dimension
  has since shipped in the video domain: proof the gap is real, **not** a
  convention to copy.
- **Separating gold from silver** by schema or a `*_metrics` suffix tooling
  treats as derived. They co-mingle today, and whether to adopt a convention now
  or wait for a second consumer is an **open decision** in the doc.
- **Codifying the re-extractability test as policy** — the rule above is
  reasoning that has held, not something enforced anywhere.

If a task needs one of these, that is a design conversation with the doc's
owners, not a pattern to invent in a customer workspace.

## Version anchor

Checked 2026-08-19 against oxy-internal **0.5.126**, airway **0.1.32**
(`55e2794`), airlayer **0.3.6** (`7e884e77`) — both pinned in `Cargo.lock`.
Unprefixed `src/…` paths above are inside those external crates, not
oxy-internal; `crates/…` paths are oxy-internal. Both engines are fetched by git
rev, so a bump can move this ground with no oxy-internal commit — re-check
against the pin in `Cargo.lock`, not against memory.
