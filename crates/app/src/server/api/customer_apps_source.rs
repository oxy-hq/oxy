//! `AppSource` — facade over the three runtime sources for a customer app.
//!
//! Each registered app has a `source_type` column with `source_config`
//! payload. At request time the bundle-serve handler parses the model into
//! one of these variants and dispatches:
//!
//!   - [`AppSource::V0`]: oxy is just an auth gate; it returns a minimal
//!     HTML wrapper that loads the v0.dev URL in an iframe. The v0 page
//!     itself has no oxy data access.
//!   - [`AppSource::LocalFolder`]: oxy reads `<path>/<asset>` directly —
//!     `path` IS the bundle directory (the one with `index.html`).
//!     Whatever the dev's bundler names its output (`out/`, `dist/`,
//!     `build/`, …), pass that path in. Used by developers wiring up
//!     their local `customer-apps` checkout — no S3, no state-dir copy.
//!   - [`AppSource::S3`]: the original flow — bundle was synced to
//!     `$OXY_STATE_DIR/customer-apps/<uuid>/out/` by `POST /sync` and oxy
//!     serves from there.
//!
//! Adding a fourth variant is contained: extend the enum + the `parse` +
//! `handle` methods; nothing else in the request path needs to change.

use std::path::PathBuf;

use entity::apps;
use serde::{Deserialize, Serialize};

/// The three runtime sources oxy knows how to render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppSource {
    /// External v0.dev (or similar) URL, wrapped in an auth-gated iframe.
    V0 { url: String },
    /// Absolute path on the oxy host's filesystem that contains
    /// `index.html` and the rest of the static assets. No implicit
    /// subdirectory — for a Next.js static export point at
    /// `<project>/out`; for Vite/Astro/Rsbuild point at `<project>/dist`.
    /// Only meaningful when oxy is running on a developer machine.
    LocalFolder { path: PathBuf },
    /// Bundle synced from `s3://<bucket>/apps/<uuid>/out/` to state dir by
    /// the sync handler. The default for hosted instances.
    S3,
}

/// Wire representation of `source_config` per variant. Kept as a tagged
/// union so the API contract is explicit on both ends.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SourceSpec {
    V0 { url: String },
    Local { path: String },
    S3,
}

impl SourceSpec {
    /// Serialise to the `(source_type, source_config)` column pair stored on
    /// the `apps` table.
    pub fn into_columns(self) -> (String, serde_json::Value) {
        match self {
            SourceSpec::V0 { url } => ("v0".to_string(), serde_json::json!({ "url": url })),
            SourceSpec::Local { path } => {
                ("local".to_string(), serde_json::json!({ "path": path }))
            }
            SourceSpec::S3 => ("s3".to_string(), serde_json::json!({})),
        }
    }
}

#[derive(Debug)]
pub enum ParseError {
    /// `source_type` is not one of the known variants.
    UnknownType(String),
    /// `source_config` is missing a required field, or has the wrong shape.
    BadConfig(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::UnknownType(t) => write!(f, "unknown source_type {t:?}"),
            ParseError::BadConfig(msg) => write!(f, "bad source_config: {msg}"),
        }
    }
}

impl std::error::Error for ParseError {}

impl AppSource {
    /// Read `(source_type, source_config)` from an `apps` row and produce
    /// a typed variant. Existing rows from before the migration land with
    /// `source_type = "s3"` and `source_config = {}` so they round-trip
    /// cleanly to [`AppSource::S3`].
    pub fn from_model(app: &apps::Model) -> Result<Self, ParseError> {
        match app.source_type.as_str() {
            "v0" => {
                let url = app
                    .source_config
                    .get("url")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ParseError::BadConfig("missing v0.url".to_string()))?;
                Ok(AppSource::V0 {
                    url: url.to_string(),
                })
            }
            "local" => {
                let path = app
                    .source_config
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ParseError::BadConfig("missing local.path".to_string()))?;
                Ok(AppSource::LocalFolder {
                    path: PathBuf::from(path),
                })
            }
            "s3" => Ok(AppSource::S3),
            other => Err(ParseError::UnknownType(other.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use entity::apps;
    use serde_json::json;

    fn fake_app(source_type: &str, source_config: serde_json::Value) -> apps::Model {
        apps::Model {
            visibility: "org".to_string(),
            id: uuid::Uuid::nil(),
            slug: "x".to_string(),
            name: "X".to_string(),
            org_id: uuid::Uuid::nil(),
            project_id: uuid::Uuid::nil(),
            branch: "main".to_string(),
            source_repo: "oxy-hq/customer-apps".to_string(),
            status: "created".to_string(),
            source_type: source_type.to_string(),
            source_config,
            bootstrap_pr_url: None,
            last_synced_at: None,
            manifest_override: None,
            published_at: None,
            repo_path: None,
            draft_build_id: None,
            published_build_id: None,
            last_promoted_by: None,
            last_promoted_at: None,
            created_at: chrono::Utc::now().fixed_offset(),
            updated_at: chrono::Utc::now().fixed_offset(),
        }
    }

    #[test]
    fn from_model_v0_extracts_url() {
        let app = fake_app("v0", json!({ "url": "https://v0.dev/x" }));
        match AppSource::from_model(&app).unwrap() {
            AppSource::V0 { url } => assert_eq!(url, "https://v0.dev/x"),
            other => panic!("expected V0, got {other:?}"),
        }
    }

    #[test]
    fn from_model_local_extracts_path() {
        let app = fake_app("local", json!({ "path": "/abs/path" }));
        match AppSource::from_model(&app).unwrap() {
            AppSource::LocalFolder { path } => {
                assert_eq!(path, std::path::PathBuf::from("/abs/path"))
            }
            other => panic!("expected LocalFolder, got {other:?}"),
        }
    }

    #[test]
    fn from_model_s3_ignores_config() {
        let app = fake_app("s3", json!({}));
        assert_eq!(AppSource::from_model(&app).unwrap(), AppSource::S3);
    }

    #[test]
    fn from_model_v0_missing_url_errors() {
        let app = fake_app("v0", json!({}));
        let err = AppSource::from_model(&app).unwrap_err();
        assert!(matches!(err, ParseError::BadConfig(_)));
    }

    #[test]
    fn from_model_local_missing_path_errors() {
        let app = fake_app("local", json!({"url": "wrong field"}));
        let err = AppSource::from_model(&app).unwrap_err();
        assert!(matches!(err, ParseError::BadConfig(_)));
    }

    #[test]
    fn from_model_unknown_type_errors() {
        let app = fake_app("vercel", json!({}));
        let err = AppSource::from_model(&app).unwrap_err();
        assert!(matches!(err, ParseError::UnknownType(t) if t == "vercel"));
    }

    #[test]
    fn source_spec_into_columns_round_trip() {
        let cases = [
            (
                SourceSpec::V0 {
                    url: "https://v0.dev/x".to_string(),
                },
                "v0",
            ),
            (
                SourceSpec::Local {
                    path: "/p".to_string(),
                },
                "local",
            ),
            (SourceSpec::S3, "s3"),
        ];
        for (spec, expected_type) in cases {
            let (ty, cfg) = spec.into_columns();
            assert_eq!(ty, expected_type);
            assert!(cfg.is_object());
        }
    }
}
