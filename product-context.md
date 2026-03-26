# Product Context

This file is read by the bot at startup and injected into every Claude API call.
Fill it in to help Claude ask smarter clarifying questions and correctly identify
which component a bug affects. The more specific, the better.

---

## Product Overview

Oxy is an AI-powered data analytics platform that lets teams query databases,
build automated reports, and visualize data through natural language. Users
connect data sources (DuckDB, PostgreSQL, warehouses, semantic layer), ask
questions in a chat interface where AI agents generate and execute SQL, then
view streamed results. Teams can also define reusable multi-step Workflows
(YAML-based automation procedures), build configuration-driven Data Apps
(dashboards with charts and tables), and directly edit all project files in
a built-in Developer Portal IDE.

---

## Main Pages / Features

- **Home** (`/`) — Primary chat panel. Users type a question, select an AI agent, pick a mode (Ask / Build / Workflow), and submit. The agent streams back responses with SQL artifacts and formatted answers. Starting a conversation creates a new Thread and redirects to it.
- **Thread Detail** (`/threads/:id`) — Shows the full conversation history for a thread, renders agent messages with artifacts (SQL blocks, tables, charts), and provides a follow-up input for continuing the conversation. Streaming can be cancelled with a Stop button.
- **Threads** (`/threads`) — Paginated list of all past conversation threads. Supports item-per-page selection, bulk select mode (checkboxes), and navigation into individual threads.
- **Workflows** (`/workflows/:id`) — Displays a YAML-defined workflow as a visual node diagram. Users click Run to execute it; step status is shown on each node (pending → running → success/failure). Output logs and result blocks appear below the diagram.
- **Apps** (`/apps/:id`) — Runs a YAML-configured Data App automatically on load. Renders a dashboard composed of: Markdown blocks, Data Tables, Line Charts, Bar Charts, Pie Charts, interactive Controls (select dropdowns, date pickers, toggles), and multi-column Row layouts. Controls trigger re-execution of dependent SQL tasks.
- **Developer Portal / IDE** (`/ide`, `/ide/:filePath`) — Monaco-based code editor for all Oxy project files. Sidebar has two modes:
  - **Files** — raw file tree (folders: `workflows/`, `agents/`, `example_sql/`, `generated/`; root files: `config.yml`, `semantics.yml`, etc.)
  - **Objects** — files grouped by type: Agents, Procedures (Workflows), Semantic Layer, Apps
  - Supports open, edit, save (with unsaved-changes indicator), breadcrumb navigation, and undo/redo.
- **Context Graph** (`/context-graph`) — Visual graph showing relationships between data objects (agents, tables, semantic views, workflows). Provides an overview of how project entities connect.
- **Sidebar** (persistent) — Navigation links (Home, Threads, Context Graph, Developer Portal), recent thread list, workflow shortcuts, and app shortcuts.

---

## Key Components / Concepts

- **Chat Panel** — The central Q&A widget on the Home page. Contains: question textarea, Agent Selector (dropdown of available agents), mode toggle (Ask / Build / Workflow), Submit button, and Stop button during streaming.
- **Agent** — A named AI assistant defined by a `.agent.yml` file. Agents have tools (primarily `execute_sql`), a system prompt, and a target model. Built-in agents include `duckdb`, `_routing`, and optionally `semantic`.
- **Thread** — A persisted conversation (question + agent responses). Created when the user first submits from the Home page; accessible from the sidebar or Threads list.
- **Agent Message / Artifact** — Within a thread, agent responses contain free-text (`agent-response-text`) and structured artifacts (`agent-artifact`). The `execute_sql` artifact kind shows the SQL query the agent ran.
- **Workflow** — A multi-step automation procedure defined in `.workflow.yml`. Steps are visualized as diagram nodes with colored status borders (emerald = success). Triggered from the workflow page (Run button) or from chat (Workflow mode).
- **Data App** — A YAML-configured dashboard (`.app.yml`) with `tasks` (SQL queries) and `display` (visualization blocks). Runs automatically on page load; controls re-trigger tasks on change.
- **Semantic Layer** — CubeJS-powered schema defined in `.view.yml` / `.topic.yml` files, exposed to the `semantic` agent. Managed via the IDE's Objects → Semantic Layer group.
- **Developer Portal (IDE)** — Monaco editor + file browser. Two sidebar tabs: Files (raw tree) and Objects (type-grouped). Save button appears only when there are unsaved changes.
- **Context Graph** — Node/edge graph visualizing relationships between Oxy entities.
- **Streaming** — Agent responses are delivered as a server-sent event stream. A loading spinner shows while streaming; a Stop button cancels in-flight requests, which results in an "🔴 Operation cancelled" message.

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

---

## Design System — Brand Color Scale

The primary color is **Blue-500 `#3550FF`**. CSS variable `--primary` is set to this value in both light and dark mode. Use Tailwind's `text-primary`, `bg-primary`, `border-primary` etc. wherever possible.

| Token | Hex | Usage |
| --- | --- | --- |
| Blue-100 | `#D7DCFF` | Highlights, subtle backgrounds |
| Blue-200 | `#AEB9FF` | Emphasis, tinted surfaces |
| Blue-300 | `#8696FF` | Secondary emphasis |
| Blue-400 | `#5D73FF` | Hover states in dark mode (brighter = more visible) |
| **Blue-500** | **`#3550FF`** | **Primary — all key actions, `--primary` variable** |
| Blue-600 | `#2A40CC` | Gradient bottom stop, hover in light mode |
| Blue-700 | `#203099` | Deep shadows, pressed states |
| Blue-800 | `#152066` | Very deep accents |
| Blue-900 | `#0B1033` | Shadow color (`shadow-[#0B1033]/40`) |

### Usage rules examples

- Use `--primary` / `text-primary` / `bg-primary` for interactive brand elements.
- For gradient buttons: `from-[#3550FF] to-[#2A40CC]` with `hover:from-[#5D73FF] hover:to-[#3550FF]` (brighter hover for dark-mode visibility).
- Avoid Blue-600–900 for text on dark backgrounds (insufficient contrast).
- Git action buttons: **brand blue** for Commit & Push and Open PR (both use the same Blue-500→600 gradient), **amber** for conflicts. Emerald is reserved for workflow node success indicators only.
