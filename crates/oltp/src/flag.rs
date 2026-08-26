//! Runtime kill-switch for the per-org OLTP feature.
//!
//! **Why a hook and not a direct call.** The feature-flag store lives in the
//! app layer (`crates/app/src/server/feature_flags`), which depends on this
//! crate, not the other way round — so `resolver` and `provisioner` cannot read
//! it directly. The app registers a check at startup, exactly as it does for
//! the observability store's global; the enforcement points below call
//! [`is_enabled`], which delegates to whatever was registered.
//!
//! **Permissive when nothing is registered.** Tests, the `oxy oltp` CLI, and
//! any process that provisions without the server never set a check, and must
//! keep working — so an unset hook is `true`. The dark-launch default (OFF)
//! lives in the app's flag registry, which the server's registered check
//! consults; it does not belong here, or the whole integration suite would see
//! the feature disabled.

use std::sync::OnceLock;

type Check = Box<dyn Fn() -> bool + Send + Sync>;

static ENABLED_CHECK: OnceLock<Check> = OnceLock::new();

/// Register the enabled-check. First call wins; the app calls this once at
/// startup with `|| feature_flags::is_enabled("oltp")`.
///
/// **Backed by a `OnceLock`, so the tests that install `|| false` depend on
/// nextest's process-per-test isolation** — a fresh lock per test. CLAUDE.md
/// mandates nextest and bans `cargo test`; under a thread-based runner one
/// test's `set_check(false)` would leak and fail every sibling on `Disabled`.
pub fn set_check(check: Check) {
    if ENABLED_CHECK.set(check).is_err() {
        // Unreachable today because each process calls `cache::init` (the only
        // caller) exactly once — serve and worker are separate processes, never
        // in-process. NOT because a double init would error: it no longer does.
        // If a second bridge is ever wired, this warns loudly rather than
        // silently keeping the first and leaking a second refresh loop.
        tracing::warn!("oltp flag check already registered; ignoring the second");
    }
}

/// Whether the per-org OLTP feature is currently on.
///
/// Delegates to the registered check; `true` when none is registered (see the
/// module docs — tests and the CLI bypass the server).
pub fn is_enabled() -> bool {
    match ENABLED_CHECK.get() {
        Some(check) => check(),
        None => true,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn unset_is_permissive() {
        // No check registered in this (isolated) test process, so the feature
        // must read as ON — tests and the CLI never register one and must work.
        assert!(super::is_enabled());
    }
}
