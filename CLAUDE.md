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
  auth/                     # (oxy-auth) Authentication — who you are
  authz/                    # (oxy-authz) Authorization — what you may do. THE authority model
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

Many crates carry their own `CLAUDE.md` (all `agentic/*`, `authz`, `cameras`,
`integration/unifi`) — read the local one before editing a crate. The two largest crates, `core` (`oxy`) and
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
- **`oxy serve --local`** is a **legacy**, narrow single-user/no-auth mode (one fixed project
  on disk) — **not actively used or maintained**; don't design new behavior around it.
  **Local development runs the production path**: `oxy serve --enterprise` (or `oxy start` for a
  Docker-Postgres dev box) — cloud/enterprise mode, multi-tenant. Use `--enterprise`/cloud for
  tests, demos, role-split, S3/worker-fleet, and anything production-shaped. A dev box is cloud
  mode with non-prod secrets, so it's not distinguishable from prod by mode.

## Authorization

Every authorization decision goes through **`oxy-authz`** (`crates/authz`) — one exhaustive
`match` in `allows()` that states every authority ring. `oxy-auth` is authentication (who
you are); this is authorization (what you may do). Full guide: `crates/authz/CLAUDE.md`.

- **Never decide access by hand.** No `matches!(role, Owner | Admin)` in a handler, no
  reading `OXY_OWNER` / the `app_admins` table. Take a `role_guards::*` extractor (there
  are 6), or call `server::authz::enforce_guard(..)`. `crates/app/tests/authz_boundaries.rs`
  **fails the build** otherwise — its allowlist is a backlog, not an exemption list.
- **Platform standing** (`is_owner` / `is_app_admin`) is read **only** by
  `server::authz::globals`: one door for a decision, one for a flag to display.
- **The crate owns the model; the app owns fact-loading** (`server::authz::loader`) —
  loading needs DB primitives, so it can't live in the crate. Keep `oxy-authz` on
  `uuid` + `tracing`: that's what lets the model be tested without a database.
- **Decisions are `existing_allow && allows(..)`**, so the model can only ever *subtract*
  and a mis-modeled ring cannot open a hole. Pass the **real** shipped check as
  `existing_allow` — it's the oracle the differential tests use, not ceremony.
- A new `Action` needs a `Ring` (the compiler enforces it) **and** a differential case.
  Reuse a ring rather than inventing a synonym.

Two engines were evaluated and rejected (Cedar — adopted then removed; Casbin). The
reasoning is in `crates/authz/src/lib.rs` and the design doc — read it before proposing a
third. Policy-as-data is an explicit non-goal; that's the requirement that pays for an engine.

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
| `oxy-customer-apps-perf` | add/move a `/customer-apps/**` route or custom-app data endpoint; any per-request read on that hot path | serving routes need Cache-Control + SSE-safe compression; result caches keyed `project_id`-first, read after auth gates, honor `?refresh` |

PRs that violate the right-hand column should be challenged through the matching skill.

## Docs & Brand Copy

- Save design docs/specs to `internal-docs/`, not `docs/superpowers/specs/`.
- **Never regenerate homepage/positioning/tagline copy from scratch** — port it verbatim or
  flag marketing. The canonical positioning lives in `docs/snippets/positioning.mdx`
  (mirrored by `README.md`); `.github/CODEOWNERS` gates these. Any docs PR that deletes a
  landing page or touches >~50 files must confirm "positioning carried over verbatim".

## Internal engineering docs (`internal-docs/`)

`internal-docs/` holds the **living platform-implementation references and operator
runbooks** — the durable "how a subsystem works and why" that you can't cheaply grep from
code. **Consult the matching doc before working on a subsystem, and fold any non-obvious
durable fact back into it when you ship.** [`internal-docs/README.md`](internal-docs/README.md)
is the categorized index (customer-apps, worker-fleet/scaling, observability, anomaly-
monitoring, admin-surfaces, partner-platform, compile-boundary, …).

- **Prefer an existing living doc** over a new file; create one only when a subsystem has
  no home. Undated filenames are the durable references; **dated `YYYY-MM-DD-*.md` files are
  ephemeral** design/plan snapshots and get distilled + pruned by a biweekly workflow
  (`.github/workflows/internal-docs-distill.yaml`) — don't treat them as the lasting home.
- The full curation contract is `internal-docs/README.md` → **Maintenance policy**.

## Common Pitfalls

- No `--release` for local/CI. No `println!` in library code (use `tracing`).
- New crates must be added to the workspace `Cargo.toml` members list.
- Never commit `.env` files or secrets.
- **Local dev runs cloud/enterprise mode** (`oxy start` / `oxy serve --enterprise`), so a dev box
  is indistinguishable from prod by `ServeMode` — never gate dev behavior on `ServeMode::Local`
  (nobody runs the legacy `--local`) or on error heuristics. Concretely: custom-app email
  (`ctx.email.send`) hits real SES in cloud mode; on a dev box set `OXY_APP_EMAIL_LOCAL_TEST=1`
  (and `MAGIC_LINK_LOCAL_TEST=true`) to preview the rendered email in the browser instead of
  sending. Both are in `.env.example`.

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
