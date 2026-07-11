//! Process-role classification + the static manifest of `IdeOnly` routes.
//!
//! Every Oxy request flows through one of three role-shaped processes:
//!
//! - **`ide`** — singleton owning the workspace working copy + `.git`.
//! - **`serve`** — stateless fleet replica reading Postgres + S3 only.
//! - **`worker`** — TaskSpec drainer; no inbound HTTP today, has `/healthz`.
//!
//! Routes carry one of:
//!
//! - [`RouteRole::IdeOnly`] — touches the workspace FS or `.git`. 421 on a
//!   `serve` replica.
//! - [`RouteRole::FleetOk`] — Postgres / S3 / LLM only. Safe everywhere
//!   (including the worker's `/healthz`).
//! - [`RouteRole::WorkerOnly`] — reserved.
//!
//! The classification lives here as a single static table. A route that touches
//! the workspace FS / `.git` / local state dir MUST get an `IDE_ONLY_PATTERNS`
//! entry below, or it 404s/421s on a serve replica — see
//! `.claude/skills/oxy-route-classification/SKILL.md`. For the fully-FS builders
//! (git / files / data-repo) the `fully_fs_builder_routes_classify_ide_only`
//! test enforces this from the router source automatically; MIXED builders
//! (e.g. `build_app_routes`) still need a per-route test + reviewer attention.
//!
//! Routes not in [`IDE_ONLY_PATTERNS`] are [`RouteRole::FleetOk`] by default.
//! That's deliberate — the compile boundary already removed FS reads from the
//! customer-facing hot path; new FS leaks are caught by adding to this table.
//!
//! ## Matching
//!
//! Patterns are full paths (`/api/{workspace_id}/compile`, including the
//! `/api` prefix) and matched **segment-by-segment** against the request's
//! actual URI path. `{name}` matches one segment; `{*name}` matches one or
//! more trailing segments. This is intentionally NOT axum's `MatchedPath`
//! lookup: a `Router::layer` mounted outside a nested router sees only the
//! nest's *registration* pattern (e.g. `/api/{workspace_id}/{*rest}`), not
//! the resolved leaf (`/api/{workspace_id}/compile`). Matching the live URI
//! sidesteps the nesting issue entirely.

use std::sync::OnceLock;

use oxy_shared::errors::OxyError;

/// Process-role for this server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// Singleton owning the workspace FS + git working tree.
    Ide,
    /// Stateless fleet replica.
    Serve,
    /// Queue drainer. Today has only `/healthz` over HTTP.
    Worker,
    /// Single-process all-in-one. Accepts every route.
    All,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Ide => "ide",
            Role::Serve => "serve",
            Role::Worker => "worker",
            Role::All => "all",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteRole {
    IdeOnly,
    FleetOk,
    WorkerOnly,
}

impl RouteRole {
    pub fn as_str(self) -> &'static str {
        match self {
            RouteRole::IdeOnly => "ide-only",
            RouteRole::FleetOk => "fleet-ok",
            RouteRole::WorkerOnly => "worker-only",
        }
    }

    /// Whether a process of `process_role` may serve a route of `self`.
    /// Notably FleetOk is accepted by Worker too: kubelet health probes
    /// must succeed on every role.
    pub fn accepted_by(self, process_role: Role) -> bool {
        match (self, process_role) {
            (_, Role::All) => true,
            (RouteRole::IdeOnly, Role::Ide) => true,
            (RouteRole::FleetOk, Role::Ide | Role::Serve | Role::Worker) => true,
            (RouteRole::WorkerOnly, Role::Worker) => true,
            _ => false,
        }
    }
}

pub struct ManifestEntry {
    pub method: &'static str,
    pub path_pattern: &'static str,
    pub role: RouteRole,
}

/// Every route that touches workspace FS / `.git`. Patterns are FULL paths
/// including the `/api` nest prefix; `{name}` matches one segment and
/// `{*name}` matches one-or-more trailing segments.
///
/// Match-first wins. Patterns here are intended to be disjoint.
const IDE_ONLY_PATTERNS: &[ManifestEntry] = &[
    // ── IDE itself is a singleton experience ────────────────────────────────
    // The whole `/ide` SPA + every workspace-scoped IDE route surfaces FS
    // state — file tree, working-copy reads, builder edits, modeling lifecycle.
    // Block it at the page level so navigating to `/ide` on a serve replica
    // hits a hard 421 instead of rendering a UI that 421s on every action.
    // The IDE singleton owns the working copy; a fleet replica genuinely
    // has nothing useful to show here.
    ManifestEntry {
        method: "*",
        path_pattern: "/ide",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "*",
        path_pattern: "/ide/{*rest}",
        role: RouteRole::IdeOnly,
    },
    // Also block the file READ APIs that drive the IDE — when the IDE is
    // showing the working copy (with uncommitted edits), reads must come
    // from disk on the singleton, not the compile boundary on Postgres
    // which only carries promoted state.
    ManifestEntry {
        method: "GET",
        path_pattern: "/api/{workspace_id}/files",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "GET",
        path_pattern: "/api/{workspace_id}/files/{pathb64}",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "GET",
        path_pattern: "/api/{workspace_id}/files/diff-summary",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "GET",
        path_pattern: "/api/{workspace_id}/files/{pathb64}/from-git",
        role: RouteRole::IdeOnly,
    },
    // ── App data/source file reads hit local disk on the singleton ──────────
    // `get_source_file` searches `workspace_path`; `get_data` reads the local
    // state dir (stripped from the serve fleet env). Neither exists on a
    // stateless replica, so forward both to the ide instead of 404ing locally.
    ManifestEntry {
        method: "GET",
        path_pattern: "/api/{workspace_id}/apps/source/{pathb64}",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "GET",
        path_pattern: "/api/{workspace_id}/apps/file/{pathb64}",
        role: RouteRole::IdeOnly,
    },
    // Data-app EXECUTION runs the app's inline automation, whose `execute_sql`
    // tasks emit authored file-path SQL (`FROM 'oxymart.csv'`). That needs the
    // working copy + the local DuckDB connector's file_search_path, so every
    // handler that calls `AppService::run()` is pinned to the ide:
    //   • GET  /apps/{pathb64}          → get_app_data   (auto-run on load)
    //   • POST /apps/{pathb64}/run      → run_app
    //   • POST /apps/{pathb64}/result   → get_app_result
    // The non-executing surface stays FleetOk: GET /apps/ (list), GET
    // /apps/{pathb64}/displays (returns SQL templates for the FE to run), and
    // GET /apps/{pathb64}/charts/... (local-file read with an S3 fallback).
    ManifestEntry {
        method: "GET",
        path_pattern: "/api/{workspace_id}/apps/{pathb64}",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "POST",
        path_pattern: "/api/{workspace_id}/apps/{pathb64}/run",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "POST",
        path_pattern: "/api/{workspace_id}/apps/{pathb64}/result",
        role: RouteRole::IdeOnly,
    },
    // App WRITE surface — these MUTATE the workspace working copy, so they must
    // run on the ide singleton: a serve replica has no working copy, so the
    // write is silently lost (never committed/promoted to git) or 500s.
    //   • POST /apps/{pathb64}/publish    → publish_app  ┐ set_publish_state →
    //   • POST /apps/{pathb64}/unpublish  → unpublish_app ┘ fs::write the .app.yml
    //   • POST /apps/save-from-run/{run_id} → save_app_builder_run → fs::write
    //     a generated app under `workspace_path()/generated`
    // NOT runtime (DuckDB) routes — they're editing-bound, so they deliberately
    // stay OUT of `is_workspace_runtime_route` (ide-down = workspace-editing).
    ManifestEntry {
        method: "POST",
        path_pattern: "/api/{workspace_id}/apps/{pathb64}/publish",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "POST",
        path_pattern: "/api/{workspace_id}/apps/{pathb64}/unpublish",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "POST",
        path_pattern: "/api/{workspace_id}/apps/save-from-run/{run_id}",
        role: RouteRole::IdeOnly,
    },
    // ── Compile trigger reads the working copy on the singleton ─────────────
    ManifestEntry {
        method: "POST",
        path_pattern: "/api/{workspace_id}/compile",
        role: RouteRole::IdeOnly,
    },
    // ── File CRUD on the workspace ──────────────────────────────────────────
    ManifestEntry {
        method: "POST",
        path_pattern: "/api/{workspace_id}/files/{pathb64}",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "PUT",
        path_pattern: "/api/{workspace_id}/files/{pathb64}/rename-file",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "PUT",
        path_pattern: "/api/{workspace_id}/files/{pathb64}/rename-folder",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "POST",
        path_pattern: "/api/{workspace_id}/files/{pathb64}/new-file",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "POST",
        path_pattern: "/api/{workspace_id}/files/{pathb64}/new-folder",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "DELETE",
        path_pattern: "/api/{workspace_id}/files/{pathb64}/delete-file",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "DELETE",
        path_pattern: "/api/{workspace_id}/files/{pathb64}/delete-folder",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "POST",
        path_pattern: "/api/{workspace_id}/files/{pathb64}/revert",
        role: RouteRole::IdeOnly,
    },
    // ── Git operations ──────────────────────────────────────────────────────
    // IMPORTANT: `build_git_routes()` (router/workspace.rs) `.merge()`s these
    // FLAT into the workspace tree — there is NO `/git/` path segment. Each
    // one shells out to git / touches the working copy, so each must be an
    // explicit IdeOnly entry. A single `/git/{*rest}` wildcard matches NONE
    // of them. `manifest_covers_every_git_route` asserts a hand-maintained
    // list classifies IdeOnly — it does NOT introspect the live router, so a
    // new git route must be added to BOTH the router and that test's list.
    ManifestEntry {
        method: "*",
        path_pattern: "/api/{workspace_id}/branches",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "*",
        path_pattern: "/api/{workspace_id}/branches/{branch_name}",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "POST",
        path_pattern: "/api/{workspace_id}/pull-changes",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "POST",
        path_pattern: "/api/{workspace_id}/fetch",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "POST",
        path_pattern: "/api/{workspace_id}/push-changes",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "POST",
        path_pattern: "/api/{workspace_id}/force-push",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "POST",
        path_pattern: "/api/{workspace_id}/discard-all",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "POST",
        path_pattern: "/api/{workspace_id}/abort-rebase",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "POST",
        path_pattern: "/api/{workspace_id}/continue-rebase",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "POST",
        path_pattern: "/api/{workspace_id}/resolve-conflict-file",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "POST",
        path_pattern: "/api/{workspace_id}/unresolve-conflict-file",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "POST",
        path_pattern: "/api/{workspace_id}/resolve-conflict-with-content",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "POST",
        path_pattern: "/api/{workspace_id}/reset-to-commit",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "GET",
        path_pattern: "/api/{workspace_id}/recent-commits",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "GET",
        path_pattern: "/api/{workspace_id}/revision-info",
        role: RouteRole::IdeOnly,
    },
    // ── Data repositories — git checkouts under modeling/, all FS+git ───────
    // `.nest("/repositories", build_data_repo_routes())`: list, add, remove,
    // branch, branches, checkout, diff, commit, files, github. Every handler
    // operates on a repo working copy on disk. The bare `/repositories` (GET
    // list / POST add) needs its own entry — `{*rest}` does not match the
    // empty tail.
    ManifestEntry {
        method: "*",
        path_pattern: "/api/{workspace_id}/repositories",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "*",
        path_pattern: "/api/{workspace_id}/repositories/{*rest}",
        role: RouteRole::IdeOnly,
    },
    // ── Workspace-level onboarding subtree (clones + scaffolds) ─────────────
    ManifestEntry {
        method: "*",
        path_pattern: "/api/{workspace_id}/onboarding/{*rest}",
        role: RouteRole::IdeOnly,
    },
    // `onboarding-readiness` is mounted FLAT (no `/onboarding/` segment) and
    // reads config.yml from the working copy via resolve_workspace_path +
    // ConfigBuilder::with_workspace_path — IdeOnly. (`onboarding/github-setup`
    // is already covered by the `/onboarding/{*rest}` wildcard above.)
    ManifestEntry {
        method: "GET",
        path_pattern: "/api/{workspace_id}/onboarding-readiness",
        role: RouteRole::IdeOnly,
    },
    // ── Modeling / Airform — dbt projects live on disk under modeling/ ─────
    // EVERY method, and the bare `/modeling` list root, read the working copy —
    // not just POST. A method-specific entry silently left the GETs
    // (list/info/nodes/lineage) FleetOk → 404 on a serve replica with no checkout.
    ManifestEntry {
        method: "*",
        path_pattern: "/api/{workspace_id}/modeling",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "*",
        path_pattern: "/api/{workspace_id}/modeling/{*rest}",
        role: RouteRole::IdeOnly,
    },
    // ── Workspace creation (clones the repo + scaffolds config.yml) ────────
    // Lives under /orgs/{org_id}/onboarding, NOT /workspaces (which doesn't
    // exist as a POST endpoint).
    ManifestEntry {
        method: "POST",
        path_pattern: "/api/orgs/{org_id}/onboarding/demo",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "POST",
        path_pattern: "/api/orgs/{org_id}/onboarding/new",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "POST",
        path_pattern: "/api/orgs/{org_id}/onboarding/github",
        role: RouteRole::IdeOnly,
    },
    // ── Branch switch — moves HEAD on the working copy ─────────────────────
    ManifestEntry {
        method: "POST",
        path_pattern: "/api/{workspace_id}/switch-branch",
        role: RouteRole::IdeOnly,
    },
    // ── Git / working-copy STATE reads ──────────────────────────────────────
    // These read git mode + branch straight off the working copy
    // (`workspace_root.exists()` + `detect_git_mode()` in
    // `api/workspaces.rs`). On a serve replica with no PVC they hit the
    // "Workspace directory not found" degrade path: git_mode=none, branch=None.
    // The compile boundary materialises *definitions*, not git state, so these
    // must run on the node that owns the checkout.
    ManifestEntry {
        method: "GET",
        path_pattern: "/api/{workspace_id}/details",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "GET",
        path_pattern: "/api/{workspace_id}/status",
        role: RouteRole::IdeOnly,
    },
    // Worktree lifecycle diagnostic — reads the ide-local worktree registry.
    ManifestEntry {
        method: "GET",
        path_pattern: "/api/{workspace_id}/worktrees",
        role: RouteRole::IdeOnly,
    },
    // ── Process-local live SSE (BROADCASTER) ────────────────────────────────
    // The legacy workflow / agentic-task live streams subscribe to an
    // in-memory `BROADCASTER` (`api/{run,task,world_model}.rs`). The producing
    // run lives on a DIFFERENT process, so a worker-less serve replica's
    // broadcaster is empty and the stream truncates silently. Pin to the ide
    // singleton (the in-process run owner). The MODERN agentic surfaces
    // (/analytics, /agentic-workflows, /agentic-airway) are pinned to the ide
    // too — see the "agentic run/exec surface" section just below.
    ManifestEntry {
        method: "GET",
        path_pattern: "/api/{workspace_id}/events",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "GET",
        path_pattern: "/api/{workspace_id}/events/{*rest}",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "GET",
        path_pattern: "/api/{workspace_id}/world-model/events",
        role: RouteRole::IdeOnly,
    },
    // ── Agentic run/exec surface → ide singleton (ephemeral-env tier 1) ──────
    // An analytics run delegates to automation subruns enqueued `TaskScope::
    // Scoped`, so a subrun's `execute_sql` runs IN-PROCESS on whichever node
    // drives the analytics run (co-located coordinator; see
    // `agentic-pipeline::workflow_run`). A stateless serve replica has no working
    // copy, so authored file-path SQL (`FROM 'oxymart.csv'`, resolved via DuckDB
    // `file_search_path`) and any FS-touching automation / airway step fail there.
    // Per the ephemeral-env tier-1 posture (one ide owns the FS; the fleet serves
    // reads), pin the whole run/exec surface to the ide — the serve fleet
    // self-proxies via `OXY_IDE_UPSTREAM`. This DELIBERATELY reverts the earlier
    // "modern agentic streams are FleetOk via Postgres LISTEN/NOTIFY" decision
    // for these surfaces; the cross-process plumbing still exists but is unused
    // while runs are ide-pinned. Revisit when a fleet replica can run file-SQL
    // (compile-time rewrite of file refs → mirrored views, or a per-connection
    // S3→tmp materialise with `file_search_path`).
    ManifestEntry {
        method: "*",
        path_pattern: "/api/{workspace_id}/analytics/{*rest}",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "*",
        path_pattern: "/api/{workspace_id}/agentic-workflows/{*rest}",
        role: RouteRole::IdeOnly,
    },
    // Canonical alias (Procedures/Workflows -> Automations); same posture as
    // `/agentic-workflows`.
    ManifestEntry {
        method: "*",
        path_pattern: "/api/{workspace_id}/agentic-automations/{*rest}",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "*",
        path_pattern: "/api/{workspace_id}/agentic-airway/{*rest}",
        role: RouteRole::IdeOnly,
    },
    // ── Generated chart files (local disk, NO cross-node fallback) ──────────
    // Charts (`.oxy_state/charts`, `chart::get_chart` → `fs::read_to_string`,
    // 404 on miss) and exported charts (`exported_chart`, 404 on miss) live on
    // the local disk of whoever EXECUTED the run, with no S3 read-through — a
    // serve replica has neither, so it 404s. Pin to the ide node (best-effort;
    // the durable fix is an S3 mirror like the parquet cache already has).
    //
    // NB: `/results/files` is deliberately NOT listed — it IS fleet-safe.
    // `result_files::{store,get}` mirror the parquet to S3 and read it back on
    // a local miss (`runtime_artifact::{mirror,fetch}`), so any replica can
    // serve it under the Postgres+S3 fleet contract. Pinning it to ide would
    // reintroduce the SPOF the mirror removed. It stays FleetOk.
    ManifestEntry {
        method: "GET",
        path_pattern: "/api/{workspace_id}/charts/{file_path}",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "GET",
        path_pattern: "/api/{workspace_id}/exported-charts/{file_name}",
        role: RouteRole::IdeOnly,
    },
    // ── Customer-Apps EXECUTION surface (public `/api/projects/*`) ───────────
    // Bundle-SDK endpoints that build a WorkspaceManager from the working copy
    // (`build_project_context` → `effective_workspace_path`) and RUN inline:
    // query/semantic-query execute SQL over the local connector; agent asks +
    // automation runs discover the agent/automation from disk (`list_workflows`
    // + `fs::read_to_string`) and execute in-process. On a stateless serve
    // replica with no working copy these 500 / NOT_FOUND, so pin them to the
    // ide (the fleet self-proxies via OXY_IDE_UPSTREAM) — same posture as the
    // workspace `/analytics` surface. The poll/cancel/stream siblings read run
    // state from Postgres and stay FleetOk. These live in `router/public.rs`,
    // OUTSIDE build_workspace_routes, so the workspace-mount drift test cannot
    // see them — `customer_app_execution_routes_are_ide_only` pins them instead.
    ManifestEntry {
        method: "POST",
        path_pattern: "/api/projects/{project_id}/query",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "POST",
        path_pattern: "/api/projects/{project_id}/semantic-query",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "POST",
        path_pattern: "/api/projects/{project_id}/agents/{agent_id}/asks",
        role: RouteRole::IdeOnly,
    },
    ManifestEntry {
        method: "POST",
        path_pattern: "/api/projects/{project_id}/procedures/{procedure_id}/runs",
        role: RouteRole::IdeOnly,
    },
    // ── Customer-Apps FUNCTION invocation (`POST /customer-apps/<org>/<slug>/fn/<name>`) ──
    // Oxy Functions execute IN-PROCESS on the isolate runtime, and their data
    // plane builds a WorkspaceManager from the working copy
    // (`build_project_context` → `effective_workspace_path`) and reads the
    // semantic layer from disk (`ctx.semantic` → `semantics_scan_path` +
    // `resolve_and_compile`). On a stateless serve replica with no working copy
    // the invocation 500s, so pin it to the ide (the fleet self-proxies via
    // OXY_IDE_UPSTREAM). The isolate and the JS artifact (S3 build-store) are
    // themselves fleet-safe — only the FS-bound config/semantic reads force this;
    // lifting them onto the compile boundary would make this FleetOk (see the §4
    // fleet note in `2026-06-12-customer-apps-functions-design.md`). Static bundle
    // assets under `/customer-apps/<org>/<slug>/...` stay FleetOk (served from
    // S3): the 5-segment `.../fn/{name}` pattern matches ONLY the execution entry
    // point, never asset serving.
    ManifestEntry {
        method: "POST",
        path_pattern: "/customer-apps/{org}/{app}/fn/{name}",
        role: RouteRole::IdeOnly,
    },
    // NB: the on-demand single-workspace health eval
    // (`POST /api/admin/workspace-health/{workspace_id}/eval`) is intentionally
    // NOT here — it is a pure Postgres enqueue (FS-free) and serves FleetOk by
    // default. The heavy eval (workspace-context build + reconcile.yml FS
    // fallthrough) runs in the fleet executor that drains the Global
    // `health_eval_workspace` task, which lands on an FS-owning node regardless
    // of where the request was accepted. See `workspace_health_eval_is_fleet_ok`.
];

/// Set once at process startup. Subsequent calls to
/// [`init_process_role_from_env`] are no-ops; the OnceLock::set return is
/// intentionally ignored so callers can re-invoke harmlessly.
///
/// Note for tests: nextest runs each test in its own process, so the
/// integration tests in [`super::role_middleware`] can call
/// `init_process_role_from_env` repeatedly with different `OXY_ROLE`
/// values and observe the change. Under `cargo test` (shared process)
/// the second-onward call is a no-op and tests would observe a stale
/// role + race on the shared env var. CLAUDE.md mandates nextest, so
/// this isn't a practical problem — documented here so a future
/// debugger doesn't have to discover it the hard way.
static PROCESS_ROLE: OnceLock<Role> = OnceLock::new();

pub fn init_process_role_from_env() -> Role {
    let role = match std::env::var("OXY_ROLE").ok().as_deref() {
        Some("ide") => Role::Ide,
        Some("serve") => Role::Serve,
        Some("worker") => Role::Worker,
        Some("all") | None => Role::All,
        Some(other) => {
            tracing::warn!(
                value = other,
                "OXY_ROLE: unrecognised value; defaulting to 'all'"
            );
            Role::All
        }
    };
    let _ = PROCESS_ROLE.set(role);
    tracing::info!(role = role.as_str(), "role manifest initialised");
    role
}

pub fn current_process_role() -> Role {
    *PROCESS_ROLE.get().unwrap_or(&Role::All)
}

/// Whether a node in this `role` drains the durable task queue in-process — runs
/// the durable task fleet **and** the global driver — vs. offloading to a
/// separate worker fleet. Only the stateless `serve` replica offloads; `all`
/// (a single all-in-one instance), `ide`, and `worker` all drain in-process.
///
/// This is the invariant that makes a single `OXY_ROLE=all` instance
/// self-sufficient: it runs its own worker, so scheduled + manual jobs (and
/// compiles) execute without a second node. Both role-derived gates
/// (`ServeArgs::workers_disabled`, `recovery::inproc_global_worker_enabled`)
/// route their role branch through here so the invariant has one tested source
/// of truth. Explicit env/flag overrides (`--no-workers`,
/// `OXY_DISABLE_INPROCESS_WORKERS`, `OXY_INPROC_GLOBAL_WORKER`) still win at
/// those call sites.
pub fn role_runs_inprocess_workers(role: Role) -> bool {
    !matches!(role, Role::Serve)
}

/// Whether this process owns a workspace filesystem — i.e. may write the working
/// copy / `.git` / local state dir. True for every role EXCEPT the stateless
/// `serve` replica, which owns no filesystem and serves reads from the compile
/// boundary only.
pub fn process_is_fs_writable() -> bool {
    current_process_role() != Role::Serve
}

/// `super_read_only` guard for the workspace filesystem — the Postgres
/// `default_transaction_read_only` / LiteFS-replica analog. Call it as the first
/// line of any handler that MUTATES the working copy / `.git` / local state dir.
///
/// In correct operation it never fires: such routes are classified `IdeOnly`, so
/// `role_middleware` already refused/proxied them and the handler never ran on a
/// `serve` replica. The guard is **classification-independent** defense in depth:
/// if a write route is ever MISclassified `FleetOk` and reaches a stateless
/// replica (the gap the HTTP-edge drift tests can't fully close — see the #2543
/// FS-buried-in-a-domain class), the write fails loudly HERE instead of silently
/// losing data or mutating an ephemeral filesystem. The structural end-state
/// (the serve binary links no FS code at all) makes this a compile error; until
/// then this is the runtime safety net. See
/// `internal-docs/multi-instance-fleet.md`.
pub fn ensure_fs_writable(operation: &str) -> Result<(), OxyError> {
    if process_is_fs_writable() {
        return Ok(());
    }
    Err(OxyError::RuntimeError(format!(
        "refused workspace filesystem write ({operation}) on a stateless serve \
         replica — writes must run on the filesystem-owning environment (the ide). \
         This indicates a route-classification bug: the route should be IdeOnly."
    )))
}

/// Read-only endpoints that sit UNDER an otherwise-`IdeOnly` wildcard but are
/// pure Postgres reads — so they are FleetOk and win over the wildcard. Checked
/// before [`IDE_ONLY_PATTERNS`] in [`classify`].
///
/// The motivating case (HA): the agentic run/exec surface `/analytics/{*rest}`
/// is `IdeOnly` because a LIVE run executes in-process on the ide and touches
/// the FS. But VIEWING a past conversation only READS run history from Postgres
/// (`list_runs_by_thread` / `get_run_by_thread` in `agentic-http`, `state.db`
/// only — no workspace, no FS). Pinning those to the ide made loading a thread
/// depend on the singleton — a thread is *data*, not a live run, so it must
/// serve from any replica. These two reads are carved back out to FleetOk.
///
/// STRICT RULE: only add an entry here after verifying the handler touches NO
/// workspace FS / `.git` / local state and NO in-process live stream — a pure
/// Postgres (or S3) read. Anything that executes, edits, or streams a live run
/// stays `IdeOnly`.
const FLEET_OK_READ_PATTERNS: &[ManifestEntry] = &[
    // Cross-tenant health rollup — pure Postgres read, safe on any replica.
    ManifestEntry {
        method: "GET",
        path_pattern: "/api/admin/workspace-health",
        role: RouteRole::FleetOk,
    },
    // Analytics conversation history — list_runs_by_thread / get_run_by_thread
    // (agentic-http, `state.db` only).
    ManifestEntry {
        method: "GET",
        path_pattern: "/api/{workspace_id}/analytics/threads/{thread_id}/runs",
        role: RouteRole::FleetOk,
    },
    ManifestEntry {
        method: "GET",
        path_pattern: "/api/{workspace_id}/analytics/threads/{thread_id}/run",
        role: RouteRole::FleetOk,
    },
    // Workflow run history — list_runs_for_workflow (GET /runs) /
    // get_workflow_run (GET /runs/{id}) / latest_run_for_thread
    // (GET /threads/{id}/run). All `state.db` snapshots; the live `/runs/{id}/
    // events` SSE (one more segment) and POST create/cancel stay IdeOnly.
    ManifestEntry {
        method: "GET",
        path_pattern: "/api/{workspace_id}/agentic-workflows/runs",
        role: RouteRole::FleetOk,
    },
    ManifestEntry {
        method: "GET",
        path_pattern: "/api/{workspace_id}/agentic-workflows/runs/{run_id}",
        role: RouteRole::FleetOk,
    },
    ManifestEntry {
        method: "GET",
        path_pattern: "/api/{workspace_id}/agentic-workflows/threads/{thread_id}/run",
        role: RouteRole::FleetOk,
    },
    // Automation run-history aliases (mirror the agentic-workflows carve-outs).
    ManifestEntry {
        method: "GET",
        path_pattern: "/api/{workspace_id}/agentic-automations/runs",
        role: RouteRole::FleetOk,
    },
    ManifestEntry {
        method: "GET",
        path_pattern: "/api/{workspace_id}/agentic-automations/runs/{run_id}",
        role: RouteRole::FleetOk,
    },
    ManifestEntry {
        method: "GET",
        path_pattern: "/api/{workspace_id}/agentic-automations/threads/{thread_id}/run",
        role: RouteRole::FleetOk,
    },
    // Airway pipeline run history — list_runs_for_pipeline (GET /runs, `state.db`).
    ManifestEntry {
        method: "GET",
        path_pattern: "/api/{workspace_id}/agentic-airway/runs",
        role: RouteRole::FleetOk,
    },
    // Airway backfill coverage — airway_coverage (GET /coverage, `state.db`). A
    // read like run history, so it serves off the stateless fleet; the
    // chunked-backfill write stays on the ide node via the wildcard below.
    ManifestEntry {
        method: "GET",
        path_pattern: "/api/{workspace_id}/agentic-airway/coverage",
        role: RouteRole::FleetOk,
    },
    // Airway backfill ranges — airway_backfill_ranges (GET /backfill-ranges,
    // `state.db`). A workspace-scoped read like coverage/runs, and the entry
    // point for the coverage gantt (the UI lists ranges before drilling into a
    // range's coverage), so it must serve off the stateless fleet too.
    ManifestEntry {
        method: "GET",
        path_pattern: "/api/{workspace_id}/agentic-airway/backfill-ranges",
        role: RouteRole::FleetOk,
    },
];

/// Classify `(method, request_uri_path)`. Returns [`RouteRole::FleetOk`] when
/// no entry matches.
pub fn classify(method: &str, request_path: &str) -> RouteRole {
    // FleetOk read carve-outs win over the broad IdeOnly wildcards (e.g. the
    // analytics run-history reads under the otherwise-IdeOnly `/analytics`
    // execution surface — so viewing a conversation never needs the ide).
    for entry in FLEET_OK_READ_PATTERNS {
        if (entry.method == "*" || entry.method == method)
            && pattern_matches(entry.path_pattern, request_path)
        {
            return entry.role;
        }
    }
    for entry in IDE_ONLY_PATTERNS {
        if (entry.method == "*" || entry.method == method)
            && pattern_matches(entry.path_pattern, request_path)
        {
            return entry.role;
        }
    }
    RouteRole::FleetOk
}

/// `IdeOnly` routes that DEGRADE GRACEFULLY when the ide is unreachable. These
/// are read-only git **state** reads (`/details`, `/status`) the workspace page
/// needs to render but whose live value is non-essential: a dead ide must not
/// take the page down. When forwarding one of these fails to reach the ide, a
/// serve replica serves it from its own (working-copy-less) handler instead,
/// which returns `git_mode: None` — git ops correctly shown as unavailable —
/// rather than a 502.
///
/// File content (`/files/...`), compile, and git **writes** are deliberately
/// NOT here: they genuinely need the ide's working copy, so a dead ide is an
/// honest 502 for them. Keep this list to read-only state with a sensible
/// degraded form.
pub fn degrades_when_ide_unreachable(method: &str, request_path: &str) -> bool {
    method == "GET"
        && (pattern_matches("/api/{workspace_id}/details", request_path)
            || pattern_matches("/api/{workspace_id}/status", request_path)
            // The modeling LIST (dbt projects) is a read with a sensible empty
            // degrade: a serve replica has no `modeling/` dir, so the local
            // handler returns `[]` rather than 502 a homepage readiness check.
            // The modeling SUB-routes (`/modeling/{*rest}`) stay non-degradable —
            // they genuinely need the working copy.
            || pattern_matches("/api/{workspace_id}/modeling", request_path)
            // Legacy visualize-task chart files: the ide writes them to local
            // disk AND mirrors each to S3 when it serves it (see `api/chart::
            // get_chart`). With the ide down, the replica's local handler misses
            // on disk and serves the S3 mirror — so a previously-viewed chart
            // keeps loading instead of 502ing. A never-mirrored chart 404s, which
            // the FE renders as the calm "chart unavailable" panel.
            || pattern_matches("/api/{workspace_id}/charts/{file_path}", request_path))
}

/// `IdeOnly` routes whose ide-down failure is a WORKSPACE-RUNTIME outage — they
/// need the ide's local DuckDB execution environment (the connector's
/// `file_search_path` over working-copy data files), a generated run artifact
/// (chart files on local disk), or a live in-process run stream. This is the
/// subset still coupled to DuckDB / local execution, deliberately named so the
/// fleet can isolate it.
///
/// It is distinct from the *editing* IdeOnly routes (file CRUD, git, compile,
/// modeling) that need the git working copy for AUTHORING. The split lets a
/// serve replica tell the two apart when the ide is unreachable and signal the
/// FE precisely: a runtime outage pauses data / charts / runs while browsing the
/// last compiled revision stays fully available; an authoring outage pauses
/// editing. See `ide_proxy::forward_to_ide`.
///
/// Like [`degrades_when_ide_unreachable`], this is a hand-maintained subset of
/// [`IDE_ONLY_PATTERNS`]: a new DuckDB-bound IdeOnly route must be added here
/// too. The `runtime_routes_are_ide_only` test guards that every pattern here is
/// genuinely IdeOnly (catching typos / stale paths), but cannot know that a
/// future runtime route was forgotten — keep this list in sync by hand.
pub fn is_workspace_runtime_route(request_path: &str) -> bool {
    const RUNTIME_PATTERNS: &[&str] = &[
        // Data-app EXECUTION — AppService::run() over the local DuckDB connector
        // + file_search_path (auto-run on load, explicit run, cached result).
        "/api/{workspace_id}/apps/{pathb64}",
        "/api/{workspace_id}/apps/{pathb64}/run",
        "/api/{workspace_id}/apps/{pathb64}/result",
        // App DATA read — get_data() over the local state dir.
        "/api/{workspace_id}/apps/file/{pathb64}",
        // Agentic run/exec surface — subrun execute_sql runs in-process on the
        // node driving the run, against the local DuckDB connector.
        "/api/{workspace_id}/analytics/{*rest}",
        "/api/{workspace_id}/agentic-workflows/{*rest}",
        "/api/{workspace_id}/agentic-automations/{*rest}",
        "/api/{workspace_id}/agentic-airway/{*rest}",
        // Generated run artifacts — chart files on the executor's local disk.
        "/api/{workspace_id}/charts/{file_path}",
        "/api/{workspace_id}/exported-charts/{file_name}",
        // Live in-process run streams — the producing run executes on the ide.
        "/api/{workspace_id}/events",
        "/api/{workspace_id}/events/{*rest}",
        "/api/{workspace_id}/world-model/events",
    ];
    RUNTIME_PATTERNS
        .iter()
        .any(|p| pattern_matches(p, request_path))
}

pub fn dump_manifest() -> Vec<(&'static str, &'static str, &'static str)> {
    IDE_ONLY_PATTERNS
        .iter()
        .map(|e| (e.method, e.path_pattern, e.role.as_str()))
        .collect()
}

/// Segment-by-segment match. `{name}` matches one path segment;
/// `{*name}` matches one or more trailing segments. Otherwise segments
/// must compare equal.
fn pattern_matches(pattern: &str, path: &str) -> bool {
    let mut pat = pattern.trim_start_matches('/').split('/');
    let mut req = path.trim_start_matches('/').split('/');
    loop {
        match (pat.next(), req.next()) {
            (None, None) => return true,
            (Some(_), None) | (None, Some(_)) => return false,
            (Some(p), Some(r)) => {
                if is_rest_wildcard(p) {
                    return !r.is_empty();
                }
                if is_param(p) {
                    if r.is_empty() {
                        return false;
                    }
                    continue;
                }
                if p != r {
                    return false;
                }
            }
        }
    }
}

fn is_param(seg: &str) -> bool {
    seg.starts_with('{') && seg.ends_with('}') && !seg.starts_with("{*")
}

fn is_rest_wildcard(seg: &str) -> bool {
    seg.starts_with("{*") && seg.ends_with('}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_serve_offloads_workers_single_all_in_one_drains_its_own_queue() {
        // The single-instance invariant: a plain `OXY_ROLE=all` node (and ide /
        // worker) runs the durable fleet + global driver in-process, so
        // scheduled + manual jobs execute without a second node. Only the
        // stateless `serve` replica offloads to the worker fleet. Pure over
        // `Role`, so it can assert every role without touching the process-role
        // OnceLock. Guards the DX regression where a lone node silently queues
        // jobs forever with nothing draining them.
        assert!(
            role_runs_inprocess_workers(Role::All),
            "a single OXY_ROLE=all instance must drain its own queue"
        );
        assert!(role_runs_inprocess_workers(Role::Ide));
        assert!(role_runs_inprocess_workers(Role::Worker));
        assert!(
            !role_runs_inprocess_workers(Role::Serve),
            "serve is a pure reader — it offloads to the worker fleet"
        );
    }

    #[test]
    fn pattern_matches_concrete_uri() {
        // Single-segment params
        assert!(pattern_matches(
            "/api/{workspace_id}/compile",
            "/api/abc-123/compile"
        ));
        // Rest wildcards consume one OR more trailing segments
        assert!(pattern_matches(
            "/api/{workspace_id}/git/{*rest}",
            "/api/abc/git/status"
        ));
        assert!(pattern_matches(
            "/api/{workspace_id}/git/{*rest}",
            "/api/abc/git/commit/sha"
        ));
        // Non-match: wrong prefix
        assert!(!pattern_matches(
            "/api/{workspace_id}/compile",
            "/api/abc-123/threads"
        ));
        // Non-match: missing segment
        assert!(!pattern_matches(
            "/api/{workspace_id}/compile",
            "/api/abc-123"
        ));
        // Non-match: rest wildcard requires at least one segment
        assert!(!pattern_matches(
            "/api/{workspace_id}/git/{*rest}",
            "/api/abc/git"
        ));
    }

    #[test]
    fn ide_routes_classify_against_live_uri() {
        // The real shape the middleware sees: `/api` prefix + nested workspace.
        assert_eq!(
            classify("POST", "/api/d9830be4-c6a4/compile"),
            RouteRole::IdeOnly
        );
        assert_eq!(
            classify("POST", "/api/d9830be4-c6a4/files/cGF0aA"),
            RouteRole::IdeOnly
        );
        assert_eq!(
            classify("DELETE", "/api/d9830be4-c6a4/files/cGF0aA/delete-file"),
            RouteRole::IdeOnly
        );
        // Real flat git route (NOT under `/git/` — that segment doesn't exist).
        assert_eq!(
            classify("POST", "/api/d9830be4-c6a4/pull-changes"),
            RouteRole::IdeOnly
        );
        assert_eq!(
            classify("POST", "/api/d9830be4-c6a4/onboarding/demo"),
            RouteRole::IdeOnly
        );
        assert_eq!(
            classify("POST", "/api/d9830be4-c6a4/modeling/run"),
            RouteRole::IdeOnly
        );
        assert_eq!(
            classify("POST", "/api/d9830be4-c6a4/switch-branch"),
            RouteRole::IdeOnly
        );
        assert_eq!(
            classify("POST", "/api/orgs/some-org/onboarding/github"),
            RouteRole::IdeOnly
        );
        // Git/working-copy STATE reads (regression: "Workspace directory not
        // found" on a serve replica).
        assert_eq!(
            classify("GET", "/api/d9830be4-c6a4/details"),
            RouteRole::IdeOnly
        );
        assert_eq!(
            classify("GET", "/api/d9830be4-c6a4/status"),
            RouteRole::IdeOnly
        );
        // Process-local BROADCASTER live SSE (regression: silent truncation on
        // a worker-less serve replica).
        assert_eq!(
            classify("GET", "/api/d9830be4-c6a4/events"),
            RouteRole::IdeOnly
        );
        assert_eq!(
            classify("GET", "/api/d9830be4-c6a4/events/lookup"),
            RouteRole::IdeOnly
        );
        assert_eq!(
            classify("GET", "/api/d9830be4-c6a4/world-model/events"),
            RouteRole::IdeOnly
        );
    }

    /// The agentic run/exec + generated-chart surface must stay IdeOnly: a
    /// subrun's `execute_sql` runs in-process against local DuckDB, and charts
    /// are read off local disk. Flipping any of these to FleetOk would serve
    /// them on a no-working-copy replica — and for `/analytics` specifically it
    /// would bypass the runtime serve-safety gate (the conditional un-pin),
    /// breaking every workspace with an FS-bound database. This test is the
    /// regression guard for that whole surface.
    #[test]
    fn run_exec_and_chart_surface_stays_ide_only() {
        let ws = "d9830be4-c6a4";
        let cases: [(&str, String); 8] = [
            ("POST", format!("/api/{ws}/analytics/runs")),
            ("POST", format!("/api/{ws}/analytics/runs/abc/answer")),
            ("POST", format!("/api/{ws}/agentic-workflows/run")),
            ("POST", format!("/api/{ws}/agentic-airway/run")),
            ("GET", format!("/api/{ws}/charts/chart-123.png")),
            ("GET", format!("/api/{ws}/exported-charts/x.svg")),
            ("GET", format!("/api/{ws}/apps/cGF0aA")), // data-app auto-run
            ("POST", format!("/api/{ws}/apps/cGF0aA/run")), // data-app run
        ];
        for (method, path) in cases {
            assert_eq!(
                classify(method, &path),
                RouteRole::IdeOnly,
                "{method} {path} must stay IdeOnly"
            );
        }
    }

    /// Viewing a past conversation / run is a Postgres read and must NOT depend
    /// on the ide singleton (HA). The run-history reads across all three agentic
    /// surfaces are carved out to FleetOk even though they sit under the IdeOnly
    /// `/analytics` `/agentic-workflows` `/agentic-airway` wildcards; the
    /// EXECUTION + live-stream + file-read endpoints right next to them stay
    /// IdeOnly.
    #[test]
    fn agentic_run_history_reads_are_fleet_ok() {
        let ws = "d9830be4-c6a4";
        // Pure Postgres reads → FleetOk (serve from any replica).
        for path in [
            format!("/api/{ws}/analytics/threads/t-1/runs"), // list_runs_by_thread
            format!("/api/{ws}/analytics/threads/t-1/run"),  // get_run_by_thread
            format!("/api/{ws}/agentic-workflows/runs"),     // list_runs_for_workflow
            format!("/api/{ws}/agentic-workflows/runs/r-1"), // get_workflow_run
            format!("/api/{ws}/agentic-workflows/threads/t-1/run"), // latest_run_for_thread
            format!("/api/{ws}/agentic-airway/runs"),        // list_runs_for_pipeline
        ] {
            assert_eq!(
                classify("GET", &path),
                RouteRole::FleetOk,
                "{path} is a Postgres run-history read — must serve from any replica"
            );
        }
        // Execution / live-stream / FS-write endpoints under the same surfaces
        // stay IdeOnly — the carve-out must not widen to them. Note the
        // live-SSE `runs/{id}/events` has one MORE segment than the carved-out
        // `runs/{id}`, so it is not shadowed.
        for (method, path) in [
            ("POST", format!("/api/{ws}/analytics/runs")), // start (executes)
            ("GET", format!("/api/{ws}/analytics/runs/r-1/events")), // live SSE
            ("POST", format!("/api/{ws}/analytics/runs/r-1/answer")), // resume
            (
                "POST",
                format!("/api/{ws}/analytics/runs/r-1/revert-file-changes"),
            ), // FS write
            ("POST", format!("/api/{ws}/agentic-workflows/runs")), // start
            (
                "GET",
                format!("/api/{ws}/agentic-workflows/runs/r-1/events"),
            ), // live SSE
            (
                "POST",
                format!("/api/{ws}/agentic-workflows/runs/r-1/cancel"),
            ),
            ("GET", format!("/api/{ws}/agentic-workflows/files")), // workspace FS read
            ("POST", format!("/api/{ws}/agentic-airway/runs")),    // start
            ("GET", format!("/api/{ws}/agentic-airway/runs/r-1/events")), // live SSE
        ] {
            assert_eq!(
                classify(&method, &path),
                RouteRole::IdeOnly,
                "{method} {path} executes/streams/reads-FS — must stay IdeOnly"
            );
        }
    }

    #[test]
    fn workspace_health_is_fleet_ok() {
        assert_eq!(
            classify("GET", "/api/admin/workspace-health"),
            RouteRole::FleetOk
        );
    }

    #[test]
    fn workspace_health_eval_is_fleet_ok() {
        // The on-demand eval handler is a pure Postgres enqueue: it seeds a
        // Global `health_eval_workspace` task and returns 202. The heavy work
        // (workspace-context build + reconcile.yml FS fallthrough) runs in the
        // fleet executor that drains the task, NOT in this handler — route class
        // doesn't govern task execution. So the POST is FS-free and must serve
        // FleetOk: pinning it IdeOnly would block an operator from triggering an
        // eval whenever the ide is down, undercutting the offload's whole point.
        let ws = "d9830be4-c6a4";
        assert_eq!(
            classify("POST", &format!("/api/admin/workspace-health/{ws}/eval")),
            RouteRole::FleetOk
        );
    }

    #[test]
    fn runtime_routes_are_ide_only() {
        // The DuckDB / local-execution subset isolated by
        // `is_workspace_runtime_route`. Every one must (a) classify as runtime
        // and (b) be genuinely IdeOnly — a runtime path that fell to FleetOk
        // would never reach the proxy's unreachable arm, making the signal a lie.
        let ws = "d9830be4-c6a4";
        let runtime: [(&str, String); 9] = [
            ("GET", format!("/api/{ws}/apps/cGF0aA")),
            ("POST", format!("/api/{ws}/apps/cGF0aA/run")),
            ("POST", format!("/api/{ws}/apps/cGF0aA/result")),
            ("GET", format!("/api/{ws}/apps/file/cGF0aA")),
            ("POST", format!("/api/{ws}/analytics/runs")),
            ("POST", format!("/api/{ws}/agentic-airway/run")),
            ("GET", format!("/api/{ws}/charts/chart-123.png")),
            ("GET", format!("/api/{ws}/events")),
            ("GET", format!("/api/{ws}/world-model/events")),
        ];
        for (method, path) in &runtime {
            assert!(
                is_workspace_runtime_route(path),
                "{path} must be a workspace-runtime route"
            );
            assert_eq!(
                classify(method, path),
                RouteRole::IdeOnly,
                "{method} {path} must stay IdeOnly"
            );
        }

        // Editing-bound IdeOnly routes are NOT runtime — they need the git
        // working copy, not the DuckDB env, so their ide-down message differs.
        for path in [
            format!("/api/{ws}/files/cGF0aA"),
            format!("/api/{ws}/compile"),
            format!("/api/{ws}/branches"),
            format!("/api/{ws}/modeling"),
            format!("/api/{ws}/details"),
        ] {
            assert!(
                !is_workspace_runtime_route(&path),
                "{path} is editing-bound, not runtime"
            );
        }

        // FleetOk routes never reach the proxy, so they are not runtime either.
        assert!(!is_workspace_runtime_route(&format!("/api/{ws}/threads")));
        assert!(!is_workspace_runtime_route(&format!(
            "/api/{ws}/apps/cGF0aA/displays"
        )));
    }

    #[test]
    fn unknown_routes_default_to_fleet_ok() {
        assert_eq!(
            classify("GET", "/api/d9830be4-c6a4/threads"),
            RouteRole::FleetOk
        );
        assert_eq!(classify("POST", "/api/analytics/runs"), RouteRole::FleetOk);
        assert_eq!(classify("GET", "/health"), RouteRole::FleetOk);
        assert_eq!(classify("GET", "/healthz"), RouteRole::FleetOk);
        // The agentic run/exec surface (/analytics, /agentic-workflows,
        // /agentic-airway) is pinned to the ide for ephemeral-env tier 1 —
        // subruns execute in-process where the run drives and touch the FS — so
        // even the cross-process /events streams under it now classify IdeOnly.
        assert_eq!(
            classify("GET", "/api/d9830be4-c6a4/analytics/runs/abc/events"),
            RouteRole::IdeOnly
        );
        assert_eq!(
            classify(
                "GET",
                "/api/d9830be4-c6a4/agentic-workflows/runs/abc/events"
            ),
            RouteRole::IdeOnly
        );
        // /blocks reads persisted blocks from Postgres (no broadcaster) — FleetOk.
        assert_eq!(
            classify("GET", "/api/d9830be4-c6a4/blocks"),
            RouteRole::FleetOk
        );
        // Non-SSE world-model reads are FleetOk (only /world-model/events isn't).
        assert_eq!(
            classify("GET", "/api/d9830be4-c6a4/world-model/cameras"),
            RouteRole::FleetOk
        );
        // Customer-apps batch mutations touch only Postgres + the S3 build
        // store (never the workspace FS), so — like their per-app siblings —
        // they classify FleetOk by default and serve from any replica.
        assert_eq!(
            classify("POST", "/api/customer-apps/batch/publish"),
            RouteRole::FleetOk
        );
        assert_eq!(
            classify("POST", "/api/customer-apps/batch/promote-latest"),
            RouteRole::FleetOk
        );
        assert_eq!(
            classify("POST", "/api/customer-apps/batch/unpublish"),
            RouteRole::FleetOk
        );
        assert_eq!(
            classify("POST", "/api/customer-apps/batch/delete"),
            RouteRole::FleetOk
        );
    }

    #[test]
    fn org_subdomain_routes_are_fleet_ok() {
        // Both org-subdomain surfaces are Postgres-only (read workspace→org,
        // upsert the `org_subdomains` row) — no workspace FS — so they serve
        // from any replica, like `oxy-access`. See
        // `internal-docs/2026-06-22-org-subdomain-routing-design.md`.
        let id = "d9830be4-c6a4";
        // Customer read-only status (workspace-scoped).
        assert_eq!(
            classify("GET", &format!("/api/{id}/org-subdomain")),
            RouteRole::FleetOk,
        );
        // Oxy-staff control (admin surface).
        for method in ["GET", "PUT"] {
            assert_eq!(
                classify(method, &format!("/api/admin/orgs/{id}/subdomain")),
                RouteRole::FleetOk,
                "{method} admin org-subdomain must be FleetOk (Postgres-only)"
            );
        }
    }

    #[test]
    fn ide_only_accepted_by_ide_and_all_only() {
        let r = RouteRole::IdeOnly;
        assert!(r.accepted_by(Role::Ide));
        assert!(r.accepted_by(Role::All));
        assert!(!r.accepted_by(Role::Serve));
        assert!(!r.accepted_by(Role::Worker));
    }

    #[test]
    fn fleet_ok_accepted_by_every_role_including_worker() {
        let r = RouteRole::FleetOk;
        assert!(r.accepted_by(Role::Ide));
        assert!(r.accepted_by(Role::Serve));
        assert!(r.accepted_by(Role::Worker));
        assert!(r.accepted_by(Role::All));
    }

    // The next two tests set OXY_ROLE + init the process-role OnceLock; they
    // rely on nextest's per-test process isolation (CLAUDE.md mandates nextest),
    // the same pattern the role_middleware/types tests use.

    #[test]
    fn fs_write_guard_refuses_on_serve_role() {
        // SAFETY: nextest isolates this test in its own single-threaded process.
        unsafe { std::env::set_var("OXY_ROLE", "serve") };
        init_process_role_from_env();
        assert!(
            !process_is_fs_writable(),
            "serve replica owns no filesystem"
        );
        assert!(
            ensure_fs_writable("test write").is_err(),
            "serve replica must refuse a workspace FS write (super_read_only)"
        );
    }

    #[test]
    fn fs_write_guard_allows_fs_owning_roles() {
        // SAFETY: nextest isolates this test in its own single-threaded process.
        // `all` is the default; assert the guard is a no-op for an FS-owning role.
        unsafe { std::env::set_var("OXY_ROLE", "ide") };
        init_process_role_from_env();
        assert!(process_is_fs_writable(), "ide owns the working copy");
        assert!(
            ensure_fs_writable("test write").is_ok(),
            "an FS-owning role must permit workspace FS writes"
        );
    }

    #[test]
    fn ide_down_degradable_routes() {
        let ws = "/api/d9830be4-c6a4";
        // These IdeOnly reads serve a sensible degraded form on a serve replica
        // when the ide is unreachable (rather than 502) — see role_middleware.
        for path in [
            format!("{ws}/details"),
            format!("{ws}/status"),
            format!("{ws}/modeling"),
            // Charts degrade to the S3 mirror (get_chart mirrors on serve, reads
            // S3 on a local miss) so a previously-viewed chart survives ide-down.
            format!("{ws}/charts/sales-0-xyz.json"),
        ] {
            assert!(
                degrades_when_ide_unreachable("GET", &path),
                "{path} should degrade gracefully when the ide is unreachable"
            );
            // Still IdeOnly — degradation is the ide-DOWN path, not a reclassification.
            assert_eq!(
                classify("GET", &path),
                RouteRole::IdeOnly,
                "{path} stays IdeOnly"
            );
        }
        // Modeling SUB-routes + file content genuinely need the working copy —
        // they do NOT degrade (an honest 502 when the ide is down).
        assert!(!degrades_when_ide_unreachable(
            "GET",
            &format!("{ws}/modeling/p/lineage")
        ));
        assert!(!degrades_when_ide_unreachable(
            "GET",
            &format!("{ws}/files/cGF0aA")
        ));
    }

    #[test]
    fn method_wildcard_matches_any_verb() {
        // `/branches` is a `method: "*"` IdeOnly entry — both verbs match.
        assert_eq!(classify("GET", "/api/abc/branches"), RouteRole::IdeOnly);
        assert_eq!(
            classify("DELETE", "/api/abc/branches/feature-x"),
            RouteRole::IdeOnly
        );
    }

    #[test]
    fn app_data_and_source_file_reads_are_ide_only() {
        // Every handler that calls `AppService::run()` (executes the inline
        // automation's file-path SQL) is ide-pinned: get_app_data (GET
        // /apps/{pathb64}, the auto-run on load), run_app (POST .../run) and
        // get_app_result (POST .../result). The file/source reads
        // (get_source_file → workspace_path; get_data → local state dir) are
        // ide-pinned too. The non-executing surface stays fleet-served:
        // get_displays returns SQL templates for the FE to run.
        let ws = "/api/d9830be4-c6a4";
        assert_eq!(
            classify("GET", &format!("{ws}/apps/source/b3h5bWFydA")),
            RouteRole::IdeOnly
        );
        assert_eq!(
            classify("GET", &format!("{ws}/apps/file/b3h5bWFydA")),
            RouteRole::IdeOnly
        );
        // get_app_data runs the inline automation on a cold cache → ide.
        assert_eq!(
            classify("GET", &format!("{ws}/apps/b3h5bWFydA")),
            RouteRole::IdeOnly
        );
        assert_eq!(
            classify("POST", &format!("{ws}/apps/b3h5bWFydA/run")),
            RouteRole::IdeOnly
        );
        assert_eq!(
            classify("POST", &format!("{ws}/apps/b3h5bWFydA/result")),
            RouteRole::IdeOnly
        );
        // get_displays only emits SQL templates — no server-side run — so the
        // 4-segment GET pin above must NOT shadow it; it stays fleet-served.
        assert_eq!(
            classify("GET", &format!("{ws}/apps/b3h5bWFydA/displays")),
            RouteRole::FleetOk
        );
        // get_app_data_cached serves a dashboard's last cached data (boundary
        // def + disk/S3 cache, no execution) — the ide-down fallback. It MUST
        // stay FleetOk, or a serve replica would proxy it to a dead ide and
        // defeat the whole graceful-degradation feature.
        assert_eq!(
            classify("GET", &format!("{ws}/apps/b3h5bWFydA/data-cached")),
            RouteRole::FleetOk
        );
        // App WRITE surface mutates the working copy → IdeOnly (proxied to the
        // ide). FleetOk here would silently drop the publish toggle / generated
        // app on a working-copy-less replica.
        assert_eq!(
            classify("POST", &format!("{ws}/apps/b3h5bWFydA/publish")),
            RouteRole::IdeOnly
        );
        assert_eq!(
            classify("POST", &format!("{ws}/apps/b3h5bWFydA/unpublish")),
            RouteRole::IdeOnly
        );
        assert_eq!(
            classify("POST", &format!("{ws}/apps/save-from-run/run-123")),
            RouteRole::IdeOnly
        );
    }

    /// Router-DERIVED drift guard — the automated router ⇄ manifest cross-check
    /// the hand-maintained `manifest_covers_*` tests lack. Parses the route
    /// paths straight out of `router/workspace.rs` for the builders that are
    /// ENTIRELY filesystem/git (every route in them touches the working copy on
    /// the singleton) and asserts each classifies IdeOnly. A new route added to
    /// one of these builders is checked automatically — there is no separate
    /// list to forget.
    ///
    /// This canNOT cover MIXED builders (only some routes touch disk), e.g.
    /// `build_app_routes`'s `/source` + `/file` reads vs its fleet-served
    /// `/run` / `/result`. Those need a per-route test + the
    /// `oxy-route-classification` skill at authoring time — that gap is exactly
    /// how `/apps/source` shipped FleetOk and 404'd on the serve fleet.
    #[test]
    fn fully_fs_builder_routes_classify_ide_only() {
        let src = include_str!("router/workspace.rs");
        // (builder fn, mount prefix beneath /api/{workspace_id})
        let builders = [
            ("build_git_routes", ""),
            ("build_file_routes", "/files"),
            ("build_data_repo_routes", "/repositories"),
        ];
        let ws = "/api/d9830be4-c6a4";
        let mut checked = 0;
        for (builder, prefix) in builders {
            for route in route_paths_in_fn(src, builder) {
                let tail = if route == "/" { "" } else { route.as_str() };
                let concrete = concretize(&format!("{ws}{prefix}{tail}"));
                assert!(
                    ["GET", "POST", "PUT", "DELETE", "PATCH"]
                        .iter()
                        .any(|m| classify(m, &concrete) == RouteRole::IdeOnly),
                    "{prefix}{route} (mounted by {builder}) classifies FleetOk — every \
                     route in a fully-filesystem builder needs an IDE_ONLY_PATTERNS entry, \
                     or a serve replica with no working copy 404s/500s it. Add it above.",
                );
                checked += 1;
            }
        }
        assert!(
            checked >= 25,
            "parsed only {checked} routes from the FS builders — the source parser likely \
             broke; fix the parser rather than weakening the guard",
        );
    }

    /// Per-sub-route drift guard for the MIXED `build_app_routes()` builder —
    /// the structural backstop the file previously lacked. `fully_fs_builder_*`
    /// deliberately can't cover MIXED builders, and `every_workspace_mount_*`
    /// only sees the `/apps` NEST (its one-segment probe matches the IdeOnly
    /// `/apps/{pathb64}` pattern), so every app SUB-route was otherwise
    /// classified by reviewer attention alone — exactly how publish/unpublish/
    /// save-from-run wrote the working copy while classified FleetOk. This
    /// parses `build_app_routes()` and asserts each sub-route is EITHER IdeOnly
    /// OR on the explicit `APP_FLEET_OK` ack-list below, so a new app sub-route
    /// fails CI unless someone classifies it on purpose.
    #[test]
    fn every_app_sub_route_is_classified() {
        let src = include_str!("router/workspace.rs");
        // App sub-routes intentionally FleetOk: served from the compile boundary
        // / S3 / Postgres, never the working copy. REVIEW before adding one — if
        // a handler reads OR writes the working copy / local state dir, it
        // belongs in IDE_ONLY_PATTERNS, not here.
        const APP_FLEET_OK: &[&str] = &[
            "/",                              // list_apps (Postgres definitions)
            "/{pathb64}/displays",            // get_displays (SQL templates, no run)
            "/{pathb64}/data-cached",         // get_app_data_cached (boundary + S3)
            "/{pathb64}/charts/{chart_path}", // get_chart_image (local + S3 fallback)
        ];
        let ws = "/api/d9830be4-c6a4";
        let routes = route_paths_in_fn(src, "build_app_routes");
        let mut checked = 0;
        for route in &routes {
            if APP_FLEET_OK.contains(&route.as_str()) {
                continue; // intentional, reviewed FleetOk
            }
            let tail = if route == "/" { "" } else { route.as_str() };
            let concrete = concretize(&format!("{ws}/apps{tail}"));
            assert!(
                ["GET", "POST", "PUT", "DELETE", "PATCH"]
                    .iter()
                    .any(|m| classify(m, &concrete) == RouteRole::IdeOnly),
                "app sub-route {route:?} (under /apps) classifies FleetOk but is not in \
                 APP_FLEET_OK — if it reads/writes the working copy it needs an \
                 IDE_ONLY_PATTERNS entry; if it is genuinely stateless add it to \
                 APP_FLEET_OK. (This is the publish/unpublish/save-from-run gap.)",
            );
            checked += 1;
        }
        assert!(
            checked >= 5,
            "parsed only {checked} app sub-routes — the parser likely broke; fix it \
             rather than weakening the guard",
        );
        // No stale acks: every APP_FLEET_OK entry must be a current sub-route.
        let route_set: std::collections::HashSet<&str> =
            routes.iter().map(|r| r.as_str()).collect();
        for ack in APP_FLEET_OK {
            assert!(
                route_set.contains(ack),
                "APP_FLEET_OK lists {ack:?} but build_app_routes has no such route — \
                 remove the stale entry."
            );
        }
    }

    /// The Customer-Apps EXECUTION surface (public `/api/projects/*`) builds a
    /// WorkspaceManager from the working copy and runs inline, so it must be
    /// IdeOnly — a serve replica with no working copy 500s / NOT_FOUNDs it.
    /// These routes live in `router/public.rs`, outside build_workspace_routes,
    /// so the workspace-mount drift test cannot see them; this pins them by
    /// hand (and pins the Postgres-backed poll/cancel/stream siblings FleetOk so
    /// over-pinning them to the ide singleton would fail here too).
    #[test]
    fn customer_app_execution_routes_are_ide_only() {
        let pid = "d9830be4-c6a4";
        let ide_only = [
            ("POST", format!("/api/projects/{pid}/query")),
            ("POST", format!("/api/projects/{pid}/semantic-query")),
            ("POST", format!("/api/projects/{pid}/agents/agent-1/asks")),
            (
                "POST",
                format!("/api/projects/{pid}/procedures/proc-1/runs"),
            ),
        ];
        for (method, path) in &ide_only {
            assert_eq!(
                classify(method, path),
                RouteRole::IdeOnly,
                "customer-app execution route {method} {path} must be IdeOnly \
                 (runs inline from the working copy)"
            );
        }
        // Postgres-backed run-state siblings are cross-process safe → FleetOk.
        let fleet_ok = [
            ("GET", format!("/api/projects/{pid}/procedures/runs/run-1")),
            (
                "POST",
                format!("/api/projects/{pid}/procedures/runs/run-1/cancel"),
            ),
            (
                "POST",
                format!("/api/projects/{pid}/agents/asks/run-1/cancel"),
            ),
            (
                "GET",
                format!("/api/projects/{pid}/agents/runs/run-1/events"),
            ),
        ];
        for (method, path) in &fleet_ok {
            assert_eq!(
                classify(method, path),
                RouteRole::FleetOk,
                "customer-app run-state route {method} {path} must stay FleetOk \
                 (reads/writes Postgres run state, no working copy)"
            );
        }
    }

    /// Oxy Functions execute in-process from the working copy
    /// (`build_project_context` + `ctx.semantic` FS reads), so their invocation
    /// route must be IdeOnly — a serve replica forwards it to the ide. Static
    /// bundle assets are S3-backed and stay FleetOk.
    #[test]
    fn customer_app_function_route_is_ide_only() {
        assert_eq!(
            classify("POST", "/customer-apps/acme/hello-oxy/fn/post-je"),
            RouteRole::IdeOnly,
            "customer-app fn invocation must be IdeOnly (runs in-process from the working copy)"
        );
        // Static assets + index are served from S3 → any replica → FleetOk. The
        // 5-segment `.../fn/{name}` pattern must not capture these.
        assert_eq!(
            classify("GET", "/customer-apps/acme/hello-oxy/assets/main.js"),
            RouteRole::FleetOk,
            "customer-app static assets stay FleetOk"
        );
        assert_eq!(
            classify("GET", "/customer-apps/acme/hello-oxy"),
            RouteRole::FleetOk
        );
    }

    /// Path string of every `.route("PATH", ...)` mounted directly in
    /// `fn {fn_name}` of `src`. Deliberately simple text parsing (no syntax
    /// crate): the FS builders are flat lists of `.route(...)` calls.
    fn route_paths_in_fn(src: &str, fn_name: &str) -> Vec<String> {
        let start = src
            .find(&format!("fn {fn_name}"))
            .unwrap_or_else(|| panic!("{fn_name} not found in workspace.rs"));
        let rest = &src[start..];
        // Body ends at the next top-level `fn ` (column 0).
        let end = rest[1..].find("\nfn ").map(|i| i + 1).unwrap_or(rest.len());
        rest[..end]
            .split(".route(")
            .skip(1)
            .filter_map(|seg| {
                let after_quote = seg.trim_start().strip_prefix('"')?;
                let close = after_quote.find('"')?;
                Some(after_quote[..close].to_string())
            })
            .collect()
    }

    /// Replace `{name}` / `{*name}` pattern segments with a literal so a route
    /// pattern becomes a concrete request path `classify` can match.
    fn concretize(pattern: &str) -> String {
        let mut out = String::new();
        let mut chars = pattern.chars();
        while let Some(c) = chars.next() {
            if c == '{' {
                for d in chars.by_ref() {
                    if d == '}' {
                        break;
                    }
                }
                out.push('x');
            } else {
                out.push(c);
            }
        }
        out
    }

    /// Drift guard (hand-maintained — NOT router-introspecting). Every flat
    /// route mounted by `build_git_routes()` + `build_data_repo_routes()`
    /// (router/workspace.rs) touches the working copy / shells out to git, so
    /// each MUST classify IdeOnly; a route that's FleetOk lands on a serve
    /// replica with no working copy → 500. This test asserts the list below
    /// classifies IdeOnly, so it catches a route that's in the list but
    /// missing from the manifest. It does NOT see new routes added to the
    /// router — so a new git route must be added in THREE places: the router,
    /// IDE_ONLY_PATTERNS, and this list. (No automated cross-check exists
    /// across router ⇄ manifest yet.)
    #[test]
    fn manifest_covers_every_git_route() {
        let ws = "/api/d9830be4-c6a4";
        // (method, flat path) pairs straight from build_git_routes().
        let git_routes = [
            ("GET", format!("{ws}/branches")),
            ("DELETE", format!("{ws}/branches/main")),
            ("POST", format!("{ws}/switch-branch")),
            ("POST", format!("{ws}/pull-changes")),
            ("POST", format!("{ws}/fetch")),
            ("POST", format!("{ws}/push-changes")),
            ("POST", format!("{ws}/abort-rebase")),
            ("POST", format!("{ws}/continue-rebase")),
            ("POST", format!("{ws}/resolve-conflict-file")),
            ("POST", format!("{ws}/unresolve-conflict-file")),
            ("POST", format!("{ws}/resolve-conflict-with-content")),
            ("POST", format!("{ws}/force-push")),
            ("POST", format!("{ws}/discard-all")),
            ("GET", format!("{ws}/recent-commits")),
            ("GET", format!("{ws}/revision-info")),
            ("POST", format!("{ws}/reset-to-commit")),
        ];
        for (method, path) in &git_routes {
            assert_eq!(
                classify(method, path),
                RouteRole::IdeOnly,
                "git route {method} {path} must be IdeOnly (touches working copy)"
            );
        }
        // build_data_repo_routes(), nested under /repositories.
        let repo_routes = [
            ("GET", format!("{ws}/repositories")),
            ("POST", format!("{ws}/repositories")),
            ("DELETE", format!("{ws}/repositories/my-repo")),
            ("POST", format!("{ws}/repositories/my-repo/checkout")),
            ("GET", format!("{ws}/repositories/my-repo/diff")),
            ("POST", format!("{ws}/repositories/my-repo/commit")),
            ("GET", format!("{ws}/repositories/my-repo/files")),
            ("POST", format!("{ws}/repositories/github")),
        ];
        for (method, path) in &repo_routes {
            assert_eq!(
                classify(method, path),
                RouteRole::IdeOnly,
                "data-repo route {method} {path} must be IdeOnly (git working copy)"
            );
        }
        // onboarding-readiness is flat (not under /onboarding/).
        assert_eq!(
            classify("GET", &format!("{ws}/onboarding-readiness")),
            RouteRole::IdeOnly
        );
    }

    /// Coverage guard for routes that read NODE-LOCAL state the compile
    /// boundary does not materialise (git state, process-local BROADCASTER
    /// SSE). A serve replica has no working copy and no in-process run owner,
    /// so these degrade silently there ("Workspace directory not found", a
    /// truncated stream) unless classified IdeOnly. Hand-maintained, same
    /// limitation as `manifest_covers_every_git_route`: it catches a route
    /// dropped FROM the manifest, not a brand-new router route nobody listed —
    /// that's the behavioral canary's job (see internal-docs/compile-boundary.md
    /// "the role-classification canary"). Add a new state-touching route in
    /// THREE places: the router, IDE_ONLY_PATTERNS, and here.
    #[test]
    fn manifest_covers_state_touching_routes() {
        let ws = "/api/d9830be4-c6a4";
        let ide_only = [
            // git/working-copy state reads (workspace_root + detect_git_mode)
            ("GET", format!("{ws}/details")),
            ("GET", format!("{ws}/status")),
            // process-local BROADCASTER live SSE (legacy workflow/task streams)
            ("GET", format!("{ws}/events")),
            ("GET", format!("{ws}/events/lookup")),
            ("GET", format!("{ws}/events/sync")),
            ("GET", format!("{ws}/world-model/events")),
            // generated chart files on local disk (no S3 read-through)
            ("GET", format!("{ws}/charts/abc.json")),
            ("GET", format!("{ws}/exported-charts/abc.png")),
            // modeling/airform — dbt projects on disk; ALL methods + the bare
            // list root are IdeOnly (regression for the POST-only-manifest gap).
            ("GET", format!("{ws}/modeling")),
            ("GET", format!("{ws}/modeling/myproj/lineage")),
            ("POST", format!("{ws}/modeling/myproj/run")),
            // Agentic run/exec surface — ide-pinned for tier 1 (subruns run
            // in-process where the analytics run drives and touch the FS).
            ("POST", format!("{ws}/analytics/runs")),
            ("GET", format!("{ws}/analytics/runs/r1/events")),
            ("GET", format!("{ws}/agentic-workflows/files")),
            ("GET", format!("{ws}/agentic-workflows/runs/r1/events")),
            ("GET", format!("{ws}/agentic-airway/runs/r1/events")),
        ];
        for (method, path) in &ide_only {
            assert_eq!(
                classify(method, path),
                RouteRole::IdeOnly,
                "state-touching route {method} {path} must be IdeOnly \
                 (no working copy / process-local broadcaster on the serve fleet)"
            );
        }
        // Counter-guard: routes that LOOK similar but are cross-process safe
        // must stay FleetOk, or we needlessly pin the chat data plane to the
        // ide singleton. (The agentic run/exec surface — /analytics,
        // /agentic-workflows, /agentic-airway — is now ide-pinned for tier 1;
        // see the `ide_only` set above.)
        let fleet_ok = [
            ("GET", format!("{ws}/blocks")), // persisted Postgres read
            ("GET", format!("{ws}/world-model/cameras")),
            // parquet result cache — fleet-safe via the S3 read-through in
            // result_files::{store,get} (mirror on write, fetch on local miss).
            ("GET", format!("{ws}/results/files/file-123")),
            ("DELETE", format!("{ws}/results/files/file-123")),
        ];
        for (method, path) in &fleet_ok {
            assert_eq!(
                classify(method, path),
                RouteRole::FleetOk,
                "cross-process route {method} {path} must stay FleetOk"
            );
        }
    }

    // ── Stage 0b: router-derived completeness gate ─────────────────────────
    //
    // The FIRST test that derives its route set FROM the router source rather
    // than a hand-maintained list. It parses every `.route()` / `.nest()` mount
    // in `build_workspace_routes` (router/workspace.rs) and asserts each is
    // EXPLICITLY classified — IdeOnly in IDE_ONLY_PATTERNS, or acknowledged
    // FleetOk below. A new mount nobody classified fails CI here instead of
    // silently defaulting to FleetOk and 404ing on a serve replica that has no
    // working copy (the `/apps/source` outage class, oxygen-internal#2531).
    //
    // Scope/limits (honest): it guards the top-level mount surface. It does not
    // introspect nested builders' sub-routes (axum exposes no route table) nor
    // per-method gaps inside a builder — those stay guarded by the per-builder
    // tests above + the behavioral canary (internal-docs/compile-boundary.md).
    // `.merge(...)` mounts carry no path literal (git: see
    // `manifest_covers_every_git_route`).

    /// Workspace mounts that are intentionally FleetOk: every handler under them
    /// is served statelessly (compile boundary + Postgres + S3), never the
    /// working copy / `.git` / node-local disk / process-local state. Kept in
    /// sync with `build_workspace_routes` — the test rejects stale entries.
    /// REVIEW before adding one: if ANY handler under the mount reads local
    /// disk, it belongs in IDE_ONLY_PATTERNS, not here.
    const FLEET_OK_ACKNOWLEDGED: &[&str] = &[
        // workspace metadata + access control (Postgres)
        "/compile/status",
        "/members",
        "/members/{user_id}",
        "/oxy-access",
        "/custom-apps",
        "/builder-availability",
        // org bare-subdomain status — read-only, resolves workspace→org→
        // org_subdomains (Postgres only). Admin enable/disable lives under
        // /api/admin and is classified separately.
        "/org-subdomain",
        // Workspace logo: the org-uploaded logo is served from Postgres (the
        // org row) — fleet-safe. The code-first `logo.*` fallback reads the
        // workspace FS, but that read is best-effort: on a replica without the
        // files it 404s and the UI falls back to the name initial, so there is
        // no broken-content failure mode that needs pinning to the IDE node.
        "/logo",
        // run/thread/agent data planes (Postgres task router + persisted rows)
        "/threads",
        "/agents",
        "/api-keys",
        "/blocks",
        "/runs/{source_id}/{run_index}",
        "/logs",
        // NB: /analytics, /agentic-workflows, /agentic-airway are NOT here —
        // they're ide-pinned (IDE_ONLY_PATTERNS) for tier 1 (subruns run
        // in-process where the run drives and touch the FS).
        "/agentic-schedules",
        "/tests",
        "/traces",
        "/metrics",
        "/execution-analytics",
        // config-boundary-served entities (Postgres definitions, no FS)
        "/databases",
        "/integrations",
        "/secrets",
        "/apps",
        // automations + airway pipelines served from the compile boundary so the
        // customer-nav sidebar + single-automation view render on a serve replica
        // (the IdeOnly `/agentic-workflows` + `/agentic-airway` live in a crate
        // that can't reach `compiled_reader`).
        "/procedures",
        "/procedures/{path_b64}",
        "/automations",
        "/automations/{path_b64}",
        "/airway-pipelines",
        "/app-integrations",
        "/artifacts/{id}",
        // query + semantic layer (warehouse + compiled artifacts, stateless)
        "/sql/{pathb64}",
        "/sql/query",
        "/semantic",
        "/semantic/topic/{file_path_b64}",
        "/semantic/view/{file_path_b64}",
        "/semantic/preagg-status",
        "/semantic/compile",
        "/semantic/monitors",
        "/semantic/anomalies",
        "/semantic/metric-tree",
        "/semantic/metric-tree/{measure_id}/sensitivity",
        "/semantic/metric-tree/predict",
        "/semantic/metric-tree/explain",
        "/semantic/metric-tree/opportunity",
        "/semantic/metric-tree/time-dimensions",
        "/semantic/metric-tree/distribution",
        // world-model entity graph + instance drill-down. Reads the semantic
        // layer (same scan mechanism as `/semantic`) plus `.world-model.yml`,
        // which is served from the compile boundary (`world_model_configs`), so
        // these are fleet-safe — no working-copy dependency a replica lacks.
        "/semantic/world-model",
        "/semantic/world-model/instances",
        "/semantic/world-model/filter-instances",
        "/semantic/world-model/filter-counts",
        "/semantic/world-model/instance-detail",
        "/semantic/world-model/measure-breakdown",
        // world-model (Postgres + S3, read-through cached)
        "/world-model/cameras",
        "/world-model/weather/{layer}/{z}/{x}/{y}",
        "/world-model/weather/current",
        "/world-model/foot-traffic/current",
        "/world-model/foot-traffic/radar",
        "/world-model/competitors",
        // parquet result cache — fleet-safe via S3 read-through
        "/results/files/{file_id}",
        // local-mode-only scaffolding (no fleet exists in local mode)
        "/setup/empty",
        "/setup/demo",
    ];

    /// Extracts `(is_nest, path)` for every `.route("…")` / `.nest("…")` mount
    /// in `body` (handles the multi-line `.route(\n  "…"` form).
    fn parse_mounts(body: &str) -> Vec<(bool, String)> {
        let mut out = Vec::new();
        for (is_nest, marker) in [(false, ".route("), (true, ".nest(")] {
            let mut hay = body;
            while let Some(i) = hay.find(marker) {
                let after = hay[i + marker.len()..].trim_start();
                if let Some(rest) = after.strip_prefix('"')
                    && let Some(end) = rest.find('"')
                {
                    out.push((is_nest, rest[..end].to_string()));
                }
                hay = &hay[i + marker.len()..];
            }
        }
        out
    }

    /// A concrete probe URI for a mount: `{param}` segments become `x`, and a
    /// nest prefix gets one trailing segment (the manifest's `{*rest}` wildcards
    /// need ≥1 segment to match).
    fn probe_uri(path: &str, is_nest: bool) -> String {
        let mut uri = String::from("/api/d9830be4-c6a4");
        for seg in path.split('/').filter(|s| !s.is_empty()) {
            uri.push('/');
            if seg.starts_with('{') {
                uri.push('x');
            } else {
                uri.push_str(seg);
            }
        }
        if is_nest {
            uri.push_str("/probe");
        }
        uri
    }

    #[test]
    fn every_workspace_mount_is_classified() {
        let src = include_str!("router/workspace.rs");
        // Only build_workspace_routes' body — not the nested builder fns later
        // in the file (their sub-routes carry no prefix at this layer).
        let start = src
            .find("fn build_workspace_routes")
            .expect("build_workspace_routes fn present");
        let body = &src[start..];
        let end = body.find("\n}\n").unwrap_or(body.len());
        let body = &body[..end];

        let mounts = parse_mounts(body);
        assert!(
            mounts.len() > 30,
            "parser found only {} mounts — the router shape changed; fix parse_mounts",
            mounts.len()
        );

        const METHODS: [&str; 5] = ["GET", "POST", "PUT", "DELETE", "PATCH"];
        for (is_nest, path) in &mounts {
            if FLEET_OK_ACKNOWLEDGED.contains(&path.as_str()) {
                continue; // intentional, reviewed FleetOk
            }
            let probe = probe_uri(path, *is_nest);
            let ide_only = METHODS
                .iter()
                .any(|m| classify(m, &probe) == RouteRole::IdeOnly);
            assert!(
                ide_only,
                "workspace mount {path:?} is UNCLASSIFIED: not in IDE_ONLY_PATTERNS \
                 and not in FLEET_OK_ACKNOWLEDGED, so it defaults to FleetOk and will \
                 404 on a serve replica if any handler under it reads the working \
                 copy / .git / local disk. Add an IdeOnly entry to IDE_ONLY_PATTERNS, \
                 or (if it is stateless) add {path:?} to FLEET_OK_ACKNOWLEDGED.",
            );
        }

        // No stale acknowledgements: every entry must be a current mount, so the
        // list rots loudly when a route is removed or renamed.
        let mount_paths: std::collections::HashSet<&str> =
            mounts.iter().map(|(_, p)| p.as_str()).collect();
        for ack in FLEET_OK_ACKNOWLEDGED {
            assert!(
                mount_paths.contains(ack),
                "FLEET_OK_ACKNOWLEDGED lists {ack:?} but build_workspace_routes has no \
                 such mount — remove the stale entry."
            );
        }
    }
}
