//! `GET /api/customer-apps/{id}/environments/{name}/tail` — parse, admit,
//! open, stream. Mounted in the staff `/customer-apps` nest
//! (`router::global`), FleetOk: the stream is fed through `LISTEN`, so any
//! replica can hold it whichever process runs the function.
//!
//! - `?timeout=<seconds>` (1 to [`MAX_TIMEOUT_SECS`]) ends the stream with
//!   `done {"reason":"timeout"}`; without it the stream lives until the
//!   client goes or the server shuts down (`done {"reason":"shutdown"}`).
//! - `Last-Event-ID` resumes: the stream replays the outcomes of the
//!   invocations that finished after it (`resume`), then goes on live.
//! - The caller is asked again every minute while the stream lives
//!   (`recheck`); a refusal ends it with `done {"reason":"access_revoked"}`.
//!   After an hour it ends with `done {"reason":"reauthenticate"}`, so the
//!   credential itself is checked again by the next request.

use std::time::Duration;

use axum::Extension;
use axum::extract::{Path, Query};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use oxy_auth::extractor::AuthenticatedUserExtractor;
use oxy_auth::types::{AppPublishTokenAuth, AuthenticatedUser};
use serde::Deserialize;
use uuid::Uuid;

use super::admission::{self, Admitted, TailError};
use super::stream::{Asked, EventIds, OpenError, Tail};
use super::{recheck, resume};

/// The longest `?timeout=` a stream takes: a day.
pub const MAX_TIMEOUT_SECS: u64 = 86_400;

#[derive(Debug, Deserialize)]
pub struct TailQuery {
    /// Taken as text so a malformed value is this route's `invalid_timeout`,
    /// not the framework's plain-text rejection.
    timeout: Option<String>,
}

/// `timeout`, when given, as a duration in range.
fn parse_timeout(raw: Option<&str>) -> Result<Option<Duration>, TailError> {
    let Some(raw) = raw.map(str::trim).filter(|raw| !raw.is_empty()) else {
        return Ok(None);
    };
    match raw.parse::<u64>() {
        Ok(secs) if (1..=MAX_TIMEOUT_SECS).contains(&secs) => Ok(Some(Duration::from_secs(secs))),
        _ => Err(TailError::InvalidTimeout {
            max: MAX_TIMEOUT_SECS,
        }),
    }
}

pub async fn tail(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    marker: Option<Extension<AppPublishTokenAuth>>,
    Path((id, name)): Path<(Uuid, String)>,
    Query(query): Query<TailQuery>,
    headers: HeaderMap,
) -> Response {
    match open(&user, marker.is_some(), id, &name, &query, &headers).await {
        Ok(response) => response,
        Err(refused) => refused.into_response(),
    }
}

async fn open(
    user: &AuthenticatedUser,
    publish_token: bool,
    app_id: Uuid,
    name: &str,
    query: &TailQuery,
    headers: &HeaderMap,
) -> Result<Response, TailError> {
    // Syntax only, so it says nothing about the app to a caller the gates
    // below would turn away.
    let timeout = parse_timeout(query.timeout.as_deref())?;
    let Admitted { db, target } = admission::admit(user, publish_token, app_id, name).await?;
    let resume_after = resume::last_event_id(headers);
    let asked = Asked {
        resume_after,
        timeout,
        recheck: Some(recheck::of(user.clone(), app_id, name.to_string())),
    };
    let tail = Tail::open(db, target, asked).await.map_err(|e| match e {
        OpenError::Unavailable(e) => TailError::Unavailable(e.to_string()),
        OpenError::Register(e) => {
            tracing::error!(error = %e, "tail: registering the lease failed");
            TailError::Internal
        }
    })?;
    Ok(tail.into_sse(EventIds::after(resume_after.unwrap_or(0))))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Absent or blank is no deadline; a whole number of seconds in range is
    /// one; anything else is refused rather than read as no deadline.
    #[test]
    fn a_timeout_is_whole_seconds_in_range() {
        assert_eq!(parse_timeout(None).unwrap(), None);
        assert_eq!(parse_timeout(Some(" ")).unwrap(), None);
        assert_eq!(
            parse_timeout(Some("30")).unwrap(),
            Some(Duration::from_secs(30))
        );
        assert!(parse_timeout(Some("86400")).unwrap().is_some());
        for bad in ["0", "86401", "-1", "1.5", "soon"] {
            let refused = parse_timeout(Some(bad)).expect_err(bad);
            assert_eq!(refused.code(), "invalid_timeout", "{bad}");
        }
    }
}
