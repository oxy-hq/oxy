---
source:
  - oxy-hq/skills/skills/oxy-app-builder/SKILL.md
  - oxy-hq/skills/skills/oxy-app-builder/QUICK-REFERENCE.md
reconciled-at: f9ebd8af267cfea5b52fa96994763898ab8a0e34
note: |
  Authored condensation. Not auto-synced — scripts/sync-skills.sh only copies
  the verbatim YAML templates. Re-condense by hand when source material
  changes materially; keep under ~200 lines so the LLM context stays focused.
  scripts/check-skills-drift.sh flags upstream changes.
---

# Oxy App Builder Reference

An Oxy data app is a `*.app.yml` file that pairs **tasks** (operations that
produce tabular or text output) with **displays** (visualizations that render
that output). Mental model: `task -> output -> display`.

## File shape

```yaml
name: snake_case_app_name            # optional, but recommended
title: "Human-Friendly App Title"    # recommended — surfaces in app
                                     # listings; falls back to filename
description: |                        # optional, multi-line OK
  What this app shows.

tasks:                                # REQUIRED, at least one entry
  - name: task_name                   # unique, snake_case
    type: semantic_query              # or execute_sql | workflow | agent
    # ... type-specific fields

display:                              # REQUIRED, at least one entry
  - type: table
    title: "Section Title"            # optional
    data: task_name                   # references a task by name
```

## Task types

### `semantic_query` — preferred when the semantic layer covers the data

```yaml
- name: revenue_by_month
  type: semantic_query
  topic: sales_mrr                    # must match a *.topic.yml name
  dimensions:
    - sales.month                     # view.dimension
  measures:
    - sales.total_revenue
    - sales.order_count
  filters:                            # optional
    - field: sales.year
      op: eq                          # eq | neq | gt | gte | lt | lte | in
      value: 2024
  orders:                             # optional
    - field: sales.month
      direction: asc                  # asc | desc
```

**Column naming:** semantic query outputs use `view__field` (double underscore)
in display references. So `dimensions: [sales.month]` and
`measures: [sales.total_revenue]` are referenced as `x: sales__month` and
`y: sales__total_revenue`.

**Time-dimension granularity suffix.** When a `time_dimensions` entry sets
`granularity:`, the output column gets an extra `__<granularity>` suffix
appended. So:

```yaml
time_dimensions:
  - dimension: orders.created_date
    granularity: month
```

produces an output column literally named `orders__created_date__month`, and
chart `x:` must reference it with the suffix:
`x: orders__created_date__month` (NOT `x: orders__created_date`). Omitting
the suffix produces a "column not found" Binder error in the in-browser
DuckDB chart engine and the chart silently fails to render.

**Entity labeling — never put a raw UUID/FK on a chart axis.** When the
primary entity's `key:` is an opaque ID (`guid`, `restaurant_id`,
`customer_id`, `order_id`, …), do NOT use that field as the chart `x:`,
`name:`, or table-row label — the dashboard will render uninformative UUIDs.
Two ways to surface a human-readable label instead:

1. **Preferred — semantic_query via foreign entity.** If the joined view
   exposes a name dimension (e.g. `restaurants.location_name`), make sure
   the FK side declares the joined view as a `foreign` entity (see
   semantic-layer card), and pull the name through the topic:
   `dimensions: [restaurants.location_name]` instead of
   `dimensions: [orders.restaurant_id]`. Output column is then
   `restaurants__location_name`.
2. **Fallback — execute_sql with a JOIN.** When semantic_query can't reach
   the name (no foreign entity, multi-view topic doesn't exist), use
   `execute_sql` with an explicit JOIN to the lookup view and `SELECT`
   the name column directly.

If neither path is available (no name field exists in any related view),
fall back to the FK but warn in the markdown header that rows are keyed
by ID — never silently render UUIDs as a chart axis.

### `execute_sql` — fallback for custom SQL

```yaml
- name: sales_by_region
  type: execute_sql
  database: clickhouse                # clickhouse | postgres | bigquery | local
  sql_query: |
    SELECT region, SUM(amount) AS total_sales
    FROM sales
    GROUP BY region
```

Or with a file reference:

```yaml
- name: sales_report
  type: execute_sql
  database: clickhouse
  sql_file: queries/sales_report.sql
```

Column names come from the SELECT aliases (`x: region`, `y: total_sales`).

### `workflow` — invoke a sub-workflow

```yaml
- name: ops
  type: workflow
  src: workflows/operations.workflow.yml
  variables:
    period: "2024-Q4"
```

Reference the inner task outputs with dot notation: `data: ops.location_summary`.

### `agent` — AI-generated narrative

```yaml
- name: insights
  type: agent
  agent_ref: analyst.agent.yml
  inputs:                             # previous task names passed as context
    - revenue_data
  prompt: |
    Summarize the revenue data in 3 bullet points.
```

Reference the agent's text output in markdown via `{{insights}}`.

## Display types

### `markdown`

```yaml
- type: markdown
  title: "AI Insights"                # optional
  content: |
    # Header
    Free-form markdown. Agent outputs can be interpolated: {{agent_task_name}}.
```

### `table`

```yaml
- type: table
  title: "Regional Details"           # optional
  data: task_name                     # REQUIRED
```

### `line_chart`

```yaml
- type: line_chart
  title: "Revenue Over Time"
  data: monthly_revenue
  x: month                            # REQUIRED
  y: revenue                          # REQUIRED
  x_axis_label: "Month"               # optional
  y_axis_label: "Revenue ($)"         # optional
  series: region                      # optional; splits into multiple lines
```

### `bar_chart`

```yaml
- type: bar_chart
  title: "Sales by Region"
  data: regional_sales
  x: region                           # categorical
  y: total_sales                      # numeric
  series: product_category            # optional; grouped/stacked bars
```

### `pie_chart`

```yaml
- type: pie_chart
  title: "Market Share"
  data: market_data
  name: company                       # label column
  value: market_share                 # numeric column
```

## SQL dialect notes

When you author or profile SQL inside an `execute_sql` task — or when
you ask the warehouse for shape/distribution stats before picking
fields — pick the right dialect for the configured database. The
common gotchas:

| Dialect    | `DATE_TRUNC` form                                  | Stddev fn                |
| ---------- | -------------------------------------------------- | ------------------------ |
| BigQuery   | `DATE_TRUNC(<col>, MONTH)` (column first, no quotes) | `STDDEV(<col>)`          |
| Snowflake  | `DATE_TRUNC('month', <col>)`                       | `STDDEV(<col>)`          |
| Postgres   | `DATE_TRUNC('month', <col>)`                       | `STDDEV(<col>)`          |
| DuckDB     | `DATE_TRUNC('month', <col>)`                       | `STDDEV(<col>)`          |
| ClickHouse | `toStartOfMonth(<col>)`                            | `stddevPop(<col>)` (lowercase) |

Other places `.app.yml` SQL diverges across dialects:

| Concern             | Postgres / DuckDB            | Snowflake               | BigQuery                          | ClickHouse / MySQL    |
| ------------------- | ---------------------------- | ----------------------- | --------------------------------- | --------------------- |
| Identifier quoting  | `"col"`                      | `"col"`                 | `` `col` ``                       | `` `col` `` / unquoted |
| Cast to date        | `CAST(x AS DATE)`, `x::date` | `CAST(x AS DATE)`       | `CAST(x AS DATE)`                 | `toDate(x)`           |
| Date arithmetic     | `d + INTERVAL '1 day'`       | `DATEADD(day, 1, d)`    | `DATE_ADD(d, INTERVAL 1 DAY)`     | `d + INTERVAL 1 DAY`  |

## Profiling template

Before committing to a measure or entity for a chart or ranked table,
profile the underlying data so you don't ship a flat-line trend or a
top-10 with one row in it. One consolidated SELECT is enough:

```sql
SELECT
  COUNT(*) AS rows,
  COUNT(DISTINCT <entity_expr>) AS entity_card,
  MIN(<time_expr>) AS min_date, MAX(<time_expr>) AS max_date,
  COUNT(DISTINCT DATE_TRUNC('month', <time_expr>)) AS month_count,
  MIN(<measure_expr>) AS min_val, MAX(<measure_expr>) AS max_val,
  STDDEV(<measure_expr>) AS measure_stddev
FROM <table>
```

Substitute `<entity_expr>`, `<time_expr>`, `<measure_expr>`, and
`<table>` with the view's actual `expr:` strings — never guess column
names. Apply the dialect substitutions from the table above for
`DATE_TRUNC` and `STDDEV` when the warehouse is BigQuery or
ClickHouse.

A topic is fit for ranking and trend visualizations when:

- `rows ≥ 100`,
- `month_count ≥ 3` (enough time for a meaningful trend),
- `measure_stddev > 0` (not a flat measure that draws as a horizontal
  line at one value),
- `entity_card` is between 5 and 500 (top/bottom-N actually differ).

## Failure recovery

Profiling queries fail mid-build for routine reasons — dialect
mismatch, type-cast errors, an aggregation function the warehouse
doesn't expose. The recovery rule is:

1. **Simplify and retry once** — drop `STDDEV`, drop `month_count`, or
   replace `DATE_TRUNC` with the dialect's equivalent. At most two
   attempts per topic in total.
2. **Skip the topic on the second failure** — never loop on the same
   failing query. Move on to the next candidate; if none qualify,
   omit the affected block entirely rather than ship a misleading
   chart.

This rule applies anywhere in `.app.yml` authoring where you query
the warehouse before committing layout decisions, not just the
onboarding flow.

## Common patterns

### Multi-task dashboard (KPIs + trend + breakdown)

```yaml
name: executive_dashboard

tasks:
  - name: kpis
    type: semantic_query
    topic: sales_mrr
    measures:
      - sales.total_revenue
      - sales.order_count
      - sales.avg_order_value

  - name: monthly
    type: semantic_query
    topic: sales_mrr
    dimensions:
      - sales.month
    measures:
      - sales.total_revenue
    orders:
      - field: sales.month
        direction: asc

  - name: regional
    type: semantic_query
    topic: sales_mrr
    dimensions:
      - sales.region
    measures:
      - sales.total_revenue
    orders:
      - field: sales.total_revenue
        direction: desc

display:
  - type: markdown
    content: "# Executive Dashboard"

  - type: table
    title: "Key Metrics"
    data: kpis

  - type: line_chart
    title: "Monthly Revenue"
    data: monthly
    x: sales__month
    y: sales__total_revenue

  - type: bar_chart
    title: "Revenue by Region"
    data: regional
    x: sales__region
    y: sales__total_revenue

  - type: table
    title: "Regional Details"
    data: regional
```

### Semantic query + agent insights

```yaml
tasks:
  - name: revenue_by_segment
    type: semantic_query
    topic: sales_metrics
    dimensions: [segment.name]
    measures: [sales.total_revenue]

  - name: callouts
    type: agent
    agent_ref: sales_analyst.agent.yml
    inputs: [revenue_by_segment]
    prompt: "Give 3 bullet-point callouts on segment performance."

display:
  - type: markdown
    title: "Callouts"
    content: "{{callouts}}"

  - type: bar_chart
    title: "Revenue by Segment"
    data: revenue_by_segment
    x: segment__name
    y: sales__total_revenue
```

## Build rules

1. **Prefer `semantic_query` over `execute_sql`** whenever a matching topic
   exists. It surfaces join logic the LLM otherwise has to re-derive.
2. **Every `display.data` must match a `tasks[].name` exactly.** Dot notation
   (`workflow_task.inner`) is allowed only for `workflow`-type tasks.
3. **Chart column fields (`x`, `y`, `series`, `name`, `value`) must match
   real output columns.** For semantic queries that means `view__field`.
4. **Unique, snake_case task names.** Duplicates silently shadow each other
   in some dialects.
5. **Never add `# yaml-language-server:` schema comments.** Oxy's validator
   treats unknown fields strictly.

## Validation

- `oxy validate --file=my_app.app.yml` checks YAML structure only — it
  does **not** execute task SQL.
- Apps render in the Oxy web UI (`oxy start --enterprise`). `oxy run` does
  **not** execute `.app.yml` files.
- **Pre-test referenced workflows and agents before finalizing the app.**
  Each task that points at a `workflow_ref` / `agent_ref` is opaque to
  `oxy validate`; SQL errors inside those files won't surface until the
  app runs. Run any referenced `*.workflow.yml` and `*.agent.yml` once
  to confirm they execute clean before opening the app.

## Smoke-testing the app (do this before declaring done)

Schema validation (`validate_project`) only catches structural YAML errors.
It does **not** catch malformed SQL — broken JOIN syntax, dialect-specific
type mismatches, missing `ON` clauses, type coercion failures — which all
fail at task-execution time. The most common failure mode is a brand-new
`.app.yml` that loads as a blank dashboard with a runtime error, because
the model wrote SQL that *parses* but doesn't *run* on the target warehouse.

After every write or edit to a `.app.yml`, run the file end-to-end and
confirm every task succeeds. Use the `run_app(file_path, params_json)`
tool when it is available — it executes the same path the dashboard takes
on first load and returns a per-task summary. Pass `params_json: "{}"` to
exercise control defaults; pass a real JSON object when you specifically
need to validate behavior under non-default control values. If the tool
is not available in the current run, fall back to running each task
manually: invoke `semantic_query` for `type: semantic_query` tasks
(passing the same topic / dimensions / measures) and `execute_sql` for
`type: execute_sql` tasks (passing the rendered `sql_query` against the
same `database`).

Failure-recovery loop (matches the pattern used by view smoke-tests):

1. If every task succeeds, stop — the file is done.
2. If any task fails, read the error, diagnose, and call `edit_file`
   **once** with a targeted corrective edit — exact `old_string` /
   `new_string` pair (most common fixes: re-state the JOIN with `ON`,
   replace a dialect-specific function with the matrix entry from
   `## SQL dialect notes`, or drop the offending measure / dimension
   and pick a different one from the view).
3. Re-run `run_app` (or the manual fallback) **once**. If it still
   fails, stop and report the error verbatim — do **not** loop. A second
   silent retry usually compounds the bad guess.

This step is non-negotiable for onboarding apps because the user has no
chance to fix the file before opening it; for chat-builder edits, prefer
running the smoke test whenever the warehouse tools are reachable.
