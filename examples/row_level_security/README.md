# Row-Level Security Examples

This directory contains examples demonstrating how to apply row-level security via the Oxy API.

## Overview

Oxy supports two security features via API parameters:

1. **Session Filters**: Apply row-level filtering based on runtime parameters (e.g., `tenant_id`, `project_ids`)
2. **Connection Overrides**: Dynamically override database `host` and `database` at query time

## Configuration

See [`clickhouse/config.yml`](./clickhouse/config.yml) for the database configuration example.

**Note**: You must create the corresponding role and row policies in ClickHouse for filters to work. See [`Clickhouse docs`](https://clickhouse.com/docs/operations/access-rights) for more information.

## API Usage Examples

### A. Applying Session Filters

Add the `filters` parameter to your API request body. Filters are validated against the schema in `config.yml` and applied as ClickHouse session settings.

**Example: SQL Execution with Filters**

```bash
curl -X POST http://localhost:8080/api/data/sql \
  -H "Content-Type: application/json" \
  -d '{
    "sql": "SELECT * FROM sales WHERE created_at >= today()",
    "database": "clickhouse",
    "filters": {
      "tenant_id": 12345,
      "project_ids": [100, 101, 102]
    }
  }'
```

This applies the filters as ClickHouse session settings:
```sql
SET SQL_tenant_id = 12345;
SET SQL_project_ids = [100, 101, 102];
```

**Example: Agent Request with Filters**

```bash
curl -X POST http://localhost:8080/api/agents/my-agent/ask \
  -H "Content-Type: application/json" \
  -d '{
    "question": "What are the total sales for this month?",
    "filters": {
      "tenant_id": 12345,
      "project_ids": [100, 101, 102]
    }
  }'
```

**Example: Workflow Request with Filters**

```bash
curl -X POST http://localhost:8080/api/workflows/my-workflow/run \
  -H "Content-Type: application/json" \
  -d '{
    "filters": {
      "tenant_id": 12345,
      "project_ids": [100, 101, 102]
    }
  }'
```

### B. Applying Connection Overrides

Add the `connections` parameter to override the `host` or `database` for a specific database connection.

**Example: Override Database**

```bash
curl -X POST http://localhost:8080/api/data/sql \
  -H "Content-Type: application/json" \
  -d '{
    "sql": "SELECT * FROM sales",
    "database": "clickhouse",
    "connections": {
      "clickhouse": {
        "database": "tenant_12345"
      }
    }
  }'
```

**Example: Override Host**

```bash
curl -X POST http://localhost:8080/api/data/sql \
  -H "Content-Type: application/json" \
  -d '{
    "sql": "SELECT * FROM large_table",
    "database": "clickhouse",
    "connections": {
      "clickhouse": {
        "host": "https://replica.us-east-1.aws.clickhouse.cloud:8443"
      }
    }
  }'
```

**Example: Override Both Host and Database**

```bash
curl -X POST http://localhost:8080/api/workflows/my-workflow/run \
  -H "Content-Type: application/json" \
  -d '{
    "connections": {
      "clickhouse": {
        "host": "https://tenant-dedicated.us-west-2.aws.clickhouse.cloud:8443",
        "database": "tenant_12345"
      }
    }
  }'
```

**Example: Combine Filters and Connection Overrides**

```bash
curl -X POST http://localhost:8080/api/agents/my-agent/ask \
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

