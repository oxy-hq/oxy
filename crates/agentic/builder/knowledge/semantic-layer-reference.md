---
source:
  - oxy-hq/skills/skills/oxy-semantic-layer/SKILL.md
  - oxy-hq/skills/skills/oxy-semantic-layer/QUICK-REFERENCE.md
reconciled-at: 303763a60ec824429b427a91a207a5880d73fb80
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

## Validation workflow

1. `oxy validate` — checks YAML shape across the project.
2. `oxy build` — compiles the semantic layer; catches entity-key and
   join errors that pure YAML validation misses.
3. `semantic_query` tool (inside the builder) — runs a real query against a
   topic and surfaces compile-time errors with the generated SQL, which is
   the fastest way to confirm a view+topic pair is wired correctly.
