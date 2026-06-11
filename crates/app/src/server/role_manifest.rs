//! Process-role classification + the static manifest of `IdeOnly` routes.
//!
//! Every Oxy request flows through one of three role-shaped processes:
//!
//! - **`ide`** — singleton owning the workspace working copy + `.git`.
//! - **`serve`** — stateless fleet replica reading Postgres + S3 only.
//! - **`worker`** — TaskSpec drainer, never receives HTTP.
//!
//! And every HTTP route is one of:
//!
//! - [`Role::IdeOnly`] — touches the workspace FS or `.git`. Will 421 on a
//!   `serve` replica.
//! - [`Role::FleetOk`] — Postgres / S3 / LLM only. Safe everywhere.
//! - [`Role::WorkerOnly`] — internal queue endpoints. (Not used today; here
//!   for completeness when a worker-private HTTP surface lands.)
//!
//! The classification lives here as a single static table. Reviewers add new
//! `IdeOnly` patterns when they add new routes that touch FS — see
//! `.claude/skills/oxy-compile-boundary/SKILL.md` for the broader contract.
//!
//! Routes not in [`IDE_ONLY_PATTERNS`] are treated as `FleetOk` by default.
//! That's deliberate — the compile boundary already removed FS reads from the
//! customer-facing hot path, so the long tail of routes can run on any
//! replica. New FS leaks are caught by adding to this table.

use std::sync::OnceLock;

/// Which process role owns this request shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// Singleton that owns the workspace FS + git working tree.
    Ide,
    /// Stateless fleet replica.
    Serve,
    /// Queue drainer (no inbound HTTP).
    Worker,
    /// Single-process all-in-one (today's default `oxy serve`). Accepts
    /// every route; no enforcement.
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

/// What classification a route carries.
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

    /// Returns true when a process of `process_role` is allowed to serve a
    /// route of `self`. `All` and matching roles accept; everything else
    /// rejects.
    pub fn accepted_by(self, process_role: Role) -> bool {
        match (self, process_role) {
            (_, Role::All) => true,
            (RouteRole::IdeOnly, Role::Ide) => true,
            (RouteRole::FleetOk, Role::Ide | Role::Serve) => true,
            (RouteRole::WorkerOnly, Role::Worker) => true,
            _ => false,
        }
    }
}

/// One entry in the manifest. `method = "*"` matches any method.
pub struct ManifestEntry {
    pub method: &'static str,
    pub path_glob: &'static str,
    pub role: RouteRole,
}

/// The full IdeOnly manifest. Match-first wins — order matters when entries
/// overlap, but the patterns here are disjoint.
///
/// Each `path_glob` uses axum's matched-path shape, where `{name}` is a path
/// parameter and `{*name}` is a wildcard. Match is exact against the
/// `MatchedPath` extension that axum populates on every routed request.
const IDE_ONLY_PATTERNS: &[ManifestEntry] = &[
    // ── Compile trigger reads the working copy on the singleton ─────────────
    ManifestEntry { method: "POST", path_glob: "/{workspace_id}/compile", role: RouteRole::IdeOnly },

    // ── File CRUD on the workspace ──────────────────────────────────────────
    ManifestEntry { method: "POST", path_glob: "/{workspace_id}/files/{pathb64}", role: RouteRole::IdeOnly },
    ManifestEntry { method: "PUT",  path_glob: "/{workspace_id}/files/{pathb64}/rename-file", role: RouteRole::IdeOnly },
    ManifestEntry { method: "PUT",  path_glob: "/{workspace_id}/files/{pathb64}/rename-folder", role: RouteRole::IdeOnly },
    ManifestEntry { method: "POST", path_glob: "/{workspace_id}/files/{pathb64}/new-file", role: RouteRole::IdeOnly },
    ManifestEntry { method: "POST", path_glob: "/{workspace_id}/files/{pathb64}/new-folder", role: RouteRole::IdeOnly },
    ManifestEntry { method: "DELETE", path_glob: "/{workspace_id}/files/{pathb64}/delete-file", role: RouteRole::IdeOnly },
    ManifestEntry { method: "DELETE", path_glob: "/{workspace_id}/files/{pathb64}/delete-folder", role: RouteRole::IdeOnly },
    ManifestEntry { method: "POST", path_glob: "/{workspace_id}/files/{pathb64}/revert", role: RouteRole::IdeOnly },

    // ── Git operations ──────────────────────────────────────────────────────
    // All under /git/* on the workspace router. The actual list of leaf
    // routes is verbose; one wildcard catches them all.
    ManifestEntry { method: "*", path_glob: "/{workspace_id}/git/{*rest}", role: RouteRole::IdeOnly },

    // ── Onboarding clones + scaffolds files ─────────────────────────────────
    ManifestEntry { method: "*", path_glob: "/{workspace_id}/onboarding/{*rest}", role: RouteRole::IdeOnly },

    // ── Modeling / Airform — dbt projects live on disk under modeling/ ─────
    ManifestEntry { method: "POST", path_glob: "/{workspace_id}/modeling/{*rest}", role: RouteRole::IdeOnly },

    // ── Workspace creation / branch switch (touches working copy) ──────────
    ManifestEntry { method: "POST", path_glob: "/workspaces", role: RouteRole::IdeOnly },
    ManifestEntry { method: "POST", path_glob: "/{workspace_id}/branch", role: RouteRole::IdeOnly },
];

/// Process-role for THIS server. Read once from `OXY_ROLE` at startup
/// (defaults to [`Role::All`] = no enforcement).
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
                "OXY_ROLE: unrecognised value; defaulting to 'all' (no enforcement)"
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

/// Classify a (method, matched_path) tuple. Returns [`RouteRole::FleetOk`]
/// when no manifest entry matches.
pub fn classify(method: &str, matched_path: &str) -> RouteRole {
    for entry in IDE_ONLY_PATTERNS {
        if (entry.method == "*" || entry.method == method)
            && glob_match(entry.path_glob, matched_path)
        {
            return entry.role;
        }
    }
    RouteRole::FleetOk
}

/// Surface for the `/api/_internal/routing-manifest` endpoint. Returns the
/// whole table so an operator can diff classifications across deploys.
pub fn dump_manifest() -> Vec<(&'static str, &'static str, &'static str)> {
    IDE_ONLY_PATTERNS
        .iter()
        .map(|e| (e.method, e.path_glob, e.role.as_str()))
        .collect()
}

/// Tiny match function for the manifest's `{name}` + `{*name}` placeholders
/// against axum's `MatchedPath` strings. Matches axum's behavior exactly —
/// the matched_path coming in is itself in this exact form, so it's a literal
/// string compare.
fn glob_match(pattern: &str, path: &str) -> bool {
    pattern == path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ide_routes_classify_as_ide_only() {
        assert_eq!(
            classify("POST", "/{workspace_id}/compile"),
            RouteRole::IdeOnly
        );
        assert_eq!(
            classify("POST", "/{workspace_id}/files/{pathb64}"),
            RouteRole::IdeOnly
        );
        assert_eq!(
            classify("DELETE", "/{workspace_id}/files/{pathb64}/delete-file"),
            RouteRole::IdeOnly
        );
        assert_eq!(
            classify("POST", "/{workspace_id}/onboarding/{*rest}"),
            RouteRole::IdeOnly
        );
    }

    #[test]
    fn unknown_routes_default_to_fleet_ok() {
        assert_eq!(
            classify("GET", "/{workspace_id}/threads"),
            RouteRole::FleetOk
        );
        assert_eq!(
            classify("POST", "/analytics/runs"),
            RouteRole::FleetOk
        );
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
    fn fleet_ok_accepted_by_ide_serve_all() {
        let r = RouteRole::FleetOk;
        assert!(r.accepted_by(Role::Ide));
        assert!(r.accepted_by(Role::Serve));
        assert!(r.accepted_by(Role::All));
        assert!(!r.accepted_by(Role::Worker));
    }

    #[test]
    fn method_wildcard_matches_any_verb() {
        assert_eq!(
            classify("GET", "/{workspace_id}/git/{*rest}"),
            RouteRole::IdeOnly
        );
        assert_eq!(
            classify("POST", "/{workspace_id}/git/{*rest}"),
            RouteRole::IdeOnly
        );
    }
}
