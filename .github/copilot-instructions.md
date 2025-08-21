# Copilot Instructions for oxy-internal

## Project Architecture

- **Oxy** is a Rust-based framework for agentic analytics, organized as a multi-crate workspace under `crates/`.
- Major crates:
  - `core/`: Main logic and agentic analytics primitives.
  - `entity/`: Database entities and ORM models.
  - `migration/`: Database migration CLI and logic.
  - `py/`: Python bindings and integration.
- The `web-app/` directory contains a TypeScript/React frontend, built with Vite.
- Example workflows, agents, and data are in `examples/` and `sample_project/`.

## Developer Workflows

- **Build all Rust crates:**
  ```sh
  cargo build --workspace
  ```
- **Run database migrations:**
  ```sh
  cargo run -p migration -- [OPTIONS]
  # e.g. cargo run -p migration -- up
  ```
- **Test core logic:**
  ```sh
  cargo test -p core
  ```
- **Frontend build/test:**
  ```sh
  cd web-app && pnpm install && pnpm build
  pnpm test
  ```
- **Python bindings:**
  - See `crates/py/README.md` and `pyproject.toml` for poetry-based workflows.

## Conventions & Patterns

- **Database models** use SeaORM (`entity/src/*.rs`).
- **Migrations** are managed via the `migration` crate CLI (see above).
- **Agent/workflow definitions** are YAML files in `examples/` and `sample_project/`.
- **Secrets/config**: Use `.env` files (see `examples/README.md`).
- **Docs**: Main docs in `docs/`, API keys and authentication in subfolders.

## Integration Points

- **External DBs**: Connect via migration CLI and SeaORM models.
- **Python**: Interop via `crates/py/` and poetry.
- **Frontend**: Communicates with backend via API endpoints (see `web-app/src/`).

## Key Files & Directories

- `crates/core/src/`: Core Rust logic
- `crates/entity/src/`: ORM models
- `crates/migration/`: Migration CLI
- `web-app/src/`: Frontend code
- `examples/`, `sample_project/`: Example agents, workflows, and configs
- `docs/`: Documentation

## Example: Running a Migration

```sh
cargo run -p migration -- up
```

---

For more details, see [docs/](../docs/) and [README.md](../README.md).
