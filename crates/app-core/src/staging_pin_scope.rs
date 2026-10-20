//! The staging pin's task-local scope: run a future under a pinned semantic
//! revision, and read the pin back from code that is not handed it.
//!
//! Lives here (below `oxy-app`) because the compile-boundary readers
//! ([`crate::preagg_context`]) must honour the pin; the publish-time and
//! request-time pin logic stays in `oxy_app::server::api::custom_apps_staging_pin`,
//! which re-exports everything below.

use uuid::Uuid;

tokio::task_local! {
    /// The staging revision this task is pinned to — set only by
    /// [`with_staging_pin`], never by the ordinary promoted-revision pin. Lets
    /// code that is not handed the pin still hold the staging rules: the rollup
    /// short-circuit is skipped (`preagg_context`) and result caches are
    /// partitioned ([`cache_partition`]).
    static STAGING_PIN: Uuid;
}

/// Run `fut` with every compile-boundary read pinned to `pin` — or unchanged
/// when `pin` is `None`.
///
/// Use this rather than calling `compiled_reader::with_pinned_revision`
/// directly: there, `Some(None)` scoped means "this request reads the
/// FILESYSTEM everywhere", which on a replica is a 503. "No pin" has to mean
/// "don't scope at all", and this is the one place that says so.
///
/// Two callers: a custom-app draft build's data-plane reads, and a staff
/// workspace preview (`server::previews`), which pins a staging revision for a
/// whole `/api/{workspace_id}/…` request. Both get the same rules inside the
/// scope — see [`current_staging_pin`].
pub async fn with_staging_pin<F, T>(pin: Option<Uuid>, fut: F) -> T
where
    F: std::future::Future<Output = T>,
{
    match pin {
        Some(revision_id) => {
            STAGING_PIN
                .scope(
                    revision_id,
                    crate::compiled_reader::with_pinned_revision(Some(revision_id), fut),
                )
                .await
        }
        None => fut.await,
    }
}

/// The staging revision the current task is pinned to, if any. `None` outside
/// [`with_staging_pin`] — including in a task spawned from inside it, which is
/// why background work can never see one.
pub fn current_staging_pin() -> Option<Uuid> {
    STAGING_PIN.try_with(|pin| *pin).ok()
}

/// The result-cache partition for the current task: `""` unpinned, the pin
/// under one, so a branch's answer never serves live and live's never answers
/// the branch. The same shape `CustomAppContext::cache_scope` uses.
pub fn cache_partition() -> String {
    current_staging_pin()
        .map(|pin| format!("staging-pin:{pin}"))
        .unwrap_or_default()
}
