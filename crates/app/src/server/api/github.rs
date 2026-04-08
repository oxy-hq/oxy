use axum::{
    body::Bytes,
    extract::{Json, Query},
    http::HeaderMap,
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
use oxy::github::webhook;
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
pub struct CreateGitNamespaceRequest {
    pub installation_id: String,
    pub code: String,
    pub state: String,
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

/// GET /github/app-installations — list all GitHub App installations using the App JWT.
/// Returns an empty list when the GitHub App is not configured (env vars missing).
#[derive(Debug, Serialize)]
pub struct AppInstallation {
    pub id: i64,
    pub name: String,
    pub owner_type: String,
}

pub async fn list_app_installations(
    _user: AuthenticatedUserExtractor,
) -> ResponseJson<Vec<AppInstallation>> {
    let Ok(app_auth) = GitHubAppAuth::from_env() else {
        return ResponseJson(vec![]);
    };
    match app_auth.list_installations().await {
        Ok(installations) => ResponseJson(
            installations
                .into_iter()
                .map(|i| AppInstallation {
                    id: i.id,
                    name: i.name,
                    owner_type: i.owner_type,
                })
                .collect(),
        ),
        Err(_) => ResponseJson(vec![]),
    }
}

pub async fn gen_install_app_url(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
) -> Result<ResponseJson<String>, axum::http::StatusCode> {
    let _app_id = std::env::var("GITHUB_APP_ID").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let timestamp = chrono::Utc::now().timestamp();
    let state_data = format!("{}:{}", user.id, timestamp);

    let secret_key =
        std::env::var("GITHUB_STATE_SECRET").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut mac = Hmac::<Sha256>::new_from_slice(secret_key.as_bytes())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    mac.update(state_data.as_bytes());

    let signature = hex::encode(mac.finalize().into_bytes());

    let state = format!("{}:{}", state_data, signature);

    let encoded_state = urlencoding::encode(&state);
    let app_slug =
        std::env::var("GITHUB_APP_SLUG").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let url = format!(
        "https://github.com/apps/{}/installations/new?state={}",
        app_slug, encoded_state
    );

    Ok(ResponseJson(url))
}

pub async fn create_git_namespace(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Json(payload): Json<CreateGitNamespaceRequest>,
) -> Result<ResponseJson<GitHubNamespace>, axum::http::StatusCode> {
    if !validate_github_state(&payload.state, &user.id)? {
        error!("Invalid state parameter for user {}", user.id);
        return Err(StatusCode::BAD_REQUEST);
    }

    let app_auth = GitHubAppAuth::from_env()?;
    let installation = app_auth
        .get_installation_info(&payload.installation_id)
        .await?;

    let user_token = if installation.owner_type == "User" {
        app_auth.get_user_oauth_token(&payload.code).await?
    } else {
        "".to_string()
    };
    let db = establish_connection().await.map_err(|e| {
        error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Deduplicate by installation_id across the whole instance — the same org's
    // app installation should only appear once regardless of which user connected it.
    {
        use entity::git_namespaces;
        use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
        if let Some(existing) = entity::prelude::GitNamespaces::find()
            .filter(git_namespaces::Column::InstallationId.eq(installation.id))
            .one(&db)
            .await
            .map_err(|e| {
                error!("DB error checking existing namespace: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?
        {
            return Ok(ResponseJson(GitHubNamespace {
                id: existing.id,
                owner_type: existing.owner_type,
                slug: existing.slug,
                name: existing.name,
            }));
        }
    }

    let git_namespace = entity::git_namespaces::ActiveModel {
        user_id: Set(user.id),
        name: Set(installation.name),
        slug: Set(installation.slug),
        owner_type: Set(installation.owner_type),
        installation_id: Set(installation.id),
        id: Set(Uuid::new_v4()),
        provider: Set("github".into()),
        oauth_token: Set(user_token),
    };
    let git_namespace = git_namespace.insert(&db).await.map_err(|e| {
        error!("Database error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(ResponseJson(GitHubNamespace {
        id: git_namespace.id,
        owner_type: git_namespace.owner_type,
        slug: git_namespace.slug,
        name: git_namespace.name,
    }))
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
    _user: AuthenticatedUserExtractor,
) -> Result<ResponseJson<GitHubNamespacesResponse>, axum::http::StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    // GitHub App installations are instance-wide — the app is installed once on
    // the org by an admin, and all users of this Oxy instance share it.
    // PAT namespaces (slug="pat") are per-user tokens but are also listed here
    // so the UI can show them. Deduplication by installation_id is enforced at
    // creation time, so no duplicates appear here.
    let git_namespaces = entity::git_namespaces::Entity::find()
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
    if git_namespace.is_none() {
        error!("Git namespace not found for id {}", query.git_namespace_id);
        return Err(StatusCode::BAD_REQUEST);
    }
    let git_namespace = git_namespace.unwrap();

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
    if git_namespace.is_none() {
        error!("Git namespace not found for id {}", query.git_namespace_id);
        return Err(StatusCode::BAD_REQUEST);
    }
    let git_namespace = git_namespace.unwrap();

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

// ─── "Already installed" OAuth flow ─────────────────────────────────────────
//
// When the app is already installed the GitHub install URL shows a Configure
// page but does NOT fire the OAuth callback, so the postMessage relay has
// nothing to relay.  This secondary flow lets the user authenticate via GitHub
// OAuth (scoped to read:user + read:org), then the backend cross-references:
//   • GET /user/installations  — installations visible to THIS user token
//   • GET /app/installations   — installations of THIS Oxy GitHub App (JWT)
// The intersection is always limited to this app's installations, so no
// unrelated org/account is ever exposed.

/// GET /github/oauth-connect-url — signed GitHub OAuth URL for the
/// already-installed fallback flow.
pub async fn gen_oauth_connect_url(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    headers: HeaderMap,
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

    let base_url = super::auth::extract_base_url_from_headers(&headers);
    let redirect_uri = urlencoding::encode(&format!("{base_url}/github/callback")).into_owned();

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
    },
    NotInstalled,
}

/// POST /github/namespaces/oauth — exchange an OAuth code to verify GitHub identity,
/// then list all app installations (via App JWT) and auto-connect if unambiguous.
///
/// We do NOT use `GET /user/installations` because that only returns installations
/// where the user has GitHub admin/owner access — excluding regular org members.
///
/// Instead we resolve the user's GitHub identity (their login + org memberships)
/// from the OAuth token and intersect that with the app's installations by slug.
/// This naturally scopes each user to installations they actually belong to,
/// without requiring any operator configuration.
pub async fn connect_namespace_from_oauth(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    headers: HeaderMap,
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

    let user_token =
        exchange_oauth_code(&payload.code, &client_id, &client_secret, &headers).await?;

    // GET /user/installations returns all installations of THIS GitHub App that the
    // authenticated user can access. Requires the App to have "Organization > Members:
    // Read" permission — with that set, org members (not just owners) are included.
    let eligible = fetch_user_app_installations(&user_token).await?;

    tracing::debug!("GitHub OAuth: eligible installations={eligible:?}");

    let db = establish_connection().await.map_err(|e| {
        error!("DB connection failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    match eligible.as_slice() {
        [] => Ok(ResponseJson(OAuthConnectResponse::NotInstalled)),
        [single] => {
            let ns = upsert_app_namespace(&db, &user, single.id, &single.name, &single.owner_type)
                .await?;
            Ok(ResponseJson(OAuthConnectResponse::Connected {
                namespace: ns,
            }))
        }
        _ => Ok(ResponseJson(OAuthConnectResponse::Choose {
            installations: eligible,
        })),
    }
}

#[derive(Debug, Deserialize)]
pub struct OAuthNamespaceRequest {
    pub code: String,
    pub state: String,
}

#[derive(Debug, Deserialize)]
pub struct PickInstallationRequest {
    pub installation_id: i64,
}

/// POST /github/namespaces/pick — connect a specific installation chosen from
/// the OAuth picker.  The installation_id is verified against this app's
/// installations (JWT) before creating the namespace.
pub async fn pick_namespace_installation(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Json(payload): Json<PickInstallationRequest>,
) -> Result<ResponseJson<GitHubNamespace>, axum::http::StatusCode> {
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
    )
    .await?;
    Ok(ResponseJson(ns))
}

// ─── Shared helpers ───────────────────────────────────────────────────────────

async fn exchange_oauth_code(
    code: &str,
    client_id: &str,
    client_secret: &str,
    headers: &HeaderMap,
) -> Result<String, axum::http::StatusCode> {
    let base = super::auth::extract_base_url_from_headers(headers);
    let redirect_uri = format!("{base}/github/callback");

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
            ("redirect_uri", redirect_uri.as_str()),
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

async fn upsert_app_namespace(
    db: &sea_orm::DatabaseConnection,
    user: &oxy_auth::types::AuthenticatedUser,
    installation_id: i64,
    name: &str,
    owner_type: &str,
) -> Result<GitHubNamespace, axum::http::StatusCode> {
    use entity::git_namespaces;
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

    if let Some(existing) = entity::prelude::GitNamespaces::find()
        .filter(git_namespaces::Column::InstallationId.eq(installation_id))
        .one(db)
        .await
        .map_err(|e| {
            error!("DB lookup failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
    {
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
        oauth_token: Set(String::new()),
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

pub async fn github_webhook(
    headers: HeaderMap,
    payload: Bytes,
) -> Result<StatusCode, axum::http::StatusCode> {
    let signature = headers
        .get("X-Hub-Signature-256")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::BAD_REQUEST)?
        .to_string();

    webhook::handle_webhook(signature, payload).await
}
