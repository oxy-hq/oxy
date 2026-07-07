//! `GET /api/{workspace_id}/logo` — workspace branding.
//!
//! Resolution order:
//!   1. The org-level **uploaded** logo (Settings → General), stored on the
//!      organization row — see `org_logo`. White-labels the HQ chrome.
//!   2. The code-first `logo.{svg,png,jpg,jpeg,webp}` file (in that
//!      precedence order) at the workspace root, beside `config.yml`.
//!   3. 404 (the frontend then renders the name initial).
//!
//! The file candidate list is fixed — no user input ever reaches the
//! filesystem path, so there is no traversal surface.
//!
//! SVG logos can carry inline `<script>`, so every logo is served through
//! `logo_response`, which attaches download + sandbox headers that neutralize
//! script execution on direct navigation without affecting the `<img>` chrome.

use axum::extract::Path as AxumPath;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use entity::{organizations, workspaces};
use oxy::database::client::establish_connection;
use sea_orm::EntityTrait;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::server::api::middlewares::workspace_context::WorkspaceManagerExtractor;

/// Candidate file names in precedence order, with their content types.
const LOGO_CANDIDATES: [(&str, &str); 5] = [
    ("logo.svg", "image/svg+xml"),
    ("logo.png", "image/png"),
    ("logo.jpg", "image/jpeg"),
    ("logo.jpeg", "image/jpeg"),
    ("logo.webp", "image/webp"),
];

/// First existing candidate under `root`, with its content type.
fn find_logo(root: impl AsRef<Path>) -> Option<(PathBuf, &'static str)> {
    LOGO_CANDIDATES.iter().find_map(|(name, mime)| {
        let p = root.as_ref().join(name);
        p.is_file().then_some((p, *mime))
    })
}

/// Build a logo response with stored-XSS hardening headers.
///
/// An SVG logo (`image/svg+xml`) can embed `<script>` that executes if the URL
/// is opened as a **top-level document**. Both the org-uploaded logo (which an
/// org admin controls) and the code-first `logo.svg` flow through here, so the
/// vector is neutralized at this single serving boundary:
///
/// - `Content-Disposition: attachment` — a direct navigation downloads the
///   bytes instead of rendering them as a document.
/// - `Content-Security-Policy: default-src 'none'; sandbox` — even if rendered,
///   the document is sandboxed (unique origin, scripts disabled) and may load
///   no subresources.
/// - `X-Content-Type-Options: nosniff` — honor our `Content-Type` instead of
///   sniffing the bytes into an executable type.
///
/// None of these affect the legitimate `<img>` embedding in the rail/heading:
/// `<img>` ignores `Content-Disposition`, renders SVG in script-free secure
/// mode, and is not governed by the response's own CSP.
pub(crate) fn logo_response(content_type: impl Into<String>, bytes: Vec<u8>) -> Response {
    (
        [
            (header::CONTENT_TYPE, content_type.into()),
            // Branding changes anytime (git pull or re-upload) — revalidate.
            // The frontend's `?v=updated_at` busts the already-rendered <img>.
            (header::CACHE_CONTROL, "no-cache".to_string()),
            (header::CONTENT_DISPOSITION, "attachment".to_string()),
            (
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; sandbox".to_string(),
            ),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
        ],
        bytes,
    )
        .into_response()
}

/// The org-level uploaded logo for the workspace's org, if any. Two small
/// lookups (workspace → org_id → org); returns `None` in local mode (nil
/// workspace, no org row) or when no logo has been uploaded.
async fn uploaded_org_logo(workspace_id: Uuid) -> Option<Response> {
    let db = establish_connection().await.ok()?;
    let ws = workspaces::Entity::find_by_id(workspace_id)
        .one(&db)
        .await
        .ok()??;
    let org = organizations::Entity::find_by_id(ws.org_id?)
        .one(&db)
        .await
        .ok()??;
    let bytes = org.logo?;
    let mime = org
        .logo_content_type
        .unwrap_or_else(|| "image/png".to_string());
    Some(logo_response(mime, bytes))
}

pub async fn get_workspace_logo(
    AxumPath(workspace_id): AxumPath<Uuid>,
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
) -> Result<Response, StatusCode> {
    // 1. Org-level uploaded logo wins (white-label).
    if let Some(resp) = uploaded_org_logo(workspace_id).await {
        return Ok(resp);
    }
    // 2. Fall back to the code-first file at the workspace root.
    let root = workspace_manager.config_manager.workspace_path();
    let Some((path, mime)) = find_logo(root) else {
        return Err(StatusCode::NOT_FOUND);
    };
    let bytes = tokio::fs::read(&path).await.map_err(|e| {
        tracing::warn!("workspace logo read failed at {}: {e}", path.display());
        StatusCode::NOT_FOUND
    })?;
    Ok(logo_response(mime, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logo_response_carries_xss_hardening_headers() {
        // An SVG logo is attacker-controllable (org-admin upload); the serve
        // boundary must neutralize inline-script execution.
        let resp = logo_response("image/svg+xml", b"<svg/>".to_vec());
        let h = resp.headers();
        assert_eq!(h.get(header::CONTENT_TYPE).unwrap(), "image/svg+xml");
        assert_eq!(h.get(header::CONTENT_DISPOSITION).unwrap(), "attachment");
        assert_eq!(
            h.get(header::CONTENT_SECURITY_POLICY).unwrap(),
            "default-src 'none'; sandbox"
        );
        assert_eq!(h.get(header::X_CONTENT_TYPE_OPTIONS).unwrap(), "nosniff");
        assert_eq!(h.get(header::CACHE_CONTROL).unwrap(), "no-cache");
    }

    #[test]
    fn absent_logo_yields_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(find_logo(dir.path()).is_none());
    }

    #[test]
    fn precedence_svg_beats_png() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("logo.png"), b"png").unwrap();
        std::fs::write(dir.path().join("logo.svg"), b"svg").unwrap();
        let (path, mime) = find_logo(dir.path()).unwrap();
        assert!(path.ends_with("logo.svg"));
        assert_eq!(mime, "image/svg+xml");
    }

    #[test]
    fn content_types_map_by_extension() {
        for (name, expected) in LOGO_CANDIDATES {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join(name), b"x").unwrap();
            let (_, mime) = find_logo(dir.path()).unwrap();
            assert_eq!(mime, expected, "for {name}");
        }
    }

    #[test]
    fn directories_named_logo_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("logo.svg")).unwrap();
        std::fs::write(dir.path().join("logo.png"), b"png").unwrap();
        let (path, mime) = find_logo(dir.path()).unwrap();
        assert!(path.ends_with("logo.png"));
        assert_eq!(mime, "image/png");
    }
}
