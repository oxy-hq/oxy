# Migrating from SQLite to PostgreSQL

This guide helps existing Oxy users migrate from SQLite to PostgreSQL.

## Why PostgreSQL?

Starting with version 0.4.0, Oxy has migrated to PostgreSQL to provide:

- **Better concurrency**: Handle multiple simultaneous connections without locking issues
- **Production-ready performance**: Scale to larger datasets and user bases
- **Advanced features**: Support for analytics workloads with better query optimization
- **Unified architecture**: Single database system for all deployments (dev and production)

## Overview

Oxy now uses PostgreSQL exclusively:

- **Development**: Embedded PostgreSQL starts automatically - no manual setup required
- **Production**: Connect to external PostgreSQL (AWS RDS, Supabase, self-hosted, etc.)

Your existing SQLite data will **NOT** be migrated automatically. Choose one of the options below based on your needs.

---

## For Development Users

### Option 1: Fresh Start (Recommended)

Simply upgrade Oxy to the latest version. On first run, a new embedded PostgreSQL database will be created automatically.

**Your SQLite data will remain unchanged** at `~/.local/share/oxy/db.sqlite` but won't be used.

This is the easiest option if you don't need to preserve existing data (test threads, messages, etc.).

### Option 2: Migrate Existing Data

If you want to preserve your existing data, use the migration tool:

1. **Install the new version of Oxy**

2. **Run the migration tool**:

```bash
cargo run -p migration --features migration-tool --bin sqlite_to_postgres -- \
  --from sqlite://$HOME/.local/share/oxy/db.sqlite \
  --to postgresql://localhost:PORT/oxy
```

**Note**: You'll need to find the port that embedded PostgreSQL is using. Start Oxy once to see the port in the logs, or let the embedded instance start and check `~/.local/share/oxy/postgres_data/postmaster.pid`.

3. **Alternative: Use external PostgreSQL**:

```bash
# Start a local PostgreSQL (via Docker or system package)
docker run --name oxy-postgres -e POSTGRES_PASSWORD=password -e POSTGRES_DB=oxy -p 5432:5432 -d postgres:16

# Run the migration
cargo run -p migration --features migration-tool --bin sqlite_to_postgres -- \
  --from sqlite://~/.local/share/oxy/db.sqlite \
  --to postgresql://postgres:password@localhost:5432/oxy

# Configure Oxy to use this database
export OXY_DATABASE_URL=postgresql://postgres:password@localhost:5432/oxy
```

---

## For Production Users

### Prerequisites

1. **Set up a PostgreSQL database** (version 14 or higher recommended)
2. **Create a database and user for Oxy**

**Example PostgreSQL setup**:

```sql
CREATE DATABASE oxy;
CREATE USER oxy_user WITH PASSWORD 'secure_password';
GRANT ALL PRIVILEGES ON DATABASE oxy TO oxy_user;

-- PostgreSQL 15+ requires additional permission
\c oxy
GRANT ALL ON SCHEMA public TO oxy_user;
```

### Migration Steps

#### Step 1: Backup Your SQLite Database

```bash
# Find your SQLite database location
echo $OXY_STATE_DIR  # If set, it's under this directory
# Default location: ~/.local/share/oxy/db.sqlite

# Create a backup
cp ~/.local/share/oxy/db.sqlite ~/oxy-backup-$(date +%Y%m%d).sqlite
```

#### Step 2: Set Up PostgreSQL Connection

```bash
export OXY_DATABASE_URL=postgresql://oxy_user:secure_password@your-host:5432/oxy
```

For managed PostgreSQL services:

**AWS RDS**:
```bash
export OXY_DATABASE_URL=postgresql://oxy_user:password@oxy-db.xxxxx.us-east-1.rds.amazonaws.com:5432/oxy
```

**Supabase**:
```bash
export OXY_DATABASE_URL=postgresql://postgres:password@db.xxxxx.supabase.co:5432/postgres
```

**DigitalOcean**:
```bash
export OXY_DATABASE_URL=postgresql://oxy_user:password@oxy-db-do-user-xxxxx.db.ondigitalocean.com:25060/oxy?sslmode=require
```

#### Step 3: Run Migration (Dry Run First)

```bash
# Test the migration without writing data
cargo run -p migration --features migration-tool --bin sqlite_to_postgres -- \
  --from sqlite:///path/to/db.sqlite \
  --to $OXY_DATABASE_URL \
  --dry-run
```

Review the output to ensure:
- Connection to both databases succeeds
- Record counts look reasonable
- No unexpected errors

#### Step 4: Run the Actual Migration

```bash
cargo run -p migration --features migration-tool --bin sqlite_to_postgres -- \
  --from sqlite:///path/to/db.sqlite \
  --to $OXY_DATABASE_URL
```

This will:
1. Connect to both databases
2. Run all PostgreSQL migrations
3. Migrate data in dependency order (respecting foreign keys)
4. Report progress for each table

#### Step 5: Verify Migration

```bash
# Start Oxy with the new database
oxy server

# Check that your data is present
# - Verify users can log in
# - Check that projects and threads exist
# - Verify API keys work
```

#### Step 6: Update Production Configuration

Make the `OXY_DATABASE_URL` permanent in your deployment:

**Docker Compose**:
```yaml
services:
  oxy:
    environment:
      - OXY_DATABASE_URL=postgresql://oxy_user:password@postgres:5432/oxy
```

**Kubernetes**:
```yaml
env:
  - name: OXY_DATABASE_URL
    valueFrom:
      secretKeyRef:
        name: oxy-secrets
        key: database-url
```

**Systemd Service**:
```ini
[Service]
Environment="OXY_DATABASE_URL=postgresql://oxy_user:password@localhost:5432/oxy"
```

---

## Troubleshooting

### Migration Tool Errors

**Error: "Failed to connect to SQLite database"**
- Check the path to your SQLite file
- Ensure the file exists and is readable
- Use absolute paths (e.g., `/home/user/.local/share/oxy/db.sqlite`)

**Error: "Failed to connect to PostgreSQL database"**
- Verify the connection string format: `postgresql://user:password@host:port/database`
- Check that PostgreSQL is running and accessible
- Test the connection with `psql`: `psql "$OXY_DATABASE_URL"`
- Ensure the user has necessary permissions

**Error: "Failed to run migrations on PostgreSQL"**
- The target database user needs schema creation permissions
- Run `GRANT ALL ON SCHEMA public TO oxy_user;` as superuser

**Error: "Failed to insert [entity] into PostgreSQL"**
- Check for foreign key constraint violations
- This usually means data in wrong order (should not happen with the tool)
- Report the issue with the full error message

### Performance Issues

**Migration is slow**:
- For large databases (>1GB), migration can take 10-30 minutes
- Consider increasing PostgreSQL's `work_mem` temporarily
- Run the migration during off-peak hours

**Embedded PostgreSQL won't start**:
- Check available disk space in `~/.local/share/oxy/`
- Ensure no other PostgreSQL instance is using the auto-selected port
- Check logs in `~/.local/share/oxy/postgres_data/log/`

### Data Validation

After migration, verify critical data:

```bash
# Connect to PostgreSQL
psql "$OXY_DATABASE_URL"

# Check record counts
SELECT 'users' as table_name, COUNT(*) FROM users
UNION ALL SELECT 'projects', COUNT(*) FROM projects
UNION ALL SELECT 'threads', COUNT(*) FROM threads
UNION ALL SELECT 'messages', COUNT(*) FROM messages
UNION ALL SELECT 'runs', COUNT(*) FROM runs;
```

Compare these counts with your SQLite database:

```bash
# Connect to SQLite
sqlite3 ~/.local/share/oxy/db.sqlite

# Run the same count queries
SELECT 'users' as table_name, COUNT(*) FROM users
UNION ALL SELECT 'projects', COUNT(*) FROM projects
UNION ALL SELECT 'threads', COUNT(*) FROM threads
UNION ALL SELECT 'messages', COUNT(*) FROM messages
UNION ALL SELECT 'runs', COUNT(*) FROM runs;
```

---

## Rollback Plan

If you encounter critical issues after migration:

### Development

Simply delete the PostgreSQL data directory and restart:

```bash
rm -rf ~/.local/share/oxy/postgres_data
# Oxy will create a fresh embedded PostgreSQL on next start
```

Your original SQLite file remains untouched at `~/.local/share/oxy/db.sqlite`.

### Production

1. **Stop the Oxy server**
2. **Restore the `OXY_DATABASE_URL` to point to SQLite** (if you kept it):
   ```bash
   unset OXY_DATABASE_URL  # Use default SQLite
   # OR
   export OXY_DATABASE_URL=sqlite:///path/to/backup.sqlite
   ```
3. **Downgrade Oxy to the previous version** that supports SQLite
4. **Restore from backup** if needed

**Important**: Keep your SQLite backup for at least one release cycle.

---

## PostgreSQL Configuration Recommendations

For production deployments, optimize PostgreSQL settings:

### Connection Pooling

Oxy uses SeaORM's built-in connection pooling. Recommended settings:

```sql
ALTER SYSTEM SET max_connections = 100;
ALTER SYSTEM SET shared_buffers = '256MB';
ALTER SYSTEM SET effective_cache_size = '1GB';
ALTER SYSTEM SET work_mem = '16MB';
ALTER SYSTEM SET maintenance_work_mem = '64MB';
ALTER SYSTEM SET random_page_cost = 1.1;  -- For SSD storage
ALTER SYSTEM SET effective_io_concurrency = 200;

-- Apply changes
SELECT pg_reload_conf();
```

### Monitoring

Monitor these metrics:
- **Connection count**: `SELECT count(*) FROM pg_stat_activity;`
- **Database size**: `SELECT pg_size_pretty(pg_database_size('oxy'));`
- **Slow queries**: Enable `log_min_duration_statement = 1000` (log queries > 1s)

### Backups

Set up automated backups:

```bash
# Using pg_dump
pg_dump "$OXY_DATABASE_URL" > oxy-backup-$(date +%Y%m%d).sql

# Scheduled via cron (daily at 2 AM)
0 2 * * * pg_dump "$OXY_DATABASE_URL" | gzip > /backups/oxy-$(date +\%Y\%m\%d).sql.gz
```

---

## Getting Help

If you encounter issues during migration:

1. **Check the migration tool output**: It provides detailed error messages
2. **Review the logs**: `~/.local/share/oxy/postgres_data/log/` for embedded PostgreSQL
3. **Run with dry-run**: Use `--dry-run` to test without making changes
4. **Report issues**: Open an issue at [github.com/oxy-hq/oxy/issues](https://github.com/oxy-hq/oxy/issues)

Include in your report:
- Oxy version (`oxy --version`)
- PostgreSQL version (`psql --version`)
- Full error message from migration tool
- Relevant logs (redact sensitive information)

---

## FAQ

**Q: Can I continue using SQLite?**

A: No, SQLite support has been removed. However, the embedded PostgreSQL option provides the same "zero-config" experience for development.

**Q: Will embedded PostgreSQL work in Docker?**

A: Yes! Embedded PostgreSQL works in Docker containers. However, for production Docker deployments, we recommend using an external PostgreSQL instance for better control and persistence.

**Q: How much disk space does embedded PostgreSQL need?**

A: PostgreSQL binaries: ~50-100MB. Data directory: depends on your usage, typically starts at ~50MB.

**Q: Does embedded PostgreSQL affect performance?**

A: For development and small-scale usage, embedded PostgreSQL performs similarly to or better than SQLite. For production, use external PostgreSQL with proper tuning.

**Q: Can I use my existing PostgreSQL instance?**

A: Absolutely! Set `OXY_DATABASE_URL` to point to your PostgreSQL instance, and Oxy will use it instead of starting an embedded instance.

**Q: What PostgreSQL version is used?**

A: Embedded PostgreSQL uses version 16 by default. External PostgreSQL should be version 14 or higher.

---

## Additional Resources

- [PostgreSQL Official Documentation](https://www.postgresql.org/docs/)
- [Oxy Development Guide](../DEVELOPMENT.md)
- [SeaORM Documentation](https://www.sea-ql.org/SeaORM/)
- [PostgreSQL Performance Tuning](https://www.postgresql.org/docs/current/performance-tips.html)
