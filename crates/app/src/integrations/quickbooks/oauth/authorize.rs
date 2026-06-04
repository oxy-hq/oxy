//! POST /api/{project_id}/integrations/quickbooks/authorize
//!
//! Authenticated, workspace-scoped. Optionally stores the client secret in
//! the workspace secret manager (so the public callback can resolve it),
//! records a state row, and returns the Intuit consent URL for the FE to
//! hand off to (popup or full-page redirect).

use crate::integrations::quickbooks::oauth::state::{CreateState, QuickbooksOauthStateService};
use crate::server::api::middlewares::workspace_context::WorkspaceManagerExtractor;
use axum::Json;
use axum::http::{HeaderMap, StatusCode, header};
use oxy_auth::extractor::AuthenticatedUserExtractor;
use serde::{Deserialize, Serialize};

const AUTHORIZE_URL: &str = "https://appcenter.intuit.com/connect/oauth2";
const SCOPE: &str = "com.intuit.quickbooks.accounting";

#[derive(Debug, Deserialize)]
pub struct AuthorizeRequest {
    pub client_id: String,
    /// Plaintext client secret to store under `client_secret_var`. Omit to
    /// reuse an already-stored secret of that name (e.g. on reconnect).
    #[serde(default)]
    pub client_secret: Option<String>,
    pub client_secret_var: String,
    pub refresh_token_var: String,
    /// `popup` (default) or `redirect` — how the success page returns.
    #[serde(default)]
    pub mode: Option<String>,
    /// FE path to return to in `redirect` mode.
    #[serde(default)]
    pub return_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AuthorizeResponse {
    /// Pre-built Intuit consent URL. The FE opens it in a popup or navigates.
    pub url: String,
}

pub async fn authorize(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    headers: HeaderMap,
    Json(req): Json<AuthorizeRequest>,
) -> Result<Json<AuthorizeResponse>, (StatusCode, String)> {
    // Persist the client secret so the (unauthenticated) callback can resolve
    // it for the token exchange. Blank value → reuse an existing secret.
    if let Some(secret) = req
        .client_secret
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        workspace_manager
            .secrets_manager
            .upsert_secret(&req.client_secret_var, secret, user.id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("store client secret: {e}"),
                )
            })?;
    }

    let redirect_uri = callback_redirect_uri(&headers)?;
    let mode = match req.mode.as_deref() {
        Some("redirect") => "redirect",
        _ => "popup",
    }
    .to_string();

    let nonce = QuickbooksOauthStateService::create(CreateState {
        project_id: workspace_manager.workspace_id,
        client_id: req.client_id.clone(),
        client_secret_var: req.client_secret_var,
        refresh_token_var: req.refresh_token_var,
        redirect_uri: redirect_uri.clone(),
        mode,
        return_path: req.return_path,
        created_by: Some(user.id),
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let url = format!(
        "{AUTHORIZE_URL}?client_id={}&response_type=code&scope={}&redirect_uri={}&state={}",
        urlencoding::encode(&req.client_id),
        urlencoding::encode(SCOPE),
        urlencoding::encode(&redirect_uri),
        urlencoding::encode(&nonce),
    );
    Ok(Json(AuthorizeResponse { url }))
}

/// Build the public callback URL from the inbound request host/proto. BYO
/// users must register exactly this URL in their Intuit app, and it must
/// match what the token exchange later sends — so it's computed once here
/// and stored on the state row for verbatim reuse in the callback.
fn callback_redirect_uri(headers: &HeaderMap) -> Result<String, (StatusCode, String)> {
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .ok_or((StatusCode::BAD_REQUEST, "missing Host header".to_string()))?;
    let proto = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| {
            if host.starts_with("localhost") || host.starts_with("127.0.0.1") {
                "http".to_string()
            } else {
                "https".to_string()
            }
        });
    Ok(format!("{proto}://{host}/api/quickbooks/oauth/callback"))
}
