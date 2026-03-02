use axum::extract::State;
use axum::{
    extract,
    http::{HeaderMap, StatusCode},
    response::Json,
};
use chrono::{Duration, Utc};
use entity::{prelude::Users, users, users::UserStatus};
use governor::{
    DefaultKeyedRateLimiter, Quota, RateLimiter,
    clock::{Clock, DefaultClock},
};
use handlebars::Handlebars;
use jsonwebtoken::{EncodingKey, Header, encode};
use once_cell::sync::Lazy;
use oxy::config::auth::MagicLinkAuth;
use sea_orm::{ActiveModelTrait, DatabaseConnection, DbErr, EntityTrait, Set};
use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;
use url::Url;
use uuid::Uuid;

// ─── Magic Link Rate Limiter ────────────────────────────────────────────────
//
// Token-bucket rate limiter (governor). State is in-process only and resets
// on restart — intentional, no external dependency required.
// Checked before the allowlist so timing cannot reveal allowlist membership.
//
// Limit: 5 requests per email per hour.

static MAGIC_LINK_RATE_LIMITER: Lazy<DefaultKeyedRateLimiter<String>> =
    Lazy::new(|| RateLimiter::keyed(Quota::per_hour(NonZeroU32::new(5).expect("5 > 0"))));

/// Returns `None` if the request is allowed, or `Some(seconds)` with the wait
/// time until the next request is permitted.
fn check_magic_link_rate_limit(email: &str) -> Option<u64> {
    match MAGIC_LINK_RATE_LIMITER.check_key(&email.to_lowercase()) {
        Ok(()) => None,
        Err(not_until) => {
            let wait = not_until.wait_time_from(DefaultClock::default().now());
            Some(wait.as_secs().max(1))
        }
    }
}

use crate::server::router::AppState;
use oxy::{
    config::constants::AUTHENTICATION_SECRET_KEY,
    database::{client::establish_connection, filters::UserQueryFilterExt},
};
use oxy_shared::errors::OxyError;

#[derive(Deserialize)]
pub struct GoogleAuthRequest {
    pub code: String,
}

#[derive(Deserialize)]
pub struct OktaAuthRequest {
    pub code: String,
}

#[derive(Deserialize)]
pub struct MagicLinkRequest {
    pub email: String,
}

#[derive(Deserialize)]
pub struct MagicLinkVerifyRequest {
    pub token: String,
}

#[derive(Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub user: UserInfo,
}

#[derive(Serialize)]
pub struct UserInfo {
    pub id: String,
    pub email: String,
    pub name: String,
    pub picture: Option<String>,
    pub role: String,
}

#[derive(Serialize)]
pub struct MessageResponse {
    pub message: String,
}

#[derive(Serialize, Deserialize)]
struct Claims {
    sub: String,
    email: String,
    exp: usize,
    iat: usize,
}

#[derive(Serialize)]
pub struct AuthConfigResponse {
    pub is_built_in_mode: bool,
    pub auth_enabled: bool,
    pub google: Option<GoogleConfig>,
    pub okta: Option<OktaConfig>,
    pub magic_link: Option<bool>,
    pub cloud: bool,
    pub enterprise: bool,
    pub readonly: bool,
}

#[derive(Serialize)]
pub struct GoogleConfig {
    pub client_id: String,
}

#[derive(Serialize)]
pub struct OktaConfig {
    pub client_id: String,
    pub domain: String,
}

pub async fn get_config(
    State(app_state): State<AppState>,
) -> Result<Json<AuthConfigResponse>, StatusCode> {
    let auth_config = oxy::config::oxy::get_oxy_config()
        .ok()
        .and_then(|config| config.authentication);

    let has_google = auth_config
        .as_ref()
        .and_then(|auth| auth.google.as_ref())
        .is_some();
    let has_okta = auth_config
        .as_ref()
        .and_then(|auth| auth.okta.as_ref())
        .is_some();
    let has_magic_link = auth_config
        .as_ref()
        .and_then(|auth| auth.magic_link.as_ref())
        .is_some();

    let auth_enabled = has_google || has_okta || has_magic_link;

    if !auth_enabled || app_state.internal {
        return Ok(Json(AuthConfigResponse {
            is_built_in_mode: true,
            auth_enabled: false,
            google: None,
            okta: None,
            magic_link: None,
            cloud: app_state.cloud,
            enterprise: app_state.enterprise,
            readonly: app_state.readonly,
        }));
    }

    let google_client_id = auth_config
        .as_ref()
        .and_then(|auth| auth.google.as_ref())
        .map(|google| google.client_id.clone());
    let okta_config = auth_config
        .as_ref()
        .and_then(|auth| auth.okta.as_ref())
        .map(|okta| OktaConfig {
            client_id: okta.client_id.clone(),
            domain: okta.domain.clone(),
        });

    let config = AuthConfigResponse {
        is_built_in_mode: false,
        auth_enabled: true,
        google: google_client_id.map(|client_id| GoogleConfig { client_id }),
        okta: okta_config,
        magic_link: if has_magic_link { Some(true) } else { None },
        cloud: app_state.cloud,
        enterprise: app_state.enterprise,
        readonly: app_state.readonly,
    };

    Ok(Json(config))
}

pub async fn create_auth_token(user: users::Model) -> Result<String, StatusCode> {
    let connection = establish_connection().await.map_err(|e| {
        tracing::error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let user_clone = user.clone();
    let mut user_update: users::ActiveModel = user.into();
    user_update.last_login_at = Set(chrono::Utc::now().into());
    user_update.update(&connection).await.map_err(|e| {
        tracing::error!("Failed to update user last login: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let now = Utc::now();
    let exp = now + Duration::weeks(1);

    let claims = Claims {
        sub: user_clone.id.to_string(),
        email: user_clone.email.clone(),
        exp: exp.timestamp() as usize,
        iat: now.timestamp() as usize,
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(AUTHENTICATION_SECRET_KEY.as_bytes()),
    )
    .map_err(|e| {
        tracing::error!("Failed to generate JWT token: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(token)
}

pub async fn google_auth(
    headers: HeaderMap,
    extract::Json(google_request): extract::Json<GoogleAuthRequest>,
) -> Result<Json<AuthResponse>, StatusCode> {
    let base_url = extract_base_url_from_headers(&headers);
    let user_info = exchange_google_code_for_user_info(&google_request.code, &base_url)
        .await
        .map_err(|e| {
            tracing::error!("Failed to exchange Google code: {}", e);
            StatusCode::UNAUTHORIZED
        })?;

    let connection = establish_connection().await.map_err(|e| {
        tracing::error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let user = match Users::find()
        .filter_by_email(&user_info.email)
        .one(&connection)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query user: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })? {
        Some(existing_user) if existing_user.status == UserStatus::Active => {
            // Update existing active user
            let mut user_update: users::ActiveModel = existing_user.clone().into();
            user_update.name = Set(user_info.name.clone());
            user_update.picture = Set(user_info.picture.clone());
            user_update.email_verified = Set(true);
            user_update.last_login_at = Set(chrono::Utc::now().into());
            user_update.update(&connection).await.map_err(|e| {
                tracing::error!("Failed to update user: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?
        }
        Some(existing_user) if existing_user.status == UserStatus::Deleted => {
            // User account has been deleted - unauthorized
            tracing::warn!(
                "Deleted user {} attempted to authenticate via Google",
                user_info.email
            );
            return Err(StatusCode::UNAUTHORIZED);
        }
        Some(existing_user) => {
            // Handle any other status - update existing user info
            let mut user_update: users::ActiveModel = existing_user.clone().into();
            user_update.name = Set(user_info.name.clone());
            user_update.picture = Set(user_info.picture.clone());
            user_update.email_verified = Set(true);
            user_update.last_login_at = Set(chrono::Utc::now().into());
            user_update.update(&connection).await.map_err(|e| {
                tracing::error!("Failed to update user: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?
        }
        None => {
            let new_user = users::ActiveModel {
                id: Set(Uuid::new_v4()),
                email: Set(user_info.email.clone()),
                name: Set(user_info.name.clone()),
                picture: Set(user_info.picture.clone()),
                email_verified: Set(true),
                magic_link_token: sea_orm::ActiveValue::NotSet,
                magic_link_token_expires_at: sea_orm::ActiveValue::NotSet,
                role: Set(users::UserRole::Member),
                status: Set(UserStatus::Active),
                created_at: sea_orm::ActiveValue::NotSet,
                last_login_at: sea_orm::ActiveValue::NotSet,
            };

            insert_user_or_fetch_existing(new_user, &user_info.email, &connection).await?
        }
    };

    let token = create_auth_token(user.clone()).await.map_err(|e| {
        tracing::error!("Failed to create auth token: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let auth_response = AuthResponse {
        token,
        user: UserInfo {
            id: user.id.to_string(),
            email: user.email,
            name: user.name,
            picture: user.picture,
            role: user.role.as_str().to_string(),
        },
    };

    Ok(Json(auth_response))
}

pub async fn okta_auth(
    headers: HeaderMap,
    extract::Json(okta_request): extract::Json<OktaAuthRequest>,
) -> Result<Json<AuthResponse>, StatusCode> {
    let base_url = extract_base_url_from_headers(&headers);
    let user_info = exchange_okta_code_for_user_info(&okta_request.code, &base_url)
        .await
        .map_err(|e| {
            tracing::error!("Failed to exchange Okta code: {}", e);
            StatusCode::UNAUTHORIZED
        })?;

    let connection = establish_connection().await.map_err(|e| {
        tracing::error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let user = match Users::find()
        .filter_by_email(&user_info.email)
        .one(&connection)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query user: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })? {
        Some(existing_user) if existing_user.status == UserStatus::Deleted => {
            // User account has been deleted - unauthorized
            tracing::warn!(
                "Deleted user {} attempted to authenticate via Okta",
                user_info.email
            );
            return Err(StatusCode::UNAUTHORIZED);
        }
        Some(existing_user) => {
            // Update existing user info
            let mut user_update: users::ActiveModel = existing_user.clone().into();
            user_update.name = Set(user_info.name.clone());
            user_update.picture = Set(user_info.picture.clone());
            user_update.email_verified = Set(true);
            user_update.last_login_at = Set(chrono::Utc::now().into());
            user_update.update(&connection).await.map_err(|e| {
                tracing::error!("Failed to update user: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?
        }
        None => {
            let new_user = users::ActiveModel {
                id: Set(Uuid::new_v4()),
                email: Set(user_info.email.clone()),
                name: Set(user_info.name.clone()),
                picture: Set(user_info.picture.clone()),
                email_verified: Set(true),
                magic_link_token: sea_orm::ActiveValue::NotSet,
                magic_link_token_expires_at: sea_orm::ActiveValue::NotSet,
                role: Set(users::UserRole::Member),
                status: Set(UserStatus::Active),
                created_at: sea_orm::ActiveValue::NotSet,
                last_login_at: sea_orm::ActiveValue::NotSet,
            };

            insert_user_or_fetch_existing(new_user, &user_info.email, &connection).await?
        }
    };

    let token = create_auth_token(user.clone()).await.map_err(|e| {
        tracing::error!("Failed to create auth token: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let auth_response = AuthResponse {
        token,
        user: UserInfo {
            id: user.id.to_string(),
            email: user.email,
            name: user.name,
            picture: user.picture,
            role: user.role.as_str().to_string(),
        },
    };

    Ok(Json(auth_response))
}

/// Check if a database error is a unique constraint violation.
fn is_unique_violation(err: &DbErr) -> bool {
    let err_str = err.to_string().to_lowercase();
    err_str.contains("duplicate key") || err_str.contains("unique constraint")
}

/// Insert a new user, handling the race condition where another request may have
/// created the same user concurrently.
async fn insert_user_or_fetch_existing(
    new_user: users::ActiveModel,
    email: &str,
    connection: &DatabaseConnection,
) -> Result<users::Model, StatusCode> {
    match new_user.insert(connection).await {
        Ok(user) => Ok(user),
        Err(e) if is_unique_violation(&e) => {
            // Race condition: another request created the user concurrently.
            // Fetch the existing user.
            Users::find()
                .filter_by_email(email)
                .one(connection)
                .await
                .map_err(|e| {
                    tracing::error!("Failed to query user after unique violation: {}", e);
                    StatusCode::INTERNAL_SERVER_ERROR
                })?
                .ok_or_else(|| {
                    tracing::error!(
                        "User '{}' not found after unique constraint violation",
                        email
                    );
                    StatusCode::INTERNAL_SERVER_ERROR
                })
        }
        Err(e) => {
            tracing::error!("Failed to create user: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

fn extract_base_url_from_headers(headers: &HeaderMap) -> String {
    if let Some(origin) = headers.get("origin").and_then(|h| h.to_str().ok()) {
        return origin.to_string();
    }

    if let Some(referer) = headers.get("referer").and_then(|h| h.to_str().ok())
        && let Ok(url) = Url::parse(referer)
        && let Some(host) = url.host_str()
    {
        let port = url.port().map(|p| format!(":{p}")).unwrap_or_default();
        return format!("{}://{}{}", url.scheme(), host, port);
    }
    "http://localhost:3000".to_string()
}

#[derive(Deserialize)]
struct GoogleUserInfo {
    email: String,
    name: String,
    picture: Option<String>,
}

async fn exchange_google_code_for_user_info(
    code: &str,
    base_url: &str,
) -> Result<GoogleUserInfo, OxyError> {
    let auth_config = oxy::config::oxy::get_oxy_config()
        .ok()
        .and_then(|config| config.authentication);

    let google_config = auth_config.and_then(|auth| auth.google).ok_or_else(|| {
        OxyError::ConfigurationError("Google OAuth configuration not found".to_string())
    })?;

    let client = reqwest::Client::new();

    let redirect_uri = format!("{base_url}/auth/google/callback");

    let client_secret = google_config.client_secret;

    let token_request = serde_json::json!({
        "client_id": google_config.client_id,
        "client_secret": client_secret,
        "code": code,
        "grant_type": "authorization_code",
        "redirect_uri": redirect_uri
    });

    // Note: Google supports application/json for token exchange (non-standard but accepted)
    // Standard OAuth 2.0 requires application/x-www-form-urlencoded
    let token_response = client
        .post("https://oauth2.googleapis.com/token")
        .header("Content-Type", "application/json")
        .json(&token_request)
        .send()
        .await
        .map_err(|e| {
            tracing::error!("Failed to send token request to Google: {}", e);
            OxyError::ConfigurationError(format!("Failed to exchange code for token: {e}"))
        })?;

    // Check response status before parsing
    let status = token_response.status();
    if !status.is_success() {
        let error_body = token_response.text().await.unwrap_or_default();
        tracing::error!(
            "Google token exchange failed with status {}: {}",
            status,
            error_body
        );
        return Err(OxyError::ConfigurationError(format!(
            "Google token exchange failed with status {}: {}",
            status, error_body
        )));
    }

    let token_data: serde_json::Value = token_response.json().await.map_err(|e| {
        tracing::error!("Failed to parse Google token response: {}", e);
        OxyError::ConfigurationError(format!("Failed to parse token response: {e}"))
    })?;

    let access_token = token_data["access_token"]
        .as_str()
        .ok_or_else(|| OxyError::ConfigurationError("No access token in response".to_string()))?;

    let user_info_response = client
        .get("https://www.googleapis.com/oauth2/v2/userinfo")
        .header("Authorization", format!("Bearer {access_token}"))
        .send()
        .await
        .map_err(|e| {
            tracing::error!("Failed to send userinfo request to Google: {}", e);
            OxyError::ConfigurationError(format!("Failed to get user info: {e}"))
        })?;

    // Check response status before parsing
    let status = user_info_response.status();
    if !status.is_success() {
        let error_body = user_info_response.text().await.unwrap_or_default();
        tracing::error!(
            "Google userinfo request failed with status {}: {}",
            status,
            error_body
        );
        return Err(OxyError::ConfigurationError(format!(
            "Google userinfo request failed with status {}: {}",
            status, error_body
        )));
    }

    let user_info: GoogleUserInfo = user_info_response.json().await.map_err(|e| {
        tracing::error!("Failed to parse Google userinfo response: {}", e);
        OxyError::ConfigurationError(format!("Failed to parse user info: {e}"))
    })?;

    Ok(user_info)
}

#[derive(Deserialize)]
struct OktaUserInfo {
    email: String,
    name: String,
    picture: Option<String>,
}

async fn exchange_okta_code_for_user_info(
    code: &str,
    base_url: &str,
) -> Result<OktaUserInfo, OxyError> {
    let auth_config = oxy::config::oxy::get_oxy_config()
        .ok()
        .and_then(|config| config.authentication);

    let okta_config = auth_config.and_then(|auth| auth.okta).ok_or_else(|| {
        OxyError::ConfigurationError("Okta OAuth configuration not found".to_string())
    })?;

    let client = reqwest::Client::new();

    let redirect_uri = format!("{base_url}/auth/okta/callback");

    let client_secret = okta_config.client_secret;
    let okta_domain = okta_config.domain;

    // Exchange authorization code for tokens
    // OAuth 2.0 requires application/x-www-form-urlencoded for token requests
    let token_params = [
        ("client_id", okta_config.client_id.as_str()),
        ("client_secret", client_secret.as_str()),
        ("code", code),
        ("grant_type", "authorization_code"),
        ("redirect_uri", redirect_uri.as_str()),
    ];

    // Use org authorization server (matches /oauth2/v1/authorize from frontend)
    let token_url = format!("https://{}/oauth2/v1/token", okta_domain);

    let token_response = client
        .post(&token_url)
        .form(&token_params)
        .send()
        .await
        .map_err(|e| {
            tracing::error!("Failed to send token request to Okta: {}", e);
            OxyError::ConfigurationError(format!("Failed to exchange code for token: {e}"))
        })?;

    // Check response status before parsing
    let status = token_response.status();
    if !status.is_success() {
        let error_body = token_response.text().await.unwrap_or_default();
        tracing::error!(
            "Okta token exchange failed with status {}: {}",
            status,
            error_body
        );
        return Err(OxyError::ConfigurationError(format!(
            "Okta token exchange failed with status {}: {}",
            status, error_body
        )));
    }

    let token_data: serde_json::Value = token_response.json().await.map_err(|e| {
        tracing::error!("Failed to parse Okta token response: {}", e);
        OxyError::ConfigurationError(format!("Failed to parse token response: {e}"))
    })?;

    let access_token = token_data["access_token"]
        .as_str()
        .ok_or_else(|| OxyError::ConfigurationError("No access token in response".to_string()))?;

    // Get user info using the access token (use org authorization server)
    let userinfo_url = format!("https://{}/oauth2/v1/userinfo", okta_domain);

    let user_info_response = client
        .get(&userinfo_url)
        .header("Authorization", format!("Bearer {access_token}"))
        .send()
        .await
        .map_err(|e| {
            tracing::error!("Failed to send userinfo request to Okta: {}", e);
            OxyError::ConfigurationError(format!("Failed to get user info: {e}"))
        })?;

    // Check response status before parsing
    let status = user_info_response.status();
    if !status.is_success() {
        let error_body = user_info_response.text().await.unwrap_or_default();
        tracing::error!(
            "Okta userinfo request failed with status {}: {}",
            status,
            error_body
        );
        return Err(OxyError::ConfigurationError(format!(
            "Okta userinfo request failed with status {}: {}",
            status, error_body
        )));
    }

    let user_info: OktaUserInfo = user_info_response.json().await.map_err(|e| {
        tracing::error!("Failed to parse Okta userinfo response: {}", e);
        OxyError::ConfigurationError(format!("Failed to parse user info: {e}"))
    })?;

    Ok(user_info)
}

// ─── Magic Link ────────────────────────────────────────────────────────────

/// Basic RFC-5321-bounded email format check. Not exhaustive, but filters out
/// obviously malformed inputs before they reach the DB or SES.
fn is_valid_email_format(email: &str) -> bool {
    if email.len() > 254 {
        return false;
    }
    // split_once splits at the first '@'; rejecting any additional '@' in the
    // domain part enforces exactly one '@' (RFC 5321 §4.1.2).
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    if local.is_empty() || domain.is_empty() || domain.contains('@') {
        return false;
    }
    // Domain must have at least one '.' with non-empty labels on both sides.
    let labels: Vec<&str> = domain.split('.').collect();
    labels.len() >= 2 && labels.iter().all(|l| !l.is_empty())
}

fn is_email_allowed(email: &str, config: &MagicLinkAuth) -> bool {
    if config.allowed_domains.is_empty() && config.allowed_emails.is_empty() {
        return true;
    }
    // email is already lowercased at ingestion; normalize config values too so
    // operators can write "Company.com" or "company.com" interchangeably.
    for domain in &config.allowed_domains {
        if email.ends_with(&format!("@{}", domain.to_lowercase())) {
            return true;
        }
    }
    config
        .allowed_emails
        .iter()
        .any(|e| e.eq_ignore_ascii_case(email))
}

pub async fn request_magic_link(
    headers: HeaderMap,
    extract::Json(req): extract::Json<MagicLinkRequest>,
) -> axum::response::Response {
    use axum::http::header::RETRY_AFTER;
    use axum::response::IntoResponse;

    // Normalize email to lowercase at the point of ingestion so all downstream
    // code (allowlist check, DB queries, SES) operates on a consistent value.
    let req = MagicLinkRequest {
        email: req.email.to_lowercase(),
    };

    // Validate email format before doing anything else.
    if !is_valid_email_format(&req.email) {
        return (
            StatusCode::BAD_REQUEST,
            Json(MessageResponse {
                message: "Invalid email address.".to_string(),
            }),
        )
            .into_response();
    }

    // Rate limit — checked before allowlist so timing cannot reveal allowlist membership.
    if let Some(retry_after_secs) = check_magic_link_rate_limit(&req.email) {
        let mins = retry_after_secs.div_ceil(60);
        tracing::warn!("Magic link rate limit exceeded for: {}", req.email);
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [(
                RETRY_AFTER,
                axum::http::HeaderValue::from_str(&retry_after_secs.to_string())
                    .unwrap_or_else(|_| axum::http::HeaderValue::from_static("3600")),
            )],
            Json(MessageResponse {
                message: format!(
                    "Too many sign-in attempts. Please try again in {mins} minute{}.",
                    if mins == 1 { "" } else { "s" }
                ),
            }),
        )
            .into_response();
    }

    request_magic_link_inner(headers, req).await.into_response()
}

async fn request_magic_link_inner(
    headers: HeaderMap,
    req: MagicLinkRequest,
) -> Result<Json<MessageResponse>, StatusCode> {
    let auth_config = oxy::config::oxy::get_oxy_config()
        .ok()
        .and_then(|c| c.authentication)
        .and_then(|a| a.magic_link);

    let magic_link_config = auth_config.ok_or_else(|| {
        tracing::error!("Magic link auth not configured");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Always return 200 — don't leak whether email is allowed
    if !is_email_allowed(&req.email, &magic_link_config) {
        tracing::info!(
            "Magic link requested for non-allowlisted email: {}",
            req.email
        );
        return Ok(Json(MessageResponse {
            message: "If your email is eligible, a sign-in link has been sent.".to_string(),
        }));
    }

    let connection = establish_connection().await.map_err(|e| {
        tracing::error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Find or create user
    let existing = Users::find()
        .filter_by_email(&req.email)
        .one(&connection)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query user: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Resolve the user model — either the existing one or a freshly inserted one.
    // We capture the model here so we can reuse it for the token update below
    // without an extra DB round-trip.
    let user_for_update = match existing {
        Some(u) if u.status == UserStatus::Deleted => {
            // Silently succeed — don't reveal account status
            return Ok(Json(MessageResponse {
                message: "If your email is eligible, a sign-in link has been sent.".to_string(),
            }));
        }
        Some(u) => u, // existing active user — reuse model directly
        None => {
            // Auto-create new user
            let name = req.email.split('@').next().unwrap_or("User").to_string();
            let new_user = users::ActiveModel {
                id: Set(Uuid::new_v4()),
                email: Set(req.email.clone()),
                name: Set(name),
                picture: Set(None),
                email_verified: Set(false),
                magic_link_token: Set(None),
                magic_link_token_expires_at: Set(None),
                role: Set(users::UserRole::Member),
                status: Set(UserStatus::Active),
                created_at: sea_orm::ActiveValue::NotSet,
                last_login_at: sea_orm::ActiveValue::NotSet,
            };
            match new_user.insert(&connection).await {
                Ok(inserted) => inserted,
                Err(e) if is_unique_violation(&e) => {
                    // Race condition — another request created the user concurrently.
                    Users::find()
                        .filter_active_by_email(&req.email)
                        .one(&connection)
                        .await
                        .map_err(|e| {
                            tracing::error!("Failed to query user after race condition: {}", e);
                            StatusCode::INTERNAL_SERVER_ERROR
                        })?
                        .ok_or_else(|| {
                            tracing::error!("User not found after unique violation: {}", req.email);
                            StatusCode::INTERNAL_SERVER_ERROR
                        })?
                }
                Err(e) => {
                    tracing::error!("Failed to create user: {}", e);
                    return Err(StatusCode::INTERNAL_SERVER_ERROR);
                }
            }
        }
    };

    // Generate 256-bit random token
    let token_bytes: [u8; 32] = rand::random();
    let token = hex::encode(token_bytes);
    let expires_at = Utc::now() + Duration::minutes(15);

    // Update user row with token + expiry — reuses the model from above, no extra query.
    let mut active: users::ActiveModel = user_for_update.into();
    active.magic_link_token = Set(Some(token.clone()));
    active.magic_link_token_expires_at = Set(Some(expires_at.into()));
    active.update(&connection).await.map_err(|e| {
        tracing::error!("Failed to save magic link token: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Send email async
    let base_url = extract_base_url_from_headers(&headers);

    // Log the magic link URL at debug level only — the token is a session credential
    // and must not appear in production log aggregators.
    tracing::debug!(
        "Magic link for {}: {}/auth/magic-link/callback?token={}",
        req.email,
        base_url,
        token
    );

    let email_addr = req.email.clone();
    let cfg_clone = magic_link_config.clone();
    tokio::spawn(async move {
        if let Err(e) = send_magic_link_email(&email_addr, &token, &base_url, &cfg_clone).await {
            tracing::error!("Failed to send magic link email: {}", e);
        }
    });

    Ok(Json(MessageResponse {
        message: "If your email is eligible, a sign-in link has been sent.".to_string(),
    }))
}

pub async fn verify_magic_link(
    extract::Json(req): extract::Json<MagicLinkVerifyRequest>,
) -> Result<Json<AuthResponse>, StatusCode> {
    let connection = establish_connection().await.map_err(|e| {
        tracing::error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let user = Users::find()
        .filter_active_by_magic_link_token(&req.token)
        .one(&connection)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query user by magic link token: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // Check expiry
    let expires_at = user
        .magic_link_token_expires_at
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if Utc::now() > expires_at.with_timezone(&Utc) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Clear token, mark email verified
    let user_clone = user.clone();
    let mut user_update: users::ActiveModel = user.into();
    user_update.magic_link_token = Set(None);
    user_update.magic_link_token_expires_at = Set(None);
    user_update.email_verified = Set(true);
    user_update.update(&connection).await.map_err(|e| {
        tracing::error!("Failed to clear magic link token: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let token = create_auth_token(user_clone.clone()).await.map_err(|e| {
        tracing::error!("Failed to create auth token: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(AuthResponse {
        token,
        user: UserInfo {
            id: user_clone.id.to_string(),
            email: user_clone.email,
            name: user_clone.name,
            picture: user_clone.picture,
            role: user_clone.role.as_str().to_string(),
        },
    }))
}

async fn send_magic_link_email(
    to_email: &str,
    token: &str,
    base_url: &str,
    config: &MagicLinkAuth,
) -> Result<(), OxyError> {
    use crate::emails::{
        EmailMessage, EmailProvider, local_test::LocalTestEmailProvider, ses::SesEmailProvider,
    };

    let magic_link_url = format!("{base_url}/auth/magic-link/callback?token={token}");
    let message = EmailMessage {
        subject: "Sign in to Oxy".to_string(),
        html_body: build_magic_link_email_html(&magic_link_url, to_email)?,
        text_body: format!(
            "Your sign-in link for Oxy\n\nClick the link below to sign in. For security, this link expires in 15 minutes and can only be used once.\n\n{magic_link_url}\n\nThis link was requested for {to_email}. If you didn't request this, you can safely ignore this email — your account remains secure."
        ),
    };

    if std::env::var("MAGIC_LINK_LOCAL_TEST").is_ok() {
        LocalTestEmailProvider
            .send(&config.from_email, to_email, message)
            .await
    } else {
        SesEmailProvider::new(config.aws_region.as_deref())
            .await
            .send(&config.from_email, to_email, message)
            .await
    }
}

static MAGIC_LINK_TEMPLATE: Lazy<Handlebars<'static>> = Lazy::new(|| {
    let mut hbs = Handlebars::new();
    hbs.register_template_string("magic_link", include_str!("../../emails/magic_link.hbs"))
        .expect("magic_link.hbs is valid");
    hbs
});

fn build_magic_link_email_html(magic_link_url: &str, to_email: &str) -> Result<String, OxyError> {
    let data = serde_json::json!({
        "magic_link_url": magic_link_url,
        "to_email": to_email,
        "year": Utc::now().format("%Y").to_string(),
    });

    MAGIC_LINK_TEMPLATE
        .render("magic_link", &data)
        .map_err(|e| OxyError::RuntimeError(format!("Failed to render magic link template: {e}")))
}
