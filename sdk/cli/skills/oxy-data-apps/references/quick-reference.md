# Data App quick reference

Field-by-field reference for `.app.yml`. The why and the traps live in
[SKILL.md](../SKILL.md); this file is for looking things up.

## Top level

| Key | Type | Required | Notes |
| --- | --- | --- | --- |
| `title` | string | no | Display name in listings. Unset → humanized filename. |
| `description` | string | no | Free text. |
| `published` | bool | no | Default `false` = hidden from the sidebar (still in the IDE). |
| `controls` | list | no | Top-level control declarations (items use `type:`). |
| `tasks` | list | **yes** (≥ 1) | Data producers. |
| `display` | list | **yes** (≥ 1) | Rendered blocks, top to bottom. |

No other key is accepted. Omit `name:` — the filename is the name, and
`oxyc validate` reports `name` as unknown.

## Task fields (all types)

| Key | Notes |
| --- | --- |
| `name` | Unique in the app. Letter first; letters, digits, underscores. |
| `type` | `execute_sql`, `semantic_query` (use these); `workflow`, `formatter`, `conditional`, `loop_sequential`, `http_request`, `omni_query`, `looker_query` are accepted; `agent` and `airway` fail at run time. |
| `mode` | `client` (default) or `server`. Forced to `server` unless the task is inline `execute_sql` on a local DuckDB. |
| `cache` | Optional `{ enabled, path }`. Rarely needed in an app. |

### `execute_sql`

| Key | Required | Notes |
| --- | --- | --- |
| `database` | yes | A `config.yml` database name. Checked by `oxy validate`. |
| `sql_query` | one of | Inline SQL. Jinja: `{{ controls.x }}` and friends. |
| `sql_file` | one of | Workspace-relative `.sql` path. Rendered with the same Jinja; always server-side. |
| `variables` | no | Map of string → string, available to the SQL template. |
| `dry_run_limit` | no | ≥ 1. |
| `export` | no | `{ path, format: sql\|csv\|json\|txt\|docx }` — an automation feature; not needed in apps. |

Rows returned: at most 10,000.

### `semantic_query`

| Key | Notes |
| --- | --- |
| `topic` | Topic name from a `.topic.yml`. |
| `measures` | `[view.measure, …]` |
| `dimensions` | `[view.dimension, …]` |
| `time_dimensions` | `[{ dimension: view.field, granularity: year\|quarter\|month\|week\|day\|hour\|minute\|second }]` |
| `filters` | `[{ field, op, … }]` — see below |
| `orders` | `[{ field, direction: asc\|desc }]` |
| `limit`, `offset` | integers |
| `variables` | map |

Filter ops:

| `op` | Extra keys |
| --- | --- |
| `eq`, `neq`, `gt`, `gte`, `lt`, `lte` | `value` |
| `in`, `not_in` | `values: [...]` |
| `in_date_range`, `not_in_date_range` | `from`, `to` |

Output columns: `view__field`; with granularity, `view__field__<granularity>`.
Controls are **not** rendered into any `semantic_query` field.

### `workflow` (runs an automation)

| Key | Notes |
| --- | --- |
| `src` | Path to an `.automation.yml` (legacy `.procedure.yml` still works). Don't point it at a `.workflow.yml` — that extension is no longer a recognised automation file. |
| `variables` | Map passed to the automation. |

Its result reaches the app as one text value, so displays cannot address its
inner steps.

## Displays

| `type` | Required | Optional |
| --- | --- | --- |
| `markdown` | `content` | — |
| `table` | `data` | `title`, `formats: { column: currency\|percent\|number }` |
| `line_chart` | `data`, `x`, `y` | `title`, `series`, `x_axis_label`, `y_axis_label`, `y_format` |
| `bar_chart` | `data`, `x`, `y` | `title`, `series`, `y_format` |
| `pie_chart` | `data`, `name`, `value` | `title`, `value_format` |
| `row` | `children` (≥ 1 display) | `columns` (≥ 1; default = number of children) |
| `control` | `name`, `control_type` | `label`, `options` or `source`, `default` |
| `controls` | `items` (control objects using `type:`) | — |

Formats: `currency` (USD), `percent` (input already 0–100), `number`
(thousands separators). Unset → default numeric formatting.

`data:` takes a task name. `markdown` `content` is a template over task
results: `{{ task_name }}` inserts a text result; a table result renders as a
JSON code block.

## Controls

| Field | Required | Applies to | Notes |
| --- | --- | --- | --- |
| `name` | yes | all | Read as `{{ controls.<name> }}`. |
| `control_type` | yes (inline `- type: control`) | all | `select` \| `date` \| `toggle` |
| `type` | yes (top-level `controls:` and `controls.items`) | all | same values |
| `label` | no | all | Defaults to `name`. |
| `default` | no | all | May be Jinja, rendered once at load: `"{{ now(fmt='%Y-%m-%d') }}"`. |
| `options` | no | `select` | Static list; entries may be Jinja too. |
| `source` | no | `select` | Task name; its first column fills the dropdown. Use instead of `options`. |

Value types: `select` → string, `date` → `YYYY-MM-DD` string, `toggle` →
boolean. A viewer's empty value falls back to `default`; a value for an
undeclared control is dropped.

### Jinja the browser can run

| Form | Effect |
| --- | --- |
| `{{ controls.x }}` | raw value (single quotes doubled) |
| `{{ controls.x \| sqlquote }}` | `'value'` with embedded quotes escaped |
| `{{ controls.x \| default('v') }}` | value, or `v` when unset |
| `{% if controls.x %}…{% endif %}` | include the body when truthy |

Anything else runs on the server instead (full Jinja, `sqlquote` included).

## SQL dialect cheatsheet

| Dialect | Month bucket | Std-dev | Notes |
| --- | --- | --- | --- |
| BigQuery | `DATE_TRUNC(col, MONTH)` | `STDDEV(col)` | backtick identifiers; `DATE_ADD(d, INTERVAL 1 DAY)` |
| Snowflake | `DATE_TRUNC('month', col)` | `STDDEV(col)` | `"col"` identifiers |
| Postgres | `DATE_TRUNC('month', col)` | `STDDEV(col)` | `x::date` works here only |
| DuckDB | `DATE_TRUNC('month', col)` | `STDDEV(col)` | `INTERVAL '1 day'` |
| ClickHouse | `toStartOfMonth(col)` | `stddevPop(col)` | functions are case-sensitive |

`CAST(x AS DATE)` is the portable cast.

## Profiling query

Run once per candidate table before choosing a trend or a ranking. Substitute
real expressions from the table or view — never guess column names.

```sql
SELECT
  COUNT(*)                                     AS row_count,
  COUNT(DISTINCT <entity_expr>)                AS entity_count,
  MIN(<time_expr>)                             AS min_date,
  MAX(<time_expr>)                             AS max_date,
  COUNT(DISTINCT DATE_TRUNC('month', <time_expr>)) AS month_count,
  MIN(<measure_expr>)                          AS min_val,
  MAX(<measure_expr>)                          AS max_val,
  STDDEV(<measure_expr>)                       AS measure_stddev
FROM <table>
```

Worth charting when `row_count ≥ 100`, `month_count ≥ 3` (for a trend),
`measure_stddev > 0` (not a flat line), and `entity_count` between 5 and 500
(for a top-N). If the query fails, simplify once (drop `STDDEV`, swap the date
function for the dialect's); if it fails again, skip that chart.

## Checklist

- [ ] `oxyc validate --file <app>` and `oxy validate --file <app>` pass
- [ ] No top-level `name:`; `published:` set as intended
- [ ] Task names unique; every display `data:` names a real task
- [ ] Chart columns match the task's output columns exactly (`view__field`,
      `__<granularity>` for semantic queries)
- [ ] No IDs on axes or pie labels
- [ ] Inline `- type: control` blocks use `control_type:`; no controls inside a `row`
- [ ] Every `{{ controls.x }}` matches a declared control; every `source:` names a task
- [ ] Strings and dates go through `| sqlquote` with no extra quotes
- [ ] No `type: agent` or `type: airway` tasks; no `data: task.step` references
- [ ] Each task's SQL has been run once against the real database
- [ ] The app opens with every block populated, and each control re-runs its tasks
