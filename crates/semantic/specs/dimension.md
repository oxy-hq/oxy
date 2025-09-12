# Dimensions

Dimensions are attributes that can be used for grouping, filtering, and segmenting data in your semantic layer. They represent the descriptive characteristics of your entities and provide the context for analyzing measures. Dimensions enable users to slice and dice data across different attributes and create meaningful analytics.

## Purpose

Dimensions serve several key purposes:

- **Grouping**: Enable grouping of data by categorical or descriptive attributes
- **Filtering**: Provide filter options for queries and dashboards
- **Segmentation**: Allow analysis across different segments and categories
- **Context**: Add descriptive context to measures and metrics
- **Data Quality**: Define validation rules and constraints for data values

## Configuration

Dimensions are defined within view files under the `dimensions` section:

```yaml
dimensions:
  - name: order_id
    type: string
    description: "Unique identifier for the order"
    expr: order_id
    samples: ["ORD-2024-001234", "ORD-2024-001235", "ORD-2024-001236"]
    synonyms: ["order_number", "order_ref"]

  - name: status
    type: string
    description: "Order status"
    expr: status
    samples: ["completed", "pending", "shipped"]
    synonyms: ["order_status", "state"]
```

## Properties

| Property      | Type     | Required | Description                                                   |
| ------------- | -------- | -------- | ------------------------------------------------------------- |
| `name`        | string   | Yes      | Unique identifier for the dimension within the view           |
| `type`        | string   | Yes      | Data type: `string`, `number`, `date`, `datetime`, `boolean`  |
| `description` | string   | No       | Human-readable description of what this dimension represents  |
| `expr`        | string   | Yes      | SQL expression that defines how to calculate this dimension   |
| `samples`     | string[] | No       | Example values to help users understand the dimension content |
| `synonyms`    | string[] | No       | Alternative names or terms that refer to this dimension       |

## Dimension Types

### String

Text-based dimensions for categorical data, names, IDs, and other string values.

```yaml
- name: customer_name
  type: string
  description: "Full name of the customer"
  expr: customer_name
  samples: ["John Smith", "Jane Doe", "Michael Johnson"]
  synonyms: ["name", "full_name", "customer_full_name"]
```

### Number

Numeric dimensions for quantities, IDs, scores, and other numeric values.

```yaml
- name: quantity
  type: number
  description: "Quantity of items ordered"
  expr: quantity
  samples: [5, 2, 10]
  synonyms: ["qty", "amount", "count"]
```

### Date

Date-only dimensions for analyzing trends over time.

```yaml
- name: order_date
  type: date
  description: "Date when the order was placed"
  expr: order_date
  samples: ["2024-03-15", "2024-03-16", "2024-03-17"]
  synonyms: ["date", "order_day", "purchase_date"]
```

### Datetime

Date and time dimensions for precise temporal analysis.

```yaml
- name: created_at
  type: datetime
  description: "Timestamp when the record was created"
  expr: created_at
```

### Boolean

True/false dimensions for binary attributes.

```yaml
- name: is_first_order
  type: boolean
  description: "Whether this is the customer's first order"
  expr: is_first_order
```

## Optional Fields

### Sample Values

The `samples` field provides example values that help users understand what kind of data the dimension contains. This is particularly useful for:

- Onboarding new users to understand data formats
- Documentation and training materials
- Auto-generating example queries
- Data catalog and discovery tools

```yaml
- name: product_category
  type: string
  description: "Product category classification"
  expr: category
  samples: ["Electronics", "Clothing", "Home & Garden"]
```

### Synonyms

The `synonyms` field contains an array of alternative names that users might use to refer to this dimension. This enables:

- Natural language query processing
- Search and discovery functionality
- Mapping from different naming conventions
- Supporting multiple user vocabularies

```yaml
- name: revenue
  type: number
  description: "Total revenue amount"
  expr: revenue
  synonyms: ["sales", "income", "total_sales", "gross_revenue"]
```

## Examples

### E-commerce Order Dimensions

```yaml
dimensions:
  - name: order_id
    type: string
    description: "Unique identifier for the order"
    expr: order_id
    samples: ["ORD-2024-001234", "ORD-2024-001235", "ORD-2024-001236"]
    synonyms: ["order_number", "order_ref"]

  - name: customer_id
    type: string
    description: "Customer identifier"
    expr: customer_id
    samples: ["CUST-789456", "CUST-789457", "CUST-789458"]
    synonyms: ["customer_number", "cust_id"]

  - name: status
    type: string
    description: "Current order status"
    expr: status
    samples: ["shipped", "pending", "delivered"]
    synonyms: ["order_status", "state"]

  - name: order_date
    type: date
    description: "Date when the order was placed"
    expr: order_date
    samples: ["2024-03-15", "2024-03-16", "2024-03-17"]
    synonyms: ["date", "purchase_date"]

  - name: order_amount
    type: number
    description: "Total order amount before discounts"
    expr: order_amount
    samples: [129.99, 89.50, 234.75]
    synonyms: ["total", "amount", "order_total"]

  - name: is_first_order
    type: boolean
    description: "Whether this is the customer's first order"
    expr: is_first_order
    samples: [true, false, true]
    synonyms: ["first_order", "is_new_customer"]

  - name: shipping_method
    type: string
    description: "Method used for shipping"
    expr: shipping_method
    samples: ["standard", "express", "overnight"]
    synonyms: ["delivery_method", "shipping_type"]
```

### Customer Dimensions

```yaml
dimensions:
  - name: customer_id
    type: string
    description: "Unique customer identifier"
    expr: customer_id

  - name: email
    type: string
    description: "Customer email address"
    expr: email

  - name: age_group
    type: string
    description: "Customer age group"
    expr: CASE
      WHEN age < 25 THEN '18-24'
      WHEN age < 35 THEN '25-34'
      WHEN age < 45 THEN '35-44'
      WHEN age < 55 THEN '45-54'
      ELSE '55+'
    END

  - name: lifetime_value_tier
    type: string
    description: "Customer lifetime value tier"
    expr: CASE
      WHEN lifetime_value < 100 THEN 'Bronze'
      WHEN lifetime_value < 500 THEN 'Silver'
      WHEN lifetime_value < 1000 THEN 'Gold'
      ELSE 'Platinum'
    END
```

### Time-based Dimensions

```yaml
dimensions:
  - name: created_at
    type: datetime
    description: "When the record was created"
    expr: created_at

  - name: created_date
    type: date
    description: "Date portion of creation timestamp"
    expr: DATE(created_at)

  - name: created_year
    type: number
    description: "Year when record was created"
    expr: EXTRACT(YEAR FROM created_at)

  - name: created_month
    type: number
    description: "Month when record was created"
    expr: EXTRACT(MONTH FROM created_at)

  - name: created_quarter
    type: string
    description: "Quarter when record was created"
    expr: 'Q' || EXTRACT(QUARTER FROM created_at)

  - name: day_of_week
    type: string
    description: "Day of week when record was created"
    expr: TO_CHAR(created_at, 'Day')
```

## Advanced Features

### Calculated Dimensions

Dimensions can include complex SQL expressions for derived values:

```yaml
- name: customer_segment
  type: string
  description: "Customer segment based on order history"
  expr: |
    CASE 
      WHEN total_orders >= 10 AND avg_order_value >= 100 THEN 'VIP'
      WHEN total_orders >= 5 THEN 'Regular'
      WHEN total_orders >= 1 THEN 'New'
      ELSE 'Prospect'
    END
```

### Reference Other Dimensions

Dimensions can reference other dimensions in the same view:

```yaml
- name: order_amount
  type: number
  description: "Total order amount"
  expr: order_amount

- name: order_size_category
  type: string
  description: "Category based on order amount"
  expr: |
    CASE
      WHEN {{order_amount}} >= 1000 THEN 'Large'
      WHEN {{order_amount}} >= 100 THEN 'Medium'
      ELSE 'Small'
    END
```
