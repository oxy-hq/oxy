//! GET /api/quickbooks/oauth/callback
//!
//! Public — Intuit redirects the bare browser here (no JWT). The state
//! nonce is the CSRF/replay guard. Resolves the client secret, exchanges
//! the authorization code for tokens, lands the refresh token in the
//! workspace secret manager (same `upsert_secret` path as rotation), and
//! redirects to the FE success page carrying the captured realm id.

use crate::integrations::quickbooks::oauth::state::QuickbooksOauthStateService;
use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use oxy::adapters::secrets::SecretsManager;
use oxy::service::secret_manager::SecretManagerService;
use serde::Deserialize;
use serde_json::Value;
use tracing::warn;
use uuid::Uuid;

const TOKEN_URL: &str = "https://oauth.platform.intuit.com/oauth2/v1/tokens/bearer";

/// Generic message returned to the (unauthenticated) browser; details are
/// logged server-side rather than leaked into the response.
const GENERIC_ERROR: &str = "QuickBooks connection failed. Please try again.";

#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    #[serde(rename = "realmId")]
    pub realm_id: Option<String>,
    pub error: Option<String>,
}

pub async fn callback(Query(q): Query<CallbackQuery>) -> Response {
    if let Some(err) = q.error {
        return (
            StatusCode::BAD_REQUEST,
            format!("QuickBooks authorization cancelled: {err}"),
        )
            .into_response();
    }
    let code = match q.code {
        Some(c) => c,
        None => return (StatusCode::BAD_REQUEST, "missing code").into_response(),
    };
    let nonce = match q.state {
        Some(s) => s,
        None => return (StatusCode::BAD_REQUEST, "missing state").into_response(),
    };
    let realm_id = q.realm_id.unwrap_or_default();

    let state = match QuickbooksOauthStateService::consume(&nonce).await {
        Ok(s) => s,
        Err(e) => {
            warn!("quickbooks oauth: state consume failed: {e}");
            return (
                StatusCode::BAD_REQUEST,
                "QuickBooks authorization link is invalid or expired — start Connect again.",
            )
                .into_response();
        }
    };

    // Project-scoped secret store (DB-first, env fallback for resolve).
    let secrets = match SecretsManager::from_database_with_env_fallback(SecretManagerService::new(
        state.project_id,
    )) {
        Ok(s) => s,
        Err(e) => {
            warn!("quickbooks oauth: secret manager init failed: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, GENERIC_ERROR).into_response();
        }
    };

    let client_secret = match secrets.resolve_secret(&state.client_secret_var).await {
        Ok(Some(s)) => s,
        Ok(None) => {
            return (
                StatusCode::BAD_REQUEST,
                "QuickBooks client secret not found — re-enter it and try Connect again.",
            )
                .into_response();
        }
        Err(e) => {
            warn!("quickbooks oauth: resolve client secret failed: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, GENERIC_ERROR).into_response();
        }
    };

    let refresh_token =
        match exchange_code(&state.client_id, &client_secret, &code, &state.redirect_uri).await {
            Ok(t) => t,
            Err(e) => {
                warn!("quickbooks oauth: token exchange failed: {e}");
                return (
                    StatusCode::BAD_GATEWAY,
                    "QuickBooks token exchange failed — check the client id/secret and try again.",
                )
                    .into_response();
            }
        };

    if let Err(e) = secrets
        .upsert_secret(
            &state.refresh_token_var,
            &refresh_token,
            state.created_by.unwrap_or_else(Uuid::nil),
        )
        .await
    {
        warn!("quickbooks oauth: store refresh token failed: {e}");
        return (StatusCode::INTERNAL_SERVER_ERROR, GENERIC_ERROR).into_response();
    }

    Redirect::to(&success_redirect(
        &state.mode,
        &realm_id,
        state.return_path.as_deref(),
        &state.redirect_uri,
    ))
    .into_response()
}

/// Exchange an authorization code for tokens; returns the refresh token.
async fn exchange_code(
    client_id: &str,
    client_secret: &str,
    code: &str,
    redirect_uri: &str,
) -> Result<String, String> {
    let resp = reqwest::Client::new()
        .post(TOKEN_URL)
        .basic_auth(client_id, Some(client_secret))
        .header(reqwest::header::ACCEPT, "application/json")
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    let json: Value = resp.json().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("HTTP {status}: {json}"));
    }
    json["refresh_token"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "response missing refresh_token".to_string())
}

/// Where to send the browser after the token is stored.
///
/// `return_path` is the **absolute** URL of the originating frontend page
/// (sent by the FE). The frontend origin can differ from this callback's
/// origin — in dev the SPA runs on Vite (:5173) while the API/callback is
/// on the backend (:3000) — so a relative redirect would 404 on the
/// backend. We therefore target the frontend origin explicitly:
/// - popup mode → the FE-origin `/quickbooks/connected` page, which
///   postMessages the opener and closes.
/// - redirect mode → straight back to the originating FE page with the
///   result as query params (no intermediate page).
///
/// Falls back to a relative path when `return_path` isn't an absolute URL
/// (single-origin deployments where relative resolves correctly anyway).
fn success_redirect(
    mode: &str,
    realm_id: &str,
    return_path: Option<&str>,
    redirect_uri: &str,
) -> String {
    // Defense-in-depth against open redirect: only honor a `return_path`
    // that resolves to an allowed origin (this deployment's own origin, or
    // localhost in dev). Anything else is dropped and we fall back to a
    // same-origin relative target.
    let safe = return_path.filter(|rp| return_origin_allowed(rp, redirect_uri));
    if mode == "redirect" {
        let base = safe.unwrap_or("/");
        let sep = if base.contains('?') { "&" } else { "?" };
        return format!(
            "{base}{sep}qb_connected=ok&realm_id={}",
            urlencoding::encode(realm_id)
        );
    }
    let origin = safe.and_then(parse_origin).unwrap_or_default();
    format!(
        "{origin}/quickbooks/connected?mode=popup&realm_id={}",
        urlencoding::encode(realm_id)
    )
}

/// Whether `return_path` is an absolute http(s) URL whose origin is allowed:
/// the same origin as this deployment's callback (`redirect_uri`), or a
/// localhost dev origin. Blocks redirecting the browser to an arbitrary
/// external site after consent.
fn return_origin_allowed(return_path: &str, redirect_uri: &str) -> bool {
    let Ok(target) = reqwest::Url::parse(return_path) else {
        return false;
    };
    if !matches!(target.scheme(), "http" | "https") {
        return false;
    }
    match target.host_str() {
        Some("localhost") | Some("127.0.0.1") => true,
        _ => reqwest::Url::parse(redirect_uri)
            .map(|backend| backend.origin() == target.origin())
            .unwrap_or(false),
    }
}

/// Scheme+host+port of an absolute URL (e.g. `http://localhost:5173`), or
/// `None` for a relative path / opaque origin.
fn parse_origin(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    let origin = parsed.origin();
    origin.is_tuple().then(|| origin.ascii_serialization())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Backend callback origin (what `redirect_uri` on the state row looks like).
    const CB: &str = "http://localhost:3000/api/quickbooks/oauth/callback";

    #[test]
    fn success_redirect_popup_targets_frontend_origin() {
        let u = success_redirect(
            "popup",
            "9341456860808037",
            Some("http://localhost:5173/ide/x/pipelines?a=1"),
            CB,
        );
        assert_eq!(
            u,
            "http://localhost:5173/quickbooks/connected?mode=popup&realm_id=9341456860808037"
        );
    }

    #[test]
    fn success_redirect_popup_relative_when_no_origin() {
        // Single-origin deploy / relative return path → relative success URL.
        let u = success_redirect("popup", "123", None, CB);
        assert_eq!(u, "/quickbooks/connected?mode=popup&realm_id=123");
    }

    #[test]
    fn success_redirect_redirect_mode_bounces_to_return_url() {
        let u = success_redirect(
            "redirect",
            "123",
            Some("http://localhost:5173/ide/x/pipelines"),
            CB,
        );
        assert_eq!(
            u,
            "http://localhost:5173/ide/x/pipelines?qb_connected=ok&realm_id=123"
        );
    }

    #[test]
    fn success_redirect_allows_same_origin_as_callback() {
        let u = success_redirect(
            "redirect",
            "123",
            Some("https://app.example.com/ide/p?x=1"),
            "https://app.example.com/api/quickbooks/oauth/callback",
        );
        assert_eq!(
            u,
            "https://app.example.com/ide/p?x=1&qb_connected=ok&realm_id=123"
        );
    }

    #[test]
    fn success_redirect_rejects_foreign_origin() {
        // Open-redirect guard: an external return_path is dropped.
        let redirect = success_redirect("redirect", "123", Some("https://evil.example/phish"), CB);
        assert_eq!(redirect, "/?qb_connected=ok&realm_id=123");
        let popup = success_redirect("popup", "123", Some("https://evil.example/phish"), CB);
        assert_eq!(popup, "/quickbooks/connected?mode=popup&realm_id=123");
    }

    #[test]
    fn success_redirect_rejects_non_http_scheme() {
        let u = success_redirect("popup", "123", Some("javascript:alert(1)"), CB);
        assert_eq!(u, "/quickbooks/connected?mode=popup&realm_id=123");
    }
}
