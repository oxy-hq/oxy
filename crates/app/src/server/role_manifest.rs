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
//! The classification lives here as a single static table. Reviewers add new
//! `IdeOnly` patterns when they add routes that touch FS — see
//! `.claude/skills/oxy-compile-boundary/SKILL.md`.
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
    ManifestEntry {
        method: "POST",
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
    // ── Process-local live SSE (BROADCASTER) ────────────────────────────────
    // The legacy workflow / agentic-task live streams subscribe to an
    // in-memory `BROADCASTER` (`api/{run,task,world_model}.rs`). The producing
    // run lives on a DIFFERENT process, so a worker-less serve replica's
    // broadcaster is empty and the stream truncates silently. Pin to the ide
    // singleton (the in-process run owner). NB: the MODERN agentic streams
    // (/analytics, /agentic-workflows, /agentic-airway) use the cross-process
    // Postgres task router (LISTEN/NOTIFY) and are FleetOk — deliberately not
    // listed here.
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

/// Classify `(method, request_uri_path)`. Returns [`RouteRole::FleetOk`] when
/// no entry matches.
pub fn classify(method: &str, request_path: &str) -> RouteRole {
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
            || pattern_matches("/api/{workspace_id}/status", request_path))
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

    #[test]
    fn unknown_routes_default_to_fleet_ok() {
        assert_eq!(
            classify("GET", "/api/d9830be4-c6a4/threads"),
            RouteRole::FleetOk
        );
        assert_eq!(classify("POST", "/api/analytics/runs"), RouteRole::FleetOk);
        assert_eq!(classify("GET", "/health"), RouteRole::FleetOk);
        assert_eq!(classify("GET", "/healthz"), RouteRole::FleetOk);
        // Guard against OVER-classification: the modern agentic streams use the
        // cross-process Postgres task router (LISTEN/NOTIFY), so they're
        // fleet-safe and must stay FleetOk even though they end in /events.
        assert_eq!(
            classify("GET", "/api/d9830be4-c6a4/analytics/runs/abc/events"),
            RouteRole::FleetOk
        );
        assert_eq!(
            classify(
                "GET",
                "/api/d9830be4-c6a4/agentic-workflows/runs/abc/events"
            ),
            RouteRole::FleetOk
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
        // `/apps/source/{pathb64}` (get_source_file → workspace_path) and
        // `/apps/file/{pathb64}` (get_data → local state dir) read local disk
        // the stateless serve fleet lacks; they must forward to the ide, not
        // 404 locally. The 2-segment patterns must not be shadowed by the
        // 1-segment `/apps/{pathb64}` fetch, which stays fleet-served.
        let ws = "/api/d9830be4-c6a4";
        assert_eq!(
            classify("GET", &format!("{ws}/apps/source/b3h5bWFydA")),
            RouteRole::IdeOnly
        );
        assert_eq!(
            classify("GET", &format!("{ws}/apps/file/b3h5bWFydA")),
            RouteRole::IdeOnly
        );
        assert_eq!(
            classify("GET", &format!("{ws}/apps/b3h5bWFydA")),
            RouteRole::FleetOk
        );
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
        // ide singleton. Modern agentic streams ride the Postgres task router.
        let fleet_ok = [
            ("GET", format!("{ws}/analytics/runs/r1/events")),
            ("GET", format!("{ws}/agentic-workflows/runs/r1/events")),
            ("GET", format!("{ws}/agentic-airway/runs/r1/events")),
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
}
