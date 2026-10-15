---
name: oxy-data-architecture
description: >-
  Use when deciding where an aggregate gets materialised — writing an `execute_sql` rollup,
  chaining one after an `airway` ingest step, choosing rollup vs semantic `pre_aggregations`, or
  whether raw rows must be retained — and on the symptoms: a dashboard that aggregates at query
  time, a load that wrote every row yet failed, preagg that cannot bucket to 15 minutes or join
  two views, or no "bronze"/"silver"/"gold" in the schema. Pipeline internals:
  `oxy-airway-pipelines`; view syntax: `oxy-semantic-modeling`.
---

# Oxy data architecture: where results get materialised

An OLAP store wants **materialised results, not aggregation at query time.** A
dashboard that groups four million order rows on every page load is not a slow
query to tune — it is a rollup that was never built. So the work that produces
those tables has to be systematic: one place it runs, one guarantee about when it
may read, one rule for what gets kept.

That spans Airway ingest, the semantic model, and scheduled automations, which is
why it is its own skill rather than a section in one of them — and it does not
restate them. Fold mechanics, duplicate rows and the pipeline lease are
**`oxy-airway-pipelines`**; views, measures and `pre_aggregations` syntax are
**`oxy-semantic-modeling`**; task syntax and schedules are **`oxy-automations`**.

## The frame: bronze → silver → gold, as a lens

The medallion words are a useful way to think about how raw source data becomes
analytics-ready. **But nothing in Oxy — code, UI, schema names — says bronze,
silver, gold, or medallion.** Search a workspace for those words and you will not
find a tier. An agent that searches, finds nothing, and invents a tier structure
has invented it. Map the words onto what actually exists:

| medallion word | what is actually there |
|---|---|
| (transient) bronze buffer | `<schema>_raw.<table>` append buffer — **`replacing` disposition only**, not a tier |
| **silver** | `<schema>.<table>`, the normalized table a pipeline lands — every disposition ends here |
| **gold** | `execute_sql` rollup tables (`*_daily_metrics`) built by an automation, **in the same schema** |
| the promotion | the **fold**: watermark-bounded, latest-wins, merge-on-read — inline at the end of a load **and again** on the scheduled vacuum, identical SQL |
| storage | Airhouse = DuckLake on DuckDB (Parquet on object storage + a catalog) — **for pipelines that land in Airhouse** |

That last row is a default, not a law. A rollup lands wherever its `execute_sql`
step's `database:` points, so gold is not necessarily DuckLake: a gold table can
sit in a customer's ClickHouse with a semantic view (`datasource: clickhouse`)
modelled on top of it. That view is where the skills meet — a view models a
rollup an automation built — and it is the reminder to read the
`database:`/`datasource:` rather than assume the warehouse.

## Today: silver on ingest, gold in place

```
source (POS, accounting, API, …)
   │  Airway: extract + NORMALIZE on ingest
   │    flatten JSON → parent/child tables · snake_case · _aw_* lineage · type-coerce
   ├─ replacing? ──▶ <schema>_raw.<table>  (append buffer, drained by the fold)
   ▼
<schema>.<table>                  ◀── SILVER: typed, flattened (the landed table)
   │  automation: type: execute_sql — CREATE OR REPLACE TABLE … AS SELECT …
   ▼
<schema>.<x>_daily_metrics        ◀── GOLD: aggregates, same schema
```

**Normalization happens at ingest, so the landed table is already silver.** It is
not a later stage. Airway flattens nested JSON into parent/child relational
tables, snake_cases every name, coerces types, and adds lineage columns —
`_aw_id`, `_aw_load_id`, `_aw_parent_id`, `_aw_root_id`, `_aw_list_idx`. There is
no separate "raw → typed" step to insert your own logic into.

**The `_raw` buffer is a write-throughput optimization, not a tier.** It exists
only for `write_disposition: replacing`, to keep the write path proportional to
the batch instead of scanning the whole target. It is drained on fold, so steady
state is roughly empty — and when a fold fails the rows stay durable there,
which is what makes the re-run below safe rather than what makes the failure
harmless. **You cannot rebuild silver from it.**

> **Location trap.** The buffer lives at **`<schema>_raw.<table>`** — a sibling
> *schema*, where `<schema>` is the pipeline's `dataset_name`, so a pipeline
> landing in `toast_pos` buffers to `toast_pos_raw.orders`. Older comments and
> notes say `<table>_raw` inside the same schema; that was an earlier layout,
> migrated in place long ago. Follow one of them and you look in the wrong schema
> and conclude the buffer is missing.

**Gold is a `CREATE OR REPLACE TABLE` in an automation**, reading the
pipeline-landed tables and writing aggregates into the *same* dataset schema —
`toast_pos.orders_daily_metrics` beside `toast_pos.orders`. No separate marts or
gold schema exists. The `execute_sql` task:
<https://www.oxygen-hq.com/docs/guide/build/automations/task-types>

**What follows from this:** there is no durable, replayable bronze for
pipeline-fed data. To re-derive silver you **re-extract from the source**, and
silver and gold sit side by side in one schema.

## The sequencing guarantee — what makes this systematic

An automation's `type: airway` step completes only once the pipeline's
end-of-load fold has committed, so a following `execute_sql` step reads a
queryable table rather than a half-folded one. Ingest → rollup is therefore a
plain two-step automation, and the ordering is real rather than hoped-for:

```yaml
tasks:
  - name: load
    type: airway
    pipeline: pipelines/restaurant_analytics.airway.yml
  - name: rollup
    type: execute_sql
    database: analytics_warehouse
    sql_query: |
      CREATE OR REPLACE TABLE toast_pos.orders_daily_metrics AS
      SELECT business_date, restaurant_id, sum(net_amount) AS net_sales
      FROM toast_pos.orders GROUP BY 1, 2
```

The task-types page documents `type: airway` (and its optional `resources:`
subset); what it does not say is why this guarantee is the backbone of the whole
pattern. Schedule the automation, not the pipeline, whenever a rollup depends on
the load.

**A failed fold fails the run — and the fix is to re-run, never to
re-extract.** Every row can be written and the load still fail, because the fold
that makes rows visible in the public table is a separate step from writing
them. The run reports correct row counts, then ends **failed**, with a message
that names the cause outright: *"load wrote every row but N table(s) failed the
end-of-load fold … rows are durable in the staging buffer but not yet visible in
the public schema; re-run to fold them."* Take that literally. Cursors were
already saved, so the re-run re-extracts nothing and the fold sweeps the pending
buffer. A full re-extract, or a **Reset schema**, is the expensive wrong move on
a run that already landed every row. Re-run the whole automation, so the
rollup rebuilds on the folded table.

## Rollup is not preagg

Asked to make a slow dashboard fast, an agent working in the semantic model
reaches for `pre_aggregations`. It is a **different mechanism**, and it cannot do
most of what a rollup does:

| | `execute_sql` rollup | semantic `pre_aggregations` |
|---|---|---|
| lives in | an automation | a `.view.yml` block |
| reads | any SQL — joins, CTEs, window functions | **one view**: a rollup holds only that view's own dimensions and measures, with no join |
| time buckets | anything you can write (`time_bucket(INTERVAL '15 minutes', ts)`) | a calendar part only — `second`/`minute`/`hour`/`day`/`week`/`month`/`quarter`/`year`, nothing between |
| measures | anything SQL computes | only re-aggregatable ones; `custom`, `median` and `number` measures never serve from a rollup |
| refresh | your automation's schedule | `refresh_key`, by a background worker |
| consumed by | anything that reads the table — a view, an app, a query | only semantic queries the rollup fully covers; anything else falls back to the warehouse |

So **15-minute buckets and any two-table join must be a rollup table.** There is
nowhere in a `pre_aggregations` entry to name a second view or an arbitrary
bucket. Use preagg for the narrow case it fits: one view, calendar grain,
speeding up re-aggregatable measures a dashboard already queries through the
semantic model. Use a rollup for everything else, which in practice is most gold
— and model a semantic view on the rollup table so agents and apps query it by
name.

## Retaining raw data: a decision rule, not a preference

Whether raw data must be kept is a function of **re-extractability**, chosen per
source rather than applied uniformly:

| source | policy | why |
|---|---|---|
| **re-extractable** — POS and accounting APIs (Toast, QuickBooks), most APIs | the transient `_raw` buffer is fine; **the source system is the bronze** | you can always re-pull to rebuild silver; a durable warehouse copy is redundant cost |
| **non-re-extractable** — video or sensor events, files people upload by hand | the raw artifact **is durable bronze and must be retained** — a table when it is an event, object storage plus a metadata row when it is a file | you cannot re-pull yesterday's frames or re-download last quarter's portal export; drop it and the history is gone |

Non-re-extractable events take a visibly different shape: written by direct
append into a plain, immutable table, with no `_raw` buffer and no fold — they
never go through Airway at all. Before proposing a retention or cleanup job on
any raw table, run the test above; the answer for event streams is the opposite
of the answer for an API source.

**A durable bronze of files is not automatically replayable.** Re-parsing needs
whatever the original parse was *given*, and that is often more than the file: a
reporting period someone picked in a UI, text a browser extracted, a mapping
chosen at upload. Keep those inputs alongside the file, or the retained artifact
is necessary and still not sufficient to rebuild anything.

## Why the shapes are these shapes: DuckLake

`PRIMARY KEY`, `UNIQUE`, and `CREATE INDEX` are all rejected by DuckLake, and a
table declaring one stops the writer. That single constraint is why `merge`
stages then merges and why `replacing` dedups on fold — there is no
`ON CONFLICT` to lean on, and scan pruning comes from opt-in partitioning and
sort order (`SET PARTITIONED BY`, `SET SORTED BY`), not from an index.
**`oxy-airway-pipelines` owns the consequences** (duplicate rows, the
single-flight lease, reset); do not re-derive them here. The one that reaches
gold: DDL in an `execute_sql` rollup on Airhouse must carry no constraints either.

**A corollary for rollup SQL:** join parent to child on the **propagated
business key** (`invoice_id` on `invoices__line`), never on `_aw_parent_id` — it
is regenerated every load, so a join on it matches within one load and silently
stops matching across loads.

## What does not exist — do not build against it

These come up as "the obvious next step"; none of them is a convention today,
and inventing one in a customer workspace creates a second pattern nobody else
follows:

- **A shared conformance layer** — common `dim_store` / `dim_date` dimensions
  and cross-source id maps, materialised once. Cross-source keys are wired per
  domain today, mostly as hand-maintained `VALUES` tables. If you need one, build
  it as a maintained table for that domain and say it is local.
- **A separate gold schema or naming rule** that tooling treats as derived. Gold
  and silver share a schema; the `*_daily_metrics` suffix is habit, not
  enforcement.
- **The retention rule as policy.** The re-extractability test above is
  reasoning that has held, not something any tool enforces.

Checked in October 2026 against the Airway engine Oxy pins (0.1.51) and the
semantic engine it ships with. A newer Oxy can move this ground; trust a run's
own error message over this page.
