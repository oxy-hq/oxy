//! Which `threads.source` a custom-app ask is filed under, and so which chat
//! history a request reads: production's, or one app's staging.
//!
//! Here, below `oxy-app`, because the data plane's agent ask files its thread
//! under it and `oxy-api-custom-apps` lists the history by it.

use uuid::Uuid;

/// The `threads.source` of a production custom-app ask.
pub const PRODUCTION_THREAD_SOURCE: &str = "custom-app";

/// The `threads.source` of an ask on `app_id`'s staging host. Scoped to the
/// app, so one app's staging never reads another's. An unknown app (`None`)
/// still yields a staging source, which no production read matches.
pub fn staging_thread_source(app_id: Option<Uuid>) -> String {
    format!(
        "custom-app-staging:{}",
        app_id.map(|a| a.to_string()).unwrap_or_default()
    )
}
