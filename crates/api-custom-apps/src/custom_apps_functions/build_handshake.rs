//! The `/fn` build handshake: every answer names the build it ran against, and
//! a call from a page on an older build that asks for a function the live build
//! no longer has is told so.
//!
//! `/fn` always runs the LIVE build — the app row is read fresh per call — so a
//! tab left open across a publish can call a function that was renamed or
//! removed. Before this, that was a bare `FunctionNotFound` 404 reading as a
//! bug, when the fix is a reload. The SDK's `useFunction` sends the build its
//! document was served (`x-oxy-client-build`) and reads `x-oxy-live-build`
//! back; a mismatch or a `409 ClientBuildStale` makes it ask the runtime's
//! update agent to check now (`window.__oxyUpdate.check()`).
//!
//! What this deliberately does NOT do: refuse a call because the builds
//! differ. Old frontends keep working against new functions wherever the
//! function still exists — refusing them would turn every publish into an
//! outage for every open tab. Only a call that would have 404'd anyway is
//! re-labelled, and a missing header (an app built on an older SDK) is
//! *unknown*, never stale.

use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use uuid::Uuid;

use crate::custom_apps_updates::{CLIENT_BUILD_HEADER, LIVE_BUILD_HEADER};

/// `response`, naming the build that answered it. Applied to every response
/// once the build is resolved, from this one place, so no branch can forget.
pub(super) fn with_live_build(mut response: Response, build_id: Uuid) -> Response {
    if let Ok(value) = HeaderValue::from_str(&build_id.to_string()) {
        response.headers_mut().insert(LIVE_BUILD_HEADER, value);
    }
    response
}

/// The build the calling document was served, when it said so in a form we
/// can compare. Anything unparseable is the same as absent: unknown.
pub(super) fn client_build(headers: &HeaderMap) -> Option<Uuid> {
    headers
        .get(CLIENT_BUILD_HEADER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| Uuid::parse_str(v.trim()).ok())
}

/// The answer for a function the live build lacks: `409 ClientBuildStale`
/// when the caller says it runs a DIFFERENT build — the function very likely
/// existed in that one — and `None` otherwise, so the caller keeps its 404.
pub(super) fn stale_client_response(client: Option<Uuid>, live: Uuid) -> Option<Response> {
    let client = client?;
    if client == live {
        return None;
    }
    Some(
        (
            StatusCode::CONFLICT,
            axum::Json(serde_json::json!({
                "error": "ClientBuildStale",
                "message": "this page runs an older build of the app; reload to get the live one",
                "live": live,
            })),
        )
            .into_response(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers_with(value: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(CLIENT_BUILD_HEADER, HeaderValue::from_str(value).unwrap());
        h
    }

    #[test]
    fn stamps_the_live_build() {
        let id = Uuid::new_v4();
        let response = with_live_build(StatusCode::OK.into_response(), id);
        assert_eq!(response.headers()[LIVE_BUILD_HEADER], id.to_string());
    }

    #[test]
    fn reads_only_a_well_formed_client_build() {
        let id = Uuid::new_v4();
        assert_eq!(client_build(&headers_with(&id.to_string())), Some(id));
        assert_eq!(client_build(&headers_with("build-7")), None);
        assert_eq!(client_build(&HeaderMap::new()), None);
    }

    /// A missing header is an app on an older SDK: unknown, never stale.
    #[test]
    fn a_missing_or_matching_client_build_keeps_the_404() {
        let live = Uuid::new_v4();
        assert!(stale_client_response(None, live).is_none());
        assert!(stale_client_response(Some(live), live).is_none());
    }

    #[tokio::test]
    async fn a_different_client_build_is_a_409_naming_the_live_build() {
        let live = Uuid::new_v4();
        let response = stale_client_response(Some(Uuid::new_v4()), live).expect("stale");
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(json["error"], "ClientBuildStale");
        assert_eq!(json["live"], live.to_string());
    }
}
