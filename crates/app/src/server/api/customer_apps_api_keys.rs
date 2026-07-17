//! `POST /api/customer-apps/{id}/api-keys` — mint an API key bound to a
//! specific customer app. Used for the Vercel-hosted bundle's server-side
//! calls back into oxy (the bundle's client-side calls use the visitor's
//! cookie via the same-origin reverse proxy).
//!
//! The row is tagged with `app_id` (so deleting the app cascades and
//! revokes the key) and `project_id` (matches the existing scoping column).
//! **However**, `ApiKeyService::validate_api_key` today only looks up the
//! row by `key_hash` and authenticates the request as the minting *user* —
//! `app_id` / `project_id` are not yet enforced at request time. In other
//! words, a leaked key has the minter's user-level access, not "this app's
//! project only". Proper scope enforcement (a downstream middleware that
//! reads `app_id` off `ValidatedApiKey` and rejects out-of-scope requests)
//! is a separate follow-up; this PR makes the binding *recordable* so that
//! work has something to read.
//!
//! Mounted behind the `/customer-apps` nest's `oxy_owner_or_app_admin_guard` so only
//! app-admins can mint. Org owners get the same effective access via the
//! parallel `/admin/apps` surface.
//!
//! TODO(hash-at-rest): the `key_hash` column currently stores the raw key
//! to match the existing `validate_api_key` lookup. When the auth-hardening
//! PR flips this minter, `ApiKeyService::create_api_key` (auth crate) needs
//! to flip in lockstep — see the matching TODO there.

use axum::Json;
use axum::extract::Path;
use axum::http::StatusCode;
use chrono::Utc;
use entity::{api_keys, apps};
use oxy::database::client::establish_connection;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use sea_orm::{ActiveModelTrait, ActiveValue, EntityTrait};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct MintRequest {
    /// Optional human-readable label shown in admin (e.g. "production",
    /// "preview deploy"). Defaults to the app slug.
    pub name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MintResponse {
    pub id: Uuid,
    /// Plaintext key — shown **once**. Pastes straight into Vercel's
    /// project env as `OXY_API_KEY`.
    pub key: String,
    pub name: String,
    pub app_id: Uuid,
    pub project_id: Uuid,
    pub created_at: String,
}

pub async fn mint(
    Path(app_id): Path<Uuid>,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    body: Option<Json<MintRequest>>,
) -> Result<Json<MintResponse>, (StatusCode, String)> {
    let body = body.map(|Json(b)| b).unwrap_or(MintRequest { name: None });
    let db = establish_connection()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("db: {e}")))?;

    let app = apps::Entity::find_by_id(app_id)
        .one(&db)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("app lookup: {e}"),
            )
        })?
        .ok_or((StatusCode::NOT_FOUND, "app not found".to_string()))?;

    let name = body
        .name
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| format!("customer-app:{}", app.slug));

    let raw_key = generate_key();
    let now = Utc::now().fixed_offset();
    let id = Uuid::new_v4();

    let model = api_keys::ActiveModel {
        id: ActiveValue::Set(id),
        user_id: ActiveValue::Set(user.id),
        // TODO(hash-at-rest): store sha256(raw_key) once
        // ApiKeyService::validate_api_key learns to hash the lookup key.
        // Flip both sites in lockstep.
        key_hash: ActiveValue::Set(raw_key.clone()),
        name: ActiveValue::Set(name.clone()),
        // TODO(active-and-expiry-check): minted with no expiry, and
        // ApiKeyService::validate_api_key currently ignores `is_active`
        // and `expires_at` (see crates/auth/src/api_key_domain.rs). A
        // leaked key stays valid until the row (or its parent app) is
        // hard-deleted. The auth-hardening PR should (a) teach
        // validate_api_key to honour both columns, and (b) either give
        // mint a default TTL or surface one on MintRequest.
        expires_at: ActiveValue::Set(None),
        last_used_at: ActiveValue::Set(None),
        created_at: ActiveValue::Set(now),
        updated_at: ActiveValue::Set(now),
        is_active: ActiveValue::Set(true),
        project_id: ActiveValue::Set(app.project_id),
        app_id: ActiveValue::Set(Some(app.id)),
    };
    model
        .insert(&db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("insert: {e}")))?;

    Ok(Json(MintResponse {
        id,
        key: raw_key,
        name,
        app_id: app.id,
        project_id: app.project_id,
        created_at: now.to_rfc3339(),
    }))
}

/// `oxy_app_` prefix makes a stray key obvious in logs/dumps and easy to
/// grep for. 32 bytes of entropy from a UUIDv4 (122 random bits) — fine
/// for bearer credentials at this scope; rotate via the same endpoint.
fn generate_key() -> String {
    let raw = Uuid::new_v4().simple().to_string();
    let raw2 = Uuid::new_v4().simple().to_string();
    format!("oxy_app_{raw}{raw2}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_has_recognisable_prefix() {
        let k = generate_key();
        assert!(k.starts_with("oxy_app_"));
        // 8 chars prefix + 32 hex + 32 hex = 72.
        assert_eq!(k.len(), 8 + 64);
    }

    #[test]
    fn key_is_nondeterministic() {
        assert_ne!(generate_key(), generate_key());
    }
}
