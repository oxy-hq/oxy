//! Runtime detector for "this process reached for a working copy it does not
//! have".
//!
//! Everything else guarding the compile boundary is static: route
//! classification, the `ConfigManager<WorkingCopy>` capability, the allowlist tests.
//! Static analysis has one weakness that matters — it can tell you the sites it
//! checked, never the ones it missed. Both production incidents were sites
//! nobody had thought to check.
//!
//! This is the dynamic half. `internal-docs/compile-boundary.md` describes it as
//! the target enforcement:
//!
//! > boot an `OXY_ROLE=serve` process with no working copy … and fail the build
//! > on any route that hits a state-missing path. **This catches the ones nobody
//! > remembered.** The runtime signal it asserts on … should also emit a `WARN`
//! > + metric in prod, so a leak that escapes CI is visible, not silent.
//!
//! Two consumers, one instrument:
//!
//! * **production** — a leak logs at WARN with the workspace and path, and ticks
//!   a counter, so it shows up on a dashboard instead of as a customer report
//!   four days later;
//! * **tests** — [`leaks`] is readable, so a canary can drive traffic against a
//!   process configured as a diskless replica and assert it stayed at zero.
//!
//! Lives in `oxy` rather than `oxy-app` because the path resolvers do
//! (`adapters::workspace`), and core cannot depend on the app crate that owns
//! `OXY_ROLE`. The app pushes the answer down at boot via
//! [`set_process_owns_workspace_files`].

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Defaults to `true`: an un-configured process is a single-process instance
/// (`oxy run`, a test, `oxy serve --local`), and those genuinely own their
/// files. Only the split fleet sets it to `false`, and it does so explicitly.
static OWNS_WORKSPACE_FILES: AtomicBool = AtomicBool::new(true);

static LEAKS: AtomicU64 = AtomicU64::new(0);

/// Declare whether this process owns a workspace working copy. Called once at
/// startup from the app's role initialisation; `ide` and `all` pass `true`,
/// `serve` and `worker` pass `false`.
pub fn set_process_owns_workspace_files(owns: bool) {
    OWNS_WORKSPACE_FILES.store(owns, Ordering::Relaxed);
}

/// Whether this process owns a workspace working copy.
pub fn process_owns_workspace_files() -> bool {
    OWNS_WORKSPACE_FILES.load(Ordering::Relaxed)
}

/// How many times a workspace path has been resolved on a process that owns no
/// working copy, since startup.
///
/// Every one of these is a read that will come back empty rather than fail —
/// the shape behind both the Toast and Slack outages. Non-zero on a `serve`
/// replica means a route is reaching for a disk that is not there.
pub fn leaks() -> u64 {
    LEAKS.load(Ordering::Relaxed)
}

/// Reset the counter. For tests that want to attribute leaks to one request
/// rather than to everything since startup.
pub fn reset_leaks() {
    LEAKS.store(0, Ordering::Relaxed);
}

/// Record that a workspace path was resolved, and report whether that was a
/// leak.
///
/// Called from the path resolvers rather than from every reader: a path is the
/// one thing every filesystem read needs, so this is the choke point that does
/// not depend on anyone remembering to instrument their own call site.
pub fn note_workspace_path_resolved(
    workspace_id: Option<uuid::Uuid>,
    path: &std::path::Path,
) -> bool {
    if process_owns_workspace_files() {
        return false;
    }
    LEAKS.fetch_add(1, Ordering::Relaxed);
    tracing::warn!(
        workspace_id = ?workspace_id,
        path = %path.display(),
        "resolved a workspace path on a process that owns no working copy. The \
         directory does not exist here, so whatever reads it next will get an \
         empty result rather than an error — this is the shape behind the Toast \
         (#2816) and Slack outages. Either the route belongs on the ide — mount \
         it with `route_ide` rather than `route_fleet` — or the read belongs on \
         the compile boundary."
    );
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_process_that_owns_files_never_reports_a_leak() {
        set_process_owns_workspace_files(true);
        reset_leaks();
        assert!(!note_workspace_path_resolved(
            None,
            std::path::Path::new("/w")
        ));
        assert_eq!(leaks(), 0);
    }

    #[test]
    fn a_diskless_process_counts_every_resolution() {
        set_process_owns_workspace_files(false);
        reset_leaks();
        assert!(note_workspace_path_resolved(
            None,
            std::path::Path::new("/w")
        ));
        assert!(note_workspace_path_resolved(
            None,
            std::path::Path::new("/x")
        ));
        assert_eq!(leaks(), 2);
        // Leave the global as the rest of the suite expects.
        set_process_owns_workspace_files(true);
    }
}
