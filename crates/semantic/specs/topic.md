````yaml
# Topics

Topics represent collections of related views, entities, and metrics that form logical business domains within your semantic layer. They provide a high-level organizational structure that helps users discover and explore related data concepts together.

## Purpose

Topics serve several key purposes:
- **Logical Organization**: Group related views into meaningful business domains
- **Data Discovery**: Help users find relevant data by business area
- **AI Context**: Provide semantic context to AI agents for better query understanding
- **Documentation**: Create clear boundaries and documentation for different data domains

## Configuration

Topics are defined in `.topic.yaml` files within the `topics/` directory:

```yaml
# topics/sales.topic.yaml
name: sales
description: "Sales data model including orders, customers, and products"

# The base view from which all other views should be joined
base_view: orders

# Include views in this topic
views:
  - orders
  - customers
  - products

# Optional: Default filters applied to all queries in this topic
default_filters:
  - field: "status"
    op: "neq"
    value: "cancelled"
````

## Properties

| Property          | Type   | Required | Description                                                                       |
| ----------------- | ------ | -------- | --------------------------------------------------------------------------------- |
| `name`            | string | Yes      | Unique identifier for the topic                                                   |
| `description`     | string | Yes      | Human-readable description of the business domain                                 |
| `base_view`       | string | No       | The primary view that serves as the starting point for all queries in this topic. |
| `views`           | array  | Yes      | List of view names included in this topic                                         |
| `default_filters` | array  | No       | List of structured filters automatically applied to all queries in this topic     |

## Base View Behavior

When a `base_view` is specified for a topic, it enforces a strict query construction pattern:

- **All queries must start from the base view**: The base view serves as the root of the join tree for every query in the topic
- **Consistent join paths**: Other views are always joined to the base view (directly or transitively).

**Example**: In a `sales` topic with `base_view: orders`, a query for "customer lifetime value" would:

1. Start from the `orders` view
2. Join to the `customers` view via the relationship defined in the views
3. Aggregate order data grouped by customer

The query **cannot** start directly from `customers` and join to `orders` - it must always originate from `orders`.

## Examples

### E-commerce Topics

```yaml
# topics/sales.topic.yaml
name: sales
description: "Sales performance and order management data"
base_view: orders
views:
  - orders
  - order_items
  - customers
  - products
default_filters:
  - field: "status"
    op: "not_in"
    value: ["cancelled", "refunded"]
  - field: "is_test"
    op: "eq"
    value: false

# topics/marketing.topic.yaml
name: marketing
description: "Marketing campaigns, attribution, and customer acquisition"
base_view: campaigns
views:
  - campaigns
  - attribution
  - customer_acquisition
  - website_sessions
default_filters:
  - field: "campaign_status"
    op: "eq"
    value: "active"

# topics/finance.topic.yaml
name: finance
description: "Financial reporting, revenue recognition, and accounting"
base_view: financial_transactions
views:
  - revenue_recognition
  - financial_transactions
  - cost_centers
  - budgets
default_filters:
  - field: "is_reconciled"
    op: "eq"
    value: true
```

### SaaS Business Topics

```yaml
# topics/product_usage.topic.yaml
name: product_usage
description: "Product engagement, feature usage, and user behavior analytics"
base_view: user_sessions
views:
  - user_sessions
  - feature_usage
  - user_journeys
  - product_events
default_filters:
  - field: "session_duration_seconds"
    op: "gt"
    value: 10

# topics/customer_success.topic.yaml
name: customer_success
description: "Customer health, retention, and support metrics"
base_view: customer_health
views:
  - customer_health
  - support_tickets
  - churn_analysis
  - renewal_forecasts
default_filters:
  - field: "customer_status"
    op: "in"
    values: ["active", "at_risk"]

# topics/growth.topic.yaml
name: growth
description: "User acquisition, activation, and growth metrics"
base_view: user_signups
views:
  - user_signups
  - onboarding_funnel
  - activation_metrics
  - cohort_analysis
default_filters:
  - field: "is_spam"
    op: "eq"
    value: false
  - field: "signup_source"
    op: "neq"
    value: "internal_testing"
```

### Healthcare Topics

```yaml
# topics/patient_care.topic.yaml
name: patient_care
description: "Patient outcomes, treatment effectiveness, and care quality"
base_view: patient_records
views:
  - patient_records
  - treatments
  - outcomes
  - care_episodes

# topics/operations.topic.yaml
name: operations
description: "Hospital operations, resource utilization, and efficiency"
base_view: facility_metrics
views:
  - bed_utilization
  - staff_scheduling
  - equipment_usage
  - facility_metrics
```

## Topic Organization

### Directory Structure

```
topics/
├── sales.topic.yaml
├── marketing.topic.yaml
├── finance.topic.yaml
├── product.topic.yaml
└── operations.topic.yaml

views/
├── orders.view.yaml
├── customers.view.yaml
├── products.view.yaml
├── campaigns.view.yaml
└── ...
```

### Cross-Topic Relationships

Views can reference entities from other topics, enabling cross-domain analysis:

```yaml
# topics/sales.topic.yaml
views:
  - orders      # contains customer entity
  - customers   # primary customer view

# topics/marketing.topic.yaml
views:
  - campaigns   # contains customer entity for attribution
```

This allows queries like "Show campaign performance by customer segment" that span both marketing and sales topics.
