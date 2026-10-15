# Data App examples

Requests turned into a plan and a finished `.app.yml`. The SQL is
Postgres / DuckDB / Snowflake dialect; see the dialect cheatsheet in
[quick-reference.md](quick-reference.md#sql-dialect-cheatsheet) for BigQuery and
ClickHouse. `warehouse` stands for whatever database name `config.yml` uses.

## 1. KPIs and a monthly trend

**Request:** "Create a sales dashboard with total revenue, number of orders,
and a monthly revenue trend, from the orders table."

**Questions worth asking first:** what period should it cover? Any breakdown
(region, category)? Which column is the order date?

**Plan:**

```text
App: sales_dashboard.app.yml — sales KPIs and the monthly revenue trend
Data: warehouse.orders
Tasks:
  kpis:          headline numbers  -> total_revenue, order_count, avg_order_value
  monthly_trend: revenue per month -> month, revenue
Displays:
  markdown:   title and one-line intro
  table:      kpis (currency formats)
  line_chart: monthly_trend, x=month, y=revenue
```

**`sales_dashboard.app.yml`:**

```yaml
title: Sales Dashboard
description: Sales KPIs and the monthly revenue trend.
published: true

tasks:
  - name: kpis
    type: execute_sql
    database: warehouse
    sql_query: |
      SELECT
        ROUND(SUM(amount), 2) AS total_revenue,
        COUNT(*)              AS order_count,
        ROUND(AVG(amount), 2) AS avg_order_value
      FROM orders

  - name: monthly_trend
    type: execute_sql
    database: warehouse
    sql_query: |
      SELECT
        DATE_TRUNC('month', order_date) AS month,
        ROUND(SUM(amount), 2)           AS revenue
      FROM orders
      GROUP BY 1
      ORDER BY 1

display:
  - type: markdown
    content: |
      # Sales Dashboard

      Overview of sales performance.

  - type: table
    title: Key metrics
    data: kpis
    formats:
      total_revenue: currency
      avg_order_value: currency
      order_count: number

  - type: line_chart
    title: Monthly revenue
    data: monthly_trend
    x: month
    y: revenue
    x_axis_label: Month
    y_axis_label: Revenue
    y_format: currency
```

## 2. Segments: share, detail, and trend

**Request:** "Show revenue by customer segment as a pie chart, a breakdown
table, and how each segment trends over time."

**Plan:**

```text
App: customer_segments.app.yml — revenue distribution and trend by segment
Data: warehouse.customers joined to warehouse.orders
Tasks:
  segment_summary: revenue by segment   -> segment, revenue, customer_count, revenue_per_customer
  segment_trends:  monthly, per segment -> month, segment, revenue
Displays:
  row: pie_chart (segment_summary) + table (segment_summary)
  line_chart: segment_trends, series=segment
```

**`customer_segments.app.yml`:**

```yaml
title: Customer Segments
description: Revenue distribution and trends across customer segments.
published: true

tasks:
  - name: segment_summary
    type: execute_sql
    database: warehouse
    sql_query: |
      SELECT
        c.segment,
        ROUND(SUM(o.amount), 2)                                      AS revenue,
        COUNT(DISTINCT c.customer_id)                                AS customer_count,
        ROUND(SUM(o.amount) / COUNT(DISTINCT c.customer_id), 2)      AS revenue_per_customer
      FROM customers c
      JOIN orders o ON o.customer_id = c.customer_id
      GROUP BY c.segment
      ORDER BY revenue DESC

  - name: segment_trends
    type: execute_sql
    database: warehouse
    sql_query: |
      SELECT
        DATE_TRUNC('month', o.order_date) AS month,
        c.segment,
        ROUND(SUM(o.amount), 2)           AS revenue
      FROM customers c
      JOIN orders o ON o.customer_id = c.customer_id
      GROUP BY 1, 2
      ORDER BY 1, 2

display:
  - type: markdown
    content: |
      # Customer Segments

  - type: row
    columns: 2
    children:
      - type: pie_chart
        title: Revenue share
        data: segment_summary
        name: segment            # a readable label, never customer_id
        value: revenue
        value_format: currency
      - type: table
        title: Segment summary
        data: segment_summary
        formats:
          revenue: currency
          revenue_per_customer: currency

  - type: line_chart
    title: Monthly revenue by segment
    data: segment_trends
    x: month
    y: revenue
    series: segment              # one line per segment
    y_format: currency
```

## 3. From the semantic model

**Request:** "Build a revenue report from our sales topic — monthly revenue
and revenue by store."

When the workspace has a semantic model, prefer `semantic_query`: the
measures are defined once and the app cannot drift from them. Output columns
are `view__field`, plus `__<granularity>` on a bucketed time dimension.

**`revenue_report.app.yml`:**

```yaml
title: Revenue Report
published: true

tasks:
  - name: revenue_by_month
    type: semantic_query
    topic: sales
    measures: [orders.total_revenue]
    time_dimensions:
      - dimension: orders.order_date
        granularity: month
    filters:
      - field: orders.order_date
        op: gte
        value: "2024-01-01"
    orders:
      - field: orders.order_date
        direction: asc

  - name: revenue_by_store
    type: semantic_query
    topic: sales
    measures: [orders.total_revenue]
    dimensions: [stores.store_name]   # the readable name via a joined view, not orders.store_id
    orders:
      - field: orders.total_revenue
        direction: desc
    limit: 20

display:
  - type: line_chart
    title: Revenue by month
    data: revenue_by_month
    x: orders__order_date__month       # granularity suffix is required
    y: orders__total_revenue
    y_format: currency

  - type: bar_chart
    title: Top 20 stores
    data: revenue_by_store
    x: stores__store_name
    y: orders__total_revenue
    y_format: currency
```

If a chart should follow a control, the semantic query cannot do it —
controls are not rendered into `semantic_query` fields. Use an `execute_sql`
task for that chart.

## 4. A minimal edit

**Request:** "Add a bar chart of revenue by product category to
`sales_report.app.yml`."

1. Read the file and say what it does now ("monthly trend and KPIs from
   `orders`").
2. Propose the smallest change — one task, one display — and keep the file's
   existing naming and database.
3. Apply it, then validate and open the app.

```yaml
# tasks: — add
  - name: revenue_by_category
    type: execute_sql
    database: warehouse
    sql_query: |
      SELECT product_category, ROUND(SUM(amount), 2) AS revenue
      FROM orders
      GROUP BY product_category
      ORDER BY revenue DESC

# display: — add after the trend chart
  - type: bar_chart
    title: Revenue by product category
    data: revenue_by_category
    x: product_category
    y: revenue
    y_format: currency
```

## Controls

The full interactive pattern — `select` filled from a task, `date`, `toggle`,
SQL templating and a row layout — is
[templates/dashboard-with-controls.app.yml](../templates/dashboard-with-controls.app.yml).
