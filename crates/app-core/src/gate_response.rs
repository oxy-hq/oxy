//! The `{ message, code? }` JSON envelope the custom-app data-plane gate chain
//! answers a refusal with.
//!
//! Kept in sync with `query.rs`'s `ApiErr` — both deliberately use the same
//! `{ message, code? }` shape so callers can render errors uniformly across
//! endpoints. Every 500 the chain raises off a `DbErr` carries a `code` from
//! `DbFailure`; see `oxy_custom_apps::gate::GateError` for why.
//!
//! Here, below both `oxy-app` (the data plane's handlers) and the
//! `oxy-api-custom-apps` surface, because both turn a refusal into a response.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[derive(serde::Serialize)]
struct GateErr {
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<&'static str>,
}

/// `status` with a `{ "message": … }` body.
pub fn err(status: StatusCode, msg: impl Into<String>) -> Response {
    (
        status,
        Json(GateErr {
            message: msg.into(),
            code: None,
        }),
    )
        .into_response()
}

/// `status` with a `{ "message": …, "code": … }` body.
pub fn err_with_code(status: StatusCode, msg: impl Into<String>, code: &'static str) -> Response {
    (
        status,
        Json(GateErr {
            message: msg.into(),
            code: Some(code),
        }),
    )
        .into_response()
}

/// The response for a refusal reported as plain values (a status, a message
/// and an optional code): what `oxy_custom_apps::gate::GateError` carries.
pub fn refusal(status: u16, message: &str, code: Option<&'static str>) -> Response {
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    match code {
        Some(code) => err_with_code(status, message, code),
        None => err(status, message),
    }
}
