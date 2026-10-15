---
name: oxy-data-apps
description: >-
  Use when creating, editing, reviewing, or debugging an Oxy Data App — a declarative `.app.yml`
  file of tasks (`execute_sql`, `semantic_query`) and displays (tables, line / bar / pie charts,
  markdown, side-by-side rows) — for a dashboard, report, KPI page, or data app, including
  interactive filters: dropdowns, date pickers, toggles and other controls wired into SQL with
  `{{ controls.x }}`. Also when an app renders blank or shows an error card, a chart has no data,
  a control changes nothing, or an app is missing from the sidebar (`published:`). Not for
  code-first React / Vite apps with an `oxy-app.json` manifest shipped with `oxyc publish` — those
  are custom apps; use `oxy-custom-apps`.
---

# Oxy Data Apps

A **Data App** is one YAML file, `<name>.app.yml`, in an Oxy workspace. Its
**tasks** fetch data; its **displays** render that data as tables, charts and
markdown; optional **controls** let a viewer filter it live. Oxy renders the
file directly — there is no build step and no front-end code.

## Data app or custom app?

| | Data App (this skill) | Custom app |
| --- | --- | --- |
| What it is | Tasks + displays in one `.app.yml` | A React / TypeScript / Vite bundle |
| Identified by | the `.app.yml` file | an `oxy-app.json` manifest |
| Ships by | living in the workspace, `published: true` | `oxyc publish` |
| Use when | the page is tables, charts and markdown | the UI is bespoke |

Code-first React apps are a different product — see the `oxy-custom-apps` skill.

## The file

```yaml
title: Sales Overview            # shown in listings; falls back to the filename
description: |
  Revenue KPIs and the monthly trend.
published: true                  # default false = hidden from the sidebar

tasks:                           # required, at least one
  - name: monthly_revenue
    type: execute_sql
    database: warehouse          # a database name from config.yml
    sql_query: |
      SELECT DATE_TRUNC('month', order_date) AS month,
             SUM(amount) AS revenue
      FROM orders
      GROUP BY 1
      ORDER BY 1

display:                         # required, at least one
  - type: line_chart
    title: Revenue by month
    data: monthly_revenue        # a task name
    x: month
    y: revenue
    y_format: currency
```

- **Top-level keys are fixed:** `title`, `description`, `controls`, `tasks`,
  `display`, `published`. Anything else is rejected.
- **Leave `name:` out.** The app's name is its filename (`sales.app.yml` →
  `sales`). The server tolerates a `name:` key, but the published schema does
  not list it, so `oxyc validate` reports it as an unknown field.
- **Where it can live:** any `**/*.app.yml` in the workspace, except under a
  dot-prefixed directory or one named `target`, `node_modules`, `dist` or
  `build`, and never with `.test.` in the filename. Those paths are skipped,
  so the app silently does not exist.

## Tasks

Every task has a unique `name` (start with a letter; letters, digits and
underscores) and a `type`. Two types carry almost every app:

### `execute_sql`

```yaml
- name: by_region
  type: execute_sql
  database: warehouse
  sql_query: |
    SELECT region, SUM(amount) AS total
    FROM orders
    GROUP BY region
    ORDER BY total DESC
```

- `database` must name a database in `config.yml` (`oxy validate` checks it).
- Use `sql_query` (inline) **or** `sql_file` (a workspace-relative path to a
  `.sql` file). Inline SQL is the only form the browser can re-run when a
  control changes; `sql_file` tasks always run on the server.
- Output columns are your `SELECT` aliases — that is what `x:`, `y:` and
  friends reference.
- Results are capped at 10,000 rows. Aggregate in SQL; never ship raw rows to
  a chart.

### `semantic_query`

Asks the workspace's semantic model instead of writing SQL:

```yaml
- name: revenue_trend
  type: semantic_query
  topic: sales
  measures: [orders.total_revenue]
  dimensions: [orders.region]
  time_dimensions:
    - dimension: orders.order_date
      granularity: month          # year|quarter|month|week|day|hour|minute|second
  filters:
    - field: orders.order_date
      op: gte                     # eq neq gt gte lt lte take `value`
      value: "2024-01-01"         # in / not_in take `values: [...]`
  orders:
    - field: orders.order_date
      direction: asc
  limit: 500
```

- **Output columns are `view__field`**: `orders__total_revenue`,
  `orders__region`. A time dimension with a `granularity` gets one more
  suffix: `orders__order_date__month`. Reference exactly those names from the
  display, or the chart fails to bind its column.
- **Controls do not reach a `semantic_query`.** Its fields are sent as
  written; `{{ controls.x }}` is not rendered there. A filter that must follow
  a control has to be an `execute_sql` task.
- `in_date_range` / `not_in_date_range` filters take `from` and `to`.

### Other task types — mostly don't

App tasks run through the automation runner, so the schema accepts every
automation task type. In a Data App:

- **`type: agent` does not work.** It validates, but a Data App runs its
  tasks without an agent runtime, so the run fails with
  `agent step '…' requires an InlineAgentRunner`. For AI commentary on the
  data, point people at chat with an `.agentic.yml` agent instead (the
  `oxy-agents` skill).
- **`type: workflow`** (the YAML keyword is still `workflow`) runs an
  automation: `src: automations/report.automation.yml` plus optional
  `variables:`. Its result reaches the app as **one text value**, not as
  tables, so `data: report.some_step` passes validation (only the part before
  the dot is checked) and renders nothing. Put the queries in the app as
  `execute_sql` / `semantic_query` tasks instead.
- **`type: airway`** cannot run inside an app at all.
- A **misspelled `type:`** passes `oxy validate` and fails at run time with
  `unknown task type`. `oxyc validate` does catch it.

## Displays

Each `display:` entry is a block, rendered top to bottom. A block that fails
to parse becomes a single error card; the rest of the app still renders.

| `type` | Required | Optional |
| --- | --- | --- |
| `markdown` | `content` | — |
| `table` | `data` | `title`, `formats` |
| `line_chart` | `data`, `x`, `y` | `title`, `series`, `x_axis_label`, `y_axis_label`, `y_format` |
| `bar_chart` | `data`, `x`, `y` | `title`, `series`, `y_format` |
| `pie_chart` | `data`, `name`, `value` | `title`, `value_format` |
| `row` | `children` | `columns` (≥ 1; defaults to the number of children) |
| `control` / `controls` | see [Controls](#controls) | |

- **`data:` names a task.** `oxy validate` rejects a reference to a task that
  does not exist.
- **`series:`** splits a line or bar chart into one series per value of that
  column (multiple lines, grouped bars).
- **Number formats** — `y_format`, `value_format`, and the per-column
  `formats:` map on a table take `currency`, `percent` (the value is already
  0–100, not a ratio) or `number` (thousands separators). Table `formats` keys
  are output column names.
- **`markdown` has no `title`** (a `title:` there is silently ignored) — put a
  heading in `content`. Its content is a template: `{{ task_name }}` inserts a
  task's result, useful for a text result and useless for a table (it renders
  as a JSON code block).
- **Axis labels exist only on `line_chart`.** On `bar_chart`,
  `x_axis_label` / `y_axis_label` are silently ignored — no validator flags
  them.
- **`row` children are displays, never controls.** A `type: control` inside a
  row is not registered as a control and does not render.

### Labels, not IDs

Never put an opaque key (`customer_id`, a UUID) on a chart axis, a pie
`name:`, or as the only label in a table. Pull a readable name instead:
through the semantic model (a joined view's name dimension, e.g.
`stores__store_name` — the `oxy-semantic-modeling` skill covers foreign
entities), or with an explicit `JOIN` in `execute_sql`. If no name exists
anywhere, keep the ID but say so in a markdown note.

## Controls

Controls are dropdowns, date pickers and toggles rendered as a bar above the
app. Changing one re-runs every task whose SQL reads it.

| Kind | Widget | Value |
| --- | --- | --- |
| `select` | dropdown | string |
| `date` | date picker | string `YYYY-MM-DD` |
| `toggle` | on/off switch | boolean |

### Declaring them

There are three equivalent places. Pick one per app — they are merged, so a
control declared twice shows up twice.

```yaml
display:
  # 1. Inline, one block per control (recommended). The kind is `control_type:`.
  - type: control
    name: region
    control_type: select       # NOT `type:` — `type: control` already took that key
    label: Region
    options: [All, North, South]
    default: All

  # 2. Inline group. Items use plain `type:`.
  - type: controls
    items:
      - name: start_date
        type: date
        default: "2024-01-01"

# 3. Top-level array, beside tasks/display. Items use plain `type:`.
controls:
  - name: holidays_only
    type: toggle
    default: false
```

Fields: `name` (required; read as `controls.<name>`), the kind (`control_type`
inline, `type` otherwise), `label` (defaults to the name), `default`,
`options` (static list) **or** `source` (a task whose **first column** fills
the dropdown — include a sentinel such as `All` in that query yourself).

`default:` and `options:` may be Jinja evaluated once when the app loads —
`default: "{{ now(fmt='%Y-%m-%d') }}"`. A value a viewer sends is data, never
a template. An empty value falls back to the default.

### Using them in SQL

```sql
SELECT category, SUM(revenue) AS total
FROM sales
WHERE sale_date >= {{ controls.start_date | sqlquote }}
  AND ({{ controls.region | sqlquote }} = 'All'
       OR region = {{ controls.region | sqlquote }})
  {% if controls.holidays_only %}AND period = 'Holiday'{% endif %}
GROUP BY category
```

1. **Pipe every string and date through `| sqlquote`.** It adds the quotes
   and escapes embedded ones.
2. **Never add quotes around it** — `'{{ controls.x | sqlquote }}'` produces
   `''value''`.
3. **Optional filters use a sentinel.** A `select` always has a value, so
   offer `All` and write `(… = 'All' OR col = …)`.
4. **Toggles go in `{% if controls.flag %}…{% endif %}`.**
5. **Dates are strings** — compare directly or `CAST(… AS DATE)`.

### Where a re-run happens (`mode`)

Each task takes `mode: client` (the default) or `mode: server`.

- **Client** — the browser re-runs the SQL in an in-browser DuckDB. It only
  applies when **every** task feeding a display is an inline `execute_sql`
  against a **local DuckDB** database, reading quoted files
  (`FROM 'sales.csv'`) or inline `VALUES`. Each task runs alone, so one task's
  SQL cannot read another task's result.
- **Server** — everything else, automatically: warehouses, DuckLake,
  `sql_file`, `semantic_query`. Setting `mode: server` is always safe.
- The browser understands only four Jinja forms: `{{ controls.x }}`,
  `{{ controls.x | sqlquote }}`, `{{ controls.x | default('v') }}`, and
  `{% if controls.x %}…{% endif %}`. Anything else — `{% else %}`, loops,
  comparisons — makes the browser give up and re-run on the server for the
  rest of the session. It still works, just slower; put comparison logic in
  SQL (`CASE`, `OR`) to keep the fast path.

## Publishing and visibility

- **`published` defaults to `false`, and an unpublished app is hidden from the
  sidebar.** Authors still open, edit and run it from the IDE, so "it works
  for me but nobody can find it" is almost always this line.
- Publish by setting `published: true`, or with the IDE's publish toggle,
  which writes that same line into the file. Changing it needs an editing
  role; Viewers cannot.
- There is nothing to upload. `oxyc publish` is for custom apps and has
  nothing to do with Data Apps.

## Building one

For a new app, plan before writing YAML, and confirm the plan with the user:

```text
App: <file name> — <one-sentence purpose>
Data: <database.table or semantic topic>
Tasks:    <task_name>: <intent> -> columns <a, b, c>
Displays: <type>: <what it shows> (data: <task_name>)
```

1. **Profile before charting.** Before you commit to a trend or a ranking,
   check that the data supports it — enough rows, at least three months for a
   trend, a measure that actually varies, an entity count worth ranking. The
   profiling query is in [the quick reference](references/quick-reference.md#profiling-query).
2. **Write the file.** Start from a template (below), match the database's SQL
   dialect, alias every output column, `ORDER BY` anything a chart draws.
3. **Validate.**
   - `oxyc validate --file apps/sales.app.yml` — offline schema check;
     catches unknown keys, a wrong `type`, `type:` vs `control_type:`.
   - `oxy validate --file apps/sales.app.yml` — also checks that each
     `database:` exists in `config.yml` and each display's `data:` names a
     task.
   - Neither runs SQL, checks a `{{ controls.x }}` reference against the
     declared controls, or checks that `source:` names a task.
4. **Run the queries.** Copy a task's SQL into a scratch file with literal
   values in place of the controls, then
   `oxy run scratch.sql --database warehouse`. For a referenced automation,
   `oxy run automations/report.automation.yml`.
5. **Open the app** (locally: `oxy start --enterprise`, then the IDE).
   Every block should be populated; a failing task shows an error naming it.
   Change each control and watch its tasks re-run.

**When something fails, fix it once.** Read the error and make one targeted
change — restate a `JOIN … ON`, swap a function for the dialect's equivalent,
drop a measure that will not resolve — and re-run. If it fails again, stop and
report the error verbatim; do not loop on guesses. If a profiling query fails
twice, drop that chart rather than ship a misleading one.

**Editing an existing app:** read it, say what it does now, propose the
smallest change, and keep its existing style and naming.

## Pitfalls

| Symptom | Cause | Fix |
| --- | --- | --- |
| App missing from the sidebar | `published` is false (the default) | `published: true`, or the IDE publish toggle |
| App does not exist at all | path under a dot dir / `build` / `dist`, or `.test.` in the name | move or rename it |
| `oxyc validate`: unknown field `name` | top-level `name:` | delete it; the filename is the name |
| Chart blank, column not found | `semantic_query` column referenced without `view__` or the `__<granularity>` suffix | use the exact output column name |
| Control renders but changes nothing | its value is not used in any `execute_sql`, or it feeds a `semantic_query` | template it into `execute_sql` SQL |
| `type: select` inside `- type: control` rejected | the inline form uses `control_type:` | `control_type: select` |
| SQL error near `''value''` | quotes added around `sqlquote` | remove your quotes |
| Control inside a `row` never appears | rows only hold displays | move it to the top level of `display:` |
| `requires an InlineAgentRunner` | `type: agent` task | remove it; use chat with an agent |
| `data: task.step` renders nothing | sub-automation results arrive as one text value | query directly in the app |
| `unknown task type` at run time | misspelled task `type:` | fix it; `oxyc validate` catches this |
| Dashboard slow on every control change | browser path not possible, or rare Jinja forced the server | keep to the four Jinja forms; aggregate in SQL |
| Chart axis is a list of UUIDs | key column on `x:` / `name:` | join to a readable name |

## Read next

- [references/quick-reference.md](references/quick-reference.md) — every field
  of every task, display and control; dialect cheatsheet; checklist.
- [references/examples.md](references/examples.md) — prompts turned into plans
  and finished apps, plus a minimal edit.
- [templates/simple-dashboard.app.yml](templates/simple-dashboard.app.yml) —
  KPIs, a trend, and a breakdown.
- [templates/dashboard-with-controls.app.yml](templates/dashboard-with-controls.app.yml)
  — all three control kinds, a data-driven dropdown, and a row layout.
- Related skills: `oxy-semantic-modeling` (views and topics a `semantic_query`
  reads), `oxy-automations` (`.automation.yml`), `oxy-agents` (`.agentic.yml`),
  `oxy-custom-apps` (code-first apps).
