use axum::body::Bytes;
use axum::http::StatusCode;
use hmac::{Hmac, Mac};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde::Deserialize;
use sha2::Sha256;
use tracing::{error, info};

use crate::database::client::establish_connection;
use oxy_shared::errors::OxyError;

#[derive(Debug, Deserialize)]
pub struct WebhookPayload {
    pub action: String,
    pub installation: Installation,
}

#[derive(Debug, Deserialize)]
pub struct Installation {
    pub id: i64,
    pub account: Account,
}

#[derive(Debug, Deserialize)]
pub struct Account {
    pub login: String,
    #[serde(rename = "type")]
    pub account_type: String,
}

pub fn verify_signature(secret: &str, payload: &[u8], signature: &str) -> Result<bool, OxyError> {
    let signature = signature
        .strip_prefix("sha256=")
        .ok_or_else(|| OxyError::RuntimeError("Invalid signature format".to_string()))?;

    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .map_err(|e| OxyError::RuntimeError(format!("Invalid secret key: {}", e)))?;

    mac.update(payload);

    let expected = hex::encode(mac.finalize().into_bytes());

    Ok(constant_time_eq::constant_time_eq(
        signature.as_bytes(),
        expected.as_bytes(),
    ))
}

pub async fn handle_webhook(
    signature: String,
    payload: Bytes,
) -> Result<StatusCode, axum::http::StatusCode> {
    info!("Received GitHub webhook event: {:?}", payload);

    let webhook_secret =
        std::env::var("GITHUB_WEBHOOK_SECRET").unwrap_or_else(|_| "default_secret".to_string());

    if !verify_signature(&webhook_secret, &payload, &signature)
        .map_err(|_| StatusCode::UNAUTHORIZED)?
    {
        error!("Invalid webhook signature");
        return Err(StatusCode::UNAUTHORIZED);
    }

    let webhook: WebhookPayload = serde_json::from_slice(&payload).map_err(|e| {
        error!("Failed to parse webhook payload: {}", e);
        StatusCode::BAD_REQUEST
    })?;

    info!(
        "Received GitHub webhook: action={}, installation_id={}",
        webhook.action, webhook.installation.id
    );

    if webhook.action != "deleted" {
        info!(
            "Ignoring webhook event type '{}' (only 'deleted' events are processed)",
            webhook.action
        );
        return Ok(StatusCode::OK);
    }

    handle_installation_deleted(webhook.installation).await?;

    Ok(StatusCode::OK)
}

async fn handle_installation_deleted(installation: Installation) -> Result<(), StatusCode> {
    let db = establish_connection()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    info!(
        "Processing installation.deleted for installation_id={}, account={} ({})",
        installation.id, installation.account.login, installation.account.account_type
    );

    entity::git_namespaces::Entity::delete_many()
        .filter(entity::git_namespaces::Column::InstallationId.eq(installation.id))
        .exec(&db)
        .await
        .map_err(|e| {
            error!("Database error while deleting namespaces: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    info!(
        "Successfully processed installation.deleted for git_namespace {}",
        installation.id
    );

    Ok(())
}
