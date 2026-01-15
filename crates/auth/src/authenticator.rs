use axum::http::{HeaderMap, StatusCode};
use std::future::Future;

use crate::types::Identity;

pub trait Authenticator<TSource = HeaderMap> {
    type Error: std::error::Error + Into<StatusCode>;

    fn authenticate(
        &self,
        source: &TSource,
    ) -> impl Future<Output = Result<Identity, Self::Error>> + Send;
}
