# Entities

Entities represent distinct objects or concepts within your data model. They are the building blocks that enable automatic relationship discovery and intelligent joins between views. Entities are similar to entities in dbt MetricFlow and help create a semantic understanding of how different pieces of data relate to each other.

## Purpose

Entities serve several key purposes:

- **Automatic Joins**: Enable the semantic layer to automatically join views based on shared entities
- **Relationship Discovery**: Help the system understand how different data sources relate to each other
- **Query Intelligence**: Allow AI agents to understand the structure of your data model
- **Data Lineage**: Provide clear lineage and relationships between different data objects

## Entity Types

### Primary Entity

A primary entity represents the main subject of a view. Each view should have exactly one primary entity.

### Foreign Entity

A foreign entity represents a reference to an entity that is primarily defined in another view. This enables joins between views.

## Configuration

Entities are defined within view files under the `entities` section:

```yaml
entities:
  - name: order
    type: primary
    description: "Individual order transaction"
    key: order_id

  - name: customer
    type: foreign
    description: "Customer who placed the order"
    key: customer_id
```

For entities with composite keys, use the `keys` field instead:

```yaml
entities:
  - name: order_line_item
    type: primary
    description: "Individual line item within an order"
    keys:
      - order_id
      - line_item_id

  - name: order
    type: foreign
    description: "Order containing this line item"
    key: order_id
```

## Properties

| Property      | Type          | Required | Description                                                                    |
| ------------- | ------------- | -------- | ------------------------------------------------------------------------------ |
| `name`        | string        | Yes      | Unique identifier for the entity within the semantic layer                     |
| `type`        | string        | Yes      | Entity type: `primary` or `foreign`                                            |
| `description` | string        | Yes      | Human-readable description of what this entity represents                      |
| `key`         | string        | No\*     | The dimension that should be used as the key for the entity (single key)       |
| `keys`        | array[string] | No\*     | The dimensions that should be used as the keys for the entity (composite keys) |

\*Note: Either `key` or `keys` must be provided, but not both. Use `key` for single-column keys and `keys` for composite (multi-column) keys.

## Examples

### E-commerce Data Model

```yaml
# views/orders.view.yaml
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
    description: "Product referenced in order line items"
    key: product_id

# views/customers.view.yaml
entities:
  - name: customer
    type: primary
    description: "Individual customer account"
    key: customer_id

# views/products.view.yaml
entities:
  - name: product
    type: primary
    description: "Individual product in catalog"
    key: product_id
```

### Composite Key Example

For tables with composite primary keys, use the `keys` field:

```yaml
# views/order_items.view.yaml
entities:
  - name: order_item
    type: primary
    description: "Individual line item within an order"
    keys:
      - order_id
      - line_item_id

  - name: order
    type: foreign
    description: "Order containing this line item"
    key: order_id

  - name: product
    type: foreign
    description: "Product in this line item"
    key: product_id

# views/inventory_locations.view.yaml
entities:
  - name: inventory_location
    type: primary
    description: "Product inventory at a specific warehouse"
    keys:
      - product_id
      - warehouse_id

  - name: product
    type: foreign
    description: "Product in inventory"
    key: product_id

  - name: warehouse
    type: foreign
    description: "Warehouse storing the product"
    key: warehouse_id
```

### Financial Data Model

```yaml
# views/transactions.view.yaml
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

# views/accounts.view.yaml
entities:
  - name: account
    type: primary
    description: "Customer financial account"
    key: account_id

  - name: customer
    type: foreign
    description: "Customer who owns the account"
    key: customer_id
```

## Automatic Joins

When entities are properly defined, the semantic layer can automatically join views that share common entities. For example:

- A query requesting `orders.total_revenue` and `customers.acquisition_channel` would automatically join the orders and customers views on the shared `customer` entity
- The system understands that `orders.customer_id` relates to `customers.customer_id` through the `customer` entity

For entities with composite keys, the system generates multi-condition join predicates by pairing keys in order. Both entities must have the same number of keys for the join to be valid.
