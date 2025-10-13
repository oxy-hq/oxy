````yaml
# Views

Views represent logical data models that define how to access and interpret data from your underlying data sources. They are the core buildings of the semantic layer, encapsulating the business logic needed to transform raw data into meaningful business concepts. Views define entities, dimensions, and measures that enable consistent, governed access to data across your organization.

## Purpose

Views serve several key purposes:
- **Data Abstraction**: Abstract away the complexity of underlying database schemas and provide business-friendly interfaces
- **Business Logic**: Encode business rules, calculations, and transformations in a centralized location
- **Consistency**: Ensure consistent definitions of business metrics and dimensions across all consuming applications
- **Governance**: Provide a controlled layer for data access, security, and quality validation
- **AI Context**: Give AI agents semantic understanding of your data structure and business concepts
- **Cross-Platform**: Enable portability across different BI tools and query engines

## Configuration

Views are defined in `.view.yaml` files within the `views/` directory:

```yaml
# views/orders.view.yaml
name: orders
description: "Order transactions and related data"
table: "public.orders"

entities:
  - name: order
    type: primary
    description: "Individual order transaction"
    key: order_id

dimensions:
  - name: order_id
    type: string
    description: "Unique identifier for the order"
    expr: order_id

measures:
  - name: count
    type: count
    description: "Total number of orders"
````

## Properties

| Property      | Type   | Required    | Description                                                |
| ------------- | ------ | ----------- | ---------------------------------------------------------- |
| `name`        | string | Yes         | Unique identifier for the view within the semantic layer   |
| `description` | string | Yes         | Human-readable description of what this view represents    |
| `datasource`  | string | Yes         | Name of the datasource to use for this view                |
| `table`       | string | Conditional | Database table reference (required if `sql` not specified) |
| `sql`         | string | Conditional | Custom SQL query (required if `table` not specified)       |
| `entities`    | array  | Yes         | List of entities that define the core objects in this view |
| `dimensions`  | array  | Yes         | List of dimensions (attributes) available in this view     |
| `measures`    | array  | No          | List of measures (aggregations) available in this view     |

## Datasource Configuration

The `datasource` property allows you to specify which datasource to use for this view. This is particularly useful when working with multiple databases or when you need to route queries to specific data sources.

```yaml
name: sales_data
description: "Sales data from the main analytics warehouse"
datasource: "analytics"
table: "public.sales"
```

When `datasource` is specified:

- The semantic layer will route queries for this view to the specified datasource
- Table references in SQL will be resolved within the context of that datasource
- Cross-datasource joins can be orchestrated at the semantic layer level

If no `datasource` is specified, the view will use the default datasource configured for the semantic layer.

## Data Source Configuration

### Table-based Views

The most common type of view references a single table:

```yaml
name: customers
description: "Customer master data"
datasource: "analytics"
table: "public.customers"
```

### SQL-based Views

For more complex data sources, you can use custom SQL:

```yaml
name: customer_metrics
description: "Aggregated customer metrics and segmentation"
datasource: "analytics"
sql: |
  SELECT 
    customer_id,
    COUNT(DISTINCT order_id) as total_orders,
    SUM(order_amount) as lifetime_value,
    MAX(order_date) as last_order_date,
    MIN(order_date) as first_order_date,
    CASE 
      WHEN SUM(order_amount) >= 1000 THEN 'High Value'
      WHEN SUM(order_amount) >= 500 THEN 'Medium Value'
      ELSE 'Low Value'
    END as value_segment
  FROM orders
  GROUP BY customer_id
```

### Cross-database Views

Views can reference tables from different databases:

```yaml
name: unified_orders
description: "Orders from multiple systems unified"
datasource: "ecommerce"
sql: |
  SELECT 
    'shopify' as source_system,
    order_id,
    customer_id,
    order_date,
    order_amount
  FROM shopify.orders

  UNION ALL

  SELECT 
    'amazon' as source_system,
    order_id,
    customer_id,
    order_date,
    order_amount
  FROM amazon.orders
```

## Examples

### E-commerce Order View

```yaml
name: orders
description: "Order transactions and related data"
datasource: "ecommerce"
table: "public.orders"

entities:
  - name: order
    type: primary
    description: "Individual order transaction"
    key: order_id

  - name: customer
    type: foreign
    description: "Customer who placed the order"
    key: customer_id

  - name: product
    type: foreign
    description: "Product referenced in order items"
    key: product_id

dimensions:
  - name: order_id
    type: string
    description: "Unique identifier for the order"
    expr: order_id

  - name: customer_id
    type: string
    description: "Customer identifier"
    expr: customer_id

  - name: status
    type: string
    description: "Order status"
    expr: status
    severity: error

  - name: order_date
    type: date
    description: "Date when the order was placed"
    expr: order_date

  - name: order_amount
    type: number
    description: "Total order amount before discounts"
    expr: order_amount
    format: "$#,##0.00"

  - name: discount_amount
    type: number
    description: "Total discount applied to the order"
    expr: discount_amount
    format: "$#,##0.00"

  - name: net_amount
    type: number
    description: "Order amount after discounts"
    expr: order_amount - COALESCE(discount_amount, 0)
    format: "$#,##0.00"

  - name: is_first_order
    type: boolean
    description: "Whether this is the customer's first order"
    expr: is_first_order

  - name: shipping_method
    type: string
    description: "Method used for shipping"
    expr: shipping_method

measures:
  - name: count
    type: count
    description: "Total number of orders"

  - name: total_revenue
    type: sum
    description: "Total revenue from orders"
    expr: "{{ order_amount }}"
    format: "$#,##0.00"

  - name: total_net_revenue
    type: sum
    description: "Total net revenue after discounts"
    expr: "{{ net_amount }}"
    format: "$#,##0.00"

  - name: average_order_value
    type: average
    description: "Average order value"
    expr: "{{ order_amount }}"
    format: "$#,##0.00"

  - name: unique_customers
    type: count_distinct
    description: "Number of unique customers"
    expr: "{{ customer_id }}"

  - name: large_orders
    type: count
    description: "Number of large orders (>= $1000)"

  - name: cancelled_orders
    type: count
    description: "Number of cancelled orders"
```

### Customer Demographics View

```yaml
name: customers
description: "Customer profile and demographic information"
datasource: "crm"
table: "public.customers"

entities:
  - name: customer
    type: primary
    description: "Individual customer account"
    key: customer_id

dimensions:
  - name: customer_id
    type: string
    description: "Unique customer identifier"
    expr: customer_id

  - name: email
    type: string
    description: "Customer email address"
    expr: email
    hidden: true # PII

  - name: registration_date
    type: date
    description: "Date when customer registered"
    expr: registration_date

  - name: age_group
    type: string
    description: "Customer age group"
    expr: |
      CASE 
        WHEN age < 25 THEN '18-24'
        WHEN age < 35 THEN '25-34'
        WHEN age < 45 THEN '35-44'
        WHEN age < 55 THEN '45-54'
        ELSE '55+'
      END

  - name: country
    type: string
    description: "Customer country"
    expr: country

  - name: acquisition_channel
    type: string
    description: "How the customer was acquired"
    expr: acquisition_channel

measures:
  - name: customer_count
    type: count
    description: "Total number of customers"

  - name: new_customers_last_30_days
    type: count
    description: "Customers registered in the last 30 days"
```

### Financial Transactions View

```yaml
name: transactions
description: "Financial transaction records with enhanced attributes"
datasource: "financial"
sql: |
  SELECT 
    t.*,
    a.account_type,
    a.account_status,
    m.merchant_category,
    m.merchant_name,
    CASE 
      WHEN t.amount >= 10000 THEN 'Large'
      WHEN t.amount >= 1000 THEN 'Medium'
      ELSE 'Small'
    END as transaction_size
  FROM transactions t
  LEFT JOIN accounts a ON t.account_id = a.account_id
  LEFT JOIN merchants m ON t.merchant_id = m.merchant_id

entities:
  - name: transaction
    type: primary
    description: "Individual financial transaction"
    key: transaction_id

  - name: account
    type: foreign
    description: "Account involved in the transaction"
    key: account_id

  - name: merchant
    type: foreign
    description: "Merchant where transaction occurred"
    key: merchant_id

dimensions:
  - name: transaction_id
    type: string
    description: "Unique transaction identifier"
    expr: transaction_id

  - name: account_id
    type: string
    description: "Account identifier"
    expr: account_id

  - name: transaction_date
    type: date
    description: "Date of the transaction"
    expr: transaction_date

  - name: amount
    type: number
    description: "Transaction amount"
    expr: amount
    format: "$#,##0.00"

  - name: transaction_type
    type: string
    description: "Type of transaction"
    expr: transaction_type

  - name: merchant_category
    type: string
    description: "Merchant category code"
    expr: merchant_category

  - name: is_fraudulent
    type: boolean
    description: "Whether transaction was flagged as fraudulent"
    expr: is_fraudulent

  - name: transaction_size
    type: string
    description: "Size category of the transaction"
    expr: transaction_size

measures:
  - name: transaction_count
    type: count
    description: "Total number of transactions"

  - name: transaction_volume
    type: sum
    description: "Total transaction volume"
    expr: "{{ amount }}"
    format: "$#,##0.00"

  - name: average_transaction_size
    type: average
    description: "Average transaction amount"
    expr: "{{ amount }}"
    format: "$#,##0.00"

  - name: fraudulent_transactions
    type: count
    description: "Number of fraudulent transactions"
```

## Advanced Features

### Dynamic References

Reference columns directly without table prefixes:

```yaml
dimensions:
  - name: full_name
    type: string
    description: "Customer full name"
    expr: CONCAT(first_name, ' ', last_name)
```

### Template References

Reference dimensions and measures from the same view or other views using template syntax:

#### Same-View References

Reference dimensions from the same view using `{{dimension_name}}`:

```yaml
dimensions:
  - name: order_amount
    type: number
    expr: order_amount

  - name: order_category
    type: string
    expr: |
      CASE
        WHEN {{order_amount}} >= 1000 THEN 'Large'
        WHEN {{order_amount}} >= 100 THEN 'Medium'
        ELSE 'Small'
      END
```

#### Cross-Entity References

Reference dimensions and measures from other entities using `{{entity_name.field_name}}`:

```yaml
measures:
  - name: total_revenue
    type: sum
    description: "Total revenue from orders"
    expr: "{{order.order_amount}}"

  - name: customer_lifetime_value
    type: custom
    description: "Average order value per customer"
    expr: "{{order.total_revenue}} / {{customer.customer_count}}"

dimensions:
  - name: customer_segment
    type: string
    description: "Customer segment based on order history"
    expr: |
      CASE
        WHEN {{order.total_orders}} >= 10 THEN 'VIP'
        WHEN {{order.total_orders}} >= 5 THEN 'Regular'
        ELSE 'New'
      END
```

### Computed Columns

Create derived columns using SQL expressions:

```yaml
dimensions:
  - name: days_since_registration
    type: number
    description: "Number of days since customer registered"
    expr: DATE_DIFF(CURRENT_DATE(), registration_date, DAY)

  - name: customer_lifetime_months
    type: number
    description: "Customer lifetime in months"
    expr: DATE_DIFF(CURRENT_DATE(), registration_date, MONTH)
```

### Conditional Logic

Use CASE statements for complex business logic:

```yaml
dimensions:
  - name: customer_segment
    type: string
    description: "Customer segment based on order history and value"
    expr: |
      CASE 
        WHEN total_orders >= 10 AND lifetime_value >= 1000 THEN 'VIP'
        WHEN total_orders >= 5 OR lifetime_value >= 500 THEN 'Regular'
        WHEN total_orders >= 1 THEN 'New'
        ELSE 'Prospect'
      END
```

## Performance Optimization

### Indexing Hints

Provide hints for optimal performance:

```yaml
dimensions:
  - name: customer_id
    type: string
    description: "Customer identifier"
    expr: customer_id

  - name: order_date
    type: date
    description: "Order date"
    expr: order_date
    # This dimension is commonly used for filtering
```

## Data Quality and Validation

### Value Constraints

Define severity levels for data quality:

```yaml
dimensions:
  - name: status
    type: string
    description: "Order status"
    expr: status
    severity: error # Will fail queries with invalid values
```

## Best Practices

1. **Business-Centric Design**: Design views around business concepts, not technical table structures

2. **Consistent Naming**: Use clear, consistent naming conventions across all views

3. **Complete Documentation**: Provide comprehensive descriptions for all entities, dimensions, and measures

4. **Performance Awareness**: Consider query performance when designing complex calculated dimensions

5. **Validation Rules**: Use severity levels to ensure data quality

6. **Hidden Fields**: Mark PII and technical fields as hidden when appropriate

7. **Entity Relationships**: Clearly define entities to enable automatic joins

8. **Incremental Development**:Start with simple views and incrementally add complexity

## Validation Rules

- View names must be unique within the semantic layer
- Either `table` or `sql` must be specified, but not both
- View names should follow naming conventions (lowercase, underscore-separated)
- Each view must have at least one entity
- Each view must have at least one dimension
- Primary entity type is required and should be unique per view
- SQL expressions must be valid for the target database

## Integration with Topics

Views are organized into topics for better discoverability:

```yaml
# topics/sales.topic.yaml
name: sales
description: "Sales data model including orders, customers, and products"
views:
  - orders
  - customers
  - products
```

This enables logical grouping and helps AI agents understand the business context of related views.
