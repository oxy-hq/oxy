//! `GET <base>__oxy/build` — which build this app serves right now.
//!
//! The probe a long-open tab (an iPad left on a checklist, a kiosk, an installed
//! PWA) polls to learn that a publish happened. `runtime.js` compares the answer
//! with `window.__OXY_APP__.buildId` — the build that served its document — and
//! a mismatch is the only signal it acts on.
//!
//! ## Why a reserved endpoint rather than polling the shell
//!
//! Polling the HTML would record a view per poll and ship a whole document to
//! compare one id. This answers one UUID, is dispatched inside the app's own
//! gate (so it works on the subpath, custom-app subdomain and org-subdomain
//! surfaces with no CORS), and returns before view and SLI recording, exactly
//! as the beacon does — a poll is not traffic.
//!
//! ## What it costs
//!
//! For a published production app on a warm pod: nothing beyond the cached
//! resolution chain every asset already uses. An unpublished app (the draft
//! channel, `unpromoted_build`) and a sandbox (`sandbox_build`) make the same
//! one uncached read their HTML does — once per tab per ~5 minutes.
//!
//! The answer rides the per-pod 60 s resolution cache, so two pods can disagree
//! for up to a minute after a publish. The client absorbs that with a 60 s floor
//! between detection and reload; see `runtime.js` § *updates*.

use axum::http::header;
use axum::response::{IntoResponse, Response};
use sea_orm::DatabaseConnection;
use uuid::Uuid;

use oxy_app_core::custom_app_environment::AppEnvironment;

use oxy_custom_apps::env_resolve::EnvironmentBuilds;

/// Answer the probe with the build this request's environment and channel
/// would serve — the same `build_to_serve` the bundle dispatch calls, so
/// production, the draft channel, staging and sandboxes each compare against
/// what they actually serve.
pub(super) async fn live_build_for(
    db: &DatabaseConnection,
    app: &entity::apps::Model,
    environment: &AppEnvironment,
    environments: &EnvironmentBuilds,
) -> Response {
    let (build, _label) = super::build_to_serve(db, app, environment, environments).await;
    live_build_response(build)
}

/// `200 {"buildId": "<uuid>"}`, or the reserved 404 when nothing is served.
///
/// `private, no-store`: the answer changes on every publish and is behind the
/// app's auth gate, so neither the browser nor a shared cache may keep it. The
/// client also fetches with `cache: "no-store"`, which is what makes the service
/// worker pass it straight through.
pub(super) fn live_build_response(build: Option<Uuid>) -> Response {
    match build {
        Some(id) => (
            [
                (header::CONTENT_TYPE, "application/json"),
                (header::CACHE_CONTROL, "private, no-store"),
            ],
            serde_json::json!({ "buildId": id }).to_string(),
        )
            .into_response(),
        None => crate::client::reserved_not_found(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    #[tokio::test]
    async fn answers_the_served_build_uuid_uncacheably() {
        let id = Uuid::new_v4();
        let response = live_build_response(Some(id));
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[header::CACHE_CONTROL],
            "private, no-store"
        );
        assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(json, serde_json::json!({ "buildId": id.to_string() }));
    }

    /// No build is "unknown" to the client, never "a different build": it must
    /// not be an answer a page could mistake for a publish.
    #[test]
    fn no_build_is_a_no_store_404() {
        let response = live_build_response(None);
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    }
}
