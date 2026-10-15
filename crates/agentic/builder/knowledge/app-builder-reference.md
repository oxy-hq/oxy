---
source:
  - sdk/cli/skills/oxy-data-apps/SKILL.md
  - sdk/cli/skills/oxy-data-apps/references/quick-reference.md
reconciled-hash: 246b6d44626fbbeb50b1d377e5dab234f0f670b2
note: |
  Authored condensation of the platform skills listed above. Not auto-synced —
  scripts/sync-skills.sh only copies the verbatim YAML templates. Re-condense
  when a source changes; keep under ~200 lines so the LLM context stays focused.
  scripts/check-skills-drift.sh flags a changed source and prints the hash to record.
---

# Oxy Data App Reference

A **Data App** is one file, `<name>.app.yml`: **tasks** fetch data,
**displays** render it as tables, charts and markdown, optional **controls**
filter it live. Oxy renders the file directly — no build step, no front-end
code.

> **Two app models — don't confuse them.** A **custom app** is a
> React/TypeScript/Vite bundle identified by an **`oxy-app.json`** manifest
> and shipped with `oxyc publish` (the `oxy-custom-apps` platform skill covers
> it). `oxy-app.json` is that product's manifest, not an alternative to
> `.app.yml` — never claim "there is no oxy-app.json". Build a `.app.yml` when
> the page is tables, charts and markdown; a custom app when the UI is bespoke.

## File

```yaml
title: Sales Overview            # listings; falls back to the filename
description: Revenue KPIs and the monthly trend.
published: true                  # default false = hidden from the sidebar
tasks:                           # required, ≥ 1
  - name: monthly_revenue
    type: execute_sql
    database: warehouse          # a config.yml database name
    sql_query: |
      SELECT DATE_TRUNC('month', order_date) AS month, SUM(amount) AS revenue
      FROM orders GROUP BY 1 ORDER BY 1
display:                         # required, ≥ 1
  - type: line_chart
    title: Revenue by month
    data: monthly_revenue
    x: month
    y: revenue
    y_format: currency
```

- **Top-level keys are fixed:** `title`, `description`, `controls`, `tasks`,
  `display`, `published`. Anything else is rejected.
- **Leave `name:` out** — the filename is the name (`sales.app.yml` →
  `sales`); `oxyc validate` reports `name` as an unknown field.
- **Location:** any `**/*.app.yml` except under a dot-directory, `target`,
  `node_modules`, `dist` or `build`, and never with `.test.` in the name —
  those paths are skipped and the app silently does not exist.
- **`published` defaults to `false`** = hidden from the sidebar (authors still
  see it in the IDE). "Works for me but nobody can find it" is this line.

## Tasks

Every task has a unique `name` (letter first; letters, digits, underscores)
and a `type`. Use `execute_sql` and `semantic_query`.

### `semantic_query` — prefer it when a matching topic exists

```yaml
- name: revenue_trend
  type: semantic_query
  topic: sales
  measures: [orders.total_revenue]
  dimensions: [stores.store_name]
  time_dimensions:
    - dimension: orders.order_date
      granularity: month          # year|quarter|month|week|day|hour|minute|second
  filters:
    - field: orders.order_date
      op: gte                     # eq neq gt gte lt lte → `value`
      value: "2024-01-01"         # in / not_in → `values: [...]`
  orders:                         # in_date_range / not_in_date_range → `from`, `to`
    - field: orders.order_date
      direction: asc
  limit: 500                      # also `offset`
```

- **Output columns are `view__field`** (`orders__total_revenue`,
  `stores__store_name`); with a granularity, one more suffix:
  `orders__order_date__month`. Reference exactly those from displays or the
  chart fails to bind its column.
- **Controls do not reach a `semantic_query`** — `{{ controls.x }}` is not
  rendered there. A filter that must follow a control needs `execute_sql`.

### `execute_sql`

```yaml
- name: by_region
  type: execute_sql
  database: warehouse
  sql_query: |
    SELECT region, SUM(amount) AS total FROM orders GROUP BY region ORDER BY total DESC
```

- `sql_query` (inline) **or** `sql_file` (workspace-relative `.sql`, always
  server-side). Optional `variables:` map is available to the template.
- Output columns are the `SELECT` aliases. Results cap at **10,000 rows** —
  aggregate in SQL, never ship raw rows to a chart.

### Other task types — don't

- **`type: agent` does not work** in a Data App: it validates, then fails
  with `agent step '…' requires an InlineAgentRunner`. For AI commentary,
  point people at chat with an `.agentic.yml` agent.
- **`type: workflow`** (`src: automations/x.automation.yml`, `variables:`)
  returns **one text value**, so `data: report.some_step` passes validation
  and renders nothing. Put the queries in the app instead.
- **`type: airway`** cannot run in an app. A misspelled `type:` passes
  `oxy validate` and fails at run time with `unknown task type`.

## Displays

Rendered top to bottom; a block that fails to parse becomes one error card.

| `type` | Required | Optional |
| --- | --- | --- |
| `markdown` | `content` | — (a `title:` is silently ignored) |
| `table` | `data` | `title`, `formats: { column: currency\|percent\|number }` |
| `line_chart` | `data`, `x`, `y` | `title`, `series`, `x_axis_label`, `y_axis_label`, `y_format` |
| `bar_chart` | `data`, `x`, `y` | `title`, `series`, `y_format` (axis labels silently ignored) |
| `pie_chart` | `data`, `name`, `value` | `title`, `value_format` |
| `row` | `children` (displays only) | `columns` (≥ 1; default = number of children) |

- `data:` names a task. `series:` splits a chart per value of that column.
- Formats: `currency` (USD), `percent` (value already 0–100, not a ratio),
  `number` (thousands separators).
- `markdown` `content` is a template: `{{ task_name }}` inserts a text result
  (a table renders as a JSON code block). Put headings in `content`.
- **`row` children are displays, never controls** — a control inside a row
  is not registered and does not render.
- **Labels, not IDs.** Never put an opaque key (`customer_id`, a UUID) on an
  axis, a pie `name:`, or as a table's only label. Pull a readable name
  through the semantic model (a foreign entity → the joined view's name
  dimension, e.g. `stores__store_name`; see the semantic-layer card) or an
  explicit `JOIN` in `execute_sql`. If none exists, keep the ID and say so in
  a markdown note.

## Controls

| Kind | Widget | Value |
| --- | --- | --- |
| `select` | dropdown | string |
| `date` | date picker | string `YYYY-MM-DD` |
| `toggle` | on/off | boolean |

Three equivalent declaration places — **pick one per app**; they merge, so a
control declared twice renders twice:

```yaml
display:
  - type: control              # 1. inline (recommended): kind is `control_type:`
    name: region
    control_type: select       # NOT `type:` — `type: control` took that key
    label: Region
    options: [All, North, South]
    default: All
  - type: controls             # 2. inline group: items use plain `type:`
    items:
      - { name: start_date, type: date, default: "2024-01-01" }
controls:                      # 3. top-level array: items use plain `type:`
  - { name: holidays_only, type: toggle, default: false }
```

Fields: `name` (read as `controls.<name>`), the kind, `label`, `default`, and
`options` (static) **or** `source` (a task whose **first column** fills the
dropdown — include an `All` sentinel row in that query yourself). `default:`
and `options:` may be Jinja rendered once at load
(`"{{ now(fmt='%Y-%m-%d') }}"`). An empty viewer value falls back to the
default; a value for an undeclared control is dropped.

**Using them in SQL** (`execute_sql` only):

```sql
WHERE sale_date >= {{ controls.start_date | sqlquote }}
  AND ({{ controls.region | sqlquote }} = 'All' OR region = {{ controls.region | sqlquote }})
  {% if controls.holidays_only %}AND period = 'Holiday'{% endif %}
```

1. Pipe every string and date through `| sqlquote` (adds and escapes quotes).
2. **Never add quotes around it** — `'{{ controls.x | sqlquote }}'` → `''value''`.
3. Optional filters use the `All` sentinel; a `select` always has a value.
4. Toggles go in `{% if controls.flag %}…{% endif %}`.
5. Dates are strings — compare directly or `CAST(… AS DATE)`.

**`mode`:** `client` (default) re-runs in the browser's DuckDB only when
**every** task feeding a display is inline `execute_sql` on a **local DuckDB**
database (quoted files or `VALUES`); each runs alone, so one task can't read
another's result. Everything else — warehouses, DuckLake, `sql_file`,
`semantic_query` — runs on the **server** automatically; `mode: server` is
always safe. The browser understands only `{{ controls.x }}`,
`{{ controls.x | sqlquote }}`, `{{ controls.x | default('v') }}` and
`{% if controls.x %}…{% endif %}`; anything else (`{% else %}`, loops,
comparisons) falls back to the server for the rest of the session — still
works, slower. Put comparison logic in SQL (`CASE`, `OR`).

## SQL dialects and profiling

| Dialect | Month bucket | Std-dev | Notes |
| --- | --- | --- | --- |
| BigQuery | `DATE_TRUNC(col, MONTH)` | `STDDEV(col)` | backtick identifiers; `DATE_ADD(d, INTERVAL 1 DAY)` |
| Snowflake | `DATE_TRUNC('month', col)` | `STDDEV(col)` | `"col"` identifiers |
| Postgres | `DATE_TRUNC('month', col)` | `STDDEV(col)` | `x::date` works here only |
| DuckDB | `DATE_TRUNC('month', col)` | `STDDEV(col)` | `INTERVAL '1 day'` |
| ClickHouse | `toStartOfMonth(col)` | `stddevPop(col)` | functions are case-sensitive |

`CAST(x AS DATE)` is the portable cast. **Profile before charting** a trend
or ranking — one query per candidate table, with the view's real `expr:`
strings (never guessed columns):

```sql
SELECT COUNT(*) AS row_count, COUNT(DISTINCT <entity_expr>) AS entity_count,
       MIN(<time_expr>) AS min_date, MAX(<time_expr>) AS max_date,
       COUNT(DISTINCT DATE_TRUNC('month', <time_expr>)) AS month_count,
       MIN(<measure_expr>) AS min_val, MAX(<measure_expr>) AS max_val,
       STDDEV(<measure_expr>) AS measure_stddev
FROM <table>
```

Worth charting when `row_count ≥ 100`, `month_count ≥ 3` (trend),
`measure_stddev > 0` (not a flat line), `entity_count` 5–500 (top-N). If it
fails, simplify once (drop `STDDEV`, swap the date function); if it fails
again, drop that chart rather than ship a misleading one.

## Building and validating

For a new app, plan first and confirm it with the user (`App`, `Data`,
`Tasks: name → columns`, `Displays: type (data: task)`). Start from the
template, match the dialect, alias every column, `ORDER BY` what a chart
draws. Editing: read the file, say what it does, make the smallest change in
its existing style.

- `validate_project` / `oxyc validate --file` — schema: unknown keys, wrong
  `type`, `type:` vs `control_type:`. `oxy validate --file` also checks each
  `database:` exists and each `data:` names a task. **Neither runs SQL**,
  checks `{{ controls.x }}` against declared controls, or checks `source:`.
- Locally, apps open via `oxy start --enterprise` in the IDE.

### Smoke-test the app (do this before declaring done)

Validation misses SQL that *parses* but doesn't *run* on the target warehouse
— the most common failure is a new app that opens as a blank dashboard. After
every write or edit to a `.app.yml`, call `run_app(file_path, params_json)`
when available — it runs the same path as the dashboard's first load and
returns a per-task summary. `params_json: "{}"` exercises control defaults;
pass real values to test non-default behaviour. Without `run_app`, run each
task manually: `semantic_query` with the same topic/measures/dimensions,
`execute_sql` with the rendered SQL against the same `database`.

**Fix once, then stop:** if a task fails, read the error and make **one**
targeted `edit_file` (restate a `JOIN … ON`, swap a function for the
dialect's equivalent, drop a measure that won't resolve), re-run once, and if
it still fails report the error verbatim — don't loop on guesses. This is
non-negotiable for onboarding apps (the user can't fix the file before
opening it); for chat-builder edits, smoke-test whenever warehouse tools are
reachable.

## Pitfalls

| Symptom | Fix |
| --- | --- |
| missing from the sidebar | `published: true` (default is false) |
| app doesn't exist at all | move out of a dot/`build`/`dist` dir; drop `.test.` from the name |
| `oxyc validate`: unknown field `name` | delete top-level `name:` |
| chart blank, column not found | use `view__field` and the `__<granularity>` suffix |
| control changes nothing | template it into `execute_sql`; it can't feed `semantic_query` |
| `type: select` inside `- type: control` rejected | `control_type: select` |
| SQL error near `''value''` | remove your quotes around `sqlquote` |
| control inside a `row` never appears | move it to the top level of `display:` |
| `requires an InlineAgentRunner` | remove the `type: agent` task |
| `data: task.step` renders nothing | query directly in the app |
| slow on every control change | keep to the four Jinja forms; aggregate in SQL |
| axis is a list of UUIDs | join to a readable name |
