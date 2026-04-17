# Product Context

This file is read by the bot at startup and injected into every Claude API call.
Fill it in to help Claude ask smarter clarifying questions and correctly identify
which component a bug affects. The more specific, the better.

---

## Product Overview

Oxy is an AI-powered data analytics platform that lets teams query databases,
build automated reports, and visualize data through natural language. Users
connect data sources (DuckDB, PostgreSQL, BigQuery, Snowflake, ClickHouse,
warehouses, semantic layer, Looker), ask questions in a chat interface where
AI agents generate and execute SQL, then view streamed results. Teams can also
define reusable multi-step Procedures (YAML-based automation, formerly called
Workflows/Automations), build configuration-driven Data Apps (dashboards with
charts, tables, and interactive controls), and directly edit all project files
in a built-in Developer Portal IDE with SQL IDE and Git integration.

Oxy supports three deployment modes:
- **Local** (`oxy start`) — PostgreSQL auto-managed in Docker, single workspace
- **Remote** (`oxy serve --local`) — Single fixed workspace on a VM/container, embedded PostgreSQL
- **Cloud / Multi-workspace** (`oxy serve`) — Multi-tenant platform with GitHub-based workspace import, role-based access control, and magic link authentication

---

## Main Pages / Features

- **Home** (`/`) — Primary chat panel. Users type a question, select an AI agent, pick a mode (Ask / Build / Workflow), and submit. The agent streams back responses with SQL artifacts and formatted answers. Starting a conversation creates a new Thread and redirects to it.
- **Thread Detail** (`/threads/:id`) — Shows the full conversation history for a thread, renders agent messages with artifacts (SQL blocks, tables, charts), and provides a follow-up input for continuing the conversation. Streaming can be cancelled with a Stop button.
- **Threads** (`/threads`) — Paginated list of all past conversation threads. Supports item-per-page selection, bulk select mode (checkboxes), and navigation into individual threads.
- **Workflows** (`/workflows/:id`) — Displays a YAML-defined workflow as a visual node diagram. Users click Run to execute it; step status is shown on each node (pending → running → success/failure). Output logs and result blocks appear below the diagram.
- **Apps** (`/apps/:id`) — Runs a YAML-configured Data App automatically on load. Renders a dashboard composed of: Markdown blocks, Data Tables, Line Charts, Bar Charts, Pie Charts, interactive Controls (select dropdowns, date pickers, toggles), and multi-column Row layouts. Controls inject values into SQL via Jinja `controls` context and trigger re-execution of dependent tasks. Results are cached by parameter hash; use `?refresh` to force re-execution.
- **Developer Portal / IDE** (`/ide`, `/ide/:filePath`) — Monaco-based code editor for all Oxy project files. Sidebar sections:
  - **Files** — raw file tree (folders: `workflows/`, `agents/`, `example_sql/`, `generated/`; root files: `config.yml`, `semantics.yml`, etc.)
  - **Objects** — files grouped by type: Agents, Procedures (Workflows), Semantic Layer, Apps
  - **Database / SQL IDE** — Multi-tab SQL editor with schema browser, Cmd/Ctrl+Enter execution, database connection management, and Parquet-backed result tables with paging/sorting
  - **Settings** — Secrets panel (LLM API keys always visible, scans `key_var` and credential vars from config)
  - **Observability** — Version badge with build metadata (commit hash, timestamp)
  - Supports open, edit, save (with unsaved-changes indicator), breadcrumb navigation, undo/redo, and Git workflow (branch protection, merge conflict resolution, branch-aware file operations).
  - **Readonly mode** — `oxy serve --readonly` disables all file modifications via API (405 responses), reflected in UI.
- **Agent Testing** (`/tests`) — Test dashboard for managing and executing agent test suites:
  - **Test files** (`*.agent.test.yml` / `*.aw.test.yml`) with LLM-as-judge correctness evaluation
  - Run individual tests or all tests project-wide with tag filtering and accuracy thresholds
  - **Human verdicts** — reviewers submit Pass/Fail on individual test case results
  - Pass rate history, consistency metrics, and per-run detail views
- **Looker Explore** — Browse Looker data models from the Dev Portal semantic layer. Compile queries to SQL, browse dimensions/measures. Requires `oxy looker sync` (auto-triggered by `oxy build`).
- **Context Graph** (`/context-graph`) — Visual graph showing relationships between data objects (agents, tables, semantic views, workflows). Provides an overview of how project entities connect.
- **Workspace Management** (multi-workspace mode) — Import repositories from GitHub, switch workspaces, invite members with role-based access (Owner/Admin/Member). Owner set via `OXY_OWNER` env var.
- **Sidebar** (persistent) — Navigation links (Home, Threads, Context Graph, Developer Portal), recent thread list, workflow shortcuts, and app shortcuts.

---

## Key Components / Concepts

- **Chat Panel** — The central Q&A widget on the Home page. Contains: question textarea, Agent Selector (dropdown of available agents), mode toggle (Ask / Build / Workflow), Submit button, and Stop button during streaming.
- **Agent (classic)** — A named AI assistant defined by a `.agent.yml` file. Agents have tools (primarily `execute_sql`), a system prompt, and a target model. Built-in agents include `duckdb`, `_routing`, and optionally `semantic`.
- **Agentic Agent** — A newer agent type defined in `.agentic.yml` files that runs a multi-step reasoning pipeline (FSM-based) rather than a single LLM call. Two kinds:
  - **Analytics agent** — Clarify → specify metrics/dimensions → generate SQL → execute → interpret results. Supports extended thinking toggle, per-state model overrides, time-aware queries, and verified query badges for semantic layer queries.
  - **App builder agent** — Generates a complete `.app.yml` Data App from natural language description.
  - Both support **human-in-the-loop suspension**: the agent pauses mid-pipeline to ask the user a clarifying question, then resumes via `POST /analytics/runs/:id/answer`.
- **Builder Agent** — A copilot agent (Build mode in chat) that reads, modifies, and creates project files through an AI pipeline. Sends targeted line edits rather than full file replacements. Toggled with `Cmd+I`.
- **Thread** — A persisted conversation (question + agent responses). Created when the user first submits from the Home page; accessible from the sidebar or Threads list.
- **Agent Message / Artifact** — Within a thread, agent responses contain free-text (`agent-response-text`) and structured artifacts (`agent-artifact`). The `execute_sql` artifact kind shows the SQL query the agent ran.
- **Procedure (formerly Workflow/Automation)** — A multi-step automation defined in `.procedure.yml` (also accepts `.workflow.yml` and `.automation.yml` for backward compatibility). Steps are visualized as diagram nodes with colored status borders (emerald = success). Supports step replay (re-execute from a specific step forward). Task types include SQL execution, `looker_query`, and more. Triggered from the procedure page (Run button) or from chat (Workflow mode).
- **Data App** — A YAML-configured dashboard (`.app.yml`) with `tasks` (SQL queries) and `display` (visualization blocks). Runs automatically on page load; results cached by default. Interactive controls (select, date picker, toggle) inject values via Jinja and re-trigger dependent tasks on change. App builder agents can generate these from natural language.
- **Looker Integration** — Full Looker platform integration: `oxy looker sync` fetches explore metadata, `looker_query` task type for procedures, `AutoLookerQuery` FSM trigger for agentic workflows, OAuth2 client with token management.
- **Semantic Layer** — Powered by [airlayer](https://github.com/oxy-hq/airlayer), an in-process Rust semantic engine. Schema defined in `.view.yml` / `.topic.yml` files. `airlayer` compiles these definitions into dialect-specific SQL with automatic join resolution, fan-out protection via CTEs, and multi-dialect support (Postgres, DuckDB, BigQuery, Snowflake, etc.). Exposed to the `semantic` agent. Managed via the IDE's Objects → Semantic Layer group.
- **Developer Portal (IDE)** — Monaco editor + file browser + SQL IDE + Git workflow. Sidebar tabs: Files, Objects, Database, Settings, Observability. Save button appears only when there are unsaved changes. Supports readonly mode (`--readonly` flag). Git flow includes auto-init, protected main branch (edits auto-redirect to new branch), merge conflict resolution, and branch-aware file CRUD.
- **SQL IDE** — Multi-tab Monaco SQL editor within the Dev Portal. Schema browser, Cmd/Ctrl+Enter execution, Parquet-backed result tables with sorting/paging. Database connections managed centrally with manual refresh.
- **Authentication** — Magic link (passwordless) via AWS SES. Endpoints: `/auth/magic-link/request` and `/auth/magic-link/verify`. Domain restrictions configurable. Local dev mode writes HTML to temp file. Legacy password auth removed.
- **Agent Testing** — `*.agent.test.yml` / `*.aw.test.yml` files with LLM-as-judge evaluation. `oxy test` CLI with tag filtering and accuracy thresholds. Human verdicts for manual review. Project-wide test runs with pass rate tracking.
- **Context Graph** — Node/edge graph visualizing relationships between Oxy entities.
- **Streaming** — Agent responses are delivered as a server-sent event stream. A loading spinner shows while streaming; a Stop button cancels in-flight requests, which results in an "Operation cancelled" message. Agentic agents stream reasoning trace events with suspension support.
- **YAML Validation** — Strict validation with `deny_unknown_fields` on Config, Workflow, AppConfig, Semantics. `oxy validate --file` for single-file validation. Catches common typos like `steps:` vs `tasks:`.

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

## Key File Extensions

| Extension | Type | Description |
| --- | --- | --- |
| `.agent.yml` | Classic Agent | Single LLM call with tools |
| `.agentic.yml` | Agentic Agent | Multi-step FSM pipeline (analytics or app builder) |
| `.procedure.yml` | Procedure | Multi-step automation (also `.workflow.yml`, `.automation.yml`) |
| `.app.yml` | Data App | Dashboard with tasks and display blocks |
| `.view.yml` | Semantic View | Semantic layer entity definition |
| `.topic.yml` | Semantic Topic | Semantic layer topic definition |
| `.agent.test.yml` | Agent Test | Test suite for classic agents |
| `.aw.test.yml` | Agentic Test | Test suite for agentic workflows |

---

### Usage rules examples

- Use `--primary` / `text-primary` / `bg-primary` for interactive brand elements.
- For gradient buttons: `from-[#3550FF] to-[#2A40CC]` with `hover:from-[#5D73FF] hover:to-[#3550FF]` (brighter hover for dark-mode visibility).
- Avoid Blue-600–900 for text on dark backgrounds (insufficient contrast).
- Git action buttons: **brand blue** for Commit & Push and Open PR (both use the same Blue-500→600 gradient), **amber** for conflicts. Emerald is reserved for workflow node success indicators only.
