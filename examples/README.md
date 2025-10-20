# Examples

This directory contains example configurations and demonstrations for Oxy.

## Quick Start

**Instruction:** Copy the `.env.example` file to `.env` and update it with the necessary values.

> **Hint:** You can find the password in 1Password under the relevant entry.

```sh
cp .env.example .env
```

## Example Configurations

### `config.yml`

Standard configuration file with traditional hardcoded database connections. Good starting point for local development.

### `config-with-env-vars.yml` (NEW)

Demonstrates the new environment variable support for database connections. This is the recommended approach for:

- Production deployments
- Container/Kubernetes environments
- Keeping sensitive data out of version control
- Multi-environment setups (dev/staging/prod)

See the [Environment Variables Reference](https://docs.oxy.tech/reference/environment-variables) for detailed documentation on using environment variables.

## Environment Variable Support

All database connection parameters can now be read from environment variables using the `_var` suffix:

```yaml
databases:
  - name: clickhouse
    type: clickhouse
    host_var: CLICKHOUSE_HOST # Instead of hardcoded host
    user_var: CLICKHOUSE_USER # Instead of hardcoded user
    password_var: CLICKHOUSE_PASSWORD
    database_var: CLICKHOUSE_DATABASE
```

Supported databases:

- ✅ ClickHouse (host, user, password, database)
- ✅ PostgreSQL (host, port, user, password, database)
- ✅ MySQL (host, port, user, password, database)
- ✅ Redshift (host, port, user, password, database)
- ✅ BigQuery (key_path)

For more details, see the [Environment Variables Reference](https://docs.oxy.tech/reference/environment-variables)
