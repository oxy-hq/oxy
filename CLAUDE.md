# Claude Code Assistant Guidelines

Oxy (brand: **Oxygen**) is a Rust workspace + web frontend. The `oxy` CLI/server
binary lives in the `app` crate (the default workspace member).

- **Rust** edition 2024, MSRV 1.92.0 · **async** Tokio · **ORM** Sea-ORM (PostgreSQL) · **HTTP** Axum
- **Frontend** Vite + React + TypeScript, **pnpm** (never npm/yarn)

## Workspace Layout

```
crates/
  app/                      # (oxy-app / oxy binary) CLI + HTTP server, default member
  core/                     # (oxy) Core platform library, published as "oxy"
  auth/                     # (oxy-auth) Auth & authorization
  entity/  migration/       # Sea-ORM entities / migrations
  semantic/                 # (oxy-semantic) Semantic query layer (airlayer)
  shared/                   # (oxy-shared) Shared types, errors, infra
  project/                  # (oxy-project) Project/model config domain
  thread/                   # (oxy-thread) Thread/conversation domain (thin)
  oxy-compile/              # (oxy-compile) Compile boundary: workspace FS → Postgres rows
  workspace-fs/             # (oxy-workspace-fs) Workspace filesystem helpers (thin)
  git/                      # (oxy-git) Git client / worktree ops
  platform/                 # (oxy-platform) Platform services
  billing/                  # (oxy-billing) Stripe billing
  metric-monitoring/        # (oxy-metric-monitoring) Anomaly monitors / metric tree
  observability/            # (oxy-observability) Customer-facing observability backend
  airform/                  # (oxy-airform) dbt-style modeling
  airhouse/                 # (airhouse) Warehouse + connector
  cameras/                  # (oxy-cameras) Camera fleet domain
  test-utils/               # (oxy-test-utils) Fixtures & mocks
  agentic/
    core/ runtime/ pipeline/ analytics/ builder/ automation/ airway/
    connector/ http/ llm/ semantic/    # see crates/agentic/CLAUDE.md for layering
  infrastructure/llm/{anthropic,gemini,ollama,openai,oxy-llm}
  infrastructure/semantic/  # (oxy-airlayer-compat) airlayer compatibility shim
  integration/{looker,unifi,omni}
web-app/                    # Frontend (see web-app/CLAUDE.md)
```

Many crates carry their own `CLAUDE.md` (all `agentic/*`, `cameras`, `integration/unifi`) —
read the local one before editing a crate. The two largest crates, `core` (`oxy`) and
`app` (`oxy-app`), have crate-root `CLAUDE.md` guides too; start there before diving in.

## Build

**Never use `--release`** locally or in CI checks — debug only (`cargo build`/`check`/`run`).

- **Check every affected package**, not just one: `cargo check --workspace`, or run
  `cargo check -p <crate>` for each changed crate (e.g. both `oxy` and `oxy-app`).
- **Filter output** to actionable lines: `cargo check 2>&1 | grep -E "^(error|warning\[)"`.

## Testing

- Use **`cargo nextest run`**, never `cargo test`. Scope with `-p <crate>`, `--test <file>`, or a test name.
- After CLI changes: `cargo build` then exercise `./target/debug/oxy <command>`.
- Write tests alongside the change; for bug fixes, add a failing test first when practical.

### Browser tests (UI features)

UI changes under `web-app/` default to a regression flow in `web-app/tests/agentic/flows/`
(Playwright + LLM action selection, ~$0.002/run after first record). The
[`agentic-browser-test`](.claude/skills/agentic-browser-test/SKILL.md) skill owns
authoring/maintenance; drive it via slash commands (`/test-feature`,
`/agentic-test-add-case`, `/run-agentic-tests`, `/fix-agentic-test`,
`/accept-agentic-healing`). Mechanics: `web-app/tests/agentic/README.md`.

## Committing

Follow [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/):
types `feat|fix|refactor|docs|test|build|chore|perf|style|ci`. Name the area after the
colon (`fix: web-app chart rendering bug`). Subject: imperative, <72 chars, no period;
put the "why" in the body.

## Code Style

**Rust** — `cargo fmt`; clear clippy (`cargo clippy --workspace`); prefer
`thiserror`/`OxyError` (see `oxy_shared::errors`); use `tracing` (`info!`/`warn!`/`debug!`),
never `println!` in library crates; CLI output via the `StyledText` trait from `oxy::theme`.

**Frontend** — pnpm only; `pnpm exec <tool>` not `npx`; lint/format with Biome
(`pnpm exec biome check --write <file>`). Full conventions in `web-app/CLAUDE.md`.

## Database & Runtime

- **DB**: dev auto-starts embedded PostgreSQL (`~/.local/share/oxy/postgres_data/`); override
  with `OXY_DATABASE_URL`. Migrations run on startup (`cargo run --bin migration` to force).
  Entities in `entity`, migrations in `migration`.
- **Docker** (`oxy start`): containers managed via `bollard` (not docker-compose) —
  `oxy-postgres` container, `oxy-postgres-data` volume; `oxy start --clean` for a fresh slate.
- **`oxy serve --local`** is a narrow single-user/no-auth mode for a dev running Oxy against
  their own project on disk. It is **not** the default. The production code path is
  `--enterprise` — use it for tests, demos, role-split, S3/worker-fleet, or anything
  production-shaped. `--local` only for the laptop-exploration case.

## Product Context (Web UI)

@product-context.md

## Backend Architecture

@internal-docs/backend-architecture.md

## Project Skills (invoke, don't rederive)

These skills encode already-made decisions. When the work matches, **invoke the skill** —
each `SKILL.md` carries the full trigger list and contract. The load-bearing constraints:

| Skill | Governs — invoke when… | Non-negotiable |
| ----- | ---------------------- | -------------- |
| `oxy-scaling-design` | multi-instance / worker fleet / lease table / `OXY_ROLE` / stateless serve | code-first is sacred; git is source of truth; task-claim ≠ workspace-ownership (two leases) |
| `oxy-task-spec-default` | background work in `crates/app/src/server/` (`tokio::spawn`, periodic loops, LLM calls, clones >5s) | new long-running work is a `TaskSpec` in `agentic_task_queue`, **not** a spawn in an HTTP handler |
| `oxy-compile-boundary` | new `.foo.yml` file type, or any per-request read that walks the workspace FS | every workspace artifact is a `*_definitions` Postgres row keyed by `revision_id`, not an FS read |
| `oxy-route-classification` | add/move a route under `server/router/`, or a handler touching disk/`.git`/state dir | FS-touching routes MUST be `IdeOnly` in `role_manifest.rs`; persisted-data reads MUST stay `FleetOk` |
| `oxy-customer-apps-perf` | add/move a `/customer-apps/**` route or customer-app data endpoint; any per-request read on that hot path | serving routes need Cache-Control + SSE-safe compression; result caches keyed `project_id`-first, read after auth gates, honor `?refresh` |

PRs that violate the right-hand column should be challenged through the matching skill.

## Docs & Brand Copy

- Save design docs/specs to `internal-docs/`, not `docs/superpowers/specs/`.
- **Never regenerate homepage/positioning/tagline copy from scratch** — port it verbatim or
  flag marketing. The canonical positioning lives in `docs/snippets/positioning.mdx`
  (mirrored by `README.md`); `.github/CODEOWNERS` gates these. Any docs PR that deletes a
  landing page or touches >~50 files must confirm "positioning carried over verbatim".

## Common Pitfalls

- No `--release` for local/CI. No `println!` in library code (use `tracing`).
- New crates must be added to the workspace `Cargo.toml` members list.
- Never commit `.env` files or secrets.

## code-review-graph MCP (use BEFORE Grep/Glob/Read)

This repo has a knowledge graph — it's faster, cheaper, and gives structural context
(callers, dependents, test coverage) that file scanning can't. Reach for it first:

| Tool | Use when |
| ---- | -------- |
| `semantic_search_nodes` / `query_graph` | finding code, tracing callers/callees/imports/tests |
| `detect_changes` + `get_review_context` | reviewing a diff (risk-scored, token-efficient) |
| `get_impact_radius` / `get_affected_flows` | blast radius of a change |
| `get_architecture_overview` / `list_communities` | high-level structure |
| `refactor_tool` | planning renames, finding dead code |

The graph auto-updates on file changes. Fall back to Grep/Glob/Read only when it doesn't cover what you need.
