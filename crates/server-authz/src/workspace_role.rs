//! The caller's resolved workspace role, carried as a request extension. The
//! middleware that computes and inserts it (`workspace_middleware` /
//! `local_context_middleware`) stays in `oxy-app` because it needs `AppState`
//! (WorkspaceManager + caches); this crate owns only the newtype and its
//! extension-reading extractors, so the workspace role guards can live here.

use axum::extract::{FromRequestParts, OptionalFromRequestParts};
use axum::http::StatusCode;
use axum::http::request::Parts;
use entity::workspace_members::WorkspaceRole;
use std::future::Future;

/// The resolved workspace role for the current user.
#[derive(Clone, Debug)]
pub struct EffectiveWorkspaceRole(pub WorkspaceRole);

impl<S> FromRequestParts<S> for EffectiveWorkspaceRole
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
            .get::<EffectiveWorkspaceRole>()
            .cloned()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR);

        async move { result }
    }
}

/// Local mode doesn't attach an `EffectiveWorkspaceRole`, so handlers that work
/// in both modes (e.g. `list_apps`) read it as `Option<EffectiveWorkspaceRole>`.
impl<S> OptionalFromRequestParts<S> for EffectiveWorkspaceRole
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Option<Self>, Self::Rejection>> + Send {
        let result = parts.extensions.get::<EffectiveWorkspaceRole>().cloned();
        async move { Ok(result) }
    }
}
