//! Which pod a process is, and which pods a route may run on.
//!
//! Two pure enums, here rather than in `oxy-app` so that crates BELOW it can
//! name them. `agentic-http` is the reason: it owns ~40 routes under
//! `/analytics` and `/agentic-*`, cannot import `oxy-app` (the layering forbids
//! it), and therefore could not state whether its own routes need the ide. That
//! left `oxy-app` declaring them from the outside, as four wildcard entries in a
//! hand-written table, with the per-route carve-outs that a wildcard forces.
//!
//! The env plumbing stays in `oxy-app` — reading `OXY_ROLE`, the process-wide
//! `OnceLock`, the route patterns. Only the vocabulary moves.

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

/// Which pods may serve a route.
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

/// One route's declared role, as a crate states it about its OWN routes.
///
/// `path` is relative to wherever the router is nested — the mounting crate
/// prepends its prefix. That is what lets a sub-router declare per-route
/// instead of being covered by a wildcard from outside.
#[derive(Clone, Copy, Debug)]
pub struct RouteRoleDecl {
    /// HTTP method, or `"*"` for any.
    pub method: &'static str,
    /// Path relative to the mount point, e.g. `"/runs/{run_id}"`.
    pub path: &'static str,
    pub role: RouteRole,
}
