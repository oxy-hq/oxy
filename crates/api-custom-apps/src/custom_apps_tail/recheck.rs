//! A stream is one request, and may stay open for hours. Two things keep it
//! from outliving the access that opened it:
//!
//! - **Every [`RECHECK_EVERY`]** the decision is asked again, and a refusal
//!   ends the stream with `done {"reason":"access_revoked"}` — the answer the
//!   next request would have had. Every caller is asked
//!   [`admission::admit`] again (a grant removed, a role lowered, a sandbox
//!   deleted). A sandbox agent token is first admitted again by its id
//!   (`oxy_auth::token::sandbox_recheck::readmit`: revoked, expired, its
//!   minter gone), which is what stops it "on its next request" elsewhere.
//! - **After [`REAUTHENTICATE_AFTER`]** the stream ends with
//!   `done {"reason":"reauthenticate"}`, and a client that wants more opens a
//!   new request with `Last-Event-ID` (`oxyc logs --follow` does, at once).
//!   That request passes authentication and the console's layers again, so
//!   what this module cannot re-read in place — a session's expiry, a
//!   personal token revoked, an assume session started — holds within that
//!   bound.
//!
//! A database error is not a refusal: the stream goes on and is asked again
//! at the next interval.

use std::sync::Arc;
use std::time::Duration;

use futures::future::BoxFuture;
use oxy::database::client::establish_connection;
use oxy_auth::types::AuthenticatedUser;
use oxy_server_authz::Caller;
use oxy_shared::errors::OxyError;
use uuid::Uuid;

use super::admission::{self, TailError};

/// How often an open stream's caller is asked again.
pub const RECHECK_EVERY: Duration = Duration::from_secs(60);

/// The longest one request streams before its client must authenticate
/// again.
pub const REAUTHENTICATE_AFTER: Duration = Duration::from_secs(60 * 60);

/// Asks whether the stream's caller may still read it.
pub type Recheck = Box<dyn Fn() -> BoxFuture<'static, bool> + Send + Sync>;

/// The recheck for `user`'s stream of environment `name` of app `app_id`.
pub fn of(user: AuthenticatedUser, app_id: Uuid, name: String) -> Recheck {
    let user = Arc::new(user);
    let name: Arc<str> = Arc::from(name);
    Box::new(move || {
        let (user, name) = (Arc::clone(&user), Arc::clone(&name));
        Box::pin(async move { still_admitted(&user, app_id, &name).await })
    })
}

async fn still_admitted(user: &AuthenticatedUser, app_id: Uuid, name: &str) -> bool {
    let token = Caller::from_user(user)
        .sandbox_agent()
        .map(|reach| reach.token_id);
    if let Some(token) = token
        && !token_still_admits(token).await
    {
        return false;
    }
    match admission::admit(user, false, app_id, name).await {
        Ok(_) | Err(TailError::Internal) => true,
        Err(refused) => {
            tracing::info!(%app_id, environment = name, code = refused.code(), "tail: the caller no longer admits; ending the stream");
            false
        }
    }
}

/// Whether sandbox agent token `token` would still be admitted. Only a
/// refusal of the token says no; a database that could not answer says yes.
async fn token_still_admits(token: Uuid) -> bool {
    let Ok(db) = establish_connection().await else {
        return true;
    };
    match oxy_auth::token::sandbox_recheck::readmit(&db, token).await {
        Ok(_) => true,
        Err(e) if is_database_failure(&e) => {
            tracing::warn!(%token, error = %e, "tail: could not re-read the sandbox agent token; asking again later");
            true
        }
        Err(e) => {
            tracing::info!(%token, error = %e, "tail: the sandbox agent token no longer admits; ending its stream");
            false
        }
    }
}

/// An error that says the database did not answer, not that the token was
/// refused.
fn is_database_failure(error: &OxyError) -> bool {
    matches!(
        error,
        OxyError::DBError(_) | OxyError::DatabaseUnreachable(_) | OxyError::Database(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A database that could not answer keeps the stream; a refused token
    /// ends it.
    #[test]
    fn only_a_refusal_ends_a_stream() {
        for failure in [
            OxyError::DBError("pool timed out".into()),
            OxyError::DatabaseUnreachable("connection refused".into()),
            OxyError::Database("dropped".into()),
        ] {
            assert!(is_database_failure(&failure), "{failure}");
        }
        let refused = OxyError::AuthenticationError("Invalid API key".into());
        assert!(!is_database_failure(&refused));
    }
}
