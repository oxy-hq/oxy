use axum::{
    extract::{FromRequestParts, Request},
    http::{StatusCode, request::Parts},
};

use super::types::AuthenticatedUser;

/// Extractor for authenticated user in route handlers
#[derive(Clone)]
pub struct AuthenticatedUserExtractor(pub AuthenticatedUser);

impl<S> FromRequestParts<S> for AuthenticatedUserExtractor
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
            .get::<AuthenticatedUser>()
            .cloned()
            .map(AuthenticatedUserExtractor)
            .ok_or(StatusCode::UNAUTHORIZED);

        async move { result }
    }
}

/// Optional user extractor that doesn't fail if user is not authenticated
#[derive(Clone)]
pub struct OptionalUserExtractor(pub Option<AuthenticatedUser>);

impl<S> FromRequestParts<S> for OptionalUserExtractor
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let result = OptionalUserExtractor(parts.extensions.get::<AuthenticatedUser>().cloned());

        async move { Ok(result) }
    }
}

/// Extension trait to extract authenticated user from request
pub trait RequestUserExt {
    fn user(&self) -> Option<&AuthenticatedUser>;
}

impl RequestUserExt for Request {
    fn user(&self) -> Option<&AuthenticatedUser> {
        self.extensions().get::<AuthenticatedUser>()
    }
}
