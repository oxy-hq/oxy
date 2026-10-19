//! Custom-app webhook receiver for a **non-production** environment —
//! `POST /api/webhooks/apps-env/{environment}/{org}/{app}/{function}`
//! (`internal-docs/2026-09-10-custom-app-environments-design.md` §4.5,
//! ruling 2).
//!
//! One anonymous route for staging and every `dev-<handle>` sandbox, beside
//! production's (`webhooks::app_function`), whose verification it reuses. The
//! environment is a path segment, not a query string, so a provider that
//! drops the query cannot land a delivery on production; and this route never
//! reaches production — `production` in that segment is not found.
//!
//! **Every "does this endpoint exist" answer is the same `404 no such
//! webhook`**: what production 404s on, plus an unknown environment, a sandbox
//! that does not exist, is being deleted or is past its idle expiry, an
//! environment that serves no build, and a function with no `webhook` block
//! *in that environment's build*.
//!
//! **No secret fallback.** The signature is checked against the environment's
//! own `apps/<id>/<env>/<secretVar>` and nothing else — not staging's for a
//! sandbox (which `ctx.env` does fall back to), not production's. A webhook
//! secret proves the sender; a fallback would make a delivery signed for
//! staging verify on every sandbox's URL. Unset or blank → `401 webhook
//! signing secret is not configured`; wrong → `401 signature mismatch`, before
//! anything is queued.
//!
//! Accepted → `202`, queued as a `webhook` task naming the environment. The
//! runner checks the `webhook` block again in the build it resolves, and a
//! sandbox's idle expiry, and runs under the environment's policy
//! (`custom_apps_functions::queued_admission`), as the system identity.

use axum::body::Bytes;
use axum::extract::Path;
use axum::http::{HeaderMap, StatusCode};
use chrono::Utc;
use oxy::service::secret_manager::SecretManagerService;
use oxy_app_core::custom_app_environment::AppEnvironment;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use super::app_function::{
    no_such_webhook, payload_of, resolve_app, secret_not_configured, verify_signature,
};
use crate::server::api::custom_apps_env_resolve::{resolve_function_environment, sandbox_row};
use crate::server::api::custom_apps_functions::check_run::{self, Asked, TriggerError};
use crate::server::api::custom_apps_functions::{FunctionJobTrigger, function_webhook_contract};
use crate::server::api::custom_apps_sandboxes::activity::is_past_idle_expiry;
use crate::server::api::custom_apps_secrets::scope::secret_name;

type Refusal = (StatusCode, String);

/// A webhook endpoint that exists: the app, the environment, the function,
/// and its contract `(secretVar, signatureHeader, encoding)`.
struct Endpoint {
    app: entity::apps::Model,
    environment: AppEnvironment,
    function: String,
    contract: (String, String, String),
}

/// `POST /api/webhooks/apps-env/{environment}/{org_slug}/{app_slug}/{function_name}`
#[tracing::instrument(skip_all, fields(environment = %path.0, org = %path.1, app = %path.2, function = %path.3))]
pub async fn app_function_env_webhook(
    Path(path): Path<(String, String, String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, Refusal> {
    let db = oxy::database::client::establish_connection()
        .await
        .map_err(|e| {
            tracing::error!("db connection failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "database unavailable".to_string(),
            )
        })?;
    let endpoint = resolve_endpoint(&db, &path).await?;
    // Past this point the endpoint is admitted to exist, so failures are 401 —
    // the caller has proven nothing yet.
    let (secret_var, signature_header, encoding) = &endpoint.contract;
    let keys = environment_secret(&endpoint, secret_var).await?;
    verify_signature(&headers, &body, signature_header, encoding, &keys)?;
    enqueue(&db, &endpoint, &body).await
}

/// The environment a path segment names, when this route serves it: any
/// non-production name. `production` and anything `AppEnvironment::parse`
/// rejects name nothing here.
fn non_production(raw: &str) -> Option<AppEnvironment> {
    AppEnvironment::parse(raw).filter(|environment| *environment != AppEnvironment::Production)
}

/// The endpoint the path names, or the one `404`.
async fn resolve_endpoint(
    db: &DatabaseConnection,
    (environment, org_slug, app_slug, function): &(String, String, String, String),
) -> Result<Endpoint, Refusal> {
    let environment = non_production(environment).ok_or_else(no_such_webhook)?;
    let app = resolve_app(db, org_slug, app_slug)
        .await
        .ok_or_else(no_such_webhook)?;
    if !is_open(db, &app, &environment).await {
        return Err(no_such_webhook());
    }
    let manifest = function_manifest(db, &app, &environment, function)
        .await
        .ok_or_else(no_such_webhook)?;
    let contract = function_webhook_contract(&manifest).ok_or_else(no_such_webhook)?;
    Ok(Endpoint {
        app,
        environment,
        function: function.clone(),
        contract,
    })
}

/// Whether the environment takes deliveries: staging always (its build is
/// asked next); a sandbox only while it exists, is not being deleted and is
/// not past its idle expiry. A lookup that fails is a no.
async fn is_open(
    db: &DatabaseConnection,
    app: &entity::apps::Model,
    environment: &AppEnvironment,
) -> bool {
    if !matches!(environment, AppEnvironment::Dev { .. }) {
        return true;
    }
    let Ok(Some(row)) = sandbox_row(db, app.id, environment).await else {
        return false;
    };
    matches!(is_past_idle_expiry(db, &row, Utc::now()).await, Ok(false))
}

/// The function's `manifest_json` in the build `environment` serves — that
/// build only, never production's or staging's.
async fn function_manifest(
    db: &DatabaseConnection,
    app: &entity::apps::Model,
    environment: &AppEnvironment,
    function: &str,
) -> Option<serde_json::Value> {
    let build_id = resolve_function_environment(db, app, environment)
        .await
        .ok()?
        .build_id?;
    entity::app_functions::Entity::find()
        .filter(entity::app_functions::Column::BuildId.eq(build_id))
        .filter(entity::app_functions::Column::Name.eq(function))
        .one(db)
        .await
        .ok()?
        .and_then(|f| f.manifest_json)
}

/// The environment's own value of `secret_var`, with no fallback. Unset or
/// blank is the same `401`: an empty key would be a valid HMAC key anyone
/// could sign with.
async fn environment_secret(endpoint: &Endpoint, secret_var: &str) -> Result<String, Refusal> {
    let name = secret_name(endpoint.app.id, &endpoint.environment, secret_var);
    SecretManagerService::new(endpoint.app.project_id)
        .get_secret(&name)
        .await
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| {
            tracing::warn!(
                app = %endpoint.app.id, environment = %endpoint.environment, secret = %secret_var,
                "rejecting app webhook: the environment's own secret does not resolve",
            );
            secret_not_configured()
        })
}

/// Queue the verified body as a `webhook` task naming the environment, and
/// answer at once. A build that moved under the request is the same `404`;
/// failing to record the job is ours, a `500` the provider retries.
async fn enqueue(
    db: &DatabaseConnection,
    endpoint: &Endpoint,
    body: &[u8],
) -> Result<StatusCode, Refusal> {
    let asked = Asked {
        trigger: FunctionJobTrigger::Webhook,
        environment: &endpoint.environment,
        credential_token_id: None,
    };
    let payload = Some(payload_of(body));
    match check_run::trigger(db, endpoint.app.id, &endpoint.function, payload, asked).await {
        Ok(run_id) => {
            tracing::info!(app = %endpoint.app.id, environment = %endpoint.environment, run = %run_id, "app webhook accepted");
            Ok(StatusCode::ACCEPTED)
        }
        Err(
            TriggerError::AppNotFound
            | TriggerError::NoBuild(_)
            | TriggerError::FunctionNotFound(_)
            | TriggerError::NoWebhook { .. },
        ) => Err(no_such_webhook()),
        Err(e) => {
            tracing::error!("app webhook enqueue failed: {e}");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not enqueue the webhook job".to_string(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The route serves every non-production name and never production's.
    #[test]
    fn the_segment_names_a_non_production_environment_or_nothing() {
        assert_eq!(non_production("staging"), Some(AppEnvironment::Staging));
        assert_eq!(
            non_production("dev-a1"),
            Some(AppEnvironment::Dev {
                handle: "a1".into()
            })
        );
        for nothing in ["production", "Production", "prod", "", "dev-", "staging "] {
            assert_eq!(non_production(nothing), None, "{nothing:?}");
        }
    }
}
