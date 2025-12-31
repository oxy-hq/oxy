# Oxy Demo with Docker Compose

This is a minimal Docker Compose setup for running Oxy with PostgreSQL, designed for demo and evaluation purposes.

## Quick Start

1. **Set your API key** (required):
   ```bash
   export OPENAI_API_KEY=your-api-key-here
   ```

2. **Start the services**:
   ```bash
   docker-compose -f docker-compose.demo.yml up
   ```

3. **Access Oxy**:
   - Web UI: http://localhost:3000
   - PostgreSQL: localhost:5432 (user: `demo`, password: `demo`, database: `demo`)

## What's Included

- **Oxy Service**: Uses the official pre-built Docker image from `ghcr.io/oxy-hq/oxy`
- **PostgreSQL**: A linked PostgreSQL 16 database for Oxy to connect to

## Volume Mounts

- **Project Directory**: `./examples` is mounted to `/workspace` in the container
  - Edit files in the `examples/` directory from your host machine
  - Changes are immediately reflected in the container

- **State Directory**: `oxy-state` volume persists Oxy's data
  - Database files, cache, and workflow history
  - Survives container restarts

## Configuration

### Environment Variables

Edit `docker-compose.demo.yml` to customize:

- `OPENAI_API_KEY`: Your OpenAI API key (required)
- `OXY_STATE_DIR`: Where Oxy stores its data (default: `/var/lib/oxy/data`)
- `DATABASE_URL`: PostgreSQL connection string

### Using Your Own Project

Replace the `./examples` mount with your own project directory:

```yaml
volumes:
  - ./my-project:/workspace  # Change this line
  - oxy-state:/var/lib/oxy/data
```

## Common Commands

```bash
# Start services in background
docker-compose -f docker-compose.demo.yml up -d

# View logs
docker-compose -f docker-compose.demo.yml logs -f

# Stop services
docker-compose -f docker-compose.demo.yml down

# Stop and remove volumes (⚠️ deletes all data)
docker-compose -f docker-compose.demo.yml down -v

# Pull latest oxy image
docker-compose -f docker-compose.demo.yml pull oxy
```

## Accessing the Container

Run commands inside the Oxy container:

```bash
# Get a shell
docker exec -it oxy-demo bash

# Run oxy commands
docker exec -it oxy-demo oxy --help
docker exec -it oxy-demo oxy version
```

## Notes

- This setup is for **demo/evaluation purposes only**
- For production deployments, see the [official documentation](https://oxy.tech/docs)
- The examples directory contains sample workflows and configurations you can explore
- Data persists in Docker volumes between restarts

## Troubleshooting

**Oxy fails to start:**
- Ensure `OPENAI_API_KEY` is set (either in docker compose or environment file that is read by docker compose)
- Check logs: `docker-compose -f docker-compose.demo.yml logs oxy`

**Database connection issues:**
- Wait for PostgreSQL to be healthy (has a healthcheck)
- Verify connection string in `DATABASE_URL`

**Permission issues with mounted volumes:**
- Ensure the `./examples` directory exists and is readable
