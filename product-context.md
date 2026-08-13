# Product Context

Injected into every Claude API call. Its only job: clarify what an agent
**cannot** cheaply retrieve from the code — renamed concepts, confusing
distinctions, product intent for triage, counterintuitive gotchas. **Not** a
feature catalog or changelog. If you'd find it by reading the module, cut it.

---

## Orientation

Oxy (user-facing brand: **Oxygen**) is the operating system for AI
transformation: connect a warehouse and ask questions in chat, where agents write
and run SQL and stream results back. Teams also build **Automations** (YAML),
**Data Apps** (YAML dashboards), **custom apps** (code-first React bundles),
dbt-style transforms via **Airform**, and edit project files in the IDE.

Deployment modes — almost every bug report depends on which one:
- **Cloud / enterprise** (`oxy serve`, `oxy serve --enterprise`, `oxy start` for a Docker-Postgres dev box) — the real, maintained path: multi-tenant, RBAC, magic-link auth, Stripe billing (`ServeMode::Cloud`). **"Run it locally" means this.** So dev-vs-prod is *not* distinguishable by mode — a dev box is cloud mode with non-prod secrets (no working SES → `OXY_APP_EMAIL_LOCAL_TEST=1` previews email).
- **Legacy single-project** (`oxy serve --local`, `ServeMode::Local`) — one fixed workspace, **no auth**. **Not maintained** — never design behavior around it; when in doubt, assume cloud.

---

## Terminology & renames (easy to get wrong)

- **Automation** = the thing formerly called **Procedure / Workflow**. Canonical file `.automation.yml` (`.procedure.yml` still accepted); `.workflow.yml` is **no longer a recognized file kind** — only `oxy migrate-automations` reads legacy files, to rename them. Canonical route `/automations/:id`; `/workflows/:id`, `/procedures`, `/agentic-workflows` are aliases. Rust types keep the `Workflow*`/`Procedure*` names — `type: workflow` and the `agentic_workflow_state` table are wire/storage contracts.
- **Orchestrator Dashboard** replaced the old **Coordinator** surface.
- **Oxygen Factory** = the Developer Portal / IDE (formerly Studio / Oxygen Builder / Oxygen Core). Same `/ide` surface, just the rail label.
- **Agentic Agent** (`.agentic.yml`) = Oxy's multi-step FSM agent (two kinds: **analytics** and **app builder**), distinct from the single-shot sense of "agent."
- **Builder Agent** = the file-editing copilot (chat **Build** mode) — distinct from the *app builder* agentic agent.
- **Custom Apps Platform** (code-first React+Vite bundles, shipped with `oxy publish`) is **not** YAML **Data Apps** (`.app.yml` dashboards). User-facing copy says **"custom app"**, but routes and the `<org>--<slug>.customer-apps.oxygen-hq.com` host still say **"customer app"** / `customer-apps`. Local dev against live cloud data goes through an `oxy proxy` sidecar: analytics events are dropped, side-effecting function/agent calls held unless confirmed.
- **Oxy Functions** = server-side TypeScript handlers declared in `oxy-app.json` and shipped *inside* a Custom App bundle (frontend and backend promote and roll back together) — **not** a YAML Automation task or Data App `task`. One runs as an HTTP route (`useFunction`), a cron job on the durable task queue, or an Airway transform step. Side-effecting powers are **capability-gated in the manifest and fail closed**: `secrets.write`, `email.send` (SES; the **platform** owns `from`, the function sets only `replyTo`), `storage.read/write` (private per-app silo, presigned URLs only, no public access). Isolate gotchas: email templates must be **preact** via `@oxy-hq/sdk/email` (react-dom needs node:stream / Web Streams), `Buffer`/`TextEncoder` don't exist (use the SDK's byte↔base64 helpers), and `ctx.fetch` decodes as UTF-8 text — a binary body needs `encoding: "base64"` or it silently corrupts.
- Four similarly-named third-party engines that are easy to confuse: **airlayer** (semantic layer), **Airform** (dbt-style modeling), **Airway** (ELT), **Airhouse** (a warehouse + connector).
- **Verified Query** = a plain `.sql` file the analytics agent runs *as-is* when it matches the question, bypassing LLM SQL generation; shows a **Verified** badge.
- **Two subdomain schemes, easy to confuse** — an **org subdomain** (`<org-slug>.oxygen-hq.com`) boots the whole product pre-scoped to that org's default project, serving its apps under `/a/<slug>/`; a **custom-app subdomain** (`<org>--<slug>.customer-apps.oxygen-hq.com`) serves one externally-hosted app at its own root.

---

## Roles (the same word means different things at different levels)

- **Platform standing is a grant, not a rank** — a row in `app_admins` carries a **role** (a capability preset) and a **scope** (all orgs, or a list). `is_app_admin` says only *that* someone is staff, so nothing may authorize from it.
- **Global Owner** (`OXY_OWNER` env allow-list → `is_owner`) — Oxy staff; reaches everything, incl. the Billing queue and Global-admin management. Still a boolean: it's root.
- **Global Admin** (role `global_admin`, seeded by `OXY_GLOBAL_ADMINS`; legacy `OXY_APP_ADMINS` still accepted) — Oxy ops; reaches most of admin + every custom app, but **not** Billing or Global-admin management.
- **App Operator** (role `app_operator`) — ships and develops custom apps and **nothing else**: no org deletion, member/org settings, billing, partners, or platform machinery. Optionally scoped to specific orgs. Exists because "manages apps" used to require Global Admin — which silently included deleting any org.
- **Partner** (capability-gated, **not** an org membership) — a distributor tier between Oxy staff and tenants: owns downstream orgs and, only for those, creates orgs, manages members (owner-seizure guardrail), publishes their apps and names each app's audience. Scoped to exactly its grants — **no** general platform reach; every action lands in an append-only per-org audit log.
- **Org Owner / Admin / Member** (`role` in `org_members`) — tenant-internal only, **no** platform reach. Workspace role derives via `EffectiveWorkspaceRole`, so an org Member who is a workspace Admin still reaches Databases / Secrets / Apps / API Keys. Airhouse settings stay open to every member *by design* — the credential it mints is their own, read-only and time-limited.
- **Per-app scope** — a custom app is visible org-wide (default) or restricted to named **org teams**/members; a **per-app admin** role can extend app-admin rights, through a team, to someone who isn't org staff. **A grant narrows within an org, never widens into one** — a non-member holding a grant is denied. An app must ask the authz model for the caller's role, never infer it, so an in-app privileged view can't be forged.
- **Staff reach is not standing** — staff and partners entering a tenant workspace need an explicit **assume-role session**: 60 minutes, non-renewable, reason recorded to the impersonation log (`oxy assume` from the CLI). A staff-facing "you don't have permission" usually means *no active assume session*, not a mis-modeled role.

---

## Surfaces (for "which component is this?" triage)

- **Home / HQ launcher** (`/`, `/home`) — apps-first, org-branded landing that lists only the **custom apps the viewer can actually open**. **Ask Oxygen** (⌘K) is a right-side **drawer** that compacts the page beside it (not an overlay); "Full view" promotes to `/threads/:id`. The rail's **Chat** entry was formerly "Threads".
- **Apps** (`/apps/:id`) — a Data App; results cached by parameter hash (`?refresh` forces a re-run).
- **Developer Portal / IDE** (`/ide`) — protected `main` auto-redirects edits to a new branch; `oxy serve --readonly` makes all writes 405. Semantic surfaces (World Model, explorer, Metric Tree) read the **branch selected in the IDE**, not always `main`.
- **Admin → Workspace Health** — **opt-in per workspace**: no `health_check:` block in `config.yml` — or an unparseable one — means the workspace is never evaluated and is *absent* from the admin table rather than shown healthy. Five passive dimensions plus **smoke tests** that really run connection / topic / app / agent probes (the data- and token-spending ones opt-in). Only an unhealthy *transition* pages Slack, never "degraded".
- **World Model Graph** (Globe icon in the rail) — interactive map of the semantic layer, driven by `.world-model.yml`. Distinct from the **Context Graph** and the **Metric Tree** — three different graph surfaces. Selecting a measure also sizes **opportunities**; the refusals are deliberate — per-unit rates only (`gap × volume`, never raw totals), only segments passing a significance test, and no per-unit denominator means refused rather than sized, so an empty list is usually correct.

---

## Components worth clarifying

- **Agentic Agent** — supports **human-in-the-loop suspension** (pauses on a clarifying question, then resumes). When the semantic layer lacks a measure it needs, it answers from what exists rather than handing off to the Builder Agent mid-run.
- **Pre-aggregation** — semantic queries may serve from local Parquet rollups instead of the warehouse; a **Pre-aggregated** badge means it came from a rollup (stale data under that badge is a freshness bug).
- **Airway ELT** (`.airway.yml`) — credentials live in the secret manager, never the YAML. Partial failures record as `completed_with_errors`, not full-run failures. Schema migration is **additive only**, so a pipeline on a wrong schema can't self-heal without an explicit **Reset schema**; a retry resumes the same run from its cursor rather than re-pulling history.
- **Universal Slack Bot** — one shared multi-tenant app (no per-customer installs); Slack users are matched to Oxy users **by email**.
- **Airhouse** — first-class connector with **ephemeral** credentials (workspaces mint short-lived creds from a service account; no rotation surface).
- **Metric Tree & Anomaly Monitoring** — a `.monitor.yml` watches a measure over time (per-segment via `filters`/`group_by`); anomalies land in the **Insights Inbox** with AI root-cause, also reachable from chat and the SDK. The separate **Metric Tree** (a Semantic Layer IDE tab) decomposes a top-line metric into driver metrics. Two deliberate silences that read as bugs: scans exclude the current *incomplete* period, and a series isn't scanned until it has **~8 seasonal cycles of history** (≈8 weeks daily), so a new segment stays quiet rather than reporting its opening ramp. **Explain** compares the same phase one cycle back (Monday vs prior Monday), scopes to the segment that fired, prunes time-part / row-key dims so day-of-week or `*_id` is never a "driver", and sorts drivers into explaining / offsetting / **mechanical** / undetermined — one that merely tracks its base is mechanical, deliberately credited as neither cause nor offset.
- **Authentication** — magic-link only (passwordless, AWS SES); legacy password auth removed. CI publishes use a long-lived, publish-scoped `OXY_TOKEN`, not a session.
- **Design system** — use semantic tokens, not raw hex; **emerald is reserved for workflow-node success only**.

---

## Counterintuitive gotchas (high-cost, hard to guess)

- **DuckDB concurrent init** — two handles opening the same file concurrently have caused SIGSEGV; the pool serializes init, so code opening DuckDB outside it must too.
- **DuckLake has no indexes** — DDL against Airhouse/DuckLake tables (today: the camera-fleet schema) must avoid `CREATE INDEX`, `PRIMARY KEY`, and `UNIQUE`; a table carrying one fails and the writer goes inert from there on. Ordering-based predicate pushdown is the substitute, not an index.
- **Observability is ClickHouse-only** — one backend, no default in any mode (`--local` included), so unset means capture is simply off. The former `duckdb` / `postgres` / `airhouse` labels don't fall back to anything: the server boots with capture off and logs a loud migration error, so a stale label reads as "no traces", not as a crash. No data crosses over.
- **Observability serving has two repeat footguns** — timestamps must go out as ISO-8601 UTC or the browser mis-parses them (render crash, waterfall spans collapsed to slivers), and trace queries need hard time/size caps: an unbounded scan took the backend offline, not merely timed out.
- **Empty-result warehouse queries** can panic in the shared Arrow bridge (DuckDB / Snowflake / MotherDuck / connectorx); each path must short-circuit its empty shape. Oversized results hit a cross-connector memory backstop and unbounded semantic/SQL-IDE queries cap at 10k rows — both flag **truncated**, so "missing rows" may be a cap, not a query bug.
- **Semantic file discovery** must skip hidden/build dirs (`.worktrees`, `.git`, `.oxy_state`, `target`, `node_modules`, …); stray copies there trigger spurious "duplicate view name" errors.
- **Two distinct worker concepts, easy to conflate** — the *durable task fleet* (in-process by default, standalone via `oxy worker`) runs queued `TaskSpec` jobs; the *global singleton worker* (`OXY_INPROC_GLOBAL_WORKER`) drives schedules, monitor scans, and pre-aggregation. Toggling one does not affect the other, and with the singleton off **schedule CRUD works but nothing ever fires**. Multi-replica safety is a CAS `next_run_at` claim (missed runs collapse to one); cross-process cancel is a polled flag.
- **Every Builder/analytics/workflow SSE stream must emit a terminal event** (`done` / `error` / `cancelled`) — even when it fails before the orchestrator loop starts (a broken `.view.yml`) — or the frontend hangs forever.
- **Multi-tenant scoping is a correctness invariant** — orchestrator and pre-aggregation endpoints must filter by `workspace_id`, secret lookups by project, or runs/secrets leak across tenants.
- **Mode-dependent LLM-key check** — the home readiness check must read the **workspace secrets store** in cloud mode (env vars only in legacy local), and only for the *selected agent's* provider; getting this wrong shows a false "LLM key not set" and disables the chat panel.
- **Git subdirectory workspaces** — tooling walks up to the real `.git`; branch switching must re-resolve the in-repo subdirectory inside the worktree, or `config.yml` reads as "not found."
- **Freshness-aware analytics** — the agent knows each source's loaded-through date (a view's `meta:` freshness contract) and answers "data covers through <date>" instead of reporting an unloaded recent range as a real zero.
- **Production-only "missing workspace/topic" errors are instance affinity, not bad YAML** — anything read per request must come from the compiled workspace in Postgres, not a working copy present only on the owning instance. Symptoms look like content bugs: "Topic not found" with an empty topic list, "failed to read workspace", rejected webhooks, custom-app `origin not allowed`. A not-yet-compiled workspace must return a **retryable** state; mid-deploy "workspace directory not found" is transient — retry, don't surface it.
- **Test after pipeline/LLM changes** — **Azure OpenAI** routes through the OSS path (history of agentic incompatibilities). `vendor: openai_compat` demands an explicit `api_url` so EU/GDPR traffic can never silently route to OpenAI, and an `llm.vendor` override outranks the vendor inherited from a referenced model.
- **Serve ports are `OXY_HTTP_PORT` / `OXY_HTTP_INTERNAL_PORT`** — renamed off `OXY_PORT` / `OXY_INTERNAL_PORT`, which collided with Kubernetes-injected vars and could stop a self-hosted deploy booting.
- **`oxy run` works with no database** (history/checkpoints fall back to no-op storage) — code assuming a real storage backend must check runtime mode.
- **Input that must be sanitized/allowlisted** — DuckDB config SQL escaped against quote injection; the `http_request` automation task is HTTPS-only and blocks localhost / cloud-metadata / private-IP egress unless a per-task `allow_hosts` opts in. Uploaded custom-app bundles are size-capped **on unpack** — one serve instance hosts many apps.
- **Custom-app subdomains ride a server-side session cookie, separate from main-site client login state** — logout must clear it server-side, and every OAuth provider (not just magic-link) must preserve the return-to-app destination, or the subdomain and main site disagree on whether you're signed in.

---

## Key file extensions

`.agentic.yml` · `.automation.yml` (+ legacy `.procedure.yml`) · `.app.yml` · `.view.yml` / `.topic.yml` · `.sql` · `.airway.yml` · `.world-model.yml` · `oxy-app.json` (custom-app manifest) · `oxy.yml` under `modeling/<project>/` (dbt target → Oxy connection map). Two with semantics you can't guess:

- **`.monitor.yml`** — per-granularity `schedule:`, gated by `OXY_INPROC_GLOBAL_WORKER`. `timezone` only bites on a `type: datetime` time dimension — a `type: date` business-date column is already a local calendar date, bucketed raw, so `timezone` is inert there. `freshness` (`3d`) means "trust nothing newer than this horizon," so a lagging warehouse's unloaded buckets stop reading as a collapse.
- **`reconcile.yml`** — root-only singleton; compares an Oxy measure (semantic **or** raw scalar `sql:`) against a live external source (Toast) with abs+pct tolerance, driving the admin workspace-health **Reconciliation** dimension. Same `timezone` / `freshness` semantics, same `date`-column inertness — but on a `week`/`month` grain a `freshness` under one full grain buys a settle time that swings with the weekday, so the check reconciles for days and then reports drift that isn't there.
