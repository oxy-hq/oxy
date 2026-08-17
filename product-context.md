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
and dbt-style transforms via **Airform**.

Deployment modes — almost every bug report depends on which one:
- **Cloud / enterprise** (`oxy serve`, `oxy serve --enterprise`, `oxy start` for a Docker-Postgres dev box) — the real, maintained path: multi-tenant, RBAC, magic-link auth, Stripe billing (`ServeMode::Cloud`). **"Run it locally" means this.** So dev-vs-prod is *not* distinguishable by mode — a dev box is cloud mode with non-prod secrets.
- **Legacy single-project** (`oxy serve --local`, `ServeMode::Local`) — one fixed workspace, **no auth**. **Not maintained** — never design behavior around it; when in doubt, assume cloud.

---

## Terminology & renames (easy to get wrong)

- **Automation** = the thing formerly called **Procedure / Workflow**. Canonical file `.automation.yml` (`.procedure.yml` still accepted); `.workflow.yml` is **no longer a recognized file kind**. Canonical route `/automations/:id`; `/workflows/:id` and `/procedures` are aliases. Rust types keep the `Workflow*`/`Procedure*` names — `type: workflow` and the `agentic_workflow_state` table are wire/storage contracts.
- **Oxygen Factory** = the Developer Portal / IDE (formerly Studio / Oxygen Builder / Oxygen Core). Same `/ide` surface, just the rail label. The **Orchestrator Dashboard** replaced the old **Coordinator** surface.
- **Agentic Agent** (`.agentic.yml`) = Oxy's multi-step FSM agent (two kinds: **analytics** and **app builder**), distinct from the single-shot sense of "agent."
- **Builder Agent** = the file-editing copilot (chat **Build** mode) — distinct from the *app builder* agentic agent.
- **Custom Apps Platform** (code-first React+Vite bundles, shipped with `oxy publish`) is **not** YAML **Data Apps** (`.app.yml` dashboards). User-facing copy says **"custom app"**, but routes and the `<org>--<slug>.customer-apps.oxygen-hq.com` host still say `customer-apps`. Local dev against live cloud data goes through an `oxy proxy` sidecar (side-effecting function/agent calls held, analytics dropped).
- **Oxy Functions** = server-side TypeScript handlers declared in `oxy-app.json` and shipped *inside* a Custom App bundle (frontend and backend promote and roll back together) — **not** a YAML Automation task or Data App `task`. One runs as an HTTP route (`useFunction`), a cron job, or an Airway transform step. Side-effecting powers are **capability-gated in the manifest and fail closed**: `secrets.write`, `email.send` (the platform owns `from`; the function sets only `replyTo`), `storage.read/write` (private per-app silo, presigned URLs only). Isolate gotchas: email templates must be **preact** via `@oxy-hq/sdk/email` (react-dom needs node:stream); there is no `Buffer`/`TextEncoder` (use `btoa`/`atob` or the SDK's byte↔base64 helpers); `ctx.fetch` decodes as UTF-8, so a binary body needs `encoding: "base64"` or it silently corrupts; and an app's **first** `storage.retention` rule changes how uploads are signed, so a non-SDK uploader starts failing with an opaque error.
- Four similarly-named third-party engines that are easy to confuse: **airlayer** (semantic layer), **Airform** (dbt-style modeling), **Airway** (ELT), **Airhouse** (a warehouse + connector).
- **Verified Query** = a plain `.sql` file the analytics agent runs *as-is* when it matches the question, bypassing LLM SQL generation.
- **Two subdomain schemes, easy to confuse** — an **org subdomain** (`<org-slug>.oxygen-hq.com`) boots the whole product pre-scoped to that org's default project, serving its apps under `/a/<slug>/`; a **custom-app subdomain** (`<org>--<slug>.customer-apps.oxygen-hq.com`) serves one externally-hosted app at its own root.

---

## Roles (the same word means different things at different levels)

- **Platform standing is a grant, not a rank** — a row in `app_admins` carries a **role** (a capability preset) and a **scope** (all orgs, or a list). `is_app_admin` says only *that* someone is staff, so nothing may authorize from it.
- **Global Owner** (`OXY_OWNER` env allow-list → `is_owner`) — Oxy staff; reaches everything, incl. the Billing queue and Global-admin management. Still a boolean: it's root.
- **Global Admin** (role `global_admin`, seeded by `OXY_GLOBAL_ADMINS`; legacy `OXY_APP_ADMINS` still accepted) — Oxy ops; reaches most of admin + every custom app, but **not** Billing or Global-admin management.
- **App Operator** (role `app_operator`) — ships and develops custom apps and **nothing else**. Optionally scoped to named orgs, and an out-of-scope request answers **not found** rather than *not allowed* so scope can't be probed — a 404 in an admin surface may be a scope boundary, not a missing row. A grant you issue must be strictly weaker than your own.
- **Partner** (capability-gated, **not** an org membership) — a distributor tier between Oxy staff and tenants: owns downstream orgs and, only for those, creates orgs, manages members (owner-seizure guardrail), publishes their apps and names each app's audience. **No** general platform reach; every action lands in an append-only per-org audit log.
- **Org Owner / Admin / Member** (`role` in `org_members`) — tenant-internal only, **no** platform reach. Workspace role derives via `EffectiveWorkspaceRole`, so an org Member who is a workspace Admin still reaches Databases / Secrets / Apps / API Keys. Airhouse settings stay open to every member *by design* — the credential it mints is their own, read-only and time-limited.
- **Per-app scope** — a custom app is visible org-wide (default) or restricted to named **org teams**/members, and a **per-app admin** role can extend app-admin rights through a team to someone who isn't org staff. **A grant narrows within an org, never widens into one** — a non-member holding a grant is denied. An app must ask the authz model for the caller's role, never infer it, so an in-app privileged view can't be forged.
- **Staff reach is not standing** — staff and partners entering a tenant workspace need an explicit **assume-role session** (60 min, non-renewable, reason logged; `oxy assume` from the CLI). A staff-facing "you don't have permission" usually means *no active assume session*, not a mis-modeled role.

---

## Surfaces (for "which component is this?" triage)

- **Home / HQ launcher** (`/`, `/home`) — apps-first landing listing only the **custom apps the viewer can actually open**. **Ask Oxygen** (⌘K) is a right-side **drawer** that compacts the page beside it (not an overlay); "Full view" promotes to `/threads/:id`. The rail's **Chat** entry was formerly "Threads".
- **Developer Portal / IDE** (`/ide`) — protected `main` auto-redirects edits to a new branch; `oxy serve --readonly` makes all writes 405. Semantic surfaces (World Model, explorer, Metric Tree) read the **branch selected in the IDE**, not always `main`.
- **Admin → Workspace Health** — **opt-in per workspace**: no `health_check:` block in `config.yml` — or an unparseable one — means the workspace is never evaluated and is *absent* from the admin table rather than shown healthy. Five passive dimensions plus opt-in **smoke tests** that really run probes. Only an unhealthy *transition* pages Slack, never "degraded".
- **World Model Graph** (Globe icon in the rail) — map of the semantic layer, driven by `.world-model.yml`. Distinct from the **Context Graph** and the **Metric Tree** — three different graph surfaces. Selecting a measure also sizes **opportunities**; the refusals are deliberate — per-unit rates only (`gap × volume`, never raw totals), only significant segments, and no per-unit denominator means refused rather than sized, so an empty list is usually right.

---

## Components worth clarifying

- **Agentic Agent** — when the semantic layer lacks a measure it needs, it answers from what exists rather than handing off to the Builder Agent mid-run; it can also suspend on a clarifying question and resume.
- **Pre-aggregation** — a **Pre-aggregated** badge means the result came from a local Parquet rollup, not the warehouse (stale data under that badge is a freshness bug).
- **Airway ELT** (`.airway.yml`) — credentials live in the secret manager, never the YAML. Partial failures record as `completed_with_errors`, not full-run failures. Schema migration is **additive only**, so a pipeline on a wrong schema can't self-heal without an explicit **Reset schema**; a retry resumes the same run from its cursor rather than re-pulling history. **One run per pipeline per workspace**, DB-leased on every path (automation steps included) — concurrent runs share one cursor and fold overlapping snapshots into the served table. A busy pipeline makes the next run *wait*; a submit joins the run already queued (ten **Run now** clicks = one run), and a schedule tick that joins is not a failure. `allow_concurrent_runs: true` opts out. Sources declare a **contract** (rows immutable / versioned / opaque, whether the cursor tracks modification time, how far back it restates) that a per-source-kind **admission policy** gates on, resolved at *queue* time so a later edit never rewrites past runs. A step delegating to a sub-run is capped at ~30 min.
- **Universal Slack Bot** — one shared multi-tenant app (no per-customer installs); Slack users are matched to Oxy users **by email**.
- **Airhouse** — first-class connector with **ephemeral** credentials (workspaces mint short-lived creds from a service account; no rotation surface).
- **Metric Tree & Anomaly Monitoring** — a `.monitor.yml` watches a measure over time (per-segment via `filters`/`group_by`); anomalies land in the **Insights Inbox** with AI root-cause. The separate **Metric Tree** (a Semantic Layer IDE tab) decomposes a top-line metric into drivers. Two deliberate silences that read as bugs: scans exclude the current *incomplete* period, and a series isn't scanned until it has **~8 seasonal cycles of history** (≈8 weeks daily), so a new segment stays quiet rather than reporting its opening ramp. Severity is *distance past the envelope edge*, the same quantity that decided the firing, so ordering and filtering agree; too few same-phase points means no opinion, floored at **Medium** (a spike on a short, flat series). Many segments breaching the same bucket group into one **cohort**; a `calendar:` block only *labels* a cohort with a named date, never filters. **Explain** compares the same phase one cycle back (Monday vs prior Monday), scopes to the segment that fired, prunes time-part dims, and keeps a `*_id` as a driver when the semantic layer declares it an **entity** (the looks-like-a-key heuristic is only a fallback). Drivers sort into explaining / offsetting / **mechanical** / undetermined — one that merely tracks its base is mechanical, deliberately credited as neither cause nor offset.
- **Authentication** — magic-link only (passwordless, AWS SES); legacy password auth removed. CI publishes use a long-lived, publish-scoped `OXY_TOKEN`, not a session.
- **Design system** — use semantic tokens, not raw hex; **emerald is reserved for workflow-node success only**.

---

## Counterintuitive gotchas (high-cost, hard to guess)

- **DuckDB concurrent init** — two handles opening the same file concurrently have caused SIGSEGV; the pool serializes init, so code opening DuckDB outside it must too.
- **DuckLake has no indexes** — DDL against Airhouse/DuckLake tables must avoid `CREATE INDEX`, `PRIMARY KEY`, and `UNIQUE`; a table carrying one fails and the writer goes inert from there on. Ordering-based predicate pushdown is the substitute, not an index.
- **Observability is ClickHouse-only** — one backend, no default in any mode (`--local` included), so unset means capture is simply off; a stale `duckdb` / `postgres` / `airhouse` label falls back to nothing, so it reads as "no traces" rather than crashing.
- **Observability serving has two repeat footguns** — timestamps must go out as ISO-8601 UTC or the browser mis-parses them (render crash, spans collapsed to slivers), and trace queries need hard time/size caps: an unbounded scan took the backend offline, not merely timed out.
- **Empty-result warehouse queries** can panic in the shared Arrow bridge (DuckDB / Snowflake / MotherDuck / connectorx); each path must short-circuit its empty shape. Oversized results hit a cross-connector memory backstop and unbounded semantic/SQL-IDE queries cap at 10k rows — both flag **truncated**, so "missing rows" may be a cap, not a query bug.
- **An external source's own `modified` timestamp can lie** — Toast doesn't advance an order's modified time when a card is captured, and a late edit can land below a cursor's floor and never be seen again, so such resources re-read a trailing window every run; a backfill repairing them must widen **both** ends of its range, since the axis it filters on is the untrustworthy one. Relatedly, only **one** component may rotate a QuickBooks refresh token (Intuit voids the old one whenever it issues a new one, so two rotators deadlock into `invalid_grant` until a manual re-auth) — a read-only `access_token_var` keeps a pipeline out of that role. Such fixes stop *new* bad rows; ones already landed need a one-time cleanup.
- **Semantic file discovery** must skip hidden/build dirs (`.worktrees`, `.git`, `.oxy_state`, `target`, `node_modules`, …); stray copies there trigger spurious "duplicate view name" errors.
- **Two distinct worker concepts, easy to conflate** — the *durable task fleet* (in-process by default, standalone via `oxy worker`) runs queued `TaskSpec` jobs; the *global singleton worker* (`OXY_INPROC_GLOBAL_WORKER`) drives schedules, monitor scans, and pre-aggregation. Toggling one does not affect the other, and with the singleton off **schedule CRUD works but nothing ever fires**. Missed schedule runs collapse to one.
- **Every Builder/analytics/workflow SSE stream must emit a terminal event** (`done` / `error` / `cancelled`) — even when it fails before the orchestrator loop starts (a broken `.view.yml`) — or the frontend hangs forever.
- **Mode-dependent LLM-key check** — any "is a key set?" test must read the **workspace secrets store** in cloud (env vars only in legacy local), and only for the *selected agent's* provider. Home deliberately no longer checks on the common path, so a keyless workspace surfaces on the first failed message — never force-redirect into the setup wizard over it.
- **Git subdirectory workspaces** — tooling walks up to the real `.git`; branch switching must re-resolve the in-repo subdirectory inside the worktree, or `config.yml` reads as "not found."
- **Production-only "missing workspace/topic" errors are instance affinity, not bad YAML** — anything read per request must come from the compiled workspace in Postgres, not a working copy present only on the owning instance. Symptoms look like content bugs: "Topic not found" with an empty topic list, "failed to read workspace", rejected webhooks, custom-app `origin not allowed`, a pipeline that exists reading as missing. **Not compiled yet** must answer *retryable*, distinct from genuinely-not-found; mid-deploy "workspace directory not found" is transient — retry, don't surface it.
- **Test after pipeline/LLM changes** — **Azure OpenAI** routes through the OSS path (history of agentic incompatibilities). `vendor: openai_compat` demands an explicit `api_url` so EU/GDPR traffic can never silently route to OpenAI, and an `llm.vendor` override outranks the vendor inherited from a referenced model.
- **Input that must be sanitized/allowlisted** — DuckDB config SQL escaped against quote injection; the `http_request` automation task is HTTPS-only and blocks localhost / cloud-metadata / private-IP egress unless a per-task `allow_hosts` opts in. Uploaded custom-app bundles are size-capped **on unpack** — one serve instance hosts many apps.
- **Custom-app subdomains ride a server-side session cookie, separate from main-site client login state** — logout must clear it server-side, and every OAuth provider (not just magic-link) must preserve the return-to-app destination, or the two disagree on whether you're signed in.

---

## Key file extensions

Most are self-describing (`.agentic.yml`, `.automation.yml`, `.app.yml`, `.view.yml` / `.topic.yml`, `.airway.yml`, `.world-model.yml`, `oxy-app.json`); `oxy.yml` under `modeling/<project>/` maps a dbt target to an Oxy connection. Two with semantics you can't guess:

- **`.monitor.yml`** — per-granularity `schedule:`. `timezone` only bites on a `type: datetime` time dimension — a `type: date` business-date column is already a local calendar date, bucketed raw, so `timezone` is inert there. `freshness` (`3d`) means "trust nothing newer than this horizon," so a lagging warehouse's unloaded buckets stop reading as a collapse.
- **`reconcile.yml`** — root-only singleton; compares an Oxy measure (semantic **or** raw scalar `sql:`) against a live external source (Toast) with abs+pct tolerance, feeding the workspace-health **Reconciliation** dimension. Same `timezone` / `freshness` semantics as `.monitor.yml` — but on a `week`/`month` grain a `freshness` under one full grain buys a settle time that swings with the weekday, so the check reconciles for days and then reports drift that isn't there.
