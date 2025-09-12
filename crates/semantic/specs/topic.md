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

# Include views in this topic
views:
  - orders
  - customers
  - products
````

## Properties

| Property      | Type   | Required | Description                                       |
| ------------- | ------ | -------- | ------------------------------------------------- |
| `name`        | string | Yes      | Unique identifier for the topic                   |
| `description` | string | Yes      | Human-readable description of the business domain |
| `views`       | array  | Yes      | List of view names included in this topic         |

## Examples

### E-commerce Topics

```yaml
# topics/sales.topic.yaml
name: sales
description: "Sales performance and order management data"
views:
  - orders
  - order_items
  - customers
  - products

# topics/marketing.topic.yaml
name: marketing
description: "Marketing campaigns, attribution, and customer acquisition"
views:
  - campaigns
  - attribution
  - customer_acquisition
  - website_sessions

# topics/finance.topic.yaml
name: finance
description: "Financial reporting, revenue recognition, and accounting"
views:
  - revenue_recognition
  - financial_transactions
  - cost_centers
  - budgets
```

### SaaS Business Topics

```yaml
# topics/product_usage.topic.yaml
name: product_usage
description: "Product engagement, feature usage, and user behavior analytics"
views:
  - user_sessions
  - feature_usage
  - user_journeys
  - product_events

# topics/customer_success.topic.yaml
name: customer_success
description: "Customer health, retention, and support metrics"
views:
  - customer_health
  - support_tickets
  - churn_analysis
  - renewal_forecasts

# topics/growth.topic.yaml
name: growth
description: "User acquisition, activation, and growth metrics"
views:
  - user_signups
  - onboarding_funnel
  - activation_metrics
  - cohort_analysis
```

### Healthcare Topics

```yaml
# topics/patient_care.topic.yaml
name: patient_care
description: "Patient outcomes, treatment effectiveness, and care quality"
views:
  - patient_records
  - treatments
  - outcomes
  - care_episodes

# topics/operations.topic.yaml
name: operations
description: "Hospital operations, resource utilization, and efficiency"
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
