# Product Context

This file is read by the bot at startup and injected into every Claude API call.
Fill it in to help Claude ask smarter clarifying questions and correctly identify
which component a bug affects. The more specific, the better.

---

## Product Overview

Oxy (user-facing brand: **Oxygen**) is an AI-powered data analytics platform
that lets teams query databases, build automated reports, and visualize data
through natural language. Users connect data sources (DuckDB, PostgreSQL,
BigQuery, Snowflake, ClickHouse, Airhouse, warehouses, semantic layer, Looker),
ask questions in a chat interface where AI agents generate and execute SQL,
then view streamed results. Teams can also define reusable multi-step
Procedures (YAML-based automation, formerly called Workflows/Automations),
build configuration-driven Data Apps (dashboards with charts, tables, and
interactive controls), author dbt-style SQL transformations natively via
Airform, and directly edit all project files in a built-in Developer Portal
IDE with SQL IDE, Modeling, and Git integration.

Oxy supports three deployment modes:
- **Local** (`oxy start`) — PostgreSQL auto-managed in Docker, single workspace
- **Remote** (`oxy serve --local`) — Single fixed workspace on a VM/container, embedded PostgreSQL
- **Cloud / Multi-workspace** (`oxy serve`) — Multi-tenant platform with multi-organization support, GitHub-based workspace import, role-based access control, magic link authentication, and per-seat Stripe billing

---

## Main Pages / Features

- **Home** (`/`) — Primary chat panel. Users type a question, select an AI agent, pick a mode (Ask / Build / Workflow), and submit. The agent streams back responses with SQL artifacts and formatted answers. Starting a conversation creates a new Thread and redirects to it.
- **Thread Detail** (`/threads/:id`) — Shows the full conversation history for a thread, renders agent messages with artifacts (SQL blocks, tables, charts), and provides a follow-up input for continuing the conversation. Streaming can be cancelled with a Stop button.
- **Threads** (`/threads`) — Paginated list of all past conversation threads. Supports item-per-page selection, bulk select mode (checkboxes), and navigation into individual threads.
- **Workflows** (`/workflows/:id`) — Displays a YAML-defined workflow as a visual node diagram. Users click Run to execute it; step status is shown on each node (pending → running → success/failure). Output logs and result blocks appear below the diagram.
- **Apps** (`/apps/:id`) — Runs a YAML-configured Data App automatically on load. Renders a dashboard composed of: Markdown blocks, Data Tables, Line Charts, Bar Charts, Pie Charts, interactive Controls (select dropdowns, date pickers, toggles), and multi-column Row layouts. Controls inject values into SQL via Jinja `controls` context and trigger re-execution of dependent tasks. Results are cached by parameter hash; use `?refresh` to force re-execution.
- **Developer Portal / IDE** (`/ide`, `/ide/:filePath`) — Monaco-based code editor for all Oxy project files. A **Light / Dark / System** theme toggle in the user menu reflows the entire UI (surfaces, text contrast, status colors, charts, Monaco) without a page refresh; the selection is persisted across reloads and Light is the default surface. Sidebar sections:
  - **Files** — raw file tree (folders: `workflows/`, `agents/`, `example_sql/`, `generated/`, `modeling/`; root files: `config.yml`, etc.)
  - **Objects** — files grouped by type: Agents, Procedures (Workflows), Semantic Layer, Apps
  - **Database / SQL IDE** — Multi-tab SQL editor with schema browser, Cmd/Ctrl+Enter execution, database connection management, and Parquet-backed result tables with paging/sorting
  - **Modeling** — Browse dbt-style SQL projects (one per `modeling/<project>/` directory) powered by Airform. Run, test, seed, compile, analyze, and generate docs for dbt models; inspect streaming `NodeStarted`/`NodeCompleted` events; explore model-level and column-level lineage graphs. Supports Snowflake, BigQuery, and DuckDB.
  - **Pipelines** (`/ide/pipelines`) — Lists Airway ELT pipelines, embeds a per-pipeline monitor view (phase bar + per-resource grid + run history), and jumps straight to YAML edit. A 3-step **New Pipeline** card wizard walks through source → destination-from-config.yml → details, persisting credentials (e.g., Toast) to the secret manager.
  - **Settings** — Opens the Unified Settings Dialog (see below). Secrets panel within it always shows LLM API keys and scans `key_var` and credential vars from config.
  - **Observability** — Version badge with build metadata (commit hash, timestamp). Configurable observability backends (DuckDB, PostgreSQL, ClickHouse, or Airhouse) for storing tracing, intent-classification, and metric-usage data; a setup banner appears when none is configured.
  - A **Pull** button appears in the header when the active branch is behind the remote, allowing one-click sync without opening a terminal.
  - Supports open, edit, save (with unsaved-changes indicator), breadcrumb navigation, undo/redo, and Git workflow (branch protection, merge conflict resolution, branch-aware file operations). In local project mode, files can be saved directly on the main branch with deployment to a separate branch.
  - **Readonly mode** — `oxy serve --readonly` disables all file modifications via API (405 responses), reflected in UI.
- **Unified Settings Dialog** — A Notion-style modal that replaces the older split between the org settings modal and `/ide/settings/*` pages. Groups organization-level settings (members, billing, security) and workspace-level settings (data sources, secrets, observability, repo linking) under one grouped sidebar. In local mode, organization-only sections are hidden so the sidebar shows only what applies to a single-workspace deployment. Entry points are wired from the app sidebar footer, the IDE Settings tab, database sidebar "Add database" links, and home-page setup gaps.
- **Agent Testing** (`/tests`) — Test dashboard for managing and executing agent test suites:
  - **Test files** (`*.agent.test.yml`) with LLM-as-judge correctness evaluation
  - Run individual tests or all tests project-wide with tag filtering and accuracy thresholds
  - **Human verdicts** — reviewers submit Pass/Fail on individual test case results
  - Pass rate history, consistency metrics, and per-run detail views
- **Looker Explore** — Browse Looker data models from the Dev Portal semantic layer. Compile queries to SQL, browse dimensions/measures. Requires `oxy looker sync` (auto-triggered by `oxy build`).
- **Context Graph** (`/context-graph`) — Visual graph showing relationships between data objects (agents, tables, semantic views, workflows). Provides an overview of how project entities connect.
- **Organization Management** (multi-workspace mode) — Oxy supports multiple organizations, each with separate workspaces, members, and data sources. **Post-login onboarding** uses a single unified flow (`AgenticSetupPage`) across demo, GitHub, and blank workspace types, with up to four dismissible status rows on the home page (missing LLM key / warehouse credentials / no databases / no agents) replacing forced redirects back into the wizard. A **workspace dispatcher** automatically opens the most recently used workspace in the selected org after login. **Bulk invitations** allow owners to invite multiple members in a single action. Admins manage **per-seat Stripe billing** (self-serve checkout, billing portal for payment methods, invoices, subscriptions) directly from the settings dialog; admin tooling supports manual subscription resync and graduated tiered pricing.
- **Workspace Management** (multi-workspace mode) — Import repositories from GitHub, switch workspaces, invite members with role-based access (Owner/Admin/Member). Owner set via `OXY_OWNER` env var.
- **Sidebar** (persistent) — Navigation links (Home, Threads, Context Graph, Developer Portal), recent thread list, workflow shortcuts, and app shortcuts. In multi-workspace mode, the user menu contains an **organization switcher** showing the current org's member and workspace counts, and lets users switch between orgs or create new ones.

---

## Key Components / Concepts

- **Chat Panel** — The central Q&A widget on the Home page. Contains: question textarea, Agent Selector (dropdown of available agents), mode toggle (Ask / Build / Workflow), Submit button, and Stop button during streaming.
- **Agentic Agent** — The Oxy agent type, defined in `.agentic.yml` files that runs a multi-step reasoning pipeline (FSM-based) rather than a single LLM call. Two kinds:
  - **Analytics agent** — Clarify → specify metrics/dimensions → generate SQL → execute → interpret results. Supports extended thinking toggle, per-state model overrides, time-aware queries, and verified query badges for semantic layer queries.
  - **App builder agent** — Generates a complete `.app.yml` Data App from natural language description.
  - Both support **human-in-the-loop suspension**: the agent pauses mid-pipeline to ask the user a clarifying question, then resumes via `POST /analytics/runs/:id/answer`.
- **Builder Agent** — A copilot agent (Build mode in chat) that reads, modifies, and creates project files through an AI pipeline. Sends targeted line edits rather than full file replacements. Toggled with `Cmd+I`. Carries a `run_app` tool to execute a Data App by path and feed structured results back as context, a `lookup_reference` tool that fetches authoritative cards (semantic layer, app builder, agent builder, agentic builder) on demand only when needed, plus 15+ dbt-aware tools (`run_dbt_models`, `test_dbt_models`, `compile_dbt_model`, `get_dbt_lineage`, `init_dbt_project`, etc.) for scaffolding and managing dbt projects. `read_file` returns raw content (no line-number prefixes) capped at 100k characters. When a single response edits multiple files, the UI shows one sequential confirm/reject prompt per file; rejecting one file does not block the others. Auto-applied Builder edits emit `FileChanged` events (computed by comparing pre/post file contents) and can be reverted via `POST /runs/:id/revert-file-changes`. Builder/analytics pipeline failures surface structured user-friendly error messages; the SSE stream always emits a terminal event (`done`, `error`, or `cancelled`), even when the pipeline fails before the orchestrator loop starts.
- **Workspace Onboarding (Builder pipeline)** — The same Builder agent powers first-run workspace setup, generating semantic views, topics, and Data Apps in a multi-phase pipeline. Onboarding runs **end-to-end smoke tests** against each generated artifact (apps execute through the same path as a real dashboard load) and self-corrects on failure, so malformed SQL, broken JOINs, dialect mismatches, and type errors are caught during onboarding rather than surfacing as blank charts later. Semantic views use a **pattern-first, warehouse-aware** date-handling workflow (ClickHouse/BigQuery/Snowflake/Postgres/DuckDB each get their own cast), declare `type: foreign` entities for FK columns so dashboards show human-readable references, and the App phase **profiles topics** (rows, cardinality, coverage, stddev) to pick the highest-scoring one for the initial dashboard. Server restarts mid-onboarding resume against the same model and vendor.
- **Verified Queries** — Pre-written `.sql` files in the workspace are auto-discovered by the analytics agent and executed as-is when they match a user's question, bypassing LLM SQL generation entirely. Results are flagged as verified in the analytics pipeline and surface a **Verified** badge in the UI (also shown for semantic-layer queries).
- **SQL Modeling (Airform)** — dbt-compatible transformations native to Oxy via [Airform](https://github.com/oxy-hq/airform). Projects live under `modeling/<project>/` with an `oxy.yml` mapping dbt profile targets to Oxy database connections (Oxy validates dialect compatibility before running). Lifecycle commands (run/test/seed/compile/analyze/docs) are available from both the IDE Modeling area and Builder agent chat. Execution streams `NodeStarted`/`NodeCompleted` events over SSE.
- **Universal Slack Bot** — A single, shared multi-tenant Slack app that any organization can connect via OAuth — no per-customer app installations. Each Slack team links to one Oxy organization; users pick the workspace and agent per-thread via a Block Kit interface. Slack accounts are matched to Oxy users by email with magic-link fallback for unmatched users. Bot tokens are encrypted per-organization. SQL queries executed by agents render as native inline code blocks in Slack messages on successful runs (long queries are truncated with a link to the full thread). Chart images are uploaded directly to Slack via `files.uploadV2`, with optional S3 presigned URL fallback (`OXY_S3_PRESIGN_TTL_SECONDS`, `OXY_S3_PUBLIC_URL_BASE`) for CDN serving.
- **Airhouse Integration** — A first-class Oxy connector. Local-mode startup seeds the local organization, workspace, and user membership idempotently so provisioning works in development. The connector distinguishes DDL/DML (CREATE/INSERT/UPDATE) — executed directly — from SELECT-family queries — wrapped in subqueries. Server startup validates that all required Airhouse env vars are set together and fails fast if a subset is provided. Credentials follow an **ephemeral model**: workspaces mint short-lived credentials on demand from a service account (no per-user state, no rotation surface), and admins can revoke a specific credential via `DELETE /airhouse/me/tokens/:username`. Admin Airhouse operations distinguish auth failures, permission errors, and rate-limit responses with actionable error messages.
- **@Mention** — A cross-surface mention system available in the home Chat Panel (Ask and Build modes), the Builder Dialog and follow-up input, and inside agentic suspension answers. A shared highlight component renders orange tokens uniformly across inputs; backspace deletes a mention as a single atomic token; Escape reliably dismisses the suggestion popup.
- **Thread** — A persisted conversation (question + agent responses). Created when the user first submits from the Home page; accessible from the sidebar or Threads list.
- **Agent Message / Artifact** — Within a thread, agent responses contain free-text (`agent-response-text`) and structured artifacts (`agent-artifact`). The `execute_sql` artifact kind shows the SQL query the agent ran.
- **Procedure (formerly Workflow/Automation)** — A multi-step automation defined in `.procedure.yml` (also accepts `.workflow.yml` and `.automation.yml` for backward compatibility). Steps are visualized as diagram nodes with colored status borders (emerald = success). Supports step replay (re-execute from a specific step forward). Task types include SQL execution, `looker_query`, and more. Triggered from the procedure page (Run button), from chat (Workflow mode), or programmatically via the `/workflows` HTTP surface (start/list/cancel/inspect runs, browse workflow files). Workflow tasks support `cache_enabled` for hash-based step skipping and `retry_from_run_id` for resuming from a specific step forward, making long runs cheaper to iterate on after a partial failure. Step results are persisted as incremental JSONB deltas (O(steps) DB writes, not O(steps²)).
- **Data App** — A YAML-configured dashboard (`.app.yml`) with `tasks` (SQL queries) and `display` (visualization blocks). Runs automatically on page load; results cached by default. Interactive controls (select, date picker, toggle) inject values via Jinja and re-trigger dependent tasks on change. App builder agents can generate these from natural language. AI tools (`EditDataApp`, `ReadDataApp`) enable context-aware editing of existing app configurations during agent execution.
- **Data App Publish/Draft** — Each `.app.yml` carries a `published: bool` field (defaults to `false`) controlling sidebar visibility. The sidebar fetches only published apps; drafts remain editable from the IDE Objects view with a draft indicator. `POST /:projectId/apps/:pathb64/publish` and `.../unpublish` toggle the state; Owners, Admins, and Members can publish (Viewers cannot). The `list_apps` response includes a `can_publish` flag derived from the caller's workspace role so the UI can adapt actions per user.
- **Airway ELT Pipelines** — A queue-driven streaming ELT subsystem. Define a pipeline in `.airway.yml` by pointing a `source` (`rest_api`, `filesystem`, `sql_database`, `postgres_cdc`, `toast`) at a `destination` that references a database from `config.yml`; credentials stay in the secret manager and minijinja `variables` make pipelines configurable. Execution is streaming/concurrent (per-resource producers, bounded channels, per-batch commits) and partial failures are recorded as `completed_with_errors` audits rather than failing the whole run. Lifecycle is driven by a rich event taxonomy (`PipelinePlan`, `ExtractStarted/Progress`, `Normalize*`, `TableLoadStarted/LoadProgress/TableLoaded/TableLoadFailed`, `ResourceFailed`). Runnable from the Developer Portal Pipelines section or from the CLI via `oxy airway run`. Airhouse and `airhouse_managed` destinations broker credentials through the system-account token broker at runtime.
- **Looker Integration** — Full Looker platform integration: `oxy looker sync` fetches explore metadata, `looker_query` task type for procedures, `AutoLookerQuery` FSM trigger for agentic workflows, OAuth2 client with token management.
- **Semantic Layer** — Powered by [airlayer](https://github.com/oxy-hq/airlayer), an in-process Rust semantic engine. Schema defined in `.view.yml` / `.topic.yml` files. `airlayer` compiles these definitions into dialect-specific SQL with automatic join resolution, fan-out protection via CTEs, and multi-dialect support (Postgres, DuckDB, BigQuery, Snowflake, etc.). Time dimensions support configurable granularity (day, week, month, quarter, year) and relative time filters (e.g., "last 30 days", "this quarter"). Exposed to the `semantic` agent. Managed via the IDE's Objects → Semantic Layer group.
- **Developer Portal (IDE)** — Monaco editor + file browser + SQL IDE + Git workflow. Sidebar tabs: Files, Objects, Database, Settings, Observability. Save button appears only when there are unsaved changes. Supports readonly mode (`--readonly` flag). Git flow includes auto-init, protected main branch (edits auto-redirect to new branch), merge conflict resolution, and branch-aware file CRUD.
- **SQL IDE** — Multi-tab Monaco SQL editor within the Dev Portal. Schema browser, Cmd/Ctrl+Enter execution, Parquet-backed result tables with sorting/paging. Database connections managed centrally with manual refresh.
- **Authentication** — Magic link (passwordless) via AWS SES. Endpoints: `/auth/magic-link/request` and `/auth/magic-link/verify`. Domain restrictions configurable. Local dev mode writes HTML to temp file. Legacy password auth removed.
- **Agent Testing** — `*.agent.test.yml` files with LLM-as-judge evaluation. `oxy test` CLI with tag filtering and accuracy thresholds. Human verdicts for manual review. Project-wide test runs with pass rate tracking.
- **Context Graph** — Node/edge graph visualizing relationships between Oxy entities.
- **Streaming** — Agent responses are delivered as a server-sent event stream. A loading spinner shows while streaming; a Stop button cancels in-flight requests, which results in an "Operation cancelled" message. Agentic agents stream reasoning trace events with suspension support.
- **YAML Validation** — Strict validation with `deny_unknown_fields` on Config, Workflow, AppConfig, Semantics. `oxy validate --file` for single-file validation; `oxy validate` also recognizes `.agentic.yml` files, with IDE schema validation highlighting issues on save. Catches common typos like `steps:` vs `tasks:`.
- **Observability Backends** — Oxy can route tracing and performance data to a configurable backend: DuckDB (default for local), PostgreSQL, ClickHouse, or Airhouse. `OXY_OBSERVABILITY_BACKEND=airhouse` routes spans, intent classifications, intent clusters, and metric usage to an Airhouse instance over the pgwire protocol (tables namespaced with an `oxy_obs_` prefix). TLS is the default via `tokio-postgres-rustls`; `OXY_AIRHOUSE_OBS_INSECURE=true` opts into plaintext for localhost only. Env-var credentials (`OXY_AIRHOUSE_OBS_USER`, `OXY_AIRHOUSE_OBS_PASSWORD`, `OXY_AIRHOUSE_OBS_DATABASE`) are the configuration surface for Airhouse-observability. Configured via the IDE's Observability tab or server environment; a banner appears in the UI when the backend is not yet set up. The DuckDB backend runs periodic 5-minute WAL checkpoints, autocheckpoints at 32 MB, and explicitly checkpoints on graceful shutdown to bound WAL growth.
- **Oxygen Design System** — A three-layer design token system (raw palette → semantic tokens → component variables) drives the entire web app. Light, Dark, and System theme modes are available; Light is the default. The full Oxy-Blue palette (50–950) plus secondary hues (purple, pink, red, amber, orange, green, sky, indigo) are exposed as semantic tokens, with brand-blue chart ramps automatically inverted in dark mode for legibility. Inter ships bundled (no CDN fetch) with a typography scale: `.t-display`, `.t-h1`–`.t-h3`, `.t-body`, `.t-small`, `.t-button`, `.t-label`, `.t-code`.
- **`oxy init` Starter Project** — `oxy init` ships a focused single-story template (one agentic agent, the oxymart sales semantics, three example SQL queries, two procedures, one Data App) so new users land in a minimal, easy-to-explore workspace. The full multi-domain demo (training/fitness, airhouse astronauts, v0/builder) is a separate internal showcase package used only for the deployed capability demo.
- **CLI without a database** — `oxy run` no longer requires a database connection. Run history and checkpoints fall back to a no-op storage implementation when no database is configured, so workflows, agents, and SQL files can execute locally without setup.
- **Billing Feature Flag** — The backend `/auth/config` response exposes `billing_enabled` so the UI can stay in sync with deployment configuration. The "Billing" tab in organization settings is hidden when billing is disabled; the admin billing queue surfaces a clear "Billing is disabled" notice instead of an empty state.

---

## Common Bug Areas

- **Agent selector loading** — The agent selector button briefly shows empty text or "undefined" before agents load from the API. Tests guard against this but it can surface as a race condition in the UI.
- **Streaming cancellation** — The Stop button must cleanly cancel the SSE stream and surface the "Operation cancelled" message. Edge cases: stop mid-chunk, stop immediately after submit, or stop during follow-up.
- **Follow-up input re-enablement** — After streaming completes or is cancelled, the follow-up textarea should become enabled. It can remain disabled if the stream doesn't close cleanly.
- **Workflow node status borders** — Success/failure state is communicated via CSS border color on diagram nodes. Border color not updating is a common rendering bug when the event stream finishes out of order.
- **App auto-run** — Data Apps run immediately on load with no manual trigger. Failures here are silent if the task API call errors but the UI doesn't surface the error state.
- **IDE save button state** — The save button should appear only when there are unsaved changes and disappear after a successful save. It can get stuck visible or hidden due to editor state sync issues.
- **IDE Objects mode grouping** — Agents, Procedures, Semantic Layer, and Apps groups must all render correctly. Missing groups usually indicate a file-discovery or YAML-parse error in the backend.
- **Sidebar thread list staleness** — The sidebar shows recent threads fetched at load. After creating a new thread it may not appear in the list without a refresh.
- **Workflow mode in chat** — Requires selecting a workflow from the workflow selector dropdown before submitting. The selector and the title input have separate validation states that can desync.
- **Context Graph load** — The graph container and "Context Graph Overview" text are the key indicators of a successful load. The graph can silently fail to render if the backend entity graph is empty or malformed.
- **Agentic suspension/resume** — Human-in-the-loop suspension requires the frontend to detect `suspended` events and display an inline prompt. Resume via `/analytics/runs/:id/answer` must preserve all pipeline state. Edge cases: suspension during concurrent loop execution, double-submit of answer.
- **Chart rendering race condition** — Concurrent DuckDB initialization calls can return an uninitialized instance, causing chart render failures.
- **Builder multi-file creation** — Builder agent can fail when creating several files at once, most noticeable during onboarding flows.
- **Timestamp rendering** — Text with colons (e.g., `08:58 UTC`) can be mangled in rendered agent responses.
- **IDE Git branch operations** — Branch-aware file operations depend on a branch query parameter. Protected branch edits auto-redirect to a new branch — desync between UI branch state and API branch param causes silent failures.
- **Merge conflict resolution** — Conflicts must be reviewable and resolvable in-IDE. Incomplete resolution can leave branch in dirty state.
- **Test dashboard progress bar** — Stale state issue where "Run All" progress bar doesn't appear until page remount.
- **App result caching** — Cached results served by default; stale cache can show outdated data if underlying SQL or schema changed. Use `?refresh` to force re-execution.
- **Secrets panel variable discovery** — Panel scans `key_var` fields and database credential vars from config. Missing variables if config format changes or new variable patterns introduced.
- **SQL IDE error display** — Duplicate error notifications can appear in the SQL editor; errors may persist in the results panel if the dismiss action is unavailable or the editor state isn't reset between queries.
- **Environment variable startup validation** — Missing required environment variables cause a startup failure with an error message. Misconfigured deployments (e.g., missing GitHub App credentials in multi-workspace mode) may silently fall back to empty strings if validation is bypassed.
- **DuckDB file path validation** — The DuckDB connector only supports CSV and Parquet files. Stray files (e.g., `.DS_Store`) or unsupported formats in the data directory will break DuckDB initialization.
- **Snowflake column stats** — Numeric and date columns in Snowflake can fail during schema discovery with a type-casting error, causing the schema browser to show incomplete or no metadata for those columns.
- **Agentic agent validation errors** — Misconfigured `.agentic.yml` files can surface as generic error banners at runtime. Run `oxy validate` before deployment and check IDE schema hints to catch root-cause issues early.
- **Agentic runs in local mode** — Agentic analytics runs under `oxy serve --local` have historically had server-side errors; this deployment combination deserves explicit end-to-end testing after changes to the agentic pipeline.
- **App switching shows stale data** — Navigating between two apps in the same workspace can show "No data found" on every chart/table if the `AppPreview` component retains the previous app's task results. Verify display blocks reset between apps without a manual refresh.
- **DuckDB concurrent init** — Two concurrent `duckdb_database` handles opening the same file have caused SIGSEGV crashes. The DuckDB pool now serializes initialization, but any new code that opens DuckDB outside the pool must respect this constraint.
- **HITL events on batched writes** — `FileChangePending` events must fire for every file in a batched Builder write, only when the pipeline is actually suspended. Missed events silently skip approvals; spurious events cause stale UI state. Builder change IDs must be stable across re-edits so the frontend doesn't hold stale references.
- **Multi-org switching** — Clicking a different organization in the sidebar can race between URL and app state, silently failing or bouncing back. Onboarding state must be stored per-workspace (not in a shared browser key) and cleared on logout so it does not leak across workspaces or sessions.
- **Per-agent LLM key readiness** — The home page readiness check must resolve which LLM provider the *currently selected agent* uses and only flag missing keys for that provider. In local mode it must also read environment-variable secrets (e.g. `OPENAI_API_KEY` in `.env`); otherwise the chat panel gets disabled with a false "LLM key not set" warning. Cloud mode reads from the workspace secrets store, not server env vars.
- **LLM rate limits and connect failures** — HTTP 429 responses from Anthropic/OpenAI/OpenAI-compatible providers are retried with exponential backoff; if retries are exhausted, the run suspends with a clear rate-limit message. Transient connect/TLS timeouts to OpenAI/Azure OpenAI are also retried automatically with a 2-minute budget; permanent errors (`insufficient_quota`, `context_length_exceeded`, oversized input) fail fast. Regressions here typically surface as runs that die on a single transient failure.
- **Workspace deletion redirect** — Deleting the current workspace must redirect immediately by optimistically removing it from the cache and clearing the persisted last-workspace ID; otherwise the dispatcher routes back to the deleted workspace.
- **Onboarding completion summary** — Multi-topic onboarding produces multiple Data Apps (overview + topic dashboards). The "Workspace ready" summary must list every generated app file with one EXPLORE button per dashboard, not just the overview.
- **@Mention input edge cases** — Cursor placement after selecting a mention, atomic backspace deletion, Escape dismissal, highlight alignment during horizontal scroll on long lines, and mention popups inside agentic suspension answers (no stale cursor when navigating between suspended questions) are all known fragile interactions.
- **Slack chart publishing** — If S3/chart-renderer init fails (bad credentials, unreachable AWS endpoint), the bot must fall back to text-only responses and log a warning, not silently drop the user's workspace-picker submission. Private S3 buckets and SSE-KMS encryption require presigned URLs; misconfigured TTL or `OXY_S3_PUBLIC_URL_BASE` can produce broken/expired image links.
- **Slack block injection** — The webhook handler that re-posts source messages must strictly allowlist block kinds (`section`, `context`, `divider`, `header`, `image`) and drop interactive types (`actions`, `input`, `rich_text`, `video`) so attackers cannot inject buttons/inputs via crafted source messages.
- **DuckDB SQL injection** — User-provided strings embedded in DuckDB configuration SQL (S3 secrets, catalog schema names, storage paths) must be escaped against single-quote injection.
- **Secret cross-project leakage** — Secret lookups must always filter by project; a missing project filter can expose secrets across projects in multi-project deployments.
- **Azure OpenAI agentic compatibility** — Azure OpenAI deployments route through the OSS execution path and have a history of compatibility issues with the agentic pipeline. After changes to the agentic LLM layer, exercise an Azure OpenAI agent end-to-end.
- **Mobile dialog layering** — Settings and Manage Workspaces dialogs must be rendered at the app root and must close the navigation sidebar on mobile when opened; otherwise they fight the sidebar drawer for screen space and can appear behind it.
- **"Ghost" unclickable pages** — A Radix dropdown menu issue can leave the page unclickable after navigating away from an open menu. Pointer-events on the body must be cleared explicitly when menus close; regressions here surface as the entire page becoming inert.
- **Theme persistence and chart legibility in dark mode** — The Light / Dark / System toggle must persist across reloads and reflow surfaces, status colors, Monaco, and charts atomically. Brand-blue chart ramps are inverted automatically in dark mode for legibility — regressions appear as low-contrast text on dark backgrounds or charts that read as a single hue.
- **Builder onboarding artifact quality** — Generated semantic views and Data Apps are smoke-tested end-to-end during onboarding. Regressions where the smoke test is skipped, falsely passes, or fails to self-correct typically surface as blank charts, "No data found" panels, or dialect-mismatched SQL only after the user opens the dashboard. Topic profiling/scoring and FK-aware entity declaration are easy to silently break and should be verified after onboarding-pipeline changes.
- **Workspace onboarding resume after restart** — A server restart mid-onboarding must resume the Builder pipeline against the same model and vendor it was using before, even if `config.yml` hadn't been written yet. Otherwise the second run can pick a different provider and either re-prompt for keys or produce inconsistent artifacts.
- **`oxy run` with no database** — Run history and checkpoint storage have a no-op fallback when no database is configured. Code that assumes a real storage backend must check the runtime mode; otherwise local `oxy run` invocations error out instead of executing.
- **Billing UI feature-flag drift** — The UI's billing tab visibility depends on `/auth/config.billing_enabled`. If the backend flag and frontend state desync, admins either see a broken billing surface in deployments without billing, or no billing surface in deployments that do have it.
- **Postgres error fidelity** — Postgres connector errors must preserve SQLSTATE codes and the original server message; generic stringification loses diagnostic information that operators rely on when reading logs and error responses.
- **Data App publish/draft surfacing** — Sidebar must show only published apps (`published: true`); drafts must remain reachable from the IDE Objects view with a draft indicator. The `list_apps` `can_publish` flag is derived from the caller's workspace role — Viewers cannot publish — so UI actions must hide when missing. Regressions tend to surface as drafts leaking into team navigation or publish/unpublish silently no-oping for Viewer roles.
- **Airway pipeline reliability** — Partial extract/sink failures should be recorded as `completed_with_errors` audits, not full-run failures. Pipeline event taxonomy (`PipelinePlan` → `ExtractStarted/Progress` → `Normalize*` → `TableLoad*` / `ResourceFailed`) drives the IDE monitor view; missing or out-of-order events render an incorrect phase bar. Airhouse/`airhouse_managed` destinations must broker credentials through the system-account token broker at runtime, not pre-stage them.
- **Workflow SSE terminal events** — The Builder/analytics/workflow SSE stream must always emit a terminal event (`done`, `error`, or `cancelled`), including when the pipeline fails before the orchestrator loop starts (e.g., on a broken `.view.yml`). Missing terminal events cause the frontend to hang waiting forever.
- **Airhouse observability schema setup** — DuckLake does not implement indexes, so the Airhouse observability backend must not run `CREATE INDEX` during schema initialization. Regressions here cause spans/intent/metric capture to silently go inert after the first table.
- **ErrorBoundary coverage on third-party renderers** — SQL editors, diff/merge-conflict viewers, Monaco editors, workflow diagrams, the context graph, and Markdown answer content are individually wrapped with `ErrorBoundary` so a broken renderer on one panel does not blank the entire page. New panels embedding heavy third-party renderers should preserve this pattern.
- **Local-mode onboarding state isolation** — The onboarding wizard keys progress by a deployment-stable `storage_key` (`local:{hash(canonical_path)}`), not a shared `LOCAL_WORKSPACE_ID`. Running `oxy --local` in two project directories on the same dev port must not leak wizard state across them.

---

## Key API Endpoints (Agentic)

| Method | Path | Description |
| --- | --- | --- |
| `POST` | `/analytics/runs` | Start an analytics run for a given agent and question |
| `GET` | `/analytics/runs/:id/events` | SSE stream of live reasoning steps and results |
| `POST` | `/analytics/runs/:id/answer` | Resume a suspended run with a human answer |
| `GET` | `/analytics/threads/:thread_id/run` | Get run summary with status, answer, and UI event replay |
| `POST` | `/analytics/app-runs` | Start an app builder run |
| `GET` | `/analytics/app-runs/:id/events` | SSE stream of build steps and generated app |
| `POST` | `/analytics/app-runs/:id/answer` | Resume a suspended build with a human answer |
| `POST` | `/analytics/app-runs/:id/cancel` | Cancel a running or suspended build |

## Key API Endpoints (Agent Testing)

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/api/projects/:id/tests` | List all test files with case counts |
| `GET` | `/api/projects/:id/tests/:pathb64` | Resolve a specific test file config |
| `POST` | `/api/projects/:id/tests/:pathb64/run` | Run a test file; stream events via SSE and persist results |
| `GET` | `/api/projects/:id/test-runs` | List test runs for a file |
| `GET` | `/api/projects/:id/test-runs/:runId` | Detailed case results for a run |
| `POST` | `/api/projects/:id/test-runs/:runId/cases/:caseIndex/human-verdict` | Set or update a human verdict on a test case |
| `GET` | `/api/projects/:id/test-project-runs` | List project-level runs |
| `POST` | `/api/projects/:id/test-project-runs` | Run all test files as a project run; stream events via SSE |
| `DELETE` | `/api/projects/:id/test-project-runs/:runId` | Delete a project run |

## Key API Endpoints (Airhouse)

| Method | Path | Description |
| --- | --- | --- |
| `DELETE` | `/airhouse/me/tokens/:username` | Revoke a specific ephemeral Airhouse credential (replaces the previous password-rotation flow) |

## Key API Endpoints (Data Apps Publishing)

| Method | Path | Description |
| --- | --- | --- |
| `POST` | `/:projectId/apps/:pathb64/publish` | Publish a Data App so it surfaces in the workspace sidebar |
| `POST` | `/:projectId/apps/:pathb64/unpublish` | Move a Data App back to draft state (hidden from sidebar) |

## Key API Endpoints (Builder Runs)

| Method | Path | Description |
| --- | --- | --- |
| `POST` | `/runs/:id/revert-file-changes` | Revert one or more files changed by the Builder during a run |

## Key API Endpoints (Workflows)

| Method | Path | Description |
| --- | --- | --- |
| (various) | `/workflows` | Start, list, cancel, and inspect workflow runs; browse workflow files programmatically. The SSE event stream recognises workflow terminal events for proper client-side cleanup. |

## Key File Extensions

| Extension | Type | Description |
| --- | --- | --- |
| `.agentic.yml` | Agentic Agent | Multi-step FSM pipeline (analytics or app builder) |
| `.procedure.yml` | Procedure | Multi-step automation (also `.workflow.yml`, `.automation.yml`) |
| `.app.yml` | Data App | Dashboard with tasks and display blocks |
| `.view.yml` | Semantic View | Semantic layer entity definition |
| `.topic.yml` | Semantic Topic | Semantic layer topic definition |
| `.sql` | Verified Query | Pre-written SQL auto-discovered by the analytics agent; executed as-is when matched to a question, surfaced as a Verified result |
| `oxy.yml` (under `modeling/<project>/`) | Modeling Project Config | Maps dbt profile targets to Oxy database connections for an Airform-managed dbt project |
| `.airway.yml` | Airway ELT Pipeline | Streaming ELT pipeline spec (source + destination + minijinja variables); never holds credentials directly |

---

### Usage rules examples

- The web app ships on a **three-layer design token system** (raw palette → semantic tokens → component variables) with **Light, Dark, and System** theme modes; Light is the default surface, with Dark mirroring the same ladder using brighter hues for visibility on dark surfaces.
- Use semantic tokens (`--primary` / `text-primary` / `bg-primary`) for interactive brand elements rather than raw hex values; semantic tokens automatically swap between Light and Dark.
- For gradient buttons: `from-[#3550FF] to-[#2A40CC]` with `hover:from-[#5D73FF] hover:to-[#3550FF]` (brighter hover for dark-mode visibility).
- Avoid Blue-600–900 for text on dark backgrounds (insufficient contrast); status hues and the Oxy-Blue ladder are tuned for AA contrast on both Light and Dark surfaces.
- Brand-blue chart ramps are inverted automatically in dark mode — do not hardcode chart colors.
- Use the **typography scale** (`.t-display`, `.t-h1`–`.t-h3`, `.t-body`, `.t-small`, `.t-button`, `.t-label`, `.t-code`) backed by bundled **Inter** rather than ad-hoc font sizing.
- Git action buttons: **brand blue** for Commit & Push and Open PR (both use the same Blue-500→600 gradient), **amber** for conflicts. Emerald is reserved for workflow node success indicators only.
