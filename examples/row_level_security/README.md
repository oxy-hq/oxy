# Row-Level Security Examples

This directory contains examples demonstrating how to apply row-level security via the Oxy API.

## Overview

Oxy supports two security features via API parameters:

1. **Session Filters**: Apply row-level filtering based on runtime parameters (e.g., `tenant_id`, `project_ids`)
2. **Connection Overrides**: Dynamically override database `host` and `database` at query time

## Configuration

This directory contains configuration examples for different databases:

- ClickHouse: See [`clickhouse/config.yml`](./clickhouse/config.yml)
- Snowflake: See [`snowflake/config.yml`](./snowflake/config.yml)

**Note**: You must create the corresponding role and row policies in your database for filters to work:

- **ClickHouse**: See [ClickHouse Access Rights docs](https://clickhouse.com/docs/operations/access-rights)
- **Snowflake**: See [Snowflake Row Access Policies docs](https://docs.snowflake.com/en/user-guide/security-row-intro)

## API Usage Examples

### A. Applying Session Filters

Add the `filters` parameter to your API request body. Filters are validated against the schema in `config.yml` and applied as ClickHouse session settings.

**Example: Agent Request with Filters**

```bash
curl -X POST http://localhost:3000/{project_id}/agents/{pathb64}/ask-sync \
  -H "Content-Type: application/json" \
  -d '{
    "question": "What are the total sales for this month?",
    "filters": {
      "tenant_id": 12345,
      "project_ids": [100, 101, 102]
    }
  }'
```

### B. Applying Connection Overrides

Add the `connections` parameter to override connection parameters for specific databases.
The available override fields depend on the database type:

**ClickHouse overrides:**

- `host` - Override the ClickHouse host/URL
- `database` - Override the database name

**Snowflake overrides:**

- `account` - Override the account identifier
- `warehouse` - Override the warehouse
- `database` - Override the database name
- `schema` - Override the schema

#### ClickHouse Examples

**Example: Override ClickHouse Database**

```bash
curl -X POST http://localhost:3000/{project_id}/agents/{pathb64}/ask-sync \
  -H "Content-Type: application/json" \
  -d '{
    "question": "What are my recent sales?",
    "connections": {
      "clickhouse": {
        "database": "tenant_12345"
      }
    }
  }'
```

**Example: Override ClickHouse Host**

```bash
curl -X POST http://localhost:3000/{project_id}/agents/{pathb64}/ask-sync \
  -H "Content-Type: application/json" \
  -d '{
    "question": "What are my recent sales?",
    "connections": {
      "clickhouse": {
        "host": "https://replica.us-east-1.aws.clickhouse.cloud:8443"
      }
    }
  }'
```

**Example: Override Both ClickHouse Host and Database**

```bash
curl -X POST http://localhost:3000/{project_id}/agents/{pathb64}/ask-sync \
  -H "Content-Type: application/json" \
  -d '{
    "question": "What are the top performing projects?",
    "connections": {
      "clickhouse": {
        "host": "https://tenant-dedicated.us-west-2.aws.clickhouse.cloud:8443",
        "database": "tenant_12345"
      }
    }
  }'
```

#### Snowflake Examples

**Example: Override Snowflake Warehouse**

```bash
curl -X POST http://localhost:3000/{project_id}/agents/{pathb64}/ask-sync \
  -H "Content-Type: application/json" \
  -d '{
    "question": "What are my recent sales?",
    "connections": {
      "snowflake": {
        "warehouse": "TENANT_12345_WH"
      }
    }
  }'
```

**Example: Override Snowflake Database and Schema**

```bash
curl -X POST http://localhost:3000/{project_id}/agents/{pathb64}/ask-sync \
  -H "Content-Type: application/json" \
  -d '{
    "question": "What are the top performing projects?",
    "connections": {
      "snowflake": {
        "database": "TENANT_12345_DB",
        "schema": "ANALYTICS"
      }
    }
  }'
```

**Example: Override All Snowflake Connection Parameters**

```bash
curl -X POST http://localhost:3000/{project_id}/agents/{pathb64}/ask-sync \
  -H "Content-Type: application/json" \
  -d '{
    "question": "What are the total sales for this quarter?",
    "connections": {
      "snowflake": {
        "account": "tenant12345",
        "warehouse": "TENANT_COMPUTE_WH",
        "database": "TENANT_ANALYTICS",
        "schema": "PUBLIC"
      }
    }
  }'
```

#### Combined Examples

**Example: Combine ClickHouse Filters and Connection Overrides**

```bash
curl -X POST http://localhost:3000/{project_id}/agents/{pathb64}/ask-sync \
  -H "Content-Type: application/json" \
  -d '{
    "question": "What are the top performing projects?",
    "filters": {
      "tenant_id": 12345,
      "project_ids": [100, 101, 102]
    },
    "connections": {
      "clickhouse": {
        "host": "https://tenant-12345.us-east-1.aws.clickhouse.cloud:8443",
        "database": "tenant_12345"
      }
    }
  }'
```

**Example: Combine Snowflake Filters and Connection Overrides**

```bash
curl -X POST http://localhost:3000/{project_id}/agents/{pathb64}/ask-sync \
  -H "Content-Type: application/json" \
  -d '{
    "question": "What are the top performing regions?",
    "filters": {
      "tenant_id": 12345,
      "region_ids": [1, 2, 3]
    },
    "connections": {
      "snowflake": {
        "warehouse": "TENANT_12345_WH",
        "database": "TENANT_12345_DB",
        "schema": "ANALYTICS"
      }
    }
  }'
```
