//! Map [`ServiceError`] to an HTTP response.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use crate::service::ServiceError;

#[derive(Debug, Serialize)]
pub struct ApiErrorBody {
    pub code: &'static str,
    pub message: String,
}

pub fn map(err: ServiceError) -> Response {
    let (status, code) = match &err {
        ServiceError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
        ServiceError::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
        ServiceError::InvalidInput(_) => (StatusCode::BAD_REQUEST, "invalid_input"),
        ServiceError::Forbidden(_) => (StatusCode::FORBIDDEN, "forbidden"),
        ServiceError::Unavailable(_) => (StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
        ServiceError::Upstream(_) => (StatusCode::BAD_GATEWAY, "upstream_error"),
        ServiceError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "database_error"),
        ServiceError::Unifi(oxy_unifi::UnifiError::Forbidden(_)) => {
            (StatusCode::FORBIDDEN, "unifi_forbidden")
        }
        ServiceError::Unifi(oxy_unifi::UnifiError::NotFound(_)) => {
            (StatusCode::NOT_FOUND, "unifi_not_found")
        }
        ServiceError::Unifi(oxy_unifi::UnifiError::RateLimited { .. }) => {
            (StatusCode::TOO_MANY_REQUESTS, "unifi_rate_limited")
        }
        ServiceError::Unifi(_) => (StatusCode::BAD_GATEWAY, "unifi_error"),
        ServiceError::Airhouse(crate::airhouse::AirhouseError::Disabled) => {
            (StatusCode::SERVICE_UNAVAILABLE, "airhouse_disabled")
        }
        ServiceError::Airhouse(_) => (StatusCode::BAD_GATEWAY, "airhouse_error"),
        ServiceError::NotImplemented(_) => (StatusCode::NOT_IMPLEMENTED, "not_implemented"),
        ServiceError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
    };
    let body = ApiErrorBody {
        code,
        message: err.to_string(),
    };
    (status, Json(body)).into_response()
}
