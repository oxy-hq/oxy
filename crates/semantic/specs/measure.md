# Measures

Measures are aggregations or calculations that provide quantitative insights from your data. They represent the "what you want to measure" in your analytics and enable users to analyze business performance through metrics like revenue, counts, averages, and custom calculations. Measures are the foundation of business intelligence and data-driven decision making.

## Purpose

Measures serve several key purposes:

- **Quantitative Analysis**: Provide numerical insights and KPIs for business analysis
- **Aggregation**: Calculate summaries across different dimensions and time periods
- **Business Metrics**: Define standardized business metrics used across the organization
- **Trend Analysis**: Enable tracking of performance over time
- **Comparative Analysis**: Allow comparison across different segments and cohorts

## Configuration

Measures are defined within view files under the `measures` section:

```yaml
measures:
  - name: total_revenue
    type: sum
    description: "Total revenue from orders"
    expr: "{{ order_amount }}"
    samples: [1234567.89, 987654.32, 2345678.90]
    synonyms: ["revenue", "sales", "income", "total_sales"]

  - name: average_order_value
    type: average
    description: "Average order value"
    expr: "{{ order_amount }}"
    samples: [156.78, 89.99, 234.56]
    synonyms: ["aov", "avg_order_value", "mean_order_amount"]
```

## Properties

| Property      | Type   | Required    | Description                                                                                 |
| ------------- | ------ | ----------- | ------------------------------------------------------------------------------------------- |
| `name`        | string | Yes         | Unique identifier for the measure within the view                                           |
| `type`        | string | Yes         | Measure type: `count`, `sum`, `average`, `min`, `max`, `count_distinct`, `median`, `custom` |
| `description` | string | No          | Human-readable description of what this measure represents                                  |
| `expr`        | string | Conditional | SQL expression for the measure (required for all types except `count`)                      |
| `filters`     | array  | No          | List of filters to apply to the measure calculation                                         |
| `samples`     | array  | No          | Sample values or example outputs to help users understand the measure                       |
| `synonyms`    | array  | No          | Alternative names or terms that refer to this measure                                       |

## Measure Types

### Count

Counts the number of records in the dataset.

```yaml
- name: total_orders
  type: count
  description: "Total number of orders"
```

### Sum

Calculates the sum of a numeric field.

```yaml
- name: total_revenue
  type: sum
  description: "Total revenue from all orders"
  expr: "{{ order_amount }}"
```

### Average

Calculates the arithmetic mean of a numeric field.

```yaml
- name: average_order_value
  type: average
  description: "Average order value"
  expr: "{{ order_amount }}"
```

### Min/Max

Finds the minimum or maximum value of a field.

```yaml
- name: min_order_amount
  type: min
  description: "Smallest order amount"
  expr: "{{ order_amount }}"

- name: max_order_amount
  type: max
  description: "Largest order amount"
  expr: "{{ order_amount }}"
```

### Count Distinct

Counts the number of unique values in a field.

```yaml
- name: unique_customers
  type: count_distinct
  description: "Number of unique customers"
  expr: "{{ customer_id }}"
```

### Median

Calculates the median (50th percentile) value.

```yaml
- name: median_order_value
  type: median
  description: "Median order value"
  expr: "{{ order_amount }}"
```

### Custom

Allows for complex custom SQL expressions.

```yaml
- name: weighted_average_rating
  type: custom
  description: "Rating weighted by number of reviews"
  expr: |
    SUM(rating * review_count) / 
    NULLIF(SUM(review_count), 0)
```

## Examples

### E-commerce Order Measures

```yaml
measures:
  # Basic aggregations
  - name: count
    type: count
    description: "Total number of orders"
    samples: [15234, 18567, 12890]
    synonyms: ["order_count", "total_orders", "number_of_orders"]

  - name: total_revenue
    type: sum
    description: "Total revenue from orders"
    expr: "{{ order_amount }}"
    samples: [2456789.12, 3123456.78, 1987654.32]
    synonyms: ["revenue", "sales", "gross_revenue", "total_sales"]

  - name: total_net_revenue
    type: sum
    description: "Total net revenue after discounts"
    expr: "{{ net_amount }}"
    samples: [2123456.78, 2789012.34, 1654321.09]
    synonyms: ["net_revenue", "net_sales", "revenue_after_discounts"]

  # Statistical measures
  - name: average_order_value
    type: average
    description: "Average order value"
    expr: "{{ order_amount }}"
    samples: [156.78, 189.45, 134.22]
    synonyms: ["aov", "avg_order_value", "mean_order_amount"]

  - name: median_order_value
    type: median
    description: "Median order value"
    expr: "{{ order_amount }}"
    samples: [89.99, 95.50, 78.25]
    synonyms: ["median_order_amount", "middle_order_value"]

  # Distinct counts
  - name: unique_customers
    type: count_distinct
    description: "Number of unique customers"
    expr: "{{ customer_id }}"
    samples: [8547, 9234, 7891]
    synonyms: ["customer_count", "distinct_customers", "unique_buyers"]

  - name: unique_products
    type: count_distinct
    description: "Number of unique products ordered"
    expr: "{{ product_id }}"
    samples: [1234, 1456, 1098]
    synonyms: ["product_count", "distinct_products", "unique_items"]

  # Filtered measures
  - name: large_orders
    type: count
    description: "Number of large orders (>= $1000)"
    filters:
      - expr: "{{ order_amount }} >= 1000"

  - name: small_orders
    type: count
    description: "Number of small orders (< $50)"
    filters:
      - expr: "{{ order_amount }} < 50"

  - name: cancelled_orders
    type: count
    description: "Number of cancelled orders"
    filters:
      - expr: "{{ status }} = 'cancelled'"

  # Time-based measures
  - name: orders_last_30_days
    type: count
    description: "Orders placed in the last 30 days"
    filters:
      - expr: "{{ order_date }} >= DATE_SUB(CURRENT_DATE(), INTERVAL 30 DAY)"

  # Customer behavior measures
  - name: first_time_customer_orders
    type: count
    description: "Orders from first-time customers"
    filters:
      - expr: "{{ is_first_order }} = true"

  - name: repeat_customer_orders
    type: count
    description: "Orders from repeat customers"
    filters:
      - expr: "{{ is_first_order }} = false"
```

### Customer Measures

```yaml
measures:
  - name: customer_count
    type: count
    description: "Total number of customers"

  - name: total_customer_lifetime_value
    type: sum
    description: "Sum of all customer lifetime values"
    expr: "{{ lifetime_value }}"

  - name: average_customer_lifetime_value
    type: average
    description: "Average customer lifetime value"
    expr: "{{ lifetime_value }}"

  - name: active_customers
    type: count
    description: "Number of active customers"
    filters:
      - expr: "{{ status }} = 'active'"

  - name: high_value_customers
    type: count
    description: "Customers with lifetime value over $1000"
    filters:
      - expr: "{{ lifetime_value }} >= 1000"

  - name: customer_acquisition_cost
    type: custom
    description: "Average cost to acquire a customer"
    expr: |
      CASE 
        WHEN COUNT(DISTINCT customer_id) > 0 
        THEN SUM(marketing_spend) / COUNT(DISTINCT customer_id)
        ELSE 0 
      END
```

### Financial Measures

```yaml
measures:
  - name: total_transactions
    type: count
    description: "Total number of transactions"

  - name: transaction_volume
    type: sum
    description: "Total transaction volume"
    expr: "{{ amount }}"

  - name: average_transaction_size
    type: average
    description: "Average transaction amount"
    expr: "{{ amount }}"

  - name: fraud_rate
    type: custom
    description: "Percentage of transactions flagged as fraud"
    expr: |
      COUNT(CASE WHEN is_fraud = true THEN 1 END) * 100.0 / 
      NULLIF(COUNT(*), 0)

  - name: fraudulent_transactions
    type: count
    description: "Number of fraudulent transactions"
    filters:
      - expr: "{{ is_fraud }} = true"
```

## Advanced Features

### Filtered Measures

Measures can include filters to calculate specific subsets:

```yaml
- name: premium_customer_revenue
  type: sum
  description: "Revenue from premium customers only"
  expr: "{{ order_amount }}"
  filters:
    - expr: "{{ customer_tier }} = 'premium'"
    - expr: "{{ order_date }} >= '2024-01-01'"
```

### Complex Custom Measures

Use custom SQL for sophisticated calculations:

```yaml
- name: customer_concentration_index
  type: custom
  description: "Herfindahl index measuring customer concentration"
  expr: |
    SUM(
      POWER(
        customer_revenue / SUM(customer_revenue) OVER(), 
        2
      )
    )

- name: cohort_retention_rate
  type: custom
  description: "3-month retention rate for customer cohorts"
  expr: |
    COUNT(DISTINCT CASE 
      WHEN DATEDIFF(last_order_date, first_order_date) >= 90 
      THEN customer_id 
    END) / NULLIF(COUNT(DISTINCT customer_id), 0)
```

### Time Window Measures

Calculate measures over specific time periods:

```yaml
- name: trailing_12_month_revenue
  type: sum
  description: "Revenue over the last 12 months"
  expr: "{{ order_amount }}"
  filters:
    - expr: "{{ order_date }} >= DATE_SUB(CURRENT_DATE(), INTERVAL 12 MONTH)"

- name: year_over_year_growth
  type: custom
  description: "Year-over-year revenue growth rate"
  expr: |
    (SUM(CASE WHEN order_date >= DATE_SUB(CURRENT_DATE(), INTERVAL 12 MONTH) 
              THEN order_amount ELSE 0 END) -
     SUM(CASE WHEN order_date >= DATE_SUB(CURRENT_DATE(), INTERVAL 24 MONTH) 
              AND order_date < DATE_SUB(CURRENT_DATE(), INTERVAL 12 MONTH)
              THEN order_amount ELSE 0 END)) /
    NULLIF(SUM(CASE WHEN order_date >= DATE_SUB(CURRENT_DATE(), INTERVAL 24 MONTH) 
                    AND order_date < DATE_SUB(CURRENT_DATE(), INTERVAL 12 MONTH)
                    THEN order_amount ELSE 0 END), 0)
```

## Filter Expressions

Filters use SQL-like expressions with field references in `{{ }}` syntax:

| Operator      | Description           | Example                                                 |
| ------------- | --------------------- | ------------------------------------------------------- |
| `=`           | Equals                | `expr: "{{ status }} = 'active'"`                       |
| `!=`          | Not equals            | `expr: "{{ status }} != 'cancelled'"`                   |
| `>`           | Greater than          | `expr: "{{ amount }} > 100"`                            |
| `>=`          | Greater than or equal | `expr: "{{ amount }} >= 100"`                           |
| `<`           | Less than             | `expr: "{{ amount }} < 1000"`                           |
| `<=`          | Less than or equal    | `expr: "{{ amount }} <= 1000"`                          |
| `IN`          | In list               | `expr: "{{ status }} IN ('active', 'pending')"`         |
| `NOT IN`      | Not in list           | `expr: "{{ status }} NOT IN ('cancelled', 'refunded')"` |
| `LIKE`        | Pattern matching      | `expr: "{{ name }} LIKE '%premium%'"`                   |
| `IS NULL`     | Is null               | `expr: "{{ field }} IS NULL"`                           |
| `IS NOT NULL` | Is not null           | `expr: "{{ field }} IS NOT NULL"`                       |
