//! World-model "Apps" — Toast / OpenWeatherMap / BestTime configuration
//! surfaced through the Workspace Settings UI.
//!
//! Each app is stored in `config.yml` as an entry under `integrations:`
//! using the existing tagged-enum schema. Credentials are referenced via
//! `*_var: SECRET_NAME` and resolved through the workspace secrets store
//! at request time. This module only edits the config metadata — secret
//! values are managed through the separate `/workspaces/.../secrets`
//! endpoints.

use axum::Json;
use axum::extract::Path;
use axum::http::StatusCode;
use oxy::config::model::{
    BestTimeIntegration, Integration, IntegrationType, OpenWeatherMapIntegration, ToastIntegration,
    UnifiIntegration,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::server::api::middlewares::role_guards::WorkspaceAdmin;
use crate::server::api::middlewares::workspace_context::{
    WorkspaceManagerReadOnly, WorkspaceManagerWorkingCopy,
};

/// Subset of the integration entry returned to the settings UI. Never
/// includes resolved secret values — only the `*_var` reference so the
/// frontend can show what secret name is wired up.
#[derive(Debug, Serialize)]
pub struct AppSummary {
    pub kind: String,
    pub name: String,
    #[serde(flatten)]
    pub config: AppConfigSummary,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum AppConfigSummary {
    Toast {
        webhook_secret_var: String,
        restaurant_guids: Vec<String>,
    },
    OpenWeatherMap {
        api_key_var: String,
    },
    BestTime {
        api_key_var: String,
    },
    Unifi {
        api_key_var: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum UpsertAppRequest {
    Toast {
        name: String,
        webhook_secret_var: String,
        #[serde(default)]
        restaurant_guids: Vec<String>,
    },
    #[serde(rename = "openweathermap")]
    OpenWeatherMap { name: String, api_key_var: String },
    #[serde(rename = "besttime")]
    BestTime { name: String, api_key_var: String },
    #[serde(rename = "unifi")]
    Unifi { name: String, api_key_var: String },
}

/// `GET /api/{workspace_id}/apps`
pub async fn list_apps(
    WorkspaceManagerReadOnly(workspace_manager): WorkspaceManagerReadOnly,
    Path(workspace_id): Path<Uuid>,
) -> Result<Json<Vec<AppSummary>>, (StatusCode, String)> {
    let _ = workspace_id;
    let config = workspace_manager.config_manager.get_config();
    let summaries = config
        .integrations
        .iter()
        .filter_map(summarize_app)
        .collect();
    Ok(Json(summaries))
}

/// `POST /api/{workspace_id}/apps`
pub async fn upsert_app(
    _: WorkspaceAdmin,
    WorkspaceManagerWorkingCopy(workspace_manager): WorkspaceManagerWorkingCopy,
    Path(workspace_id): Path<Uuid>,
    Json(body): Json<UpsertAppRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    // Defense in depth: this handler mutates `config.yml`. If the route is
    // ever misclassified `FleetOk` and reaches a stateless replica, fail
    // loudly here rather than write to a disk nobody else reads.
    crate::server::role_manifest::ensure_fs_writable("upsert app integration in config.yml")
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let integration = build_integration(body);
    workspace_manager
        .config_manager
        .upsert_integration(integration)
        .await
        .map_err(|e| {
            tracing::error!(workspace = %workspace_id, error = %e, "failed to upsert app");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to upsert app: {e}"),
            )
        })?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /api/{workspace_id}/apps/{kind}` — kind is `toast`,
/// `openweathermap`, or `besttime`. Idempotent.
pub async fn delete_app(
    _: WorkspaceAdmin,
    WorkspaceManagerWorkingCopy(workspace_manager): WorkspaceManagerWorkingCopy,
    Path((workspace_id, kind)): Path<(Uuid, String)>,
) -> Result<StatusCode, (StatusCode, String)> {
    validate_app_kind(&kind)?;
    // Defense in depth: this handler mutates `config.yml`. If the route is
    // ever misclassified `FleetOk` and reaches a stateless replica, fail
    // loudly here rather than write to a disk nobody else reads.
    crate::server::role_manifest::ensure_fs_writable("remove app integration from config.yml")
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    workspace_manager
        .config_manager
        .remove_integration_by_kind(&kind)
        .await
        .map_err(|e| {
            tracing::error!(workspace = %workspace_id, error = %e, "failed to remove app");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to remove app: {e}"),
            )
        })?;
    Ok(StatusCode::NO_CONTENT)
}

fn summarize_app(integration: &Integration) -> Option<AppSummary> {
    let (kind, config) = match &integration.integration_type {
        IntegrationType::Toast(t) => (
            "toast",
            AppConfigSummary::Toast {
                webhook_secret_var: t.webhook_secret_var.clone(),
                restaurant_guids: t.restaurant_guids.clone(),
            },
        ),
        IntegrationType::OpenWeatherMap(o) => (
            "openweathermap",
            AppConfigSummary::OpenWeatherMap {
                api_key_var: o.api_key_var.clone(),
            },
        ),
        IntegrationType::BestTime(b) => (
            "besttime",
            AppConfigSummary::BestTime {
                api_key_var: b.api_key_var.clone(),
            },
        ),
        IntegrationType::Unifi(u) => (
            "unifi",
            AppConfigSummary::Unifi {
                api_key_var: u.api_key_var.clone(),
            },
        ),
        // Omni and Looker are surfaced through other settings panels;
        // toast_analytics is an admin-only reconciliation source (config.yml).
        IntegrationType::Omni(_)
        | IntegrationType::Looker(_)
        | IntegrationType::ToastAnalytics(_) => return None,
    };
    Some(AppSummary {
        kind: kind.to_string(),
        name: integration.name.clone(),
        config,
    })
}

fn build_integration(req: UpsertAppRequest) -> Integration {
    match req {
        UpsertAppRequest::Toast {
            name,
            webhook_secret_var,
            restaurant_guids,
        } => Integration {
            name,
            integration_type: IntegrationType::Toast(ToastIntegration {
                webhook_secret_var,
                restaurant_guids,
            }),
        },
        UpsertAppRequest::OpenWeatherMap { name, api_key_var } => Integration {
            name,
            integration_type: IntegrationType::OpenWeatherMap(OpenWeatherMapIntegration {
                api_key_var,
            }),
        },
        UpsertAppRequest::BestTime { name, api_key_var } => Integration {
            name,
            integration_type: IntegrationType::BestTime(BestTimeIntegration { api_key_var }),
        },
        UpsertAppRequest::Unifi { name, api_key_var } => Integration {
            name,
            integration_type: IntegrationType::Unifi(UnifiIntegration { api_key_var }),
        },
    }
}

fn validate_app_kind(kind: &str) -> Result<(), (StatusCode, String)> {
    match kind {
        "toast" | "openweathermap" | "besttime" | "unifi" => Ok(()),
        other => Err((
            StatusCode::BAD_REQUEST,
            format!("unknown app kind '{other}'"),
        )),
    }
}
