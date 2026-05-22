---
source:
  - oxy-hq/skills/skills/oxy-semantic-layer/SKILL.md
  - oxy-hq/skills/skills/oxy-semantic-layer/QUICK-REFERENCE.md
reconciled-at: 445c5459bf050fc65b323d73933e11538555deb5
note: |
  Authored condensation. Not auto-synced — scripts/sync-skills.sh only copies
  the verbatim YAML templates. Re-condense by hand when source material
  changes materially; keep under ~200 lines so the LLM context stays focused.
  scripts/check-skills-drift.sh flags upstream changes.
---

# Oxy Semantic Layer Reference

The semantic layer is a pair of YAML file types:

- `*.view.yml` — maps a database table to entities, typed dimensions, and
  measures.
- `*.topic.yml` — groups related views into a business domain that
  natural-language queries target.

Files can live anywhere under `semantics/` — flat (`semantics/orders.view.yml`)
or nested (`semantics/views/orders.view.yml`). The parser walks the tree
recursively, so layout is up to you. Onboarding-generated projects
scaffold flat; large projects often split into `semantics/views/` and
`semantics/topics/` for organization. Both are equally valid.

## View file shape

```yaml
name: snake_case_view_name
description: "Business-friendly explanation of what this view represents"
datasource: "database_name"        # Must match a database name in config.yml
table: "schema_name.table_name"

entities:
  - name: primary_entity
    type: primary                   # exactly one primary entity per view
    description: "The main subject of this view"
    key: primary_key_dimension      # MUST reference a dimension name below

dimensions:
  - name: primary_key_dimension     # referenced by entity.key above
    type: string                    # string|number|date|datetime|boolean
    description: "Primary key of the table"
    expr: primary_key_column        # raw SQL expression

  - name: dimension_name
    type: string
    description: "Business-friendly attribute description"
    expr: column_or_expression
    samples: ["value1", "value2"]   # optional; helps the LLM pick valid filter values
    synonyms: ["alt name"]          # optional; improves natural-language matching

measures:
  - name: total_records
    type: count                     # count is the only measure type that omits `expr`

  - name: total_amount
    type: sum
    description: "Sum of amount_column"
    expr: amount_column             # required for every non-count measure

  - name: active_count
    type: count
    filters:
      - expr: "{{status}} = 'active'"   # filtered measure
```

## Critical rules

1. **Entities are required.** Every view declares exactly one `type: primary`
   entity. Without it the view cannot participate in joins and several
   validators reject the file outright.
2. **Entity `key:` MUST reference a dimension name**, never a raw column. If the
   column you want to key on is called `order_id`, define a dimension with
   `name: order_id` and `expr: order_id`, then set `key: order_id` on the
   entity.
3. **`expr` is required** on every dimension. It's required on every measure
   except `count` (which counts rows). Omitting it produces cryptic compile
   errors at `oxy build` time.
4. **Dimension types are lowercase:** `string`, `number`, `date`, `datetime`,
   `boolean`. Any other spelling silently breaks type-aware behavior.
5. **Cross-view joins happen by matching entity names.** To join
   `orders.view.yml` and `customers.view.yml`, both views must declare an
   entity named `customer` (one primary, one foreign). Name mismatch = no join.
   In practice, this means: whenever a fact-style view has a UUID/FK column
   pointing at another view's primary key (e.g. `orders.restaurant_id` →
   `restaurants.guid`), declare a **`type: foreign`** entity on the fact
   view with the same `name:` as the lookup view's primary entity, and
   `key:` set to the FK dimension. Without this, dashboards can only show
   the raw FK and end up rendering opaque UUIDs on chart axes.

   ```yaml
   # in orders.view.yml
   entities:
     - name: order
       type: primary
       key: order_id
     - name: restaurant            # same name as restaurants.view.yml's primary entity
       type: foreign
       key: restaurant_id          # the FK dimension on this view

   dimensions:
     - name: restaurant_id
       type: string
       expr: restaurant_id
   ```

   Then a topic that includes both `orders` and `restaurants` lets a
   `semantic_query` ask for `restaurants.location_name` directly, and
   airlayer resolves the join.
6. **Never add `# yaml-language-server:` schema comments.** Oxy's validator
   deny-lists unknown fields and these comments trigger false positives in
   strict parsing paths.

## Dimension & measure types

| Dimension type | Example literal         | Typical use                 |
| -------------- | ----------------------- | --------------------------- |
| `string`       | `"active"`              | Categories, IDs, text       |
| `number`       | `42`, `3.14`            | Counts, amounts, metrics    |
| `date`         | `"2024-01-01"`          | Dates without a time-of-day |
| `datetime`     | `"2024-01-01 10:30:00"` | Timestamps                  |
| `boolean`      | `true` / `false`        | Flags                       |

| Measure type     | Requires `expr`? | Notes                        |
| ---------------- | ---------------- | ---------------------------- |
| `count`          | No               | Counts rows                  |
| `count_distinct` | Yes              | Counts distinct values of expr |
| `sum`            | Yes              |                              |
| `average`        | Yes              |                              |
| `median`         | Yes              |                              |
| `min` / `max`    | Yes              |                              |
| `custom`         | Yes              | Free-form SQL, e.g. `CORR(a, b)` |

## Common dimension patterns

**Date parts (use `EXTRACT`, supported across all dialects):**

```yaml
- name: order_year
  type: number
  expr: "EXTRACT(YEAR FROM order_date)"

- name: order_month
  type: number
  expr: "EXTRACT(MONTH FROM order_date)"

- name: order_quarter
  type: number
  expr: "EXTRACT(QUARTER FROM order_date)"
```

**Categorical bucketing via `CASE`:**

```yaml
- name: price_tier
  type: string
  expr: |
    CASE
      WHEN price < 100 THEN 'Low'
      WHEN price < 500 THEN 'Medium'
      ELSE 'High'
    END
  samples: ["Low", "Medium", "High"]
```

**Filtered measures** (using `{{dim_name}}` placeholders that reference
dimensions of the same view):

```yaml
- name: completed_revenue
  type: sum
  expr: amount
  filters:
    - expr: "{{status}} = 'completed'"

- name: high_value_orders
  type: count
  filters:
    - expr: "{{amount}} >= 1000"
```

## Topic file shape

```yaml
name: snake_case_topic_name
description: "What kinds of questions this topic answers"
base_view: primary_view_name        # the main view for this topic
views:
  - primary_view_name                # include base_view here too
  # - related_view_name              # add more if they share entities

default_filters:
  - field: view_name.status
    eq:
      value: "active"

  - field: view_name.status
    not_in:
      values: ["cancelled", "test"]

  - field: view_name.created_date
    in_date_range:
      from: "90 days ago"
      to: "now"
```

Default-filter operators: `eq`, `neq`, `gt`, `gte`, `lt`, `lte`, `in`,
`not_in`, `in_date_range`, `not_in_date_range`. Scalar operators take
`value:`; array operators take `values:`; range operators take `from:` /
`to:`.

## Naming and style

- **snake_case** for every `name:` — views, topics, dimensions, measures,
  entities.
- Descriptions are **business-friendly**, not column commentary. Prefer
  "Revenue in USD" to "SUM of amount_column".
- Prefer one topic per view unless two views genuinely share a domain.
- Add `synonyms:` on any dimension/measure where a non-technical user would
  use a different word ("revenue" vs "total_amount", "customer" vs "client").
- Add `samples:` on categorical dimensions so the LLM can suggest valid filter
  values without querying the data. **`samples` is always a list of strings**,
  even when the dimension `type` is `boolean` or `number` — write
  `samples: ["true", "false"]` and `samples: ["129.99", "89.50"]`, never bare
  literals. Bare booleans/numbers fail YAML deserialization and break the
  whole semantic layer load.

## Date columns: detect format, then cast

A date dimension MUST be `type: date` (or `type: datetime`) so the semantic
layer compiles filters as date literals. The expression must produce a real
`Date` / `DateTime`, not the raw underlying column. Mismatch produces a
`TYPE_MISMATCH` at filter time — silent at view-creation, trips the first
analytics query that filters on the dimension.

**Recognition heuristic.** Any column whose business meaning is a date or
timestamp (`*_date`, `*_at`, `business_date`, `order_date`, `created`,
`updated`, `event_time`, …) where the underlying SQL type is **not already**
`Date` / `DateTime` / `TIMESTAMP` needs a wrapping cast. Match by the
column's business meaning, not its physical type.

**Sample first, then cast.** Stored formats vary across warehouses and
across tables within the same warehouse — ISO strings (`"2024-01-31"`),
compact strings (`"20240131"`), date integers (`20240131`), Unix epoch
seconds, milliseconds, and so on. Run `SELECT <col> FROM <table> LIMIT 1`
to see what's actually there before deciding the cast. Guessing is cheap
to get wrong (silent failure at query time) and cheap to verify (one tool
call).

**Cast functions per warehouse.** Combine these with the format you
sampled. The dimension is `type: date` (or `datetime`); the `expr:` wraps
the column.

| Warehouse  | Functions                                                                     |
| ---------- | ----------------------------------------------------------------------------- |
| ClickHouse | `toDate(<col>)`, `toDateTime(<col>)`, `parseDateTimeBestEffort(<col>)`, `toDate(toString(<col>))` for integer-encoded dates |
| BigQuery   | `CAST(<col> AS DATE)`, `PARSE_DATE('<format>', <col>)`, `PARSE_TIMESTAMP('<format>', <col>)`, `TIMESTAMP_SECONDS(<col>)` |
| Snowflake  | `TO_DATE(<col>[, '<format>'])`, `TO_TIMESTAMP(<col>)`, `TRY_TO_DATE(<col>)`   |
| Postgres   | `(<col>)::date`, `(<col>)::timestamp`, `to_date(<col>, '<format>')`, `to_timestamp(<col>, '<format>')` |
| DuckDB     | `CAST(<col> AS DATE)`, `strptime(<col>, '<format>')`, `epoch_ms(<col>)`       |

For columns already typed `Date` / `DateTime` / `TIMESTAMP`, no cast is
needed — `expr: <col>` is enough.

## Common error triage

| Error                    | Likely cause                                    | Fix                                               |
| ------------------------ | ----------------------------------------------- | ------------------------------------------------- |
| "Entity key not found"   | `entity.key` points to a column, not a dimension | Add a dimension whose `name` matches the key     |
| "View not found"         | File outside `semantics/` tree or `views:` typo  | Must live anywhere under `semantics/` (flat or `views/` subdir, both work); fix typos in any topic's `views:` list |
| "Cannot join views"      | Entity names differ between the two views        | Use identical entity names on both sides         |
| "Invalid SQL in `expr`"  | Column doesn't exist or dialect mismatch         | Verify against `semantics.yml` / `.databases/`    |
| `TYPE_MISMATCH` at filter | Date dimension declared `type: number` / `string` over a non-Date column; semantic layer compiles a date literal that the column type rejects | Sample one row, switch to `type: date` (or `datetime`), wrap `expr` with the appropriate cast function from the table above |
| Unknown field on parse   | Schema-comment or typo                           | Remove `# yaml-language-server:` and fix casing  |

## Pre-aggregations

Pre-aggregations let Oxy cache heavy aggregations as local Parquet files so
that repeated semantic queries skip the warehouse entirely and run in
milliseconds against an in-memory DuckDB instance.

### Declaring a pre-aggregation

Add a `pre_aggregations:` block inside a `*.view.yml`. A `refresh_key` is
required on every rollup (or set once at the view level — see below).

```yaml
pre_aggregations:
  - name: orders_by_customer_daily   # snake_case, unique within the view
    measures:
      - total_orders                 # measure names from this view's measures:
      - total_revenue
    dimensions:
      - customer_id                  # dimension names from this view's dimensions:
      - status
    time_dimension: order_date       # optional; a date/datetime dimension
    granularity: day                 # required when time_dimension is set
                                     # second|minute|hour|day|week|month|quarter|year
    refresh_key:
      every: 1h                      # rebuild whenever 1 h has elapsed since last build

  - name: orders_summary
    measures:
      - total_orders
    dimensions:
      - status
    refresh_key:
      sql: "SELECT MAX(updated_at) FROM orders"   # rebuild when the result changes
```

**Field summary:**

| Field            | Required | Description                                          |
| ---------------- | -------- | ---------------------------------------------------- |
| `name`           | Yes      | Unique rollup identifier (snake_case)                |
| `measures`       | Yes      | One or more measure names to pre-aggregate           |
| `dimensions`     | No       | Dimension names to group by                          |
| `time_dimension` | No       | Date/datetime dimension for time-based rollups       |
| `granularity`    | If `time_dimension` set | `second`, `minute`, `hour`, `day`, `week`, `month`, `quarter`, `year` |
| `refresh_key`    | Yes (per rollup, or inherited from the view) | See below |

### refresh_key reference

A `refresh_key` tells Oxy when a cached rollup is stale and must be rebuilt.
There are two mutually exclusive forms:

**`every:` (interval-based)**
```yaml
refresh_key:
  every: 1h        # rebuild after this interval regardless of data changes
                   # examples: 30m  1h  6h  24h
```
Oxy compares the time elapsed since the last successful build against the
interval. If enough time has passed the rollup is queued for rebuild. The
result is cached in-process for `renewal_threshold` (default 120 s) so the
interval is not checked on every single query.

**`sql:` (change-detection)**
```yaml
refresh_key:
  sql: "SELECT MAX(updated_at) FROM orders"
```
Oxy runs the SQL against the rollup's source database and compares the
first cell of the first row against the value stored in `manifest.json`
from the last build. If the values differ, the rollup is stale and rebuilt.
Use any single-value query that advances with new data — `MAX(updated_at)`,
`COUNT(*)`, a checksum, etc. If the SQL fails (network error, bad query),
Oxy logs a warning and skips the rebuild rather than crashing.

**View-level `refresh_key` (default for all rollups)**

Set `refresh_key:` at the top of the view (outside `pre_aggregations:`) to
apply the same key to every rollup that does not declare its own:

```yaml
name: orders
datasource: warehouse
table: "public.orders"
refresh_key:              # default for all rollups in this view
  every: 1h

pre_aggregations:
  - name: by_status
    measures: [total_orders]
    dimensions: [status]
    # inherits refresh_key: every: 1h from the view

  - name: by_customer
    measures: [total_orders]
    dimensions: [customer_id]
    refresh_key:          # overrides the view-level key for this rollup only
      sql: "SELECT MAX(updated_at) FROM orders"
```

Rollup-level `refresh_key` always takes precedence over the view-level one.
A rollup with no `refresh_key` and no view-level default will be skipped
by the background worker.

**Disabling pre-aggregations for a view**

```yaml
pre_aggregations_enabled: false   # skip all rollups in this view during oxy build
```

### Config: enable the refresh worker

Pre-aggregations need a top-level `pre_aggregations:` block in `config.yml`
so the background refresh worker actually runs. All fields are optional —
defaults shown below apply when omitted:

```yaml
pre_aggregations:
  schema: AIRLAYER             # warehouse schema for rollup tables (default: AIRLAYER)
  database: local              # connector for builds (default: each view's datasource)
  refresh_worker:
    enabled: true              # set false to disable the worker
    heartbeat: "30s"           # how often the worker checks staleness
    renewal_threshold: "120s"  # how long a cached refresh_key result stays valid
```

### Default rollup (no `pre_aggregations:` block)

A view with no `pre_aggregations:` block still gets **one** default rollup
covering all dimensions × all measures at `day` granularity. Define
explicit rollups to control the grain and avoid an oversized default.

### Build and cache

```bash
oxy build          # compiles semantic layer and builds all pre-aggregation Parquet files
```

Parquet files land in `.airlayer/cache/` (next to `config.yml`). A
`manifest.json` in that directory records which rollup covers which query
shape. The directory is created automatically by `oxy build` — do not
create or edit it by hand.

### Runtime behaviour

When a semantic query arrives (agentic analytics pipeline, IDE semantic
explorer, or procedure step), Oxy checks `manifest.json` for a rollup
that covers the requested dimensions and measures. If one exists **and**
the Parquet file is present on disk, the query executes against the local
cache — no warehouse round-trip. The ⚡ badge appears in the UI whenever
preagg served the result.

If no rollup covers the query, or the file is missing, Oxy falls back to
the warehouse transparently.

### Coverage rules (when does a rollup cover a query?)

A rollup covers a query when:
- **Every requested measure** is listed in the rollup's `measures:`.
- **Every requested dimension** is listed in the rollup's `dimensions:`.
- **Every filtered dimension** is in `dimensions:` *or* is the rollup's
  `time_dimension` (filters on the time dimension don't need it duplicated
  into `dimensions:`).
- **If a time dimension is requested**, it matches the rollup's
  `time_dimension` at an equal or coarser `granularity` (a `month` rollup
  serves `month`/`quarter`/`year`, not `day`).

A query that adds an extra dimension not in the rollup is **not** covered —
the rollup would need to include that dimension to avoid data loss during
re-aggregation.

**Non-rollupable measure types.** `custom`, `median`, and bare `number`
measures are not re-aggregatable. A query touching one falls back to the
warehouse even when the measure is listed in a rollup. `count`,
`count_distinct`, `sum`, `average`, `min`, and `max` re-aggregate
correctly.

### Common mistakes

| Mistake | Symptom | Fix |
| ------- | ------- | --- |
| Referencing a dimension that doesn't exist | `oxy build` error | Check `dimensions:` list in the view |
| `time_dimension` set without `granularity` | Schema validation error | Add `granularity: day` (or coarser) |
| Rollup exists but query still hits warehouse | Query uses a dimension not in the rollup, a filtered dimension absent from `dimensions:`, or a `custom`/`median`/`number` measure | Add the missing member to the rollup (or accept the warehouse fallback for non-rollupable measure types); then `oxy build` |
| Background worker never builds the rollup | No `pre_aggregations:` block in `config.yml`, or `refresh_worker.enabled: false` | Add the top-level `pre_aggregations:` block (see "Config: enable the refresh worker") |
| `.airlayer/cache/` missing or stale | No preagg badge in UI | Run `oxy build` |
| `refresh_key.every` too infrequent | Stale results served from cache | Shorten the interval, or rebuild with `oxy build` |
| `refresh_key.sql` query fails at runtime | Warning logged, rebuild skipped silently | Run the SQL manually against the source database to verify it returns one row/cell |
| Rollup has no `refresh_key` and no view-level default | Background worker skips the rollup | Add `refresh_key:` to the rollup or set a view-level default |

## Validation workflow

1. `oxy build` — **mandatory final step.** Compiles the semantic layer;
   catches entity-key references, cross-view join wiring, and SQL
   expression errors that pure YAML parsing cannot. Do **not** consider
   a view or topic edit done until `oxy build` passes.
2. `semantic_query` tool (inside the builder) — runs a real query against a
   topic and surfaces compile-time errors with the generated SQL, which is
   the fastest way to confirm a view+topic pair is wired correctly.

**Do NOT use `oxy validate` on view or topic files.** It is for
`*.workflow.yml`, `*.agent.yml`, and `*.app.yml` files only. Running it on
view/topic files will report misleading results — passing it does not
mean the semantic layer compiles. Use `oxy build` instead.

When refreshing or rebuilding a view, **always build fresh from the
schema source** (`semantics.yml` / `.databases/`). Do not consult git
history to recover previous definitions — old views drift from the live
schema and reintroducing them silently re-introduces bugs.
