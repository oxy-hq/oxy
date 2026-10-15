# Casting a date column per warehouse

Use when a dimension whose meaning is a date or timestamp sits on a column that
is **not** stored as `DATE` / `DATETIME` / `TIMESTAMP`. Declare the dimension
`type: date` (or `type: datetime`) and wrap its `expr` with a cast, so filters
compile against a real date instead of failing with `TYPE_MISMATCH` on the
first query that filters on it.

**Sample first.** Stored formats differ between warehouses and between tables in
one warehouse — ISO strings (`"2024-01-31"`), compact strings (`"20240131"`),
integers (`20240131`), epoch seconds or milliseconds. One
`SELECT <col> FROM <table> LIMIT 1` tells you which you have; guessing is cheap
to get wrong and the failure only shows at query time.

| Warehouse  | Cast functions |
| ---------- | -------------- |
| ClickHouse | `toDate(<col>)`, `toDateTime(<col>)`, `parseDateTimeBestEffort(<col>)`, `toDate(toString(<col>))` for integer-encoded dates |
| BigQuery   | `CAST(<col> AS DATE)`, `PARSE_DATE('<format>', <col>)`, `PARSE_TIMESTAMP('<format>', <col>)`, `TIMESTAMP_SECONDS(<col>)` |
| Snowflake  | `TO_DATE(<col>[, '<format>'])`, `TO_TIMESTAMP(<col>)`, `TRY_TO_DATE(<col>)` |
| Postgres   | `(<col>)::date`, `(<col>)::timestamp`, `to_date(<col>, '<format>')`, `to_timestamp(<col>)` |
| DuckDB / Airhouse | `CAST(<col> AS DATE)`, `strptime(<col>, '<format>')`, `to_timestamp(<col>)`, `epoch_ms(<col>)` |

A column already typed as a date or timestamp needs no cast — `expr: <col>` is
enough.

## Examples

```yaml
# ClickHouse: business_date stored as Int32 like 20240131
- name: business_date
  type: date
  expr: "toDate(toString(business_date))"

# BigQuery: order_date stored as STRING like "2024-01-31"
- name: order_date
  type: date
  expr: "PARSE_DATE('%Y-%m-%d', order_date)"

# Snowflake: created_at already a TIMESTAMP — no cast
- name: created_at
  type: datetime
  expr: created_at
```

## Derived parts re-derive from the raw column

A derived dimension (`business_month`, `day_of_week`, …) repeats the cast
inside its own `expr` rather than referencing the base date dimension:

```yaml
- name: business_quarter
  type: number
  expr: "toQuarter(toDate(toString(business_date)))"
```

If the same cast chain appears in many dimensions, the column type is the real
problem — fix it once in the pipeline that loads the table, then drop the casts.
