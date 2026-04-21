//! HTTP router, split into per-concern modules.
//!
//! - [`public`] — unauthenticated routes (health, auth, Slack, current-user)
//! - [`global`] — cloud-only org/workspace CRUD and GitHub integration
//! - [`workspace`] — the per-workspace route tree and its sub-builders
//! - [`secrets`] — secret routes gated behind an admin-only middleware
//! - [`protected`] — cloud/local composition of protected routes + middleware
//! - [`entry`] — [`api_router`] / [`internal_api_router`] public entry points
//! - [`openapi`] — the utoipa OpenAPI router used by Swagger UI

mod entry;
mod global;
mod openapi;
mod protected;
mod public;
mod secrets;
mod workspace;

use crate::server::serve_mode::ServeMode;
use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::request::Parts;
use entity::workspaces as workspace_entity;
use std::future::Future;
use tower_http::cors::{Any, CorsLayer};

pub use entry::{api_router, internal_api_router};
pub use openapi::openapi_router;

#[derive(Clone)]
pub struct AppState {
    pub enterprise: bool,
    pub internal: bool,
    pub mode: ServeMode,
    pub observability: Option<std::sync::Arc<dyn oxy_observability::ObservabilityStore>>,
    /// The server's working directory at startup. In local mode, used as the
    /// target for `POST /{workspace_id}/setup/*`. In cloud/internal mode,
    /// unused — populated with `PathBuf::new()`.
    pub startup_cwd: std::path::PathBuf,
}

#[derive(Clone)]
pub struct WorkspaceExtractor(pub workspace_entity::Model);

impl<S> FromRequestParts<S> for WorkspaceExtractor
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let result = parts
            .extensions
            .get::<workspace_entity::Model>()
            .cloned()
            .map(WorkspaceExtractor)
            .ok_or(StatusCode::UNAUTHORIZED);

        async move { result }
    }
}

pub(super) fn build_cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(Any)
        .allow_private_network(true)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any)
}

#[cfg(test)]
mod app_state_tests {
    use super::*;

    #[test]
    fn app_state_carries_mode() {
        let local = AppState {
            enterprise: false,
            internal: false,
            mode: ServeMode::Local,
            observability: None,
            startup_cwd: std::path::PathBuf::from("/tmp"),
        };
        let cloud = AppState {
            enterprise: false,
            internal: false,
            mode: ServeMode::Cloud,
            observability: None,
            startup_cwd: std::path::PathBuf::new(),
        };
        assert!(local.mode.is_local());
        assert!(!cloud.mode.is_local());
    }
}

#[cfg(test)]
mod router_split_tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn local_router_does_not_expose_organizations() {
        let router = api_router(ServeMode::Local, false, None, std::path::PathBuf::new())
            .await
            .expect("router built");
        let req = Request::builder().uri("/orgs").body(Body::empty()).unwrap();
        let resp = router.oneshot(req).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::NOT_FOUND,
            "local mode must not mount /orgs"
        );
    }

    #[tokio::test]
    async fn local_router_serves_health() {
        let router = api_router(ServeMode::Local, false, None, std::path::PathBuf::new())
            .await
            .expect("router built");
        // /live always returns 200 regardless of DB availability — confirms
        // that public routes are mounted on the local router.
        let req = Request::builder().uri("/live").body(Body::empty()).unwrap();
        let resp = router.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn local_router_mounts_workspace_routes_under_nil_uuid() {
        use crate::server::serve_mode::LOCAL_WORKSPACE_ID;
        let router = api_router(ServeMode::Local, false, None, std::path::PathBuf::new())
            .await
            .expect("router built");
        let uri = format!("/{}/agents", LOCAL_WORKSPACE_ID);
        let req = Request::builder().uri(&uri).body(Body::empty()).unwrap();
        let resp = router.oneshot(req).await.expect("oneshot");
        assert_ne!(
            resp.status(),
            StatusCode::NOT_FOUND,
            "local mode must mount workspace routes under /{{workspace_id}}, got {} for {}",
            resp.status(),
            uri
        );
    }

    #[tokio::test]
    async fn cloud_router_still_has_organizations_mounted() {
        let router = api_router(ServeMode::Cloud, false, None, std::path::PathBuf::new())
            .await
            .expect("router built");
        let req = Request::builder().uri("/orgs").body(Body::empty()).unwrap();
        let resp = router.oneshot(req).await.expect("oneshot");
        // Route is mounted → request reaches auth/handler, not the router's 404.
        assert_ne!(
            resp.status(),
            StatusCode::NOT_FOUND,
            "cloud mode must keep /orgs mounted, got {}",
            resp.status()
        );
    }
}
