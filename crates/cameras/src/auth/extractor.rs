//! Axum extractor for [`EdgeContext`].
//!
//! Mirrors `oxy-auth::extractor::AuthenticatedUserExtractor` — the
//! middleware injects an `EdgeContext` into `request.extensions`, and
//! handlers pull it out with this extractor. Rejection is 401 if the
//! middleware didn't run or didn't authenticate.

use axum::{
    extract::FromRequestParts,
    http::{StatusCode, request::Parts},
};

use super::context::EdgeContext;

#[derive(Clone)]
pub struct EdgeContextExtractor(pub EdgeContext);

impl<S> FromRequestParts<S> for EdgeContextExtractor
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
            .get::<EdgeContext>()
            .cloned()
            .map(EdgeContextExtractor)
            .ok_or(StatusCode::UNAUTHORIZED);

        async move { result }
    }
}
