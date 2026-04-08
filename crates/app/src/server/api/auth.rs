use axum::extract::State;
use axum::{
    Extension, extract,
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
use oxy::config::{ConfigBuilder, resolve_local_workspace_path};
use oxy_project::LocalGitService;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, PaginatorTrait,
    QueryFilter, Set,
};
use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;
use url::Url;
use uuid::Uuid;

// GIT_HAS_REMOTE and PROTECTED_BRANCHES_FROM_CONFIG were previously process-wide
// OnceCells, but that caused stale values in multi-project mode when
// activate_project switched to a different project after the first init.
// Both values are now computed per-request from resolved_project_path.

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
use oxy_auth::types::AuthenticatedUser;
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
    pub is_admin: bool,
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
    pub auth_enabled: bool,
    pub google: Option<GoogleConfig>,
    pub okta: Option<OktaConfig>,
    pub magic_link: Option<bool>,
    pub enterprise: bool,
    pub readonly: bool,
    /// True when no `config.yml` is found — the frontend should redirect to
    /// the onboarding wizard so the user can set up their first workspace.
    pub needs_onboarding: bool,
    /// True when running in single-workspace mode (i.e. `oxy serve --project <dir>`
    /// was used). Workspace management UI (switcher, /workspaces page) should be
    /// hidden or disabled when this is true.
    pub single_workspace: bool,
    /// True when the workspace directory contains a local git repository.
    /// Enables the branch UI (create branch, commit, diff).
    pub local_git: bool,
    /// True when a remote repository is configured (`GIT_REPOSITORY_URL` is set).
    /// Shows the Pull button and changes the Push label to "Commit & Push".
    pub git_remote: bool,
    /// The default branch name (e.g. "main", "master"). Resolved from
    /// `git symbolic-ref refs/remotes/origin/HEAD`; falls back to "main".
    /// Only meaningful when `local_git` is true.
    pub default_branch: String,
    /// Branches where saving a file auto-creates a new feature branch instead
    /// of writing directly.  Configured via `protected_branches` in config.yml;
    /// defaults to `[default_branch]` when not set.
    /// Only meaningful when `local_git` is true.
    pub protected_branches: Vec<String>,
    /// Present when `GITHUB_CLIENT_ID` is set — enables "Login with GitHub".
    pub github: Option<GitHubAuthConfig>,
    /// Set when `needs_onboarding` is forced true due to an unexpected error (e.g. the
    /// previously active workspace directory no longer exists on disk). The frontend should
    /// display this as a toast before showing the setup wizard.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_error: Option<String>,
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

#[derive(Serialize)]
pub struct GitHubAuthConfig {
    pub client_id: String,
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

    // Resolve the active workspace path: prefer the CWD-based lookup (single-workspace
    // mode), falling back to active_workspace_path (multi-workspace mode).
    let resolved_project_path: Option<std::path::PathBuf> = match resolve_local_workspace_path() {
        Ok(p) => Some(p),
        Err(_) => {
            let active = app_state.active_workspace_path.read().await;
            active.clone()
        }
    };

    // local_git is only meaningful in local (non-cloud) mode.
    // In cloud mode the git lifecycle is managed via project_repo_id.
    let local_git = resolved_project_path
        .as_deref()
        .map(|p| app_state.backend.is_git_enabled(p))
        .unwrap_or(false);

    // Resolve the default branch (e.g. "main", "master") for the local repo.
    // Used by the frontend to gate isMainEditMode and the diff-badge guard.
    let default_branch = if local_git {
        match resolved_project_path.as_deref() {
            Some(p) => LocalGitService::get_default_branch(p).await,
            None => "main".to_string(),
        }
    } else {
        "main".to_string()
    };

    // Resolve protected_branches from config.yml for the active project, falling
    // back to [default_branch].  Computed per-request so switching projects via
    // activate_project is reflected immediately.
    let protected_branches: Vec<String> = if local_git {
        let config_branches = match resolved_project_path.as_deref() {
            Some(p) => match ConfigBuilder::new().with_workspace_path(p) {
                Ok(builder) => match builder.build_with_fallback_config().await {
                    Ok(manager) => manager.protected_branches().map(|b| b.to_vec()),
                    Err(_) => None,
                },
                Err(_) => None,
            },
            None => None,
        };
        config_branches.unwrap_or_else(|| vec![default_branch.clone()])
    } else {
        vec![default_branch.clone()]
    };

    // git_remote: true when GIT_REPOSITORY_URL is set OR the repo already has
    // a configured remote (e.g. cloned manually).
    let git_remote = {
        if std::env::var("GIT_REPOSITORY_URL").is_ok() {
            true
        } else if local_git {
            match resolved_project_path.as_deref() {
                Some(p) => LocalGitService::has_remote(p).await,
                None => false,
            }
        } else {
            false
        }
    };

    // needs_onboarding: use active_workspace_path as the authoritative source.
    // If no active workspace is set (e.g. after deletion or first run), check the
    // filesystem as a fallback for single-workspace mode.
    let (needs_onboarding, workspace_error) = {
        let active = app_state.active_workspace_path.read().await;
        match active.as_deref() {
            // An active workspace directory on disk is enough: the workspace is registered
            // and may still be cloning (no config.yml yet). Checking config.yml would
            // incorrectly send users back to /setup while a GitHub clone is in progress.
            Some(path) => {
                if path.exists() {
                    (false, None)
                } else {
                    // The previously active workspace directory is gone — surface this as an
                    // error so the frontend can show a toast before redirecting to setup.
                    let msg = format!(
                        "Workspace directory '{}' no longer exists. Please set up a new workspace.",
                        path.display()
                    );
                    (true, Some(msg))
                }
            }
            None => match &app_state.workspaces_root {
                // Single-workspace mode: still require config.yml (no clone in flight here)
                None => {
                    let missing = std::env::current_dir()
                        .map(|cwd| !cwd.join("config.yml").exists())
                        .unwrap_or(true);
                    (missing, None)
                }
                // Multi-workspace mode with no active workspace: onboarding needed
                Some(_) => (true, None),
            },
        }
    };

    let single_workspace = app_state.workspaces_root.is_none();

    let github_client_id = std::env::var("GITHUB_CLIENT_ID").ok();

    if !auth_enabled || app_state.internal {
        return Ok(Json(AuthConfigResponse {
            auth_enabled: false,
            google: None,
            okta: None,
            magic_link: None,
            enterprise: app_state.enterprise,
            readonly: app_state.readonly,
            needs_onboarding,
            single_workspace,
            local_git,
            git_remote,
            default_branch,
            protected_branches,
            github: github_client_id.map(|client_id| GitHubAuthConfig { client_id }),
            workspace_error,
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
        auth_enabled: true,
        google: google_client_id.map(|client_id| GoogleConfig { client_id }),
        okta: okta_config,
        magic_link: if has_magic_link { Some(true) } else { None },
        enterprise: app_state.enterprise,
        readonly: app_state.readonly,
        needs_onboarding,
        single_workspace,
        local_git,
        git_remote,
        default_branch,
        protected_branches,
        github: github_client_id.map(|client_id| GitHubAuthConfig { client_id }),
        workspace_error,
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
            let existing_count = Users::find()
                .filter(users::Column::Email.ne(oxy_auth::LOCAL_GUEST_EMAIL))
                .count(&connection)
                .await
                .unwrap_or(1);
            let initial_role = if existing_count == 0 {
                users::UserRole::Owner
            } else if oxy_auth::should_be_admin(&user_info.email) {
                users::UserRole::Admin
            } else {
                users::UserRole::Member
            };
            let new_user = users::ActiveModel {
                id: Set(Uuid::new_v4()),
                email: Set(user_info.email.clone()),
                name: Set(user_info.name.clone()),
                picture: Set(user_info.picture.clone()),
                email_verified: Set(true),
                magic_link_token: sea_orm::ActiveValue::NotSet,
                magic_link_token_expires_at: sea_orm::ActiveValue::NotSet,
                role: Set(initial_role),
                status: Set(UserStatus::Active),
                created_at: sea_orm::ActiveValue::NotSet,
                last_login_at: sea_orm::ActiveValue::NotSet,
            };

            insert_user_or_fetch_existing(new_user, &user_info.email, &connection).await?
        }
    };

    let (token, user_info_payload) = finalize_login(user, &connection).await?;
    Ok(Json(AuthResponse {
        token,
        user: user_info_payload,
    }))
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
            let existing_count = Users::find()
                .filter(users::Column::Email.ne(oxy_auth::LOCAL_GUEST_EMAIL))
                .count(&connection)
                .await
                .unwrap_or(1);
            let initial_role = if existing_count == 0 {
                users::UserRole::Owner
            } else if oxy_auth::should_be_admin(&user_info.email) {
                users::UserRole::Admin
            } else {
                users::UserRole::Member
            };
            let new_user = users::ActiveModel {
                id: Set(Uuid::new_v4()),
                email: Set(user_info.email.clone()),
                name: Set(user_info.name.clone()),
                picture: Set(user_info.picture.clone()),
                email_verified: Set(true),
                magic_link_token: sea_orm::ActiveValue::NotSet,
                magic_link_token_expires_at: sea_orm::ActiveValue::NotSet,
                role: Set(initial_role),
                status: Set(UserStatus::Active),
                created_at: sea_orm::ActiveValue::NotSet,
                last_login_at: sea_orm::ActiveValue::NotSet,
            };

            insert_user_or_fetch_existing(new_user, &user_info.email, &connection).await?
        }
    };

    let (token, user_info_payload) = finalize_login(user, &connection).await?;
    Ok(Json(AuthResponse {
        token,
        user: user_info_payload,
    }))
}

#[derive(Deserialize)]
pub struct GitHubAuthRequest {
    pub code: String,
}

pub async fn github_auth(
    headers: HeaderMap,
    extract::Json(payload): extract::Json<GitHubAuthRequest>,
) -> Result<Json<AuthResponse>, StatusCode> {
    let base_url = extract_base_url_from_headers(&headers);
    let user_info = exchange_github_code_for_user_info(&payload.code, &base_url)
        .await
        .map_err(|e| {
            tracing::error!("Failed to exchange GitHub code: {}", e);
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
            tracing::warn!(
                "Deleted user {} attempted to authenticate via GitHub",
                user_info.email
            );
            return Err(StatusCode::UNAUTHORIZED);
        }
        Some(existing_user) => {
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
            let existing_count = Users::find()
                .filter(users::Column::Email.ne(oxy_auth::LOCAL_GUEST_EMAIL))
                .count(&connection)
                .await
                .unwrap_or(1);
            let initial_role = if existing_count == 0 {
                users::UserRole::Owner
            } else if oxy_auth::should_be_admin(&user_info.email) {
                users::UserRole::Admin
            } else {
                users::UserRole::Member
            };
            let new_user = users::ActiveModel {
                id: Set(Uuid::new_v4()),
                email: Set(user_info.email.clone()),
                name: Set(user_info.name.clone()),
                picture: Set(user_info.picture.clone()),
                email_verified: Set(true),
                magic_link_token: sea_orm::ActiveValue::NotSet,
                magic_link_token_expires_at: sea_orm::ActiveValue::NotSet,
                role: Set(initial_role),
                status: Set(UserStatus::Active),
                created_at: sea_orm::ActiveValue::NotSet,
                last_login_at: sea_orm::ActiveValue::NotSet,
            };
            insert_user_or_fetch_existing(new_user, &user_info.email, &connection).await?
        }
    };

    let (token, user_info_payload) = finalize_login(user, &connection).await?;
    Ok(Json(AuthResponse {
        token,
        user: user_info_payload,
    }))
}

#[derive(Deserialize)]
struct GitHubUserInfo {
    name: Option<String>,
    login: String,
    avatar_url: Option<String>,
    email: Option<String>,
}

#[derive(Deserialize)]
struct GitHubEmailEntry {
    email: String,
    primary: bool,
    verified: bool,
}

async fn exchange_github_code_for_user_info(
    code: &str,
    base_url: &str,
) -> Result<GoogleUserInfo, OxyError> {
    let client_id = std::env::var("GITHUB_CLIENT_ID")
        .map_err(|_| OxyError::ConfigurationError("GITHUB_CLIENT_ID not configured".to_string()))?;
    let client_secret = std::env::var("GITHUB_CLIENT_SECRET").map_err(|_| {
        OxyError::ConfigurationError("GITHUB_CLIENT_SECRET not configured".to_string())
    })?;

    let redirect_uri = format!("{base_url}/github/callback");

    let client = reqwest::Client::builder()
        .user_agent("Oxy/1.0")
        .build()
        .map_err(|e| OxyError::RuntimeError(e.to_string()))?;

    // Exchange the authorization code for an access token.
    let token_response = client
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .form(&[
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("code", code),
            ("redirect_uri", redirect_uri.as_str()),
        ])
        .send()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("GitHub token request failed: {e}")))?;

    if !token_response.status().is_success() {
        return Err(OxyError::RuntimeError(format!(
            "GitHub token exchange error: {}",
            token_response.status()
        )));
    }

    #[derive(Deserialize)]
    struct TokenResponse {
        access_token: Option<String>,
        error: Option<String>,
    }

    let token_data: TokenResponse = token_response.json().await.map_err(|e| {
        OxyError::RuntimeError(format!("Failed to parse GitHub token response: {e}"))
    })?;

    if let Some(err) = token_data.error {
        return Err(OxyError::RuntimeError(format!("GitHub OAuth error: {err}")));
    }

    let access_token = token_data.access_token.ok_or_else(|| {
        OxyError::RuntimeError("GitHub token response missing access_token".to_string())
    })?;

    // Fetch user profile.
    let user_resp: GitHubUserInfo = client
        .get("https://api.github.com/user")
        .bearer_auth(&access_token)
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("GitHub /user request failed: {e}")))?
        .json()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Failed to parse GitHub user: {e}")))?;

    // Use the profile email if set; otherwise fetch the primary verified email.
    let email = if let Some(e) = user_resp.email.filter(|e| !e.is_empty()) {
        e
    } else {
        let emails: Vec<GitHubEmailEntry> = client
            .get("https://api.github.com/user/emails")
            .bearer_auth(&access_token)
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await
            .map_err(|e| {
                OxyError::RuntimeError(format!("GitHub /user/emails request failed: {e}"))
            })?
            .json()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to parse GitHub emails: {e}")))?;

        emails
            .into_iter()
            .find(|e| e.primary && e.verified)
            .map(|e| e.email)
            .ok_or_else(|| {
                OxyError::RuntimeError(
                    "No verified primary email found on GitHub account".to_string(),
                )
            })?
    };

    let name = user_resp.name.unwrap_or_else(|| user_resp.login.clone());

    Ok(GoogleUserInfo {
        email,
        name,
        picture: user_resp.avatar_url,
    })
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

/// Ensure the user has Admin role if `should_be_admin` returns true,
/// then build the `UserInfo` payload with the correct `is_admin` flag.
///
/// Call this after every login (Google, Okta, magic link verify) so that:
/// - Setting `OXY_ADMINS` and re-logging in immediately grants admin.
/// - The first real user always gets Admin regardless of which auth provider they use.
async fn finalize_login(
    user: users::Model,
    connection: &DatabaseConnection,
) -> Result<(String, UserInfo), StatusCode> {
    // Promote to Admin if warranted (OXY_ADMINS env var or LOCAL_GUEST).
    // Never demote an existing Owner or Admin — only Members are eligible for
    // promotion.  Note: an Admin demoted to Member (via update_user) will be
    // re-promoted to Admin on their next login if they still appear in
    // OXY_ADMINS.  This is intentional — OXY_ADMINS is the authoritative
    // source for admin status.
    let user = if user.role == users::UserRole::Member && oxy_auth::should_be_admin(&user.email) {
        let mut active: users::ActiveModel = user.into();
        active.role = Set(users::UserRole::Admin);
        active.update(connection).await.map_err(|e| {
            tracing::error!("Failed to promote user to admin: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
    } else {
        user
    };

    let is_admin = user.role.is_admin_or_above();
    let token = create_auth_token(user.clone()).await.map_err(|e| {
        tracing::error!("Failed to create auth token: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let user_info = UserInfo {
        id: user.id.to_string(),
        email: user.email,
        name: user.name,
        picture: user.picture,
        role: user.role.as_str().to_string(),
        is_admin,
    };
    Ok((token, user_info))
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
    // email is already lowercased at ingestion; normalize config values too so
    // operators can write "Gmail.com" or "gmail.com" interchangeably.
    for domain in &config.blocked_domains {
        if email.ends_with(&format!("@{}", domain.to_lowercase())) {
            return false;
        }
    }
    if !config.allowed_emails.is_empty() {
        return config
            .allowed_emails
            .iter()
            .any(|e| e.eq_ignore_ascii_case(email));
    }
    true
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
            let existing_count = Users::find()
                .filter(users::Column::Email.ne(oxy_auth::LOCAL_GUEST_EMAIL))
                .count(&connection)
                .await
                .unwrap_or(1);
            let initial_role = if existing_count == 0 {
                users::UserRole::Owner
            } else if oxy_auth::should_be_admin(&req.email) {
                users::UserRole::Admin
            } else {
                users::UserRole::Member
            };
            let name = req.email.split('@').next().unwrap_or("User").to_string();
            let new_user = users::ActiveModel {
                id: Set(Uuid::new_v4()),
                email: Set(req.email.clone()),
                name: Set(name),
                picture: Set(None),
                email_verified: Set(false),
                magic_link_token: Set(None),
                magic_link_token_expires_at: Set(None),
                role: Set(initial_role),
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
    let mut user_update: users::ActiveModel = user.into();
    user_update.magic_link_token = Set(None);
    user_update.magic_link_token_expires_at = Set(None);
    user_update.email_verified = Set(true);
    let user = user_update.update(&connection).await.map_err(|e| {
        tracing::error!("Failed to clear magic link token: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let (token, user_info) = finalize_login(user, &connection).await?;
    Ok(Json(AuthResponse {
        token,
        user: user_info,
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

// ─── Invite ────────────────────────────────────────────────────────────────

static INVITE_RATE_LIMITER: Lazy<DefaultKeyedRateLimiter<String>> =
    Lazy::new(|| RateLimiter::keyed(Quota::per_hour(NonZeroU32::new(5).expect("5 > 0"))));

fn check_invite_rate_limit(email: &str) -> Option<u64> {
    match INVITE_RATE_LIMITER.check_key(&email.to_lowercase()) {
        Ok(()) => None,
        Err(not_until) => {
            let wait = not_until.wait_time_from(DefaultClock::default().now());
            Some(wait.as_secs().max(1))
        }
    }
}

#[derive(Deserialize)]
pub struct InviteRequest {
    pub email: String,
}

pub async fn invite_user(
    Extension(caller): Extension<AuthenticatedUser>,
    headers: HeaderMap,
    extract::Json(req): extract::Json<InviteRequest>,
) -> axum::response::Response {
    use axum::http::header::RETRY_AFTER;
    use axum::response::IntoResponse;

    let email = req.email.to_lowercase();

    if !is_valid_email_format(&email) {
        return (
            StatusCode::BAD_REQUEST,
            Json(MessageResponse {
                message: "Invalid email address.".to_string(),
            }),
        )
            .into_response();
    }

    // Rate limit per target email address.
    if let Some(retry_after_secs) = check_invite_rate_limit(&email) {
        let mins = retry_after_secs.div_ceil(60);
        tracing::warn!("Invite rate limit exceeded for: {}", email);
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [(
                RETRY_AFTER,
                axum::http::HeaderValue::from_str(&retry_after_secs.to_string())
                    .unwrap_or_else(|_| axum::http::HeaderValue::from_static("3600")),
            )],
            Json(MessageResponse {
                message: format!(
                    "Too many invitations to this address. Please try again in {mins} minute{}.",
                    if mins == 1 { "" } else { "s" }
                ),
            }),
        )
            .into_response();
    }

    invite_user_inner(caller, headers, email)
        .await
        .into_response()
}

async fn invite_user_inner(
    caller: AuthenticatedUser,
    headers: HeaderMap,
    email: String,
) -> axum::response::Response {
    use axum::response::IntoResponse;

    // Respect the magic-link allowlist: invited email must be eligible.
    let magic_link_config = oxy::config::oxy::get_oxy_config()
        .ok()
        .and_then(|c| c.authentication)
        .and_then(|a| a.magic_link);
    if let Some(ref cfg) = magic_link_config
        && !is_email_allowed(&email, cfg)
    {
        tracing::info!("Invite attempted for non-allowlisted email: {}", email);
        return StatusCode::FORBIDDEN.into_response();
    }

    let connection = match establish_connection().await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to establish database connection: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // Check the existing user record before doing anything else.
    let existing = match Users::find().filter_by_email(&email).one(&connection).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Failed to query user: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let user_for_update = match existing {
        Some(u) if u.status == UserStatus::Deleted => {
            tracing::warn!("Invite attempted for deleted user: {}", email);
            return StatusCode::FORBIDDEN.into_response();
        }
        // Already signed in at least once — tell the caller clearly.
        Some(u) if u.email_verified => {
            return (
                StatusCode::CONFLICT,
                Json(MessageResponse {
                    message: "This email address already has an active account.".to_string(),
                }),
            )
                .into_response();
        }
        // Invited before but hasn't accepted yet — resend.
        Some(u) => u,
        // New user — create the record now.
        None => {
            let name = email.split('@').next().unwrap_or("User").to_string();
            let new_user = users::ActiveModel {
                id: Set(Uuid::new_v4()),
                email: Set(email.clone()),
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
                    match Users::find().filter_by_email(&email).one(&connection).await {
                        Ok(Some(u)) => u,
                        Ok(None) => {
                            tracing::error!("User not found after unique violation: {}", email);
                            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                        }
                        Err(e) => {
                            tracing::error!("Failed to query user after race condition: {}", e);
                            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to create invited user: {}", e);
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
            }
        }
    };

    // Generate token and store it
    let token_bytes: [u8; 32] = rand::random();
    let token = hex::encode(token_bytes);
    let expires_at = Utc::now() + Duration::minutes(15);

    let mut active: users::ActiveModel = user_for_update.into();
    active.magic_link_token = Set(Some(token.clone()));
    active.magic_link_token_expires_at = Set(Some(expires_at.into()));
    if let Err(e) = active.update(&connection).await {
        tracing::error!("Failed to save invitation token: {}", e);
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    let base_url = extract_base_url_from_headers(&headers);
    let invited_by = if caller.name.is_empty() {
        caller.email.clone()
    } else {
        caller.name.clone()
    };

    tokio::spawn(async move {
        if let Err(e) = send_invitation_email(&email, &token, &base_url, &invited_by).await {
            tracing::error!("Failed to send invitation email: {}", e);
        }
    });

    Json(MessageResponse {
        message: "Invitation sent.".to_string(),
    })
    .into_response()
}

async fn send_invitation_email(
    to_email: &str,
    token: &str,
    base_url: &str,
    invited_by: &str,
) -> Result<(), OxyError> {
    use crate::emails::{
        EmailMessage, EmailProvider, local_test::LocalTestEmailProvider, ses::SesEmailProvider,
    };

    let auth_config = oxy::config::oxy::get_oxy_config()
        .ok()
        .and_then(|c| c.authentication)
        .and_then(|a| a.magic_link);

    let magic_link_config = auth_config.ok_or_else(|| {
        OxyError::ConfigurationError("Magic link auth not configured".to_string())
    })?;

    let magic_link_url = format!("{base_url}/auth/magic-link/callback?token={token}");
    let message = EmailMessage {
        subject: format!("{invited_by} invited you to Oxy"),
        html_body: build_invitation_email_html(&magic_link_url, to_email, invited_by)?,
        text_body: format!(
            "You've been invited to Oxy\n\n{invited_by} has invited you to join Oxy. Click the link below to accept. For security, this link expires in 15 minutes.\n\n{magic_link_url}\n\nThis invitation was sent to {to_email}. If you weren't expecting this, you can safely ignore this email."
        ),
    };

    if std::env::var("MAGIC_LINK_LOCAL_TEST").is_ok() {
        LocalTestEmailProvider
            .send(&magic_link_config.from_email, to_email, message)
            .await
    } else {
        SesEmailProvider::new(magic_link_config.aws_region.as_deref())
            .await
            .send(&magic_link_config.from_email, to_email, message)
            .await
    }
}

static INVITATION_TEMPLATE: Lazy<Handlebars<'static>> = Lazy::new(|| {
    let mut hbs = Handlebars::new();
    hbs.register_template_string("invitation", include_str!("../../emails/invitation.hbs"))
        .expect("invitation.hbs is valid");
    hbs
});

fn build_invitation_email_html(
    magic_link_url: &str,
    to_email: &str,
    invited_by: &str,
) -> Result<String, OxyError> {
    let data = serde_json::json!({
        "magic_link_url": magic_link_url,
        "to_email": to_email,
        "invited_by": invited_by,
        "year": Utc::now().format("%Y").to_string(),
    });

    INVITATION_TEMPLATE
        .render("invitation", &data)
        .map_err(|e| OxyError::RuntimeError(format!("Failed to render invitation template: {e}")))
}
