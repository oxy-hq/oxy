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
    ManifestEntry {
        method: "*",
        path_pattern: "/api/{workspace_id}/git/{*rest}",
        role: RouteRole::IdeOnly,
    },
    // ── Workspace-level onboarding subtree (clones + scaffolds) ─────────────
    ManifestEntry {
        method: "*",
        path_pattern: "/api/{workspace_id}/onboarding/{*rest}",
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
        assert_eq!(
            classify("POST", "/api/d9830be4-c6a4/git/commit"),
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
        assert_eq!(classify("GET", "/api/abc/git/status"), RouteRole::IdeOnly);
        assert_eq!(classify("POST", "/api/abc/git/commit"), RouteRole::IdeOnly);
    }
}
