# Migration Guide: SQLite to PostgreSQL

This guide covers the migration from SQLite (default) to PostgreSQL in Oxy.

## What Changed?

Starting from version 0.4.0, Oxy supports both SQLite and PostgreSQL databases:

- **SQLite (Default)**: Used for backward compatibility and simple local development
- **PostgreSQL**: Recommended for production and advanced use cases

## Why PostgreSQL?

PostgreSQL provides:

- **Better Performance**: Superior query optimization and concurrent access
- **Production Ready**: Industry-standard database for production workloads
- **Richer Features**: Advanced data types, full-text search, and JSONB support
- **Scalability**: Better handling of large datasets and high concurrency
- **Data Integrity**: Stronger consistency guarantees and transaction support

## Database Options

Oxy supports two database backends:

### SQLite (Default)

**When to use:**

- Local development and testing
- Single-user scenarios
- Simple deployments without Docker
- Getting started quickly

**How to use:**

```bash
# Just run the server - SQLite is the default
oxy serve

# Data is stored at ~/.local/share/oxy/db.sqlite
```

### PostgreSQL (Recommended for Production)

**When to use:**

- Production deployments
- Multi-user scenarios
- High concurrency workloads
- Large datasets
- When you need advanced PostgreSQL features

**How to use:**

```bash
# Option 1: Use Docker PostgreSQL (for local development)
oxy start

# Option 2: Use external PostgreSQL (for production)
export OXY_DATABASE_URL="postgresql://user:password@host:5432/database"
oxy serve
```

## Prerequisites

### Docker Installation

You must have Docker installed and running on your system:

- **macOS**: [Docker Desktop for Mac](https://docs.docker.com/desktop/install/mac-install/)
- **Windows**: [Docker Desktop for Windows](https://docs.docker.com/desktop/install/windows-install/)
- **Linux**: [Docker Engine](https://docs.docker.com/engine/install/)

**Docker Alternatives**: Oxy also works with Docker-compatible tools:

- [OrbStack](https://orbstack.dev/) (macOS, faster than Docker Desktop)
- [Rancher Desktop](https://rancherdesktop.io/)
- [Podman](https://podman.io/) (Linux)
- [Colima](https://github.com/abiosoft/colima) (macOS)

### Verify Docker Installation

```bash
docker --version
docker ps
```

If either command fails, Docker is not properly installed or running.

## Migration Paths

Choose the migration path that best fits your situation:

### Path 1: Start Using PostgreSQL (Without Migrating SQLite Data)

This approach sets up PostgreSQL without migrating existing SQLite data. Good for starting fresh or if you don't need to preserve existing data.

1. **Start PostgreSQL with Docker**:
   ```bash
   oxy start
   ```
   
   This will:
   - Start a Docker PostgreSQL container
   - Run database migrations
   - Start the Oxy server

2. **Verify everything works**:
   ```bash
   oxy status
   ```

That's it! Oxy is now using PostgreSQL. Your old SQLite database remains at `~/.local/share/oxy/db.sqlite` if you need it later.

### Path 2: Data Migration (Migrate SQLite Data to PostgreSQL)

If you have existing data in SQLite that you want to migrate to PostgreSQL:

#### Step 1: Start PostgreSQL

First, start the PostgreSQL database:

```bash
oxy start
```

This creates and starts the Docker PostgreSQL container.

#### Step 2: Migrate Your Data

In another terminal, run the migration tool:

```bash
oxy migrate-sqlite --to postgresql://postgres:postgres@localhost:15432/oxy
```

The tool will automatically migrate all your data from SQLite to PostgreSQL.

#### Step 3: Verify Migration

Check that your data is present:

```bash
# Connect to PostgreSQL
docker exec -it oxy-postgres psql -U postgres -d oxy

# Run some queries
SELECT COUNT(*) FROM users;
SELECT COUNT(*) FROM projects;
\q
```

### Path 3: External PostgreSQL (Production Recommended)

For production deployments, use an external PostgreSQL database instead of Docker:

1. **Set up external PostgreSQL**:
   - Use a managed service (AWS RDS, Google Cloud SQL, Azure Database, etc.)
   - Or run your own PostgreSQL server

2. **Configure Oxy to use external database**:
   ```bash
   export OXY_DATABASE_URL="postgresql://user:password@host:5432/database"
   ```

3. **Run migrations**:
   ```bash
   oxy migrate
   ```

4. **Start Oxy**:
   ```bash
   oxy serve
   ```

Note: When `OXY_DATABASE_URL` is set, Oxy will **not** start a Docker PostgreSQL container.

## New Commands

### `oxy start`

The primary command for local development. Starts Docker PostgreSQL and the Oxy server:

```bash
oxy start [OPTIONS]

Options:
  --port <PORT>        Server port (default: 3000)
  --host <HOST>        Server host (default: 0.0.0.0)
  --readonly           Enable read-only mode
  --http2-only         Force HTTP/2 only
```

**What it does**:

1. Checks Docker availability
2. Creates/starts `oxy-postgres` container
3. Waits for PostgreSQL to be ready
4. Runs database migrations
5. Starts the Oxy web server

**On exit** (Ctrl+C):

- Stops the Oxy server
- Stops the PostgreSQL container
- Data is preserved in the `oxy-postgres-data` volume

### `oxy status`

Check the status of Oxy services:

```bash
oxy status
```

**Shows**:

- Docker daemon status
- PostgreSQL container status (running/stopped/not created)
- Database connectivity
- Helpful Docker commands

### `oxy serve`

Start only the Oxy server (database must be running separately):

```bash
oxy serve [OPTIONS]
```

**Use cases**:

- When PostgreSQL is already running
- When using external PostgreSQL (`OXY_DATABASE_URL` is set)
- When using SQLite (default, no `OXY_DATABASE_URL` set)
- For production deployments

## Docker Management

### Container Details

- **Container name**: `oxy-postgres`
- **Image**: `postgres:18-alpine`
- **Port mapping**: `15432:5432` (host:container)
- **Volume**: `oxy-postgres-data`
- **Credentials**: `postgres:postgres`
- **Database**: `oxy`

### Useful Docker Commands

#### View Logs

```bash
# View recent logs
docker logs oxy-postgres

# Follow logs in real-time
docker logs -f oxy-postgres

# View last 50 lines
docker logs --tail 50 oxy-postgres
```

#### Access PostgreSQL Shell

```bash
# Interactive psql
docker exec -it oxy-postgres psql -U postgres -d oxy

# Run a query
docker exec -it oxy-postgres psql -U postgres -d oxy -c "SELECT COUNT(*) FROM users;"
```

#### Container Management

```bash
# Start stopped container
docker start oxy-postgres

# Stop running container
docker stop oxy-postgres

# Restart container
docker restart oxy-postgres

# Remove container (keeps data)
docker rm oxy-postgres

# Check container status
docker ps -a -f name=oxy-postgres
```

#### Volume Management

```bash
# List volumes
docker volume ls | grep oxy

# Inspect volume
docker volume inspect oxy-postgres-data

# Backup data
docker run --rm -v oxy-postgres-data:/data -v $(pwd):/backup \
  alpine tar czf /backup/oxy-postgres-backup.tar.gz -C /data .

# Restore data
docker run --rm -v oxy-postgres-data:/data -v $(pwd):/backup \
  alpine tar xzf /backup/oxy-postgres-backup.tar.gz -C /data

# Remove volume (⚠️ deletes all data)
docker volume rm oxy-postgres-data
```

## Troubleshooting

### Docker Not Found

**Error**: `Docker is not installed or not in PATH`

**Solution**:
1. Install Docker from [docker.com](https://www.docker.com/)
2. Start Docker Desktop (macOS/Windows) or Docker daemon (Linux)
3. Verify: `docker --version`

### Docker Daemon Not Running

**Error**: `Docker daemon is not running`

**Solution**:
- **macOS/Windows**: Start Docker Desktop
- **Linux**: `sudo systemctl start docker`
- Check: `docker ps`

### Port Already in Use

**Error**: `port 15432 already in use`

**Solution**:
```bash
# Check what's using the port
lsof -i :15432

# If it's an old embedded PostgreSQL
kill <PID>

# If it's another Docker container
docker ps -a
docker stop <container-name>
```

### Database Connection Failed

**Error**: `Failed to connect to Docker PostgreSQL`

**Check status**:
```bash
oxy status
```

**Common causes**:
1. Container still starting up (wait 10-30 seconds)
2. Container stopped unexpectedly (check logs: `docker logs oxy-postgres`)
3. Port conflict (see above)

**Solution**:
```bash
# Restart container
docker restart oxy-postgres

# Or recreate it
docker stop oxy-postgres
docker rm oxy-postgres
oxy start
```

### Data Loss After Update

**Scenario**: Data disappeared after updating Oxy

**Cause**: You may have removed the Docker volume

**Solution**:
1. Check if volume exists: `docker volume ls | grep oxy`
2. If missing, restore from backup (see Volume Management above)
3. If no backup, you'll need to start fresh

**Prevention**: Always backup before major updates

### Container Won't Start

**Error**: Container exits immediately

**Check logs**:
```bash
docker logs oxy-postgres
```

**Common issues**:
1. Corrupted data volume
2. Insufficient disk space
3. Permission issues

**Solution**:
```bash
# Remove and recreate (⚠️ loses data)
docker rm -f oxy-postgres
docker volume rm oxy-postgres-data
oxy start

# Or restore from backup
docker volume rm oxy-postgres-data
# ... restore backup ...
oxy start
```

## Migrating SQLite Data

If you have legacy SQLite data from a previous Oxy installation, use the built-in migration tool:

### Quick Start

```bash
# 1. Start PostgreSQL first
oxy start

# 2. In another terminal, run migration (uses default SQLite location)
oxy migrate-sqlite --to postgresql://postgres:postgres@localhost:15432/oxy

# 3. Verify migration
docker exec -it oxy-postgres psql -U postgres -d oxy -c "SELECT COUNT(*) FROM users;"
```

### Command Options

```bash
oxy migrate-sqlite [OPTIONS] --to <POSTGRES_URL>

Options:
  --from <SQLITE_URL>     SQLite database URL (optional)
                          Default: sqlite://$HOME/.local/share/oxy/db.sqlite

  --to <POSTGRES_URL>     PostgreSQL database URL (required)
                          Example: postgresql://postgres:postgres@localhost:15432/oxy

  --dry-run               Preview migration without writing data
```

### Examples

**Migrate from default SQLite location:**
```bash
oxy migrate-sqlite --to postgresql://postgres:postgres@localhost:15432/oxy
```

**Migrate from custom SQLite location:**
```bash
oxy migrate-sqlite \
  --from sqlite:///path/to/custom.sqlite \
  --to postgresql://postgres:postgres@localhost:15432/oxy
```

**Preview migration without writing data:**
```bash
oxy migrate-sqlite \
  --to postgresql://postgres:postgres@localhost:15432/oxy \
  --dry-run
```

**Migrate using environment variables:**
```bash
export SQLITE_URL="sqlite://$HOME/.local/share/oxy/db.sqlite"
export POSTGRES_URL="postgresql://postgres:postgres@localhost:15432/oxy"
oxy migrate-sqlite
```

### What Gets Migrated

The migration tool transfers all data in the correct order to respect foreign key constraints:

1. **Users** - User accounts and authentication data
2. **Workspaces** - Workspace configurations
3. **Projects** - Project definitions
4. **Databases** - Database connection configurations
5. **Workflows** - Workflow definitions
6. **Agents** - AI agent configurations
7. **Threads** - Analysis threads
8. **Messages** - Thread messages
9. **Runs** - Workflow execution records
10. **Run Steps** - Individual workflow step records
11. **Charts** - Saved visualizations
12. **Datasets** - Dataset metadata
13. **Tables** - Table metadata
14. **Columns** - Column metadata
15. **Jobs** - Background job queue
16. **Queries** - Saved queries
17. **Secrets** - Encrypted secrets

### Migration Process

1. **Connects to both databases** - Retries connection with exponential backoff
2. **Runs PostgreSQL migrations** - Ensures schema is up to date
3. **Migrates each entity in order** - Respects foreign key dependencies
4. **Validates counts** - Shows source vs. migrated record counts
5. **Reports results** - Displays success/failure for each entity

### Troubleshooting Migration

**Error: Failed to connect to PostgreSQL**
```bash
# Make sure PostgreSQL is running
oxy status

# If not running, start it
oxy start
```

**Error: SQLite database not found**
```bash
# Check default location
ls -la ~/.local/share/oxy/db.sqlite

# Or specify custom location
oxy migrate-sqlite --from sqlite:///path/to/db.sqlite --to postgresql://...
```

**Error: Table already exists or data conflicts**
```bash
# Migration requires an empty PostgreSQL database
# If you need to re-migrate, either:

# Option 1: Drop and recreate the database
docker exec -it oxy-postgres psql -U postgres -c "DROP DATABASE oxy;"
docker exec -it oxy-postgres psql -U postgres -c "CREATE DATABASE oxy;"
oxy migrate-sqlite --to postgresql://postgres:postgres@localhost:15432/oxy

# Option 2: Use a fresh Docker volume
docker stop oxy-postgres
docker rm oxy-postgres
docker volume rm oxy-postgres-data
oxy start
# Then run migration in another terminal
oxy migrate-sqlite --to postgresql://postgres:postgres@localhost:15432/oxy
```

**Verify migration success:**
```bash
# Connect to PostgreSQL
docker exec -it oxy-postgres psql -U postgres -d oxy

# Check record counts
SELECT COUNT(*) FROM users;
SELECT COUNT(*) FROM workspaces;
SELECT COUNT(*) FROM projects;
SELECT COUNT(*) FROM threads;
SELECT COUNT(*) FROM messages;
\q
```

## Production Considerations

### Don't Use Docker PostgreSQL in Production

The Docker PostgreSQL setup is designed for local development. For production:

1. **Use managed PostgreSQL**:
   - AWS RDS
   - Google Cloud SQL
   - Azure Database for PostgreSQL
   - DigitalOcean Managed Databases
   - Heroku Postgres

2. **Or self-hosted PostgreSQL**:
   - With proper backups
   - Monitoring
   - High availability
   - Security hardening

3. **Configure Oxy**:
   ```bash
   export OXY_DATABASE_URL="postgresql://user:password@host:5432/database"
   oxy migrate
   oxy serve
   ```

### Docker Compose for Production

If you must use Docker in production, use Docker Compose with proper configuration:

```yaml
version: '3.8'
services:
  postgres:
    image: postgres:18-alpine
    container_name: oxy-postgres
    restart: unless-stopped
    environment:
      POSTGRES_USER: ${POSTGRES_USER}
      POSTGRES_PASSWORD: ${POSTGRES_PASSWORD}
      POSTGRES_DB: ${POSTGRES_DB}
    volumes:
      - postgres-data:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U ${POSTGRES_USER}"]
      interval: 10s
      timeout: 5s
      retries: 5

  oxy:
    image: ghcr.io/oxy-hq/oxy:latest
    restart: unless-stopped
    ports:
      - "3000:3000"
    environment:
      OXY_DATABASE_URL: postgresql://${POSTGRES_USER}:${POSTGRES_PASSWORD}@postgres:5432/${POSTGRES_DB}
    depends_on:
      postgres:
        condition: service_healthy

volumes:
  postgres-data:
```

## FAQ

### Q: Can I still use SQLite?

**A**: Yes! SQLite is still the default database. Oxy will use SQLite unless you set `OXY_DATABASE_URL` or use `oxy start` to run with PostgreSQL.

### Q: What happens to my SQLite data?

**A**: Your SQLite database (stored at `~/.local/share/oxy/db.sqlite`) is not automatically migrated when you switch to PostgreSQL. Use the `oxy migrate-sqlite` command to transfer your data, or keep using SQLite if you prefer.

### Q: Can I use Podman instead of Docker?

**A**: Yes! Podman is compatible. The `docker` commands work with Podman's Docker-compatible CLI.

### Q: Does this work on Windows?

**A**: Yes, but you need Docker Desktop for Windows or WSL2 with Docker.

### Q: How do I upgrade PostgreSQL versions?

**A**:
```bash
# Backup data
docker exec oxy-postgres pg_dump -U postgres oxy > backup.sql

# Remove old container
docker stop oxy-postgres
docker rm oxy-postgres

# Update image in docker.rs if needed, or wait for Oxy update

# Start new container
oxy start

# Restore data if needed
docker exec -i oxy-postgres psql -U postgres oxy < backup.sql
```

### Q: Where is my data stored?

**A**: In the Docker volume `oxy-postgres-data`. Find its location:
```bash
docker volume inspect oxy-postgres-data | grep Mountpoint
```

### Q: Can I connect other tools to this PostgreSQL?

**A**: Yes! Connect to `postgresql://postgres:postgres@localhost:15432/oxy` with any PostgreSQL client:
- psql
- pgAdmin
- DBeaver
- DataGrip
- etc.

## Getting Help

If you encounter issues not covered in this guide:

1. **Check status**: `oxy status`
2. **Check logs**: `docker logs oxy-postgres`
3. **Search issues**: [GitHub Issues](https://github.com/oxy-hq/oxy/issues)
4. **Ask for help**: [Discussions](https://github.com/oxy-hq/oxy/discussions)

## Switching Back to SQLite

If you want to switch back to SQLite from PostgreSQL:

```bash
# Simply stop using `oxy start` and use `oxy serve` instead
# Make sure OXY_DATABASE_URL is not set
unset OXY_DATABASE_URL

# Start with SQLite (default)
oxy serve
```

Your SQLite database at `~/.local/share/oxy/db.sqlite` will be used automatically.
