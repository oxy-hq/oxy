# Claude Code Assistant Guidelines

## Project Overview

Oxy is a Rust workspace with a web frontend. The CLI binary lives in the `app` crate, which is the default workspace member.

### Workspace Layout

```
crates/
  app/                      # (oxy-app / oxy binary) CLI + HTTP server, default workspace member
  core/                     # (oxy) Core platform library, published as "oxy"
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
    runtime/                # (agentic-runtime) Run lifecycle, persistence, and event streaming
    pipeline/               # (agentic-pipeline) Composition facade for starting/driving pipelines
    analytics/              # (agentic-analytics) Analytics domain for the agentic framework
    builder/                # (agentic-builder) Builder domain (data apps + file edits)
    connector/              # (agentic-connector) Database connector trait and backend implementations
    db/                     # (agentic-db) Shared SeaORM entities and migrations for agentic pipeline
    http/                   # (agentic-http) Axum HTTP routes for the agentic analytics pipeline
    llm/                    # (agentic-llm) Shared LLM provider abstraction for agentic domains
    automation/             # (agentic-automation) Automation runner backed by oxy-workflow
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

**Check all affected packages** - When changes span multiple crates, run `cargo check` for each changed package, not just one:

- ✅ `cargo check -p oxy 2>&1 | grep ...` then `cargo check -p oxy-app 2>&1 | grep ...`
- ✅ `cargo check --workspace 2>&1 | grep ...` (checks everything)
- ❌ `cargo check -p oxy-app` alone when `oxy` (core) was also modified

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

## Browser Testing (UI features)

When you add or modify a UI feature in `web-app/`, consider adding an agentic browser test under `web-app/tests/agentic/flows/`. The runner is Playwright + LLM-driven action selection on cold runs, with warm-replay on subsequent runs (~$0.002 / run after first record).

The [`agentic-browser-test`](.claude/skills/agentic-browser-test/SKILL.md) skill handles authoring + maintenance. Slash commands:

- `/test-feature <description>` — one-shot `.flow.test.yml` generation from a free-form description
- `/agentic-test-add-case <flow> <description>` — extend an existing flow with a new case
- `/run-agentic-tests <pattern>` — run with `HEADED=1 DEBUG=1`; the runner auto-spawns the right backend (local vs cloud)
- `/fix-agentic-test <flow-or-bucket>` — triage a failure (Tier-1 silent re-rank / Tier-2 staged heal / behavioral / cache-health)
- `/accept-agentic-healing <flow>` — promote staged Tier-2 healing recordings

CI runs the suite as a reusable workflow at `.github/workflows/agentic-tests.yaml`, bucketed by domain (`builder`, `semantic`, `ask-agent`, `threads`, `ide`, `onboarding`). For full mechanics see `web-app/tests/agentic/README.md` and `internal-docs/agentic-browser-testing-spec.md`.

**When working on a new feature or bug fix that touches the UI**, default to drafting a regression test alongside the code change — `/test-feature` is cheap and the warm-replay model means the test costs ~$0.002 per CI run thereafter.

## Committing Changes

Commit with a clear and concise message following the [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) specification.

```bash
git commit -m "feat: add feature description"
```

- Allowed types: `feat`, `fix`, `refactor`, `docs`, `test`, `build`, `chore`, `perf`, `style`, `ci`.
- Reference the area in the subject after the colon, e.g. `fix: web-app chart rendering bug`, `feat: api add invitations/mine endpoint`.
- Subject line: imperative mood, under 72 characters, no trailing period.
- Put the "why" in the body, not the subject. Keep the subject focused on what changed.

## Code Style and Conventions

### Rust

- Follow standard `rustfmt` formatting (run `cargo fmt`).
- Address `clippy` warnings — CI runs `cargo clippy --workspace`.
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
- Use `oxy start --clean` to remove all containers and volumes before starting fresh.

## When to use `oxy serve --local`

`--local` is a single-user, single-workspace, no-auth mode for the one specific case where a developer wants to run oxy on top of a project on their disk without setting up auth providers. It is **NOT** the default and **NOT** what you should reach for when testing or demoing.

**Use `--local` only when:** a developer is exploring oxy against their own project files and explicitly wants to skip auth setup.

**Do NOT use `--local` for:**

- Any multi-instance test or showcase (use `--enterprise` + `OXY_ROLE`).
- Validating compile boundary, S3 blob offload, worker fleet, or any other production-shaped behavior — these all need the `--enterprise` code path.
- Browser tests, integration tests, or anything that approximates cloud mode.
- Demonstrating role split — `just split-up` correctly uses `--enterprise`; do not "simplify" it back to `--local`.

The production code path is `--enterprise`. Default to that for everything except the narrow single-user-on-their-laptop case.

## Product Context (Web UI)

@product-context.md

## Backend Architecture

@internal-docs/backend-architecture.md

## Project Claude Skills (backend)

This repo ships three project-specific Claude skills that codify decisions
captured in `internal-docs/2026-05-31-scaling-oxy-multi-instance-architecture.md`
and `internal-docs/compile-boundary.md`.
When the work in front of you matches their triggers, **invoke them — do
not rederive the decision from first principles**.

### `oxy-scaling-design` (`.claude/skills/oxy-scaling-design/SKILL.md`)

Quick-reference index for the multi-instance scaling design — phase status
(1 done, 1.5 / 2 / 3 / 4 / 5 / 6 / 7 pending or in flight), hard constraints
(code-first is sacred; git is the source of truth; HTTP is stateless beyond
the lease; task-claim vs workspace-ownership are TWO leases, not one), the
A–H architectural refinements that override the original sketch, and the
rejected-alternatives list (Sourcegraph gitserver, Gitaly, Apalis, etc.).

**Invoke when** the user asks "how do we scale Oxy?", proposes a change
that touches workspace ownership or worker fleet shape, asks "what phase
adds X?", or any prompt containing "multi-instance", "worker fleet",
"lease table", "Envoy ring hash", "gix migration", "smart cloning",
"durable execution", or "shard workspaces". Ground every scaling decision
in the cited section/refinement, then read the full design doc before
implementing.

### `oxy-task-spec-default` (`.claude/skills/oxy-task-spec-default/SKILL.md`)

Codifies refinement H from the scaling doc: **new long-running work
defaults to a `TaskSpec` enqueued in `agentic_task_queue`, not a
`tokio::spawn` inside an HTTP handler.** Carries a checklist (>5s?
periodic? must survive instance death? multi-step?) and the recipe
for adding a new `TaskSpec` variant + handler.

**Invoke when** writing or reviewing code in `crates/app/src/server/`
that touches background work — `tokio::spawn`, periodic loops, LLM API
calls, git clones, embedding builds, anything taking more than a few
seconds. PRs that add a new `tokio::spawn` in an HTTP handler should be
challenged through this skill: either justify the exception in writing
or migrate to a `TaskSpec`. The scope survey at
`internal-docs/2026-05-28-worker-fleet-scope-survey.md` lists pre-existing
violations to migrate.

### `oxy-compile-boundary` (`.claude/skills/oxy-compile-boundary/SKILL.md`)

The runtime no longer walks the workspace filesystem on customer-facing
requests. Every YAML entity is addressable as a `*_definitions` row keyed
by `revision_id`, written by the compile worker and read by
`crates/app/src/server/api/compiled_reader.rs`. When a new file type
joins the workspace, it owes five integration points: a walker entry, a
compile-output variant, a schema migration + entity, a writer arm, and a
reader (plus handler wiring). The boundary is always on (no feature flags);
readers fall through to the filesystem on any miss.

**Invoke when** adding a new `.foo.yml` file type, introducing a new
runtime read site that walks the workspace filesystem, wiring a handler
that calls `ConfigManager::resolve_*` or
`fs::read_to_string(workspace_path...)`, or any time someone proposes
"just read it from the workspace dir." PRs that add a per-request FS read
without a compile boundary path should be challenged through this skill.
The skill carries the five-step contract; the operator runbook is at
`internal-docs/compile-boundary.md`.

### `oxy-route-classification` (`.claude/skills/oxy-route-classification/SKILL.md`)

Every HTTP route that touches node-local disk (the workspace working copy,
`.git`, or the local state dir) MUST be classified `IdeOnly` in
`crates/app/src/server/role_manifest.rs`, or it defaults to `FleetOk` and
404s/421s on a stateless serve replica with no working copy. This is how
`GET /apps/source/<file>` shipped broken. The fully-FS builders (git / files /
data-repo) are guarded automatically by the
`fully_fs_builder_routes_classify_ide_only` test; MIXED builders
(`build_app_routes`) need a per-route entry + test.

**The HA other half:** a pure Postgres/S3 READ buried under a broad `IdeOnly`
`{*rest}` wildcard (e.g. analytics/workflow/airway run-history reads under
`/analytics/{*rest}`) must be carved back out to `FLEET_OK_READ_PATTERNS`, or
*viewing* data (a past conversation) wrongly depends on the singleton Factory.
**Full stateful-vs-HA functionality matrix + decision flow:**
`internal-docs/multi-instance-fleet.md` — read it to know
what requires the stateful instance and what is served HA.

**Invoke when** adding/moving/renaming a route under
`crates/app/src/server/router/`, or writing a handler that calls
`workspace_path()` / `ConfigManager::resolve_*` / `resolve_state_dir()` / a
`GitClient`. PRs that add an FS-reading route without an `IdeOnly` entry — or
that pin a persisted-data read to the ide — should be challenged through this
skill.

### `oxy-customer-apps-perf` (`.claude/skills/oxy-customer-apps-perf/SKILL.md`)

The serve-plane + data-plane performance baseline for customer apps (PR #2634):
content-hashed assets are `immutable` Cache-Control and HTML gets a weak
ETag + 304 (`cache_control_for` / `etag_for` in `customer_apps_serve.rs`);
`/customer-apps/{*path}` compression is SSE-safe via `DefaultPredicate`; the
`POST /query` + `semantic-query` result cache is keyed `project_id`-first
(multi-tenant isolation), read **after** `check_customer_app_gates`, and honors
`?refresh`. Complements `oxy-route-classification` (where a route runs) by
governing how fast it answers.

**Invoke when** adding/moving a route under `/customer-apps/**`, a new customer-app
data endpoint, or any per-request read on the customer-app hot path. A new serving
route with no Cache-Control/compression, or a cached endpoint whose key doesn't
start with `project_id` / reads before auth / ignores `?refresh`, should be
challenged through this skill.

## Design Docs & Specs

- Save design documents and specs to `internal-docs/`, not `docs/superpowers/specs/`.

## Docs & Brand Copy

- When restructuring or migrating docs, **never regenerate
  homepage/positioning/tagline copy from scratch** — port it verbatim from the
  prior file, or stop and flag for marketing. Brand copy is not boilerplate.
- The canonical positioning statement lives in `docs/snippets/positioning.mdx`
  (imported by the Start landing pages); `README.md` mirrors it. Change it in
  one place, with marketing sign-off — `.github/CODEOWNERS` gates these files.
- Any docs PR that deletes a landing/homepage file or touches more than ~50
  files must confirm **"positioning carried over verbatim"** (see the PR
  checklist).

## Common Pitfalls

- Do not use `--release` for local development or CI checks.
- Do not use `println!` in library code — use `tracing` macros instead.
- Do not add new crates without adding them to the workspace `Cargo.toml` members list.
- Do not commit `.env` files or secrets.

<!-- code-review-graph MCP tools -->
## MCP Tools: code-review-graph

**IMPORTANT: This project has a knowledge graph. ALWAYS use the
code-review-graph MCP tools BEFORE using Grep/Glob/Read to explore
the codebase.** The graph is faster, cheaper (fewer tokens), and gives
you structural context (callers, dependents, test coverage) that file
scanning cannot.

### When to use graph tools FIRST

- **Exploring code**: `semantic_search_nodes` or `query_graph` instead of Grep
- **Understanding impact**: `get_impact_radius` instead of manually tracing imports
- **Code review**: `detect_changes` + `get_review_context` instead of reading entire files
- **Finding relationships**: `query_graph` with callers_of/callees_of/imports_of/tests_for
- **Architecture questions**: `get_architecture_overview` + `list_communities`

Fall back to Grep/Glob/Read **only** when the graph doesn't cover what you need.

### Key Tools

| Tool | Use when |
|------|----------|
| `detect_changes` | Reviewing code changes — gives risk-scored analysis |
| `get_review_context` | Need source snippets for review — token-efficient |
| `get_impact_radius` | Understanding blast radius of a change |
| `get_affected_flows` | Finding which execution paths are impacted |
| `query_graph` | Tracing callers, callees, imports, tests, dependencies |
| `semantic_search_nodes` | Finding functions/classes by name or keyword |
| `get_architecture_overview` | Understanding high-level codebase structure |
| `refactor_tool` | Planning renames, finding dead code |

### Workflow

1. The graph auto-updates on file changes (via hooks).
2. Use `detect_changes` for code review.
3. Use `get_affected_flows` to understand impact.
4. Use `query_graph` pattern="tests_for" to check coverage.
