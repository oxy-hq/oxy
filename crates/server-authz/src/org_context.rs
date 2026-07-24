//! The request-scoped org context: which org a request targets and the caller's
//! membership in it. The DB-loading middleware that populates this
//! (`org_middleware`) stays in `oxy-app` — it needs app-internal assume-session
//! resolution — so this crate owns only the type and its extension-reading
//! extractor, letting the role guards that consume it live here too.

use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::request::Parts;
use std::future::Future;

#[derive(Clone, Debug)]
pub struct OrgContext {
    pub org: entity::organizations::Model,
    pub membership: entity::org_members::Model,
    /// `true` when `membership` is a synthetic Owner row injected because
    /// the caller is a Global Owner or Global Admin but NOT a real member
    /// of `org`. Per-org handlers that need to stay strictly within
    /// real-membership semantics (e.g. self-serve billing operations on a
    /// foreign org) should check this and reject. Defaults to `false`.
    pub is_global_override: bool,
}

pub struct OrgContextExtractor(pub OrgContext);

impl<S> FromRequestParts<S> for OrgContextExtractor
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
            .get::<OrgContext>()
            .cloned()
            .map(OrgContextExtractor)
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR);

        async move { result }
    }
}
