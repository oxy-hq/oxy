# Claude Code Assistant Guidelines

## Project Overview

Oxy is a Rust workspace with a web frontend. The CLI binary lives in the `app` crate, which is the default workspace member.

### Workspace Layout

```
crates/
  app/                      # (oxy-app / oxy binary) CLI + HTTP server, default workspace member
  core/                     # (oxy) Core platform library, published as "oxy"
  agent/                    # (oxy-agent) Agent execution engine
  auth/                     # (oxy-auth) Authentication and authorization
  entity/                   # (entity) Sea-ORM database entity models
  migration/                # (migration) Sea-ORM database migrations
  semantic/                 # (oxy-semantic) Semantic query layer powered by airlayer
  shared/                   # (oxy-shared) Shared types, errors, and infrastructure
  workflow/                 # (oxy-workflow) Workflow orchestration
  thread/                   # (oxy-thread) Thread and conversation management
  project/                  # (oxy-project) Project and workspace management
  globals/                  # (oxy_globals) Global semantics registry and inheritance support
  omni/                     # (omni) Omni integration
  a2a/                      # (a2a) A2A protocol server
  test-utils/               # (oxy-test-utils) Test utilities, fixtures, and mocks
  agentic/
    core/                   # (agentic-core) Generic agentic workflow orchestration framework
    analytics/              # (agentic-analytics) Analytics domain for the agentic framework
    app-builder/            # (agentic-app-builder) Data app builder domain for the agentic framework
    connector/              # (agentic-connector) Database connector trait and backend implementations
    db/                     # (agentic-db) Shared SeaORM entities and migrations for agentic pipeline
    http/                   # (agentic-http) Axum HTTP routes for the agentic analytics pipeline
    llm/                    # (agentic-llm) Shared LLM provider abstraction for agentic domains
    workflow/               # (agentic-workflow) Procedure runner backed by oxy-workflow
  infrastructure/llm/
    anthropic/              # (oxy-anthropic) Anthropic LLM provider
    gemini/                 # (oxy-gemini) Google Gemini provider
    ollama/                 # (oxy-ollama) Ollama provider
    openai/                 # (oxy-openai) OpenAI provider
    oxy-llm/                # (oxy-llm) Unified LLM abstraction over all providers
  integration/
    looker/                 # (oxy-looker) Looker integration
web-app/                    # Frontend (Vite + React + TypeScript)
```

### Key Technical Details

- **Rust edition:** 2024
- **MSRV:** 1.92.0
- **Async runtime:** Tokio
- **Database ORM:** Sea-ORM (PostgreSQL)
- **HTTP framework:** Axum
- **Frontend:** Vite + React + TypeScript + pnpm

## Build Guidelines

**NEVER build in release mode** - Always use debug builds:

- ✅ `cargo build`
- ✅ `cargo check`
- ✅ `cargo run`
- ❌ `cargo build --release`

Release builds take significantly longer and are only needed for production distributions.

**Filter build output** - Always pipe `cargo check` / `cargo build` through grep to reduce output noise:

- ✅ `cargo check 2>&1 | grep -E "^(error|warning\[)"`
- ✅ `cargo build 2>&1 | grep -E "^(error|warning\[)"`
- This filters out progress lines, notes, and help suggestions, keeping only actionable errors and warnings.

## Testing Guidelines

**Use cargo nextest for running tests** - Always use `cargo nextest` instead of `cargo test`:

- ✅ `cargo nextest run`
- ✅ `cargo nextest run -p oxy-app`
- ❌ `cargo test` (don't use)

Nextest provides faster, more reliable test execution with better output and parallel execution.

### Testing the CLI

After making changes to CLI commands:

```bash
# Build in debug mode
cargo build

# Test using the debug binary
./target/debug/oxy <command>
```

### Running specific tests

```bash
# Run all tests in a package
cargo nextest run -p oxy-app

# Run a specific test file
cargo nextest run --test serve

# Run a specific test by name
cargo nextest run test_internal_port_disabled
```

## Code Style and Conventions

### Rust

- Follow standard `rustfmt` formatting (run `cargo fmt`).
- Address `clippy` warnings — CI runs `cargo clippy --profile ci --workspace`.
- Use **Conventional Commits**: `feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `build:`, `chore:`.
- Prefer `thiserror` / `OxyError` for error types — look at existing patterns in `oxy_shared::errors`.
- Use `tracing` for logging (`info!`, `warn!`, `debug!`), not `println!` in library crates.
- CLI user-facing output uses the `StyledText` trait from `oxy::theme` (`.text()`, `.success()`, `.error()`, `.tertiary()`, `.secondary()`).

### Frontend (web-app)

- Uses pnpm, not npm or yarn. Always use `pnpm exec <tool>` not `npx <tool>`.
- Lint/format with Biome: `pnpm exec biome check --write <file>` to auto-fix.
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

## Product Context (Web UI)

@product-context.md

## Common Pitfalls

- Do not use `--release` for local development or CI checks.
- Do not use `println!` in library code — use `tracing` macros instead.
- Do not add new crates without adding them to the workspace `Cargo.toml` members list.
- Do not commit `.env` files or secrets.
