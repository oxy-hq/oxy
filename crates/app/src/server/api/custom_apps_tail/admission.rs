//! Who may tail which environment (ruling 18 of the environments design).
//!
//! The route sits inside the staff `/customer-apps` nest, so the console's
//! layers have already run (owner-or-app-admin, `Action::PlatformApps`, the
//! grant's app scope, no active assume session). The handler adds:
//!
//! - **a publish token** is refused (`403 publish_token_refused`);
//! - **every environment** needs the caller to administer the app
//!   (`resolve_app_role` = admin, oxy-authz `Ring::AppAdmin`) — the gate
//!   `/logs` puts on every environment too (`require_app_admin`): a
//!   function's output is the author's, not a viewer's (`custom_apps_logs`);
//! - **staging or a sandbox** then also needs `may_open_non_production`
//!   (`Action::AppNonProduction`), and a sandbox must exist;
//! - **a sandbox agent token** is asked exactly what `/logs` asks of one
//!   (`custom_apps_logs::environment::agent_environment`): a sandbox it
//!   created — or staging, under a grant naming it — where it is also the
//!   app's admin. Everything else is `404 environment_not_found`, the answer
//!   it gets on the other environment routes, so it never learns what exists.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use entity::{app_members, apps};
use oxy::database::client::establish_connection;
use oxy_app_core::custom_app_environment::AppEnvironment;
use oxy_auth::types::AuthenticatedUser;
use oxy_server_authz::Caller;
use sea_orm::{DatabaseConnection, EntityTrait};
use uuid::Uuid;

use super::stream::Target;
use crate::server::api::custom_apps_agent::{holds_staging, resolve_app_role_in};
use crate::server::api::custom_apps_auth::resolve_app_role;
use crate::server::api::custom_apps_env_resolve::{
    may_open_environment, may_open_non_production, sandbox_row,
};
use crate::server::api::custom_apps_sandbox_instance::own_since;

/// A refusal, as `{"error":"<code>","message"}` — the sandbox routes' shape.
#[derive(Debug, thiserror::Error)]
pub enum TailError {
    #[error("a publish token cannot tail an app's functions; use a staff credential")]
    PublishToken,
    #[error("no app has this id")]
    AppNotFound,
    #[error("{0:?} is not an app environment: use \"production\", \"staging\" or \"dev-<handle>\"")]
    InvalidEnvironment(String),
    #[error("this app has no environment {0}")]
    EnvironmentNotFound(String),
    #[error(
        "the {0} environment is open to Oxy staff who may open this app's non-production \
         environments"
    )]
    NonProductionRefused(String),
    #[error("an app's function output is open to the app's admins")]
    AppAdminRequired,
    #[error("timeout must be a whole number of seconds between 1 and {max}")]
    InvalidTimeout { max: u64 },
    #[error("the tail is unavailable on this replica: {0}; retry shortly")]
    Unavailable(String),
    #[error("the request could not be completed; retry shortly")]
    Internal,
}

impl TailError {
    pub fn status(&self) -> StatusCode {
        match self {
            Self::PublishToken | Self::NonProductionRefused(_) | Self::AppAdminRequired => {
                StatusCode::FORBIDDEN
            }
            Self::AppNotFound | Self::EnvironmentNotFound(_) => StatusCode::NOT_FOUND,
            Self::InvalidEnvironment(_) | Self::InvalidTimeout { .. } => StatusCode::BAD_REQUEST,
            Self::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::PublishToken => "publish_token_refused",
            Self::AppNotFound => "app_not_found",
            Self::InvalidEnvironment(_) => "invalid_environment",
            Self::EnvironmentNotFound(_) => "environment_not_found",
            Self::NonProductionRefused(_) => "non_production_refused",
            Self::AppAdminRequired => "app_admin_required",
            Self::InvalidTimeout { .. } => "invalid_timeout",
            Self::Unavailable(_) => "tail_unavailable",
            Self::Internal => "internal",
        }
    }

    fn db(what: &str, e: impl std::fmt::Display) -> Self {
        tracing::error!(error = %e, "tail: {what} failed");
        Self::Internal
    }
}

impl IntoResponse for TailError {
    fn into_response(self) -> Response {
        let body = serde_json::json!({ "error": self.code(), "message": self.to_string() });
        (self.status(), Json(body)).into_response()
    }
}

/// A request the checks let through: the connection the stream uses, and
/// what it tails.
pub struct Admitted {
    pub db: DatabaseConnection,
    pub target: Target,
}

/// Decide whether `user` may tail environment `name` of app `app_id`.
/// `publish_token` is whether the request authenticated with one.
pub async fn admit(
    user: &AuthenticatedUser,
    publish_token: bool,
    app_id: Uuid,
    name: &str,
) -> Result<Admitted, TailError> {
    if publish_token {
        return Err(TailError::PublishToken);
    }
    let db = establish_connection()
        .await
        .map_err(|e| TailError::db("connect", e))?;
    let app = apps::Entity::find_by_id(app_id)
        .one(&db)
        .await
        .map_err(|e| TailError::db("load the app", e))?
        .ok_or(TailError::AppNotFound)?;
    // Credential-aware: a token that carries no staff standing is no staff.
    let caller = Caller::from_user(user);
    let (environment, since) = if caller.is_sandbox_agent() {
        agent_environment(&db, &caller, &app, name).await?
    } else {
        (staff_environment(&db, &caller, &app, name).await?, None)
    };
    let target = Target {
        app_id,
        environment: environment.name(),
        since,
    };
    Ok(Admitted { db, target })
}

/// Everyone but a sandbox agent token: the app's admins, and for any
/// environment but production also staff with non-production reach.
async fn staff_environment(
    db: &DatabaseConnection,
    caller: &Caller,
    app: &apps::Model,
    name: &str,
) -> Result<AppEnvironment, TailError> {
    let environment =
        AppEnvironment::parse(name).ok_or_else(|| TailError::InvalidEnvironment(name.into()))?;
    // The admin gate comes first and covers every environment, as `/logs`
    // does: log output is the author's, so reach into staging or a sandbox
    // narrows the admin set and never stands in for it.
    let role = resolve_app_role(db, caller, app)
        .await
        .map_err(|e| TailError::db("resolve the app role", e))?;
    if role != Some(app_members::ROLE_ADMIN) {
        return Err(TailError::AppAdminRequired);
    }
    if environment == AppEnvironment::Production {
        return Ok(environment);
    }
    if !may_open_non_production(db, caller, app).await {
        return Err(TailError::NonProductionRefused(environment.name()));
    }
    if matches!(environment, AppEnvironment::Dev { .. }) {
        let row = sandbox_row(db, app.id, &environment)
            .await
            .map_err(|e| TailError::db("load the sandbox", e))?;
        if row.is_none() {
            return Err(TailError::EnvironmentNotFound(environment.name()));
        }
    }
    Ok(environment)
}

/// A sandbox agent token, asked what `/logs` asks of one. Returns where the
/// sandbox it has now starts, so a resume replays none of an earlier one's.
async fn agent_environment(
    db: &DatabaseConnection,
    caller: &Caller,
    app: &apps::Model,
    name: &str,
) -> Result<(AppEnvironment, Option<DateTime<Utc>>), TailError> {
    let not_found = || TailError::EnvironmentNotFound(name.to_string());
    let environment = AppEnvironment::parse(name).ok_or_else(not_found)?;
    let asked = match environment {
        AppEnvironment::Dev { .. } => true,
        AppEnvironment::Staging => holds_staging(caller, app),
        AppEnvironment::Production => false,
    };
    if !asked || !may_open_environment(db, caller, app, &environment).await {
        return Err(not_found());
    }
    match resolve_app_role_in(db, caller, app, &environment).await {
        Ok(Some(app_members::ROLE_ADMIN)) => {}
        _ => return Err(not_found()),
    }
    let Some(token) = caller.sandbox_agent().map(|reach| reach.token_id) else {
        return Err(not_found());
    };
    match own_since(db, app.id, &environment, token).await {
        Ok(Some(since)) => Ok((environment, Some(since))),
        _ => Err(not_found()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each refusal's status and code, as the CLI and the token's body layer
    /// read them.
    #[test]
    fn each_refusal_has_its_status_and_code() {
        let cases = [
            (TailError::PublishToken, 403, "publish_token_refused"),
            (TailError::AppNotFound, 404, "app_not_found"),
            (
                TailError::InvalidEnvironment("prod".into()),
                400,
                "invalid_environment",
            ),
            (
                TailError::EnvironmentNotFound("dev-a1".into()),
                404,
                "environment_not_found",
            ),
            (
                TailError::NonProductionRefused("staging".into()),
                403,
                "non_production_refused",
            ),
            (TailError::AppAdminRequired, 403, "app_admin_required"),
            (
                TailError::InvalidTimeout { max: 86_400 },
                400,
                "invalid_timeout",
            ),
            (TailError::Unavailable("x".into()), 503, "tail_unavailable"),
            (TailError::Internal, 500, "internal"),
        ];
        for (error, status, code) in cases {
            assert_eq!(
                (error.status().as_u16(), error.code()),
                (status, code),
                "{error}"
            );
        }
    }
}
