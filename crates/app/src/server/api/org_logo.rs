//! Org-level logo upload / remove. The uploaded image white-labels the
//! workspace HQ chrome (rail tile + HQ heading). Bytes live inline on the
//! organization row; **serving** is handled by `workspace_logo`, which
//! prefers the org logo over the code-first `logo.*` file.
//!
//! Admin-gated via the `OrgAdmin` extractor (same bar as renaming the org).
//! The image is sent as a raw request body with its `Content-Type` header —
//! no multipart — which keeps both the handler and the frontend trivial.

use axum::body::Bytes;
use axum::http::{HeaderMap, StatusCode, header};
use chrono::Utc;
use entity::organizations;
use oxy::database::client::establish_connection;
use sea_orm::{ActiveModelTrait, ActiveValue};

use super::middlewares::role_guards::OrgAdmin;

/// Logos are tiny; cap well below axum's default 2 MB body limit.
const MAX_LOGO_BYTES: usize = 1024 * 1024; // 1 MB

// `image/svg+xml` is intentionally allowed: SVG is the ideal logo format, and
// the stored-XSS risk it carries (inline `<script>`) is neutralized at the
// serving boundary — `workspace_logo::logo_response` serves every logo with
// `Content-Disposition: attachment` + a sandboxing CSP, so a malicious SVG
// cannot execute even via direct navigation. Do NOT drop SVG here as a "fix"
// without first removing those serve-time headers.
const ALLOWED_TYPES: [&str; 5] = [
    "image/svg+xml",
    "image/png",
    "image/jpeg",
    "image/webp",
    "image/gif",
];

/// The base content type (sans `; charset=…`) if it's an allowed image kind.
fn allowed_content_type(headers: &HeaderMap) -> Option<&'static str> {
    let raw = headers.get(header::CONTENT_TYPE)?.to_str().ok()?;
    let base = raw.split(';').next()?.trim();
    ALLOWED_TYPES
        .iter()
        .copied()
        .find(|t| t.eq_ignore_ascii_case(base))
}

/// `PUT /orgs/{org_id}/logo` — store the raw image bytes on the org row.
pub async fn upload_org_logo(
    OrgAdmin(ctx): OrgAdmin,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, StatusCode> {
    let Some(content_type) = allowed_content_type(&headers) else {
        return Err(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    };
    if body.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    if body.len() > MAX_LOGO_BYTES {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }

    let db = establish_connection().await.map_err(|e| {
        tracing::error!("upload_org_logo DB connect failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let mut active: organizations::ActiveModel = ctx.org.clone().into();
    active.logo = ActiveValue::Set(Some(body.to_vec()));
    active.logo_content_type = ActiveValue::Set(Some(content_type.to_string()));
    // Bump updated_at so the frontend's `?v=updated_at` cache-bust changes
    // and the already-rendered rail/heading <img> refetches the new logo.
    active.updated_at = ActiveValue::Set(Utc::now().fixed_offset());
    active.update(&db).await.map_err(|e| {
        tracing::error!("upload_org_logo update failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /orgs/{org_id}/logo` — clear the org logo (revert to the
/// code-first file, then the name initial).
pub async fn delete_org_logo(OrgAdmin(ctx): OrgAdmin) -> Result<StatusCode, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("delete_org_logo DB connect failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let mut active: organizations::ActiveModel = ctx.org.clone().into();
    active.logo = ActiveValue::Set(None);
    active.logo_content_type = ActiveValue::Set(None);
    active.updated_at = ActiveValue::Set(Utc::now().fixed_offset());
    active.update(&db).await.map_err(|e| {
        tracing::error!("delete_org_logo update failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers_with(ct: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(header::CONTENT_TYPE, HeaderValue::from_str(ct).unwrap());
        h
    }

    #[test]
    fn accepts_known_image_types_and_strips_charset() {
        assert_eq!(
            allowed_content_type(&headers_with("image/png")),
            Some("image/png")
        );
        assert_eq!(
            allowed_content_type(&headers_with("image/svg+xml; charset=utf-8")),
            Some("image/svg+xml")
        );
        assert_eq!(
            allowed_content_type(&headers_with("IMAGE/PNG")),
            Some("image/png")
        );
    }

    #[test]
    fn rejects_unknown_or_missing_types() {
        assert!(allowed_content_type(&headers_with("application/pdf")).is_none());
        assert!(allowed_content_type(&headers_with("text/html")).is_none());
        assert!(allowed_content_type(&HeaderMap::new()).is_none());
    }
}
