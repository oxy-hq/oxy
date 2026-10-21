//! The Layer-1 preagg cache a request carries, for handlers that compile a
//! semantic query through the same preagg-aware path the background worker uses.
//! The workspace middleware attaches it; the custom-app serve route has it
//! layered on by the composition root.

use std::future::Future;

use agentic_semantic::refresh_key_cache::RefreshKeyCache;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use oxy::config::ConfigManager;

/// Layer-1 preagg cache + renewal threshold, attached by the workspace
/// middleware so handlers can compile semantic queries through the same
/// preagg-aware path the background worker uses. Both fields are `None`
/// when no preagg worker is running (CLI, tests, internal API router).
#[derive(Clone, Default)]
pub struct PreaggCacheCtx {
    pub cache: Option<std::sync::Arc<std::sync::RwLock<RefreshKeyCache>>>,
    pub renewal_threshold_secs: Option<u64>,
}

impl PreaggCacheCtx {
    /// The renewal threshold to run the read-side freshness check against:
    /// whatever the process published, else **this workspace's own**
    /// `pre_aggregations.refresh_worker.renewal_threshold`.
    ///
    /// The fallback is the point. `None` here does not mean "120" — it means
    /// nobody published a global value, which is the normal state in `--local`
    /// and on any node whose `AppState` predates the preagg worker. Reading the
    /// workspace's config is what keeps the read side on the same number as the
    /// rebuild cycle, so an operator who lengthens the threshold sees both
    /// halves move. Hard-coding `120` at a call site silently desynchronises
    /// them, and the symptom — a rollup considered fresh that the rebuild
    /// considers due, or the reverse — surfaces only as tier flapping.
    ///
    /// Generic over the manager's disk slot: a read-only manager answers this
    /// as well as a working copy, and the anomaly surfaces hold one of each.
    pub fn renewal_threshold_secs_or<S>(&self, config_manager: &ConfigManager<S>) -> u64 {
        self.renewal_threshold_secs.unwrap_or_else(|| {
            oxy::config::preagg_check::resolve_renewal_threshold(
                config_manager.get_config().pre_aggregations.as_ref(),
            )
            .as_secs()
        })
    }
}

impl<S> FromRequestParts<S> for PreaggCacheCtx
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let ctx = parts
            .extensions
            .get::<PreaggCacheCtx>()
            .cloned()
            .unwrap_or_default();
        async move { Ok(ctx) }
    }
}
