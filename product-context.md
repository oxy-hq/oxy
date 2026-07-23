# Product Context

This file is injected into every Claude API call. Its only job is to clarify
things an agent **cannot** cheaply retrieve by reading the code — renamed
concepts, confusing distinctions between similar features, product intent for
bug triage, and a few counterintuitive gotchas. It is **not** a feature catalog
or a changelog. If a fact is obvious, or you'd find it in a minute by reading
the relevant module, it does not belong here.

---

## Orientation

Oxy (user-facing brand: **Oxygen**) is the operating system for AI
transformation: connect a warehouse, ask questions in chat where agents
generate and run SQL, and get streamed results. Teams also build **Procedures** (YAML automation), **Data
Apps** (YAML dashboards), dbt-style transforms via **Airform**, and edit project
files in the **Developer Portal IDE**.

Deployment modes — almost every bug report depends on which one:
- **Cloud / enterprise** (`oxy serve`, `oxy serve --enterprise`, or `oxy start` for a Docker-Postgres dev box) — the real, maintained path: multi-tenant, multi-org, GitHub import, RBAC, magic-link auth, Stripe billing (`ServeMode::Cloud`). **"Run it locally" means this** — local development runs cloud/enterprise mode, not the legacy mode below. So dev-vs-prod is *not* distinguishable by mode; a dev box is cloud mode with non-prod secrets (e.g. no working SES → set `OXY_APP_EMAIL_LOCAL_TEST=1` to preview email).
- **Legacy single-project** (`oxy serve --local`, `ServeMode::Local`) — one fixed workspace, embedded PostgreSQL, **no auth**. **Not actively used or maintained** — don't design new behavior around it; when in doubt, assume cloud/enterprise.

---

## Terminology & renames (easy to get wrong)

- **Automation** = the thing formerly called **Procedure / Workflow**. Canonical file is now `.automation.yml`; `.procedure.yml` is kept for back-compat. The `.workflow.yml` file extension is **no longer supported** (the runtime no longer recognizes it as a file kind — only `oxy migrate-automations` still reads legacy `.workflow.yml` files to rename them). Canonical UI route is `/automations/:id` (legacy `/workflows/:id` still renders); canonical HTTP surface is `/automations` + `/agentic-automations` (legacy `/procedures` + `/agentic-workflows` kept as aliases). Internally the Rust types are still named `Workflow*`/`Procedure*` (the `type: workflow` YAML task discriminator and the `agentic_workflow_state` table are wire/storage contracts) with canonical `Automation*` aliases. The DB table is `automation_definitions` (a `procedure_definitions` view remains for back-compat).
- **Orchestrator Dashboard** replaced the old **Coordinator** surface.
- **Oxygen Factory** = the Developer Portal / IDE, reached from the icon rail (renamed Studio → Oxygen Builder → Oxygen Core → **Oxygen Factory**). Same `/ide` surface, just the rail label.
- **Agentic Agent** (`.agentic.yml`) = Oxy's multi-step FSM agent (two kinds: **analytics** and **app builder**), distinct from the single-shot sense of "agent."
- **Builder Agent** = the file-editing copilot (chat **Build** mode) — distinct from the *app builder* agentic agent.
- **Custom Apps Platform** (code-first React+Vite bundles, shipped with `oxy publish`) is **not** the same thing as YAML **Data Apps** (`.app.yml` dashboards).
- **Oxy Functions** = server-side TypeScript handlers bundled *inside* a Custom App (declared in `oxy-app.json`, versioned and shipped with the frontend by `oxy publish`) that run on Oxy's managed runtime with data-plane access — **not** a YAML Automation task or Data App `task`. A function runs as an HTTP route (`useFunction` hook), a cron job on the durable task queue, or an Airway transform step; writing back to Oxy Secrets via `ctx.secrets.set` requires the `secrets.write` capability, and sending email via `ctx.email.send` (AWS SES under the hood; the **platform** controls the `from` address, the function sets `replyTo` only) requires the `email.send` capability. Email templates are **preact** components rendered to HTML by `@oxy-hq/sdk/email`'s `render(Component, props)` — React Email / react-dom can't run in the Functions isolate (node:stream / Web Streams).
- Four similarly-named third-party engines that are easy to confuse: **airlayer** (semantic layer), **Airform** (dbt-style modeling), **Airway** (ELT), **Airhouse** (a warehouse + connector).
- **Verified Query** = a plain `.sql` file the analytics agent runs *as-is* when it matches the question (bypassing LLM SQL generation); surfaces a **Verified** badge.
- **Two subdomain schemes, easy to confuse** — an **org subdomain** (`<org-slug>.oxygen-hq.com`) boots the whole product pre-scoped to that org's default project (skips the org/workspace picker), serving its custom apps under `/a/<slug>/`; a **custom-app subdomain** (`<org>--<slug>.customer-apps.oxygen-hq.com`) serves a single externally-hosted custom app at its own root.

---

## Roles (same word means different things at two levels)

Oxy separates **platform-level** "Global …" roles from **per-org** roles.
- **Global Owner** (`OXY_OWNER` env allow-list → `is_owner`) — Oxy staff; reaches everything, incl. the Billing queue and Global-admin management.
- **Global Admin** (`app_admins` table, seeded by `OXY_GLOBAL_ADMINS`; legacy `OXY_APP_ADMINS` still accepted → `is_app_admin`) — Oxy ops; reaches most of admin + every custom app, but **not** Billing or Global-admin management.
- **Org Owner / Admin / Member** (`role` in `org_members`) — tenant-internal only, **no** platform reach. Workspace role derives via `EffectiveWorkspaceRole`.

---

## Surfaces (for "which component is this?" triage)

- **Home / HQ launcher** (`/`, `/home`) — apps-first landing (org-branded **HQ**, a grid of **custom-app cards**). **Ask Oxygen** (⌘K) opens a resizable right-side **drawer** that compacts the page beside it (not a floating overlay); "Full view" promotes to `/threads/:id`. Chrome is a **universal top bar** plus an **icon rail** (Home · **Chat** · Apps), no left sidebar; the rail's **Chat** entry was formerly "Threads".
- **Thread** (`/threads/:id`) — conversation; messages carry free-text plus structured artifacts (the `execute_sql` artifact shows the SQL the agent ran).
- **Workflows** (`/workflows/:id`) — a Procedure as a node diagram (node border color = step status).
- **Apps** (`/apps/:id`) — a Data App; auto-runs on load; Controls inject Jinja values and re-run dependent tasks; results cached by parameter hash (`?refresh` forces re-run).
- **Developer Portal / IDE** (`/ide`) — Monaco editor; sidebar tabs **Files / Objects / Database (SQL IDE) / Modeling / Pipelines / Observability**. Git flow: protected `main` auto-redirects edits to a new branch; `oxy serve --readonly` makes all writes 405.
- **Orchestrator Dashboard** — Overview / Jobs / Runs across workflows, ELT, and agents.
- **Unified Settings Dialog** — one modal for org-level + workspace-level settings, incl. **Schedules** (cron builder targeting workflows / Airway pipelines / agents).
- **World Model Graph** (Globe icon in the rail) — interactive map of the semantic layer (entities, their measures, and how measures promote across the entity hierarchy), driven by a `.world-model.yml` display config. Distinct from the **Context Graph** and the **Metric Tree** — three different graph surfaces.

---

## Components worth clarifying

- **Agentic Agent** — multi-step FSM pipeline, not one LLM call. Supports **human-in-the-loop suspension**: pauses to ask a clarifying question, resumes via `POST /analytics/runs/:id/answer`. Per-state model overrides; extended-thinking toggle.
- **Builder Agent** — sends **targeted line edits**, not full-file rewrites; `Cmd+I` toggles it; multi-file edits prompt confirm/reject **per file**; `read_file` returns raw content capped at 100k chars. Also powers first-run **Workspace Onboarding** (generates views/topics/apps, smoke-tests each end-to-end, self-corrects, and resumes against the same model/vendor after a restart).
- **Semantic Layer** — `.view.yml` / `.topic.yml` compiled by **airlayer** to dialect-specific SQL with automatic join resolution and fan-out protection. Queries can be served from local **pre-aggregation** Parquet rollups instead of the warehouse; a **Pre-aggregated** badge means the result came from a rollup (stale data under that badge is a freshness bug).
- **Airway ELT** (`.airway.yml`) — a `source` → a `destination` defined in `config.yml`; credentials live in the secret manager, never in the YAML. Partial failures are recorded as `completed_with_errors`, not full-run failures.
- **Universal Slack Bot** — one shared multi-tenant app (OAuth; no per-customer installs); Slack users matched to Oxy users by email; bot tokens encrypted per-org.
- **Airhouse** — first-class connector with **ephemeral** credentials (workspaces mint short-lived creds from a service account; no rotation surface).
- **Metric Tree & Anomaly Monitoring** — a `.monitor.yml` defines monitors that watch a measure over time (per-segment via `filters`/`group_by`); detected anomalies land in the **Insights Inbox** with AI root-cause, and the analytics agent can list/run/explain them in chat. The **Metric Tree** (a Semantic Layer IDE tab consolidating the semantic explorer) decomposes a top-line metric into driver metrics. Scans exclude the current *incomplete* period, so a partial day/week never reads as a false drop.
- **Authentication** — magic-link only (passwordless, AWS SES); legacy password auth removed.
- **Design system** — Light default (Light / Dark / System). Use semantic tokens, not raw hex; **emerald is reserved for workflow-node success only**.

---

## Counterintuitive gotchas (high-cost, hard to guess)

- **DuckDB concurrent init** — two handles opening the same file concurrently have caused SIGSEGV; the pool serializes init, so code opening DuckDB outside the pool must respect that.
- **DuckLake has no indexes** — the Airhouse observability backend must not `CREATE INDEX`, or capture silently goes inert after the first table.
- **Empty-result warehouse queries** can panic in the shared Arrow bridge (DuckDB / Snowflake / MotherDuck / connectorx); each path must short-circuit its empty shape. Oversized results are capped by a cross-connector memory backstop, and unbounded semantic/SQL-IDE queries default to 10k rows — both flag the result **truncated**, so "missing rows" may be a silent cap, not a query bug.
- **Semantic file discovery** must skip hidden/build dirs (`.worktrees`, `.git`, `.oxy_state`, `target`, `node_modules`, `dist`, `build`); stray copies there trigger spurious "duplicate view name" errors.
- **Two distinct worker concepts, easy to conflate** — the *durable task fleet* (runs in-process by default; can run standalone via `oxy worker`, disabled with `oxy serve --no-workers` / `OXY_DISABLE_INPROCESS_WORKERS`) executes queued `TaskSpec` jobs; the *global singleton worker* (`OXY_INPROC_GLOBAL_WORKER`) drives schedules, monitor scans, and pre-aggregation. Toggling one does not affect the other.
- **Scheduled/monitor firing is gated behind `OXY_INPROC_GLOBAL_WORKER`** — with it off, schedule CRUD works but nothing actually fires. Multi-replica safety relies on a CAS `next_run_at` claim (no double-fire; missed runs collapse to a single execution); cross-process cancel uses a polled `cancel_requested_at` flag.
- **The Builder/analytics/workflow SSE stream must always emit a terminal event** (`done` / `error` / `cancelled`) — even when it fails before the orchestrator loop starts (e.g. a broken `.view.yml`) — or the frontend hangs forever.
- **Multi-tenant scoping is a correctness invariant**: orchestrator and pre-aggregation endpoints must filter by `workspace_id`, and secret lookups must filter by project, or runs/secrets leak across tenants.
- **Mode-dependent LLM-key check** — the home readiness check reads env-var secrets in local mode (e.g. `OPENAI_API_KEY` in `.env`) but the workspace secrets store in cloud mode, and only for the *selected agent's* provider; getting this wrong shows a false "LLM key not set" and disables the chat panel.
- **Git subdirectory workspaces** — tooling walks up to the real `.git`; branch switching must re-resolve the in-repo subdirectory inside the worktree, or `config.yml` is reported "not found."
- **Test these combinations after pipeline/LLM changes** — agentic analytics under `oxy serve --local` (history of server-side errors) and **Azure OpenAI** (routes through the OSS path; history of agentic incompatibilities).
- **`oxy run` works with no database** (run history/checkpoints fall back to no-op storage) — code that assumes a real storage backend must check runtime mode.
- **Input that must be sanitized/allowlisted** — DuckDB config SQL (S3 secrets, schema names, paths) escaped against single-quote injection; the Slack re-post handler allowlists block kinds (`section` / `context` / `divider` / `header` / `image`) and drops interactive types; the `http_request` automation task is HTTPS-only and blocks localhost / cloud-metadata / private-IP egress unless a per-task `allow_hosts` allowlist opts in.
- **Custom-app subdomains rely on a server-side session cookie, separate from main-site client login state** — logout must clear that cookie server-side (not just client-side), and every OAuth provider (not just magic-link) must preserve the return-to-app destination, or the subdomain and main site disagree on whether you're signed in.

---

## Key file extensions

| Extension | Type | Notes |
| --- | --- | --- |
| `.agentic.yml` | Agentic Agent | Multi-step FSM (analytics or app builder) |
| `.automation.yml` | Automation | Canonical; `.procedure.yml` also accepted (back-compat). `.workflow.yml` no longer supported |
| `.app.yml` | Data App | `tasks` + `display`; `published: bool` controls sidebar visibility |
| `.view.yml` / `.topic.yml` | Semantic View / Topic | Compiled by airlayer |
| `.sql` | Verified Query | Auto-discovered, run as-is when matched; shows a Verified badge |
| `.airway.yml` | Airway ELT pipeline | Source + destination; never holds credentials |
| `.monitor.yml` | Anomaly monitor | Watches a measure over time; per-granularity `schedule:`; gated by `OXY_INPROC_GLOBAL_WORKER` |
| `reconcile.yml` | Reconciliation checks | Root-only singleton; compares Oxy measures to a live external source (Toast) with abs+pct tolerance; drives the admin workspace-health **Reconciliation** dimension |
| `oxy.yml` (under `modeling/<project>/`) | Modeling project config | Maps dbt targets to Oxy connections |
