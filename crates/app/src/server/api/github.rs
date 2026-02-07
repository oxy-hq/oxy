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
pub struct GitHubCallbackQuery {
    pub installation_id: String,
    pub setup_action: Option<String>,
    pub state: String,
}

pub async fn gen_install_app_url(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
) -> Result<ResponseJson<String>, axum::http::StatusCode> {
    let _app_id = std::env::var("GITHUB_APP_ID").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let timestamp = chrono::Utc::now().timestamp();
    let state_data = format!("{}:{}", user.id, timestamp);

    let secret_key = std::env::var("GITHUB_STATE_SECRET")
        .unwrap_or_else(|_| "default_secret_key_change_in_production".to_string());

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

pub async fn list_git_namespaces(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
) -> Result<ResponseJson<GitHubNamespacesResponse>, axum::http::StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let git_namespaces = entity::git_namespaces::Entity::find()
        .filter(entity::git_namespaces::Column::UserId.eq(user.id))
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
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Query(query): Query<GitHubRepositoriesQuery>,
) -> Result<Json<Vec<GitHubRepository>>, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let git_namespace = entity::git_namespaces::Entity::find()
        .filter(entity::git_namespaces::Column::UserId.eq(user.id))
        .filter(entity::git_namespaces::Column::Id.eq(query.git_namespace_id))
        .one(&db)
        .await
        .map_err(|e| {
            error!("Database error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if git_namespace.is_none() {
        error!(
            "Git namespace not found for user {} and id {}",
            user.id, query.git_namespace_id
        );
        return Err(StatusCode::BAD_REQUEST);
    }
    let git_namespace = git_namespace.unwrap();
    let app_auth = GitHubAppAuth::from_env()?;
    let token = app_auth
        .get_installation_token(&git_namespace.installation_id.to_string())
        .await?;
    let client = match GitHubClient::from_token(token) {
        Ok(client) => client,
        Err(e) => {
            error!("Failed to create GitHub client: {}", e);
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    match client.list_repositories().await {
        Ok(repositories) => Ok(Json(repositories)),
        Err(e) => {
            error!("Failed to fetch repositories: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn list_branches(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Query(query): Query<GitHubBranchesQuery>,
) -> Result<Json<Vec<GitHubBranch>>, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let git_namespace = entity::git_namespaces::Entity::find()
        .filter(entity::git_namespaces::Column::UserId.eq(user.id))
        .filter(entity::git_namespaces::Column::Id.eq(query.git_namespace_id))
        .one(&db)
        .await
        .map_err(|e| {
            error!("Database error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if git_namespace.is_none() {
        error!(
            "Git namespace not found for user {} and id {}",
            user.id, query.git_namespace_id
        );
        return Err(StatusCode::BAD_REQUEST);
    }
    let git_namespace = git_namespace.unwrap();
    let app_auth = GitHubAppAuth::from_env()?;
    let token = app_auth
        .get_installation_token(&git_namespace.installation_id.to_string())
        .await?;
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

    let secret_key = std::env::var("GITHUB_STATE_SECRET")
        .unwrap_or_else(|_| "default_secret_key_change_in_production".to_string());

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
