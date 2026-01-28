# AI Agent Development Guidelines

Shared guidelines for all AI coding assistants (Claude Code, GitHub Copilot, Cursor, etc.) working on this project.

> For tool-specific settings, see [`CLAUDE.md`](./CLAUDE.md)

## Project Overview

Oxy is a Rust workspace with a web frontend. The CLI binary lives in the `app` crate, which is the default workspace member.

### Workspace Layout

```
crates/
  app/            # CLI + HTTP server (binary, default member)
  core/           # Core platform library (published as "oxy")
  agent/          # Agent execution engine
  auth/           # Authentication and authorization
  entity/         # Sea-ORM database entity models
  migration/      # Sea-ORM database migrations
  semantic/       # Semantic query layer
  shared/         # Shared types and infrastructure
  workflow/       # Workflow orchestration
  thread/         # Conversation / thread management
  project/        # Project and workspace management
  globals/        # Global semantics registry
  omni/           # Omni integration
  a2a/            # A2A protocol server
  infrastructure/llm/
    anthropic/    # Anthropic LLM provider
    gemini/       # Google Gemini provider
    ollama/       # Ollama provider
    openai/       # OpenAI provider
    oxy-llm/      # Unified LLM abstraction
web-app/          # Frontend (Vite + React + TypeScript)
```

### Key Technical Details

- **Rust edition:** 2024
- **MSRV:** 1.92.0
- **Async runtime:** Tokio
- **Database ORM:** Sea-ORM (PostgreSQL)
- **HTTP framework:** Axum
- **Frontend:** Vite + React + TypeScript + pnpm

## Build Rules

- **Always scope to the `oxy` package** — use `cargo check -p oxy` or `cargo build -p oxy`, not bare `cargo build`.
- **Never build in release mode** — debug builds only (`--release` is for CI/production only).
- **Filter build output** to reduce noise:

  ```bash
  cargo check -p oxy 2>&1 | grep -E "^(error|warning\[)"
  cargo build -p oxy 2>&1 | grep -E "^(error|warning\[)"
  ```

## Testing

```bash
# Run all workspace tests
cargo test

# Run tests for a specific crate
cargo test -p oxy

# Show stdout during tests
cargo test -- --nocapture
```

- Unit tests live inside source files in `mod tests` blocks.
- Integration tests live in `crates/{core,app,semantic}/tests/`.
- Test fixtures are in `tests/fixtures/`.
- Some tests require `OXY_DATABASE_URL` to be set (PostgreSQL).

## Code Style and Conventions

### Rust

- Follow standard `rustfmt` formatting (run `cargo fmt`).
- Address `clippy` warnings — CI runs `cargo clippy --profile ci --workspace`.
- Use **Conventional Commits**: `feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `build:`, `chore:`.
- Prefer `thiserror` / `OxyError` for error types — look at existing patterns in `oxy_shared::errors`.
- Use `tracing` for logging (`info!`, `warn!`, `debug!`), not `println!` in library crates.
- CLI user-facing output uses the `StyledText` trait from `oxy::theme` (`.text()`, `.success()`, `.error()`, `.tertiary()`, `.secondary()`).

### Frontend (web-app)

- Uses pnpm, not npm or yarn.
- Lint with ESLint, format with Prettier.
- `pnpm run dev` for development, `pnpm build` for production.

## Database

- **Development:** Oxy auto-starts an embedded PostgreSQL instance. Data is stored in `~/.local/share/oxy/postgres_data/`.
- **Custom/Production:** Set `OXY_DATABASE_URL` environment variable.
- **Migrations:** Run automatically on startup. Manual: `cargo run --bin migration`.
- **Entity models** are in the `entity` crate, migrations in `migration`.

## Docker (oxy start)

The `oxy start` command manages Docker containers programmatically via the `bollard` crate (not docker-compose).

- **PostgreSQL** container: `oxy-postgres` (volume: `oxy-postgres-data`)
- **ClickHouse** container (enterprise): `oxy-clickhouse` (volume: `oxy-clickhouse-data`)
- **OTel Collector** container (enterprise): `oxy-otel-collector`
- Enterprise services run on the `oxy-enterprise` Docker network.
- Use `oxy start --clean` to remove all containers and volumes before starting fresh.

## Common Pitfalls

- Do not run `cargo build` without `-p oxy` — the full workspace build is slow.
- Do not use `--release` for local development or CI checks.
- Do not use `println!` in library code — use `tracing` macros instead.
- Do not add new crates without adding them to the workspace `Cargo.toml` members list.
- Do not commit `.env` files or secrets.
