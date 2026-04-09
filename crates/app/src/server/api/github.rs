use axum::{
    extract::{Json, Query},
    response::Json as ResponseJson,
};
use chrono::{DateTime, Duration, Utc};
use hmac::{Hmac, Mac};
use reqwest::StatusCode;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tracing::error;
use uuid::Uuid;

use oxy::database::client::establish_connection;
use oxy::github::app_auth::GitHubAppAuth;
use oxy::github::client::GitHubClient;
use oxy::github::types::{GitHubBranch, GitHubRepository};
use oxy_auth::extractor::AuthenticatedUserExtractor;

#[derive(Debug, Serialize)]
pub struct GitHubNamespace {
    pub id: Uuid,
    pub owner_type: String,
    pub slug: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct GitHubNamespacesResponse {
    pub installations: Vec<GitHubNamespace>,
}

#[derive(Debug, Deserialize)]
pub struct GitHubRepositoriesQuery {
    pub git_namespace_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct GitHubBranchesQuery {
    pub git_namespace_id: Uuid,
    pub repo_name: String,
}

#[derive(Debug, Serialize)]
pub struct GitHubRepositoriesResponse {
    pub repositories: Vec<GitHubRepository>,
}

#[derive(Debug, Serialize)]
pub struct GitHubBranchesResponse {
    pub branches: Vec<GitHubBranch>,
}

#[derive(Debug, Deserialize)]
pub struct CreatePATNamespaceRequest {
    pub token: String,
}

#[derive(Debug, Deserialize)]
pub struct GitHubCallbackQuery {
    pub installation_id: String,
    pub setup_action: Option<String>,
    pub state: String,
}

/// POST /github/namespaces/pat — register a Personal Access Token as a GitHub connection.
/// Validates the token against the GitHub API and stores it as a namespace.
pub async fn create_pat_namespace(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Json(payload): Json<CreatePATNamespaceRequest>,
) -> Result<ResponseJson<GitHubNamespace>, axum::http::StatusCode> {
    let client = GitHubClient::from_token(payload.token.clone()).map_err(|e| {
        error!("Failed to create GitHub client from PAT: {}", e);
        StatusCode::BAD_REQUEST
    })?;

    let username = client.get_current_user().await.map_err(|e| {
        error!("PAT validation failed: {}", e);
        StatusCode::UNAUTHORIZED
    })?;

    let db = establish_connection().await.map_err(|e| {
        error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // If this user already has a PAT namespace, update it with the new token and
    // return it.  PAT namespaces use slug="pat", so (user_id, slug="pat") is unique.
    // We always update the token so that rotating a PAT is reflected immediately
    // without requiring the user to delete and re-add the namespace.
    {
        use entity::git_namespaces;
        use sea_orm::{ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter};
        if let Some(existing) = entity::prelude::GitNamespaces::find()
            .filter(git_namespaces::Column::UserId.eq(user.id))
            .filter(git_namespaces::Column::Slug.eq("pat"))
            .one(&db)
            .await
            .map_err(|e| {
                error!("DB error checking existing PAT namespace: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?
        {
            let mut active = existing.clone().into_active_model();
            active.oauth_token = Set(payload.token);
            active.name = Set(username.clone());
            active.save(&db).await.map_err(|e| {
                error!("DB error updating PAT namespace token: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
            return Ok(ResponseJson(GitHubNamespace {
                id: existing.id,
                owner_type: existing.owner_type,
                slug: existing.slug,
                name: username,
            }));
        }
    }

    let git_namespace = entity::git_namespaces::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_id: Set(user.id),
        name: Set(username.clone()),
        slug: Set("pat".to_string()),
        owner_type: Set("User".to_string()),
        installation_id: Set(0),
        provider: Set("github".to_string()),
        oauth_token: Set(payload.token),
    };

    let ns = git_namespace.insert(&db).await.map_err(|e| {
        error!("Database error inserting PAT namespace: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(ResponseJson(GitHubNamespace {
        id: ns.id,
        owner_type: ns.owner_type,
        slug: ns.slug,
        name: ns.name,
    }))
}

/// POST /github/namespaces/installation — register an existing GitHub App installation by ID.
///
/// Bypasses the OAuth callback flow entirely: uses the App's private key to fetch installation
/// info directly from the GitHub API. Intended for dev environments where only one callback URL
/// is registered and the app is already installed on the org.
///
/// The installation ID is visible in the GitHub settings URL:
/// https://github.com/settings/installations/{installation_id}
#[derive(Debug, Deserialize)]
pub struct CreateInstallationNamespaceRequest {
    pub installation_id: i64,
}

pub async fn create_installation_namespace(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Json(payload): Json<CreateInstallationNamespaceRequest>,
) -> Result<ResponseJson<GitHubNamespace>, axum::http::StatusCode> {
    let app_auth = GitHubAppAuth::from_env()?;
    let installation = app_auth
        .get_installation_info(&payload.installation_id.to_string())
        .await?;

    let db = establish_connection().await.map_err(|e| {
        error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Return existing namespace if this installation is already registered for this user.
    use entity::git_namespaces;
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
    let existing = entity::prelude::GitNamespaces::find()
        .filter(git_namespaces::Column::InstallationId.eq(installation.id))
        .one(&db)
        .await
        .map_err(|e| {
            error!("DB query failed looking up existing namespace: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if let Some(ns) = existing {
        return Ok(ResponseJson(GitHubNamespace {
            id: ns.id,
            owner_type: ns.owner_type,
            slug: ns.slug,
            name: ns.name,
        }));
    }

    let git_namespace = entity::git_namespaces::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_id: Set(user.id),
        name: Set(installation.name),
        slug: Set(installation.slug),
        owner_type: Set(installation.owner_type),
        installation_id: Set(installation.id),
        provider: Set("github".to_string()),
        oauth_token: Set("".to_string()),
    };

    let ns = git_namespace.insert(&db).await.map_err(|e| {
        error!("Database error inserting installation namespace: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(ResponseJson(GitHubNamespace {
        id: ns.id,
        owner_type: ns.owner_type,
        slug: ns.slug,
        name: ns.name,
    }))
}

pub async fn delete_git_namespace(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let ns = entity::git_namespaces::Entity::find_by_id(id)
        .one(&db)
        .await
        .map_err(|e| {
            error!("Database error fetching namespace {}: {}", id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    if ns.user_id != user.id {
        return Err(StatusCode::FORBIDDEN);
    }

    entity::git_namespaces::Entity::delete_by_id(id)
        .exec(&db)
        .await
        .map_err(|e| {
            error!("Database error deleting namespace {}: {}", id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_git_namespaces(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
) -> Result<ResponseJson<GitHubNamespacesResponse>, axum::http::StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    use sea_orm::{ColumnTrait, QueryFilter};
    // GitHub App installations (slug != "pat") are instance-wide — shared by all
    // users of this Oxy instance. PAT namespaces (slug = "pat") are per-user and
    // must only be visible to the user who created them.
    let git_namespaces = entity::git_namespaces::Entity::find()
        .filter(
            sea_orm::Condition::any()
                .add(entity::git_namespaces::Column::Slug.ne("pat"))
                .add(entity::git_namespaces::Column::UserId.eq(user.id)),
        )
        .all(&db)
        .await
        .map_err(|e| {
            error!("Database error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let response = GitHubNamespacesResponse {
        installations: git_namespaces
            .into_iter()
            .map(|ns| GitHubNamespace {
                id: ns.id,
                owner_type: ns.owner_type,
                name: ns.name,
                slug: ns.slug,
            })
            .collect(),
    };

    Ok(ResponseJson(response))
}

pub async fn list_repositories(
    _user: AuthenticatedUserExtractor,
    Query(query): Query<GitHubRepositoriesQuery>,
) -> Result<Json<Vec<GitHubRepository>>, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let git_namespace = entity::git_namespaces::Entity::find()
        .filter(entity::git_namespaces::Column::Id.eq(query.git_namespace_id))
        .one(&db)
        .await
        .map_err(|e| {
            error!("Database error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let Some(git_namespace) = git_namespace else {
        error!("Git namespace not found for id {}", query.git_namespace_id);
        return Err(StatusCode::BAD_REQUEST);
    };

    // Use the stored token directly (PAT) if available; otherwise fetch an installation token.
    let is_pat = !git_namespace.oauth_token.is_empty();
    let token = if is_pat {
        git_namespace.oauth_token.clone()
    } else {
        let app_auth = GitHubAppAuth::from_env()?;
        app_auth
            .get_installation_token(&git_namespace.installation_id.to_string())
            .await?
    };

    let client = match GitHubClient::from_token(token) {
        Ok(client) => client,
        Err(e) => {
            error!("Failed to create GitHub client: {}", e);
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    // PAT tokens use /user/repos; installation tokens use /installation/repositories.
    let repositories = if is_pat {
        client.list_user_repositories().await
    } else {
        client.list_repositories().await
    };

    match repositories {
        Ok(repos) => Ok(Json(repos)),
        Err(e) => {
            let msg = e.to_string();
            error!("Failed to fetch repositories: {}", msg);
            // Map GitHub auth/installation errors to distinct HTTP codes so the
            // client can surface a "reinstall" prompt instead of a generic error.
            // Note: we use 403 (not 401) to avoid triggering the client's global
            // "session expired → redirect to login" interceptor.
            if msg.contains("401") || msg.contains("403") {
                // Token expired, app permissions revoked, or PAT invalid.
                Err(StatusCode::FORBIDDEN)
            } else if msg.contains("404") {
                // Installation no longer exists (app was uninstalled).
                Err(StatusCode::NOT_FOUND)
            } else {
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn list_branches(
    _user: AuthenticatedUserExtractor,
    Query(query): Query<GitHubBranchesQuery>,
) -> Result<Json<Vec<GitHubBranch>>, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let git_namespace = entity::git_namespaces::Entity::find()
        .filter(entity::git_namespaces::Column::Id.eq(query.git_namespace_id))
        .one(&db)
        .await
        .map_err(|e| {
            error!("Database error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let Some(git_namespace) = git_namespace else {
        error!("Git namespace not found for id {}", query.git_namespace_id);
        return Err(StatusCode::BAD_REQUEST);
    };

    let token = if !git_namespace.oauth_token.is_empty() {
        git_namespace.oauth_token.clone()
    } else {
        let app_auth = GitHubAppAuth::from_env()?;
        app_auth
            .get_installation_token(&git_namespace.installation_id.to_string())
            .await?
    };

    let client = match GitHubClient::from_token(token) {
        Ok(client) => client,
        Err(e) => {
            error!("Failed to create GitHub client: {}", e);
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    match client.list_branches(query.repo_name).await {
        Ok(branches) => Ok(Json(branches)),
        Err(e) => {
            error!("Failed to fetch branches: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub fn validate_github_state(state: &str, user_id: &Uuid) -> Result<bool, axum::http::StatusCode> {
    let parts: Vec<&str> = state.split(':').collect();
    if parts.len() != 3 {
        return Ok(false);
    }

    let state_user_id = parts[0];
    let timestamp_str = parts[1];
    let provided_signature = parts[2];

    let parsed_user_id = match Uuid::parse_str(state_user_id) {
        Ok(id) => id,
        Err(_) => return Ok(false),
    };

    if parsed_user_id != *user_id {
        return Ok(false);
    }

    let timestamp = match timestamp_str.parse::<i64>() {
        Ok(ts) => ts,
        Err(_) => return Ok(false),
    };

    let state_time =
        DateTime::<Utc>::from_timestamp(timestamp, 0).ok_or(StatusCode::BAD_REQUEST)?;
    let now = Utc::now();
    if now.signed_duration_since(state_time) > Duration::hours(1) {
        return Ok(false);
    }

    let secret_key =
        std::env::var("GITHUB_STATE_SECRET").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let state_data = format!("{}:{}", state_user_id, timestamp_str);

    let mut mac = Hmac::<Sha256>::new_from_slice(secret_key.as_bytes())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    mac.update(state_data.as_bytes());

    let expected_signature = hex::encode(mac.finalize().into_bytes());

    Ok(constant_time_eq::constant_time_eq(
        provided_signature.as_bytes(),
        expected_signature.as_bytes(),
    ))
}

// ─── GitHub OAuth connect flow ────────────────────────────────────────────────
//
// The Workspaces UI shows a "Sign in with GitHub" button that triggers this flow:
//
//  1. GET /github/oauth-connect-url
//       Server generates a signed OAuth URL with state = {user_id}:{ts}:{hmac}.
//       State is HMAC-signed (GITHUB_STATE_SECRET), expires after 1 hour.
//
//  2. User completes GitHub OAuth in a popup.
//       GitHub redirects to /github/callback with code + state.
//       The callback page posts the code + state back via window.postMessage.
//
//  3. POST /github/namespaces/oauth { code, state }
//       a. Verify the HMAC state (user_id match, < 1 hour old).
//       b. Exchange the code for a short-lived user OAuth token.
//       c. Call GET /user/installations with that token.
//          GitHub returns only installations of THIS app the user can access.
//          Requires the App to have "Organization → Members: Read" permission
//          so regular org members (not just owners) are included.
//       d. Return one of:
//          • `connected`    — exactly one installation → auto-connect
//          • `choose`       — multiple installations → return list + selection_token
//          • `not_installed`— zero installations visible to this user
//
//  4. (choose case) POST /github/namespaces/pick { installation_id, selection_token }
//       selection_token is an HMAC token encoding the eligible IDs, bound to
//       user_id and a 10-minute timestamp.  The server verifies the token before
//       connecting, so users cannot pick an installation outside their eligible set.

/// GET /github/install-app-url?origin=https://app.example.com
///
/// Returns the GitHub App installation URL with a redirect_uri so GitHub sends
/// the user back to /github/callback after they install. The callback page picks
/// up installation_id from the query params and auto-connects the namespace.
#[derive(Debug, Deserialize)]
pub struct InstallAppUrlQuery {
    pub origin: String,
}

pub async fn get_install_app_url(
    _user: AuthenticatedUserExtractor,
    Query(query): Query<InstallAppUrlQuery>,
) -> Result<ResponseJson<String>, axum::http::StatusCode> {
    let slug = std::env::var("GITHUB_APP_SLUG").map_err(|_| {
        error!("GITHUB_APP_SLUG not configured");
        StatusCode::NOT_FOUND
    })?;
    let origin = query.origin.trim_end_matches('/');
    let redirect_uri = urlencoding::encode(&format!("{origin}/github/callback")).into_owned();
    Ok(ResponseJson(format!(
        "https://github.com/apps/{slug}/installations/new?redirect_uri={redirect_uri}"
    )))
}

/// GET /github/oauth-connect-url?origin=https://app.example.com
///
/// Returns a signed GitHub OAuth URL. The `origin` query param is supplied by
/// the frontend (`window.location.origin`) so the redirect_uri exactly matches
/// the callback URL registered in the GitHub App — no env-var override needed.
#[derive(Debug, Deserialize)]
pub struct OAuthConnectUrlQuery {
    pub origin: String,
}

pub async fn gen_oauth_connect_url(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Query(query): Query<OAuthConnectUrlQuery>,
) -> Result<ResponseJson<String>, axum::http::StatusCode> {
    let client_id =
        std::env::var("GITHUB_CLIENT_ID").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let timestamp = chrono::Utc::now().timestamp();
    let state_data = format!("{}:{}", user.id, timestamp);
    let secret_key =
        std::env::var("GITHUB_STATE_SECRET").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut mac = Hmac::<Sha256>::new_from_slice(secret_key.as_bytes())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    mac.update(state_data.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());
    let state = urlencoding::encode(&format!("{}:{}", state_data, signature)).into_owned();

    let origin = query.origin.trim_end_matches('/');
    let redirect_uri = urlencoding::encode(&format!("{origin}/github/callback")).into_owned();

    Ok(ResponseJson(format!(
        "https://github.com/login/oauth/authorize?client_id={client_id}&state={state}&redirect_uri={redirect_uri}"
    )))
}

#[derive(Debug, Serialize)]
pub struct OAuthInstallation {
    pub id: i64,
    pub name: String,
    pub owner_type: String,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum OAuthConnectResponse {
    Connected {
        namespace: GitHubNamespace,
    },
    Choose {
        installations: Vec<OAuthInstallation>,
        /// Short-lived HMAC token encoding the eligible installation IDs.
        /// Must be echoed back in the pick request so the server can verify
        /// the chosen ID was actually in the list shown to this user.
        selection_token: String,
    },
    NotInstalled,
}

/// Sign a list of eligible installation IDs into a short-lived token bound
/// to `user_id`.  Format: `{ids_csv}:{user_id}:{timestamp}:{hmac}`.
fn sign_selection_token(
    ids: &[i64],
    user_id: &uuid::Uuid,
) -> Result<String, axum::http::StatusCode> {
    let secret =
        std::env::var("GITHUB_STATE_SECRET").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let timestamp = chrono::Utc::now().timestamp();
    let ids_csv = ids
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let payload = format!("{ids_csv}:{user_id}:{timestamp}");
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    mac.update(payload.as_bytes());
    let sig = hex::encode(mac.finalize().into_bytes());
    Ok(format!("{payload}:{sig}"))
}

/// Verify a `selection_token` and return the set of eligible installation IDs.
/// Returns `Err(BAD_REQUEST)` if the token is invalid or expired (>10 min).
fn verify_selection_token(
    token: &str,
    user_id: &uuid::Uuid,
) -> Result<std::collections::HashSet<i64>, axum::http::StatusCode> {
    let secret =
        std::env::var("GITHUB_STATE_SECRET").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Format: {ids_csv}:{user_id}:{timestamp}:{hmac}
    let parts: Vec<&str> = token.rsplitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let (sig, payload) = (parts[0], parts[1]);

    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    mac.update(payload.as_bytes());
    let expected = hex::encode(mac.finalize().into_bytes());
    if !constant_time_eq::constant_time_eq(sig.as_bytes(), expected.as_bytes()) {
        return Err(StatusCode::BAD_REQUEST);
    }

    // payload = {ids_csv}:{user_id}:{timestamp}
    let segments: Vec<&str> = payload.rsplitn(3, ':').collect();
    if segments.len() != 3 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let (timestamp_str, uid_str, ids_csv) = (segments[0], segments[1], segments[2]);

    if uid_str != user_id.to_string() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let timestamp: i64 = timestamp_str.parse().map_err(|_| StatusCode::BAD_REQUEST)?;
    let age = chrono::Utc::now().timestamp() - timestamp;
    if age < 0 || age > 600 {
        // Token older than 10 minutes
        return Err(StatusCode::BAD_REQUEST);
    }

    ids_csv
        .split(',')
        .map(|s| s.parse::<i64>().map_err(|_| StatusCode::BAD_REQUEST))
        .collect()
}

/// POST /github/namespaces/oauth — exchange an OAuth code, discover app installations
/// visible to the user, and auto-connect if unambiguous.
///
/// Uses `GET /user/installations` with the user's OAuth token to discover which
/// installations of this GitHub App the user can access.  Requires the App to have
/// "Organization → Members: Read" permission so regular org members are included —
/// without it, only org owners/admins see the installation.
pub async fn connect_namespace_from_oauth(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Json(payload): Json<OAuthNamespaceRequest>,
) -> Result<ResponseJson<OAuthConnectResponse>, axum::http::StatusCode> {
    if !validate_github_state(&payload.state, &user.id)? {
        error!("Invalid OAuth state for user {}", user.id);
        return Err(StatusCode::BAD_REQUEST);
    }

    let client_id =
        std::env::var("GITHUB_CLIENT_ID").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let client_secret =
        std::env::var("GITHUB_CLIENT_SECRET").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let origin = payload.origin.trim_end_matches('/');
    let redirect_uri = format!("{origin}/github/callback");
    let user_token =
        exchange_oauth_code(&payload.code, &client_id, &client_secret, &redirect_uri).await?;

    // GET /user/installations returns all installations of THIS GitHub App that the
    // authenticated user can access. Requires the App to have "Organization > Members:
    // Read" permission — with that set, org members (not just owners) are included.
    let eligible = fetch_user_app_installations(&user_token).await?;

    tracing::debug!("GitHub OAuth: eligible installations={eligible:?}");

    let db = establish_connection().await.map_err(|e| {
        error!("DB connection failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Persist the token at the user level so GET /github/my-installations can reuse
    // it on future dialog opens, even before any namespace record carries the token.
    // This is also the backfill path for users who log in via email/Google/Okta but
    // connect GitHub repos via the OAuth popup.
    {
        use sea_orm::EntityTrait;
        if let Ok(Some(db_user)) = entity::prelude::Users::find_by_id(user.id).one(&db).await {
            let mut active: entity::users::ActiveModel = db_user.into();
            active.github_access_token = Set(Some(user_token.clone()));
            let _ = active.save(&db).await; // best-effort
        }
    }

    match eligible.as_slice() {
        [] => Ok(ResponseJson(OAuthConnectResponse::NotInstalled)),
        [single] => {
            let ns = upsert_app_namespace(
                &db,
                &user,
                single.id,
                &single.name,
                &single.owner_type,
                &user_token,
            )
            .await?;
            Ok(ResponseJson(OAuthConnectResponse::Connected {
                namespace: ns,
            }))
        }
        _ => {
            // Store the token on the first existing namespace so future calls to
            // GET /github/my-installations can skip re-auth even in the choose case.
            if let Some(existing) = entity::prelude::GitNamespaces::find()
                .filter(entity::git_namespaces::Column::UserId.eq(user.id))
                .filter(entity::git_namespaces::Column::Slug.ne("pat"))
                .one(&db)
                .await
                .ok()
                .flatten()
            {
                use entity::git_namespaces;
                use sea_orm::IntoActiveModel;
                let mut active = existing.into_active_model();
                active.oauth_token = Set(user_token.clone());
                let _ = active.save(&db).await;
            }
            let ids: Vec<i64> = eligible.iter().map(|i| i.id).collect();
            let selection_token = sign_selection_token(&ids, &user.id)?;
            Ok(ResponseJson(OAuthConnectResponse::Choose {
                installations: eligible,
                selection_token,
            }))
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct OAuthNamespaceRequest {
    pub code: String,
    pub state: String,
    /// The frontend's window.location.origin — used to reconstruct the exact
    /// redirect_uri for the GitHub token exchange (must match what was registered).
    pub origin: String,
}

#[derive(Debug, Deserialize)]
pub struct PickInstallationRequest {
    pub installation_id: i64,
    /// HMAC token returned by the `choose` response — proves this ID was in
    /// the list the server generated for this user.
    pub selection_token: String,
}

/// POST /github/namespaces/pick — connect one installation from a multi-install OAuth result.
///
/// The `selection_token` is verified to ensure the caller can only pick an
/// installation ID that was explicitly returned to them in the `choose` response.
pub async fn pick_namespace_installation(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Json(payload): Json<PickInstallationRequest>,
) -> Result<ResponseJson<GitHubNamespace>, axum::http::StatusCode> {
    let eligible_ids = verify_selection_token(&payload.selection_token, &user.id)?;
    if !eligible_ids.contains(&payload.installation_id) {
        error!(
            "User {} tried to pick installation {} not in their eligible set",
            user.id, payload.installation_id
        );
        return Err(StatusCode::FORBIDDEN);
    }

    let app_auth = GitHubAppAuth::from_env()?;
    let installation = app_auth
        .get_installation_info(&payload.installation_id.to_string())
        .await?;

    let db = establish_connection().await.map_err(|e| {
        error!("DB connection failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let ns = upsert_app_namespace(
        &db,
        &user,
        installation.id,
        &installation.name,
        &installation.owner_type,
        "", // no user token at pick-time; stored token from choose phase is preserved
    )
    .await?;
    Ok(ResponseJson(ns))
}

// ─── Shared helpers ───────────────────────────────────────────────────────────

async fn exchange_oauth_code(
    code: &str,
    client_id: &str,
    client_secret: &str,
    redirect_uri: &str,
) -> Result<String, axum::http::StatusCode> {
    #[derive(Deserialize)]
    struct TokenResponse {
        access_token: Option<String>,
        error: Option<String>,
    }

    let resp = reqwest::Client::builder()
        .user_agent("Oxy/1.0")
        .build()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .form(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("code", code),
            ("redirect_uri", redirect_uri),
        ])
        .send()
        .await
        .map_err(|e| {
            error!("GitHub token exchange failed: {}", e);
            StatusCode::BAD_GATEWAY
        })?
        .json::<TokenResponse>()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    if let Some(err) = resp.error {
        error!("GitHub OAuth error: {}", err);
        return Err(StatusCode::BAD_GATEWAY);
    }

    resp.access_token.filter(|t| !t.is_empty()).ok_or_else(|| {
        error!("GitHub token response missing access_token");
        StatusCode::BAD_GATEWAY
    })
}

/// GET /user/installations — returns installations of THIS GitHub App that the
/// authenticated user can access. Works correctly for org members (not just owners)
/// when the GitHub App has "Organization > Members: Read" permission configured.
async fn fetch_user_app_installations(
    user_token: &str,
) -> Result<Vec<OAuthInstallation>, axum::http::StatusCode> {
    #[derive(Deserialize)]
    struct Item {
        id: i64,
        account: Account,
    }
    #[derive(Deserialize)]
    struct Account {
        login: String,
        #[serde(rename = "type")]
        account_type: String,
    }
    #[derive(Deserialize)]
    struct Response {
        installations: Vec<Item>,
    }

    let resp = reqwest::Client::builder()
        .user_agent("Oxy/1.0")
        .build()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .get("https://api.github.com/user/installations")
        .header("Authorization", format!("Bearer {user_token}"))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| {
            error!("GitHub GET /user/installations failed: {}", e);
            StatusCode::BAD_GATEWAY
        })?
        .json::<Response>()
        .await
        .map_err(|e| {
            error!("Failed to parse /user/installations: {}", e);
            StatusCode::BAD_GATEWAY
        })?;

    Ok(resp
        .installations
        .into_iter()
        .map(|i| OAuthInstallation {
            id: i.id,
            name: i.account.login,
            owner_type: i.account.account_type,
        })
        .collect())
}

/// GET /github/my-installations — list app installations the current user can access
/// using their stored GitHub OAuth token, without requiring a new sign-in.
/// Returns { installations, selection_token } — same shape as the `choose` response —
/// so the frontend can feed it directly into the pick flow.
/// Returns 404 if no token is stored yet (frontend falls back to the OAuth flow).
#[derive(Serialize)]
pub struct MyInstallationsResponse {
    pub installations: Vec<OAuthInstallation>,
    pub selection_token: String,
}

pub async fn get_my_installations(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
) -> Result<ResponseJson<MyInstallationsResponse>, axum::http::StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        error!("DB connection failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

    // Resolve the best available GitHub user token for this user.
    // Priority:
    //   1. Token stored when the user logged in via "Login with GitHub"
    //      (users.github_access_token) — always present for GitHub-login users,
    //      never requires a second OAuth round-trip.
    //   2. Token stored when a namespace was connected via the OAuth popup
    //      (git_namespaces.oauth_token) — fallback for users who logged in via
    //      another method (email/password, Google, Okta) but previously connected
    //      a GitHub account through the repo-connect flow.
    let github_token = {
        let db_user = entity::prelude::Users::find_by_id(user.id)
            .one(&db)
            .await
            .map_err(|e| {
                error!("DB query failed: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        let login_token = db_user
            .and_then(|u| u.github_access_token)
            .filter(|t| !t.is_empty());

        if let Some(token) = login_token {
            token
        } else {
            // Fall back: token stored on a namespace via the OAuth popup flow.
            use entity::git_namespaces;
            entity::prelude::GitNamespaces::find()
                .filter(git_namespaces::Column::UserId.eq(user.id))
                .filter(git_namespaces::Column::Slug.ne("pat"))
                .all(&db)
                .await
                .map_err(|e| {
                    error!("DB query failed: {}", e);
                    StatusCode::INTERNAL_SERVER_ERROR
                })?
                .into_iter()
                .find(|n| !n.oauth_token.is_empty())
                .map(|n| n.oauth_token)
                .ok_or(StatusCode::NOT_FOUND)?
        }
    };

    let installations = fetch_user_app_installations(&github_token).await?;
    let ids: Vec<i64> = installations.iter().map(|i| i.id).collect();
    let selection_token = sign_selection_token(&ids, &user.id)?;
    Ok(ResponseJson(MyInstallationsResponse {
        installations,
        selection_token,
    }))
}

async fn upsert_app_namespace(
    db: &sea_orm::DatabaseConnection,
    user: &oxy_auth::types::AuthenticatedUser,
    installation_id: i64,
    name: &str,
    owner_type: &str,
    user_token: &str,
) -> Result<GitHubNamespace, axum::http::StatusCode> {
    use entity::git_namespaces;
    use sea_orm::{ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter};

    if let Some(existing) = entity::prelude::GitNamespaces::find()
        .filter(git_namespaces::Column::InstallationId.eq(installation_id))
        .one(db)
        .await
        .map_err(|e| {
            error!("DB lookup failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
    {
        // Update the stored token if a newer one is provided.
        if !user_token.is_empty() {
            let mut active = existing.clone().into_active_model();
            active.oauth_token = Set(user_token.to_string());
            active.save(db).await.map_err(|e| {
                error!("DB token update failed: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        }
        return Ok(GitHubNamespace {
            id: existing.id,
            owner_type: existing.owner_type,
            slug: existing.slug,
            name: existing.name,
        });
    }

    let ns = entity::git_namespaces::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_id: Set(user.id),
        name: Set(name.to_string()),
        slug: Set(name.to_lowercase()),
        owner_type: Set(owner_type.to_string()),
        installation_id: Set(installation_id),
        provider: Set("github".to_string()),
        oauth_token: Set(user_token.to_string()),
    }
    .insert(db)
    .await
    .map_err(|e| {
        error!("DB insert failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(GitHubNamespace {
        id: ns.id,
        owner_type: ns.owner_type,
        slug: ns.slug,
        name: ns.name,
    })
}
